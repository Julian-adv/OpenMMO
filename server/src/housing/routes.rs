use crate::game_state::GameState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use onlinerpg_shared::housing::{HouseData, RoomType};
use onlinerpg_terrain::{
    grass::{remove_grass_in_rects, GrassRemovalStats},
    io::TerrainIO,
    trees::{remove_trees_in_rects, TreeRemovalStats},
};
use std::sync::Arc;
use tracing::{error, info};

use super::{
    is_valid_house_id, next_house_id, validate_house, validate_house_neighbors, world_to_chunk,
    HousingIO, CHUNK_SIZE, MAX_NEIGHBOR_CHUNK_SPAN,
};

#[derive(Clone)]
struct HousingRouteState {
    housing: Arc<HousingIO>,
    terrain: Arc<TerrainIO>,
    game_state: Arc<GameState>,
}

const TREE_HOUSE_MARGIN: f32 = 2.0;
const GRASS_HOUSE_MARGIN: f32 = 1.0;

pub(crate) fn house_foundation_rects(house: &HouseData, margin: f32) -> Vec<[f32; 4]> {
    house
        .rooms
        .iter()
        .filter(|room| room.floor_level == 0 && room.room_type != RoomType::Stairwell)
        .map(|room| {
            let min_x = house.origin.x + room.local_x as f32;
            let min_z = house.origin.z + room.local_z as f32;
            [
                min_x - margin,
                min_z - margin,
                min_x + room.size_x as f32 + margin,
                min_z + room.size_z as f32 + margin,
            ]
        })
        .collect()
}

pub fn housing_router(
    housing_io: Arc<HousingIO>,
    terrain_io: Arc<TerrainIO>,
    game_state: Arc<GameState>,
) -> Router {
    Router::new()
        .route("/api/housing/area/{cx}/{cz}", get(get_houses_in_chunk))
        .route("/api/housing", post(create_house))
        .route(
            "/api/housing/{house_id}",
            get(get_house).put(update_house).delete(delete_house),
        )
        .with_state(HousingRouteState {
            housing: housing_io,
            terrain: terrain_io,
            game_state,
        })
}

async fn get_houses_in_chunk(
    Path((cx, cz)): Path<(i32, i32)>,
    State(state): State<HousingRouteState>,
) -> Result<Json<Vec<HouseData>>, StatusCode> {
    let mut houses = state.housing.read_chunk(cx, cz).await.map_err(|e| {
        error!("Failed to read housing chunk ({}, {}): {}", cx, cz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    state.game_state.apply_open_door_state(&mut houses).await;
    Ok(Json(houses))
}

async fn get_house(
    Path(house_id): Path<String>,
    State(state): State<HousingRouteState>,
) -> Result<Json<HouseData>, StatusCode> {
    if !is_valid_house_id(&house_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let house = state.housing.find_house(&house_id).await.map_err(|e| {
        error!("Failed to find house {}: {}", house_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match house {
        Some(mut h) => {
            state
                .game_state
                .apply_open_door_state(std::slice::from_mut(&mut h))
                .await;
            Ok(Json(h))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn create_house(
    State(state): State<HousingRouteState>,
    Json(mut house): Json<HouseData>,
) -> Result<(StatusCode, Json<HouseData>), (StatusCode, String)> {
    let _edit = state.game_state.world_edit_guard().await;
    validate_house(&house).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    let (cx, cz) = world_to_chunk(house.origin.x, house.origin.z);
    let all_houses = state.housing.read_all_houses().await.map_err(|e| {
        error!("Failed to read houses for ID allocation: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    house.id = next_house_id(cx, cz, &all_houses);

    validate_house_neighbors(&house, &all_houses).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    state.housing.write_house(&house).await.map_err(|e| {
        error!("Failed to write house {}: {}", house.id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    state.game_state.passability_add_house(&house).await;
    let (tree_stats, grass_stats) = tokio::try_join!(
        remove_house_trees(&state.terrain, &house),
        remove_house_grass(&state.terrain, &house),
    )?;
    publish_house_terrain_changes(
        &state.game_state,
        &[],
        &tree_stats.changed_tiles,
        &grass_stats.changed_tiles,
    )
    .await;
    Ok((StatusCode::CREATED, Json(house)))
}

async fn update_house(
    Path(house_id): Path<String>,
    State(state): State<HousingRouteState>,
    Json(mut house): Json<HouseData>,
) -> Result<Json<HouseData>, (StatusCode, String)> {
    let _edit = state.game_state.world_edit_guard().await;
    if !is_valid_house_id(&house_id) {
        return Err((StatusCode::BAD_REQUEST, "invalid house id".to_string()));
    }
    house.id = house_id;

    let previous = state
        .housing
        .find_house(&house.id)
        .await
        .map_err(|e| {
            error!("Failed to find house {} for update: {}", house.id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "house not found".to_string()))?;

    validate_house(&house).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    let neighbors = load_neighbors(&state.housing, &house).await?;

    validate_house_neighbors(&house, &neighbors).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    state
        .housing
        .replace_house(&previous, &house)
        .await
        .map_err(|e| {
            error!("Failed to write house {}: {}", house.id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;
    state.game_state.passability_add_house(&house).await;
    let (tree_stats, grass_stats) = tokio::try_join!(
        remove_house_trees(&state.terrain, &house),
        remove_house_grass(&state.terrain, &house),
    )?;
    publish_house_terrain_changes(
        &state.game_state,
        &[],
        &tree_stats.changed_tiles,
        &grass_stats.changed_tiles,
    )
    .await;
    Ok(Json(house))
}

async fn load_neighbors(
    housing: &HousingIO,
    house: &HouseData,
) -> Result<Vec<HouseData>, (StatusCode, String)> {
    let mut min_x = house.origin.x;
    let mut max_x = house.origin.x;
    let mut min_z = house.origin.z;
    let mut max_z = house.origin.z;
    for room in &house.rooms {
        let rx = house.origin.x + room.local_x as f32;
        let rz = house.origin.z + room.local_z as f32;
        min_x = min_x.min(rx);
        min_z = min_z.min(rz);
        max_x = max_x.max(rx + room.size_x as f32);
        max_z = max_z.max(rz + room.size_z as f32);
    }
    let c_min_x = (min_x / CHUNK_SIZE).floor() as i32;
    let c_max_x = ((max_x - 0.01) / CHUNK_SIZE).floor() as i32;
    let c_min_z = (min_z / CHUNK_SIZE).floor() as i32;
    let c_max_z = ((max_z - 0.01) / CHUNK_SIZE).floor() as i32;

    // Defense-in-depth: bounds validation keeps a real house within a few
    // chunks, so refuse to scan a large grid even if we were reached without it.
    if (c_max_x - c_min_x + 1).max(c_max_z - c_min_z + 1) > MAX_NEIGHBOR_CHUNK_SPAN {
        return Err((
            StatusCode::BAD_REQUEST,
            "house spans too many chunks".to_string(),
        ));
    }

    let mut neighbors = Vec::new();
    for cz in c_min_z..=c_max_z {
        for cx in c_min_x..=c_max_x {
            let chunk = housing.read_chunk(cx, cz).await.map_err(|e| {
                error!("Failed to read chunk for validation: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            })?;
            neighbors.extend(chunk);
        }
    }
    Ok(neighbors)
}

async fn delete_house(
    Path(house_id): Path<String>,
    State(state): State<HousingRouteState>,
) -> Result<StatusCode, StatusCode> {
    let _edit = state.game_state.world_edit_guard().await;
    if !is_valid_house_id(&house_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Search all chunks for this house
    let house = state.housing.find_house(&house_id).await.map_err(|e| {
        error!("Failed to find house {} for deletion: {}", house_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match house {
        Some(h) => {
            let (cx, cz) = super::world_to_chunk(h.origin.x, h.origin.z);
            state
                .housing
                .delete_house(&house_id, cx, cz)
                .await
                .map_err(|e| {
                    error!("Failed to delete house {}: {}", house_id, e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            state.game_state.passability_remove_house(&house_id).await;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub(crate) async fn remove_house_trees(
    terrain: &TerrainIO,
    house: &HouseData,
) -> Result<TreeRemovalStats, (StatusCode, String)> {
    let rects = house_foundation_rects(house, TREE_HOUSE_MARGIN);

    // An empty rect set yields zeroed stats from `remove_trees_in_rects`, so no
    // early return is needed here.
    let stats = remove_trees_in_rects(terrain, &rects).await.map_err(|e| {
        error!("Failed to remove trees under house {}: {}", house.id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    if stats.trees_removed > 0 {
        info!(
            "Removed {} tree(s) under house {} across {} tile(s)",
            stats.trees_removed, stats.tiles_changed, house.id
        );
    }

    Ok(stats)
}

pub(crate) async fn remove_house_grass(
    terrain: &TerrainIO,
    house: &HouseData,
) -> Result<GrassRemovalStats, (StatusCode, String)> {
    let rects = house_foundation_rects(house, GRASS_HOUSE_MARGIN);

    let stats = remove_grass_in_rects(terrain, &rects).await.map_err(|e| {
        error!("Failed to remove grass under house {}: {}", house.id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    if stats.grass_removed > 0 {
        info!(
            "Removed {} grass instance(s) under house {} across {} tile(s)",
            stats.grass_removed, stats.tiles_changed, house.id
        );
    }

    Ok(stats)
}

pub(crate) async fn publish_house_terrain_changes(
    game_state: &GameState,
    changed_height_tiles: &[(i32, i32)],
    changed_tree_tiles: &[(i32, i32)],
    changed_grass_tiles: &[(i32, i32)],
) {
    let tiles: Vec<_> = changed_height_tiles
        .iter()
        .chain(changed_tree_tiles)
        .chain(changed_grass_tiles)
        .copied()
        .collect();
    if let Err(error) = game_state.publish_terrain_tiles(&tiles).await {
        tracing::error!(%error, "Failed to publish changed terrain");
    }
}

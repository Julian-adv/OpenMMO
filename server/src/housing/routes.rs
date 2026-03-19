use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use onlinerpg_shared::housing::HouseData;
use std::sync::Arc;
use tracing::error;

use super::{next_house_id, validate_house, world_to_chunk, HousingIO, CHUNK_SIZE};

pub fn housing_router(housing_io: Arc<HousingIO>) -> Router {
    Router::new()
        .route("/api/housing/area/{cx}/{cz}", get(get_houses_in_chunk))
        .route("/api/housing", post(create_house))
        .route(
            "/api/housing/{house_id}",
            get(get_house).delete(delete_house),
        )
        .with_state(housing_io)
}

async fn get_houses_in_chunk(
    Path((cx, cz)): Path<(i32, i32)>,
    State(housing): State<Arc<HousingIO>>,
) -> Result<Json<Vec<HouseData>>, StatusCode> {
    let houses = housing.read_chunk(cx, cz).await.map_err(|e| {
        error!("Failed to read housing chunk ({}, {}): {}", cx, cz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(houses))
}

async fn get_house(
    Path(house_id): Path<String>,
    State(housing): State<Arc<HousingIO>>,
) -> Result<Json<HouseData>, StatusCode> {
    let house = housing.find_house(&house_id).await.map_err(|e| {
        error!("Failed to find house {}: {}", house_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match house {
        Some(h) => Ok(Json(h)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn create_house(
    State(housing): State<Arc<HousingIO>>,
    Json(mut house): Json<HouseData>,
) -> Result<(StatusCode, Json<HouseData>), (StatusCode, String)> {
    // Load neighbors for validation, then derive ID from loaded data
    let neighbors = load_neighbors(&housing, &house).await?;
    let (cx, cz) = world_to_chunk(house.origin.x, house.origin.z);
    house.id = next_house_id(cx, cz, &neighbors);

    if let Err(msg) = validate_house(&house, &neighbors) {
        return Err((StatusCode::BAD_REQUEST, msg));
    }

    housing.write_house(&house).await.map_err(|e| {
        error!("Failed to write house {}: {}", house.id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok((StatusCode::CREATED, Json(house)))
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
    State(housing): State<Arc<HousingIO>>,
) -> Result<StatusCode, StatusCode> {
    // Search all chunks for this house
    let house = housing.find_house(&house_id).await.map_err(|e| {
        error!("Failed to find house {} for deletion: {}", house_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match house {
        Some(h) => {
            let (cx, cz) = super::world_to_chunk(h.origin.x, h.origin.z);
            housing.delete_house(&house_id, cx, cz).await.map_err(|e| {
                error!("Failed to delete house {}: {}", house_id, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use onlinerpg_shared::housing::HouseData;
use std::sync::Arc;
use tracing::error;

use super::HousingIO;

pub fn housing_router(housing_io: Arc<HousingIO>) -> Router {
    Router::new()
        .route("/api/housing/area/{cx}/{cz}", get(get_houses_in_chunk))
        .route(
            "/api/housing/{house_id}",
            get(get_house).put(put_house).delete(delete_house),
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

async fn put_house(
    Path(house_id): Path<String>,
    State(housing): State<Arc<HousingIO>>,
    Json(mut house): Json<HouseData>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Ensure ID in path matches body
    house.id = house_id;

    // TODO: validation (room adjacency, overlap, size constraints)

    housing.write_house(&house).await.map_err(|e| {
        error!("Failed to write house {}: {}", house.id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
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

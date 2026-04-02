use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use std::sync::Arc;
use tracing::error;

use super::io::TerrainIO;

pub fn terrain_router(terrain_io: Arc<TerrainIO>) -> Router {
    Router::new()
        .route(
            "/api/terrain/height/{x}/{z}",
            get(get_heightmap).put(put_heightmap),
        )
        .route(
            "/api/terrain/splat/{x}/{z}",
            get(get_splatmap).put(put_splatmap),
        )
        .route(
            "/api/terrain/height-original/{x}/{z}",
            get(get_original_heightmap).put(put_original_heightmap),
        )
        .route(
            "/api/terrain/height-original/{x}/{z}/ensure",
            post(ensure_original_heightmap),
        )
        .route(
            "/api/terrain/grass/{x}/{z}",
            get(get_grass)
                .put(put_grass)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/terrain/grass-original/{x}/{z}",
            get(get_original_grass)
                .put(put_original_grass)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/terrain/grass-original/{x}/{z}/ensure",
            post(ensure_original_grass),
        )
        .route(
            "/api/terrain/meta/{rx}/{rz}",
            get(get_meta).put(put_meta).head(head_meta),
        )
        .route(
            "/api/terrain/minimap/{rx}/{rz}",
            get(get_minimap).put(put_minimap),
        )
        .route("/api/terrain/zones/{rx}/{rz}", get(get_zone).put(put_zone))
        .route(
            "/api/terrain/furniture/{rx}/{rz}",
            get(get_furniture).put(put_furniture),
        )
        .route(
            "/api/terrain/region/{rx}/{rz}",
            delete(delete_region_handler),
        )
        .with_state(terrain_io)
}

async fn get_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_heightmap(x, z).await.map_err(|e| {
        error!("Failed to read heightmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        data,
    )
        .into_response())
}

async fn put_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_heightmap(x, z, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write heightmap ({}, {}): {}", x, z, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_original_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_original_heightmap(x, z).await.map_err(|e| {
        error!("Failed to read original heightmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match data {
        Some(bytes) => {
            Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn put_original_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_original_heightmap(x, z, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write original heightmap ({}, {}): {}", x, z, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_original_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_original_grass(x, z).await.map_err(|e| {
        error!("Failed to read original grass ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match data {
        Some(bytes) => {
            Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn put_original_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_original_grass(x, z, &body)
        .await
        .map_err(|e| {
            error!("Failed to write original grass ({}, {}): {}", x, z, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_original_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<StatusCode, StatusCode> {
    let created = terrain.ensure_original_heightmap(x, z).await.map_err(|e| {
        error!("Failed to ensure original heightmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if created {
        Ok(StatusCode::CREATED)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

async fn ensure_original_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<StatusCode, StatusCode> {
    let created = terrain.ensure_original_grass(x, z).await.map_err(|e| {
        error!("Failed to ensure original grass ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if created {
        Ok(StatusCode::CREATED)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

async fn get_splatmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_splatmap(x, z).await.map_err(|e| {
        error!("Failed to read splatmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        data,
    )
        .into_response())
}

async fn put_splatmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_splatmap(x, z, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write splatmap ({}, {}): {}", x, z, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_grass(x, z).await.map_err(|e| {
        error!("Failed to read grass ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match data {
        Some(bytes) => Ok((
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            bytes,
        )
            .into_response()),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn put_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain.write_grass(x, z, &body).await.map_err(|e| {
        error!("Failed to write grass ({}, {}): {}", x, z, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_meta(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let meta = terrain.read_meta(rx, rz).await.map_err(|e| {
        error!("Failed to read meta ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        Json(meta),
    )
        .into_response())
}

async fn put_meta(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_meta(rx, rz, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write meta ({}, {}): {}", rx, rz, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn head_meta(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> StatusCode {
    match terrain.meta_exists(rx, rz).await {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_minimap(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_minimap(rx, rz).await.map_err(|e| {
        error!("Failed to read minimap ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match data {
        Some(bytes) => Ok((
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            bytes,
        )
            .into_response()),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn put_minimap(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    terrain.write_minimap(rx, rz, &body).await.map_err(|e| {
        error!("Failed to write minimap ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_zone(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let zone = terrain.read_zone(rx, rz).await.map_err(|e| {
        error!("Failed to read zone ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(zone).into_response())
}

async fn put_zone(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain.write_zone(rx, rz, &body).await.map_err(|e| {
        error!("Failed to write zone ({}, {}): {}", rx, rz, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_furniture(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_furniture(rx, rz).await.map_err(|e| {
        error!("Failed to read furniture ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(data).into_response())
}

async fn put_furniture(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain.write_furniture(rx, rz, &body).await.map_err(|e| {
        error!("Failed to write furniture ({}, {}): {}", rx, rz, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_region_handler(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<StatusCode, StatusCode> {
    terrain.delete_region(rx, rz).await.map_err(|e| {
        error!("Failed to delete region ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

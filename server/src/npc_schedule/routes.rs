use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use std::sync::Arc;
use tracing::error;

use super::{NpcIO, ScheduleFile};

pub fn npc_router(npc_io: Arc<NpcIO>) -> Router {
    Router::new()
        .route("/api/npcs", get(list_npcs))
        .route(
            "/api/npcs/{name}/schedule",
            get(get_schedule).put(put_schedule),
        )
        .with_state(npc_io)
}

async fn list_npcs(State(npc_io): State<Arc<NpcIO>>) -> Result<Json<Vec<String>>, StatusCode> {
    let names = npc_io.list_npcs().await.map_err(|e| {
        error!("Failed to list NPCs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(names))
}

async fn get_schedule(
    Path(name): Path<String>,
    State(npc_io): State<Arc<NpcIO>>,
) -> Result<Json<ScheduleFile>, StatusCode> {
    let schedule = npc_io
        .read_schedule(&name)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            std::io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
            _ => {
                error!("Failed to read schedule for '{}': {}", name, e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    Ok(Json(schedule))
}

async fn put_schedule(
    Path(name): Path<String>,
    State(npc_io): State<Arc<NpcIO>>,
    Json(data): Json<ScheduleFile>,
) -> Result<StatusCode, (StatusCode, String)> {
    npc_io
        .write_schedule(&name, &data)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write schedule for '{}': {}", name, e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        })?;
    Ok(StatusCode::OK)
}

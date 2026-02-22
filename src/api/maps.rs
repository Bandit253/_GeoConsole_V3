use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{CreateMapRequest, MapConfig, MapListResponse, UpdateMapRequest};
use crate::AppState;

/// List all maps
pub async fn list_maps(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<MapListResponse>> {
    let maps = tokio::task::spawn_blocking(move || {
        state.db.list_maps()
    })
    .await??;
    Ok(Json(MapListResponse {
        total: maps.len(),
        maps,
    }))
}

/// Create a new map
pub async fn create_map(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateMapRequest>,
) -> AppResult<Json<MapConfig>> {
    let map = tokio::task::spawn_blocking(move || {
        state.db.create_map(
            req.name,
            req.description,
            req.basemap,
            req.view,
        )
    })
    .await??;
    Ok(Json(map))
}

/// Get a single map by ID
pub async fn get_map(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<MapConfig>> {
    let map = tokio::task::spawn_blocking(move || {
        state.db.get_map(id)
    })
    .await??;
    Ok(Json(map))
}

/// Update a map
pub async fn update_map(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMapRequest>,
) -> AppResult<Json<MapConfig>> {
    let map = tokio::task::spawn_blocking(move || {
        state.db.update_map(
            id,
            req.name,
            req.description,
            req.basemap,
            req.view,
            req.layers,
        )
    })
    .await??;
    Ok(Json(map))
}

/// Delete a map
pub async fn delete_map(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    tokio::task::spawn_blocking(move || {
        state.db.delete_map(id)
    })
    .await??;
    Ok(StatusCode::NO_CONTENT)
}

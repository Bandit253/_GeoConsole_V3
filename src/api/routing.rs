use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::models::{RouteRequest, IsochroneRequest};
use crate::AppState;

const VALHALLA_URL: &str = "http://localhost:8002";

/// Calculate a route using Valhalla
pub async fn calculate_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RouteRequest>,
) -> AppResult<impl IntoResponse> {
    let locations: Vec<Value> = req.locations
        .iter()
        .map(|loc| json!({ "lon": loc[0], "lat": loc[1] }))
        .collect();

    let units = req.units.unwrap_or_else(|| "kilometers".to_string());
    let valhalla_request = json!({
        "locations": locations,
        "costing": req.costing,
        "units": &units,
        "directions_options": {
            "units": &units
        }
    });

    let response = state.http_client
        .post(format!("{}/route", VALHALLA_URL))
        .json(&valhalla_request)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Valhalla request failed: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!("Valhalla error: {}", error_text)));
    }

    let route: Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Valhalla response: {}", e)))?;

    Ok((StatusCode::OK, Json(route)))
}

/// Calculate an isochrone using Valhalla
pub async fn calculate_isochrone(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IsochroneRequest>,
) -> AppResult<impl IntoResponse> {
    let locations: Vec<Value> = req.locations
        .iter()
        .map(|loc| json!({ "lon": loc[0], "lat": loc[1] }))
        .collect();

    let contours: Vec<Value> = req.contours
        .iter()
        .map(|c| json!({ "time": c.time, "color": c.color }))
        .collect();

    let valhalla_request = json!({
        "locations": locations,
        "costing": req.costing,
        "contours": contours,
        "polygons": true,
        "denoise": 0.5,
        "generalize": 50
    });

    let response = state.http_client
        .post(format!("{}/isochrone", VALHALLA_URL))
        .json(&valhalla_request)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Valhalla request failed: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!("Valhalla error: {}", error_text)));
    }

    let isochrone: Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Valhalla response: {}", e)))?;

    Ok((StatusCode::OK, Json(isochrone)))
}

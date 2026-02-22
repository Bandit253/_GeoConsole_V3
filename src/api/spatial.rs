use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::Bounds;
use crate::AppState;

/// Maximum features returned in a single request to prevent browser OOM.
/// Clients should use bbox + limit for viewport-based loading of larger datasets.
const MAX_FEATURE_LIMIT: i64 = 500_000;

#[derive(Debug, Deserialize)]
pub struct FeatureQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub bbox: Option<String>,
}

/// Parse bbox string "minx,miny,maxx,maxy" into tuple
fn parse_bbox(bbox: Option<&str>) -> AppResult<Option<(f64, f64, f64, f64)>> {
    match bbox {
        None => Ok(None),
        Some(s) => {
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() != 4 {
                return Err(AppError::BadRequest("bbox must have 4 comma-separated values: minx,miny,maxx,maxy".into()));
            }
            let vals: Result<Vec<f64>, _> = parts.iter().map(|p| p.trim().parse::<f64>()).collect();
            let vals = vals.map_err(|_| AppError::BadRequest("bbox values must be valid numbers".into()))?;
            Ok(Some((vals[0], vals[1], vals[2], vals[3])))
        }
    }
}

/// Apply server-side limit cap. Returns (effective_limit, was_truncated).
fn cap_limit(requested: Option<i64>, dataset_count: i64) -> (i64, bool) {
    let effective = requested.unwrap_or(dataset_count).min(MAX_FEATURE_LIMIT);
    let truncated = requested.map_or(dataset_count > MAX_FEATURE_LIMIT, |r| r > MAX_FEATURE_LIMIT);
    (effective, truncated)
}

/// Get features as Arrow IPC binary stream
pub async fn get_features_arrow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<FeatureQuery>,
) -> AppResult<impl IntoResponse> {
    let bbox = parse_bbox(query.bbox.as_deref())?;
    let requested_limit = query.limit;
    let offset = query.offset;

    // Look up dataset to get feature_count for cap calculation
    let dataset = {
        let state_ref = state.clone();
        tokio::task::spawn_blocking(move || state_ref.db.get_dataset(id)).await??
    };
    let (effective_limit, truncated) = cap_limit(requested_limit, dataset.feature_count);

    let arrow_bytes = tokio::task::spawn_blocking(move || {
        state.db.get_features_arrow(id, Some(effective_limit), offset, bbox)
    })
    .await??;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/vnd.apache.arrow.stream"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert("X-Total-Count", HeaderValue::from(dataset.feature_count as u64));
    if truncated {
        headers.insert("X-Truncated", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, headers, arrow_bytes))
}

/// Get features as GeoJSON
pub async fn get_features_geojson(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<FeatureQuery>,
) -> AppResult<impl IntoResponse> {
    let bbox = parse_bbox(query.bbox.as_deref())?;
    let requested_limit = query.limit;
    let offset = query.offset;

    let dataset = {
        let state_ref = state.clone();
        tokio::task::spawn_blocking(move || state_ref.db.get_dataset(id)).await??
    };
    let (effective_limit, truncated) = cap_limit(requested_limit, dataset.feature_count);

    let geojson = tokio::task::spawn_blocking(move || {
        state.db.get_features_geojson(id, Some(effective_limit), offset, bbox)
    })
    .await??;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/geo+json"));
    headers.insert("X-Total-Count", HeaderValue::from(dataset.feature_count as u64));
    if truncated {
        headers.insert("X-Truncated", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, headers, geojson))
}

/// Get dataset bounds
pub async fn get_bounds(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Bounds>> {
    let bounds = tokio::task::spawn_blocking(move || {
        state.db.get_dataset_bounds(id)
    })
    .await??;
    Ok(Json(bounds))
}

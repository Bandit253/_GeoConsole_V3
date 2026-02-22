use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::models::{Dataset, DatasetListResponse};
use crate::AppState;

/// List all datasets
pub async fn list_datasets(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<DatasetListResponse>> {
    let datasets = tokio::task::spawn_blocking(move || {
        state.db.list_datasets()
    })
    .await??;
    Ok(Json(DatasetListResponse {
        total: datasets.len(),
        datasets,
    }))
}

/// Get a single dataset by ID
pub async fn get_dataset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Dataset>> {
    let dataset = tokio::task::spawn_blocking(move || {
        state.db.get_dataset(id)
    })
    .await??;
    Ok(Json(dataset))
}

/// Upload a spatial file (GeoParquet, GeoPackage, Shapefile, GeoJSON, KML)
pub async fn upload_dataset(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> AppResult<Json<Dataset>> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name = String::from("uploaded");

    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::BadRequest(e.to_string()))? 
    {
        let name = field.name().unwrap_or("").to_string();
        
        if name == "file" {
            file_name = field.file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "uploaded.geojson".to_string());
            
            let data = field.bytes().await
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            file_data = Some(data.to_vec());
        }
    }

    let data = file_data.ok_or_else(|| AppError::BadRequest("No file provided".to_string()))?;

    // Create data directory if it doesn't exist
    let data_dir = std::path::Path::new("data");
    if !data_dir.exists() {
        fs::create_dir_all(data_dir).await?;
    }

    // Save file temporarily
    let temp_path = data_dir.join(&file_name);
    let mut file = fs::File::create(&temp_path).await?;
    file.write_all(&data).await?;
    file.flush().await?;

    // Extract dataset name by removing extension
    let dataset_name = remove_spatial_extension(&file_name);

    let temp_path_str = temp_path.to_string_lossy().to_string();
    let dataset = tokio::task::spawn_blocking(move || {
        state.db.load_spatial_file(&temp_path_str, &dataset_name)
    })
    .await??;

    tracing::info!("Loaded dataset: {} ({} features, {} format)", 
        dataset.name, dataset.feature_count, get_format_name(&file_name));

    Ok(Json(dataset))
}

/// Remove spatial file extension to get dataset name
fn remove_spatial_extension(filename: &str) -> String {
    let extensions = [".parquet", ".geoparquet", ".gpkg", ".shp", ".geojson", ".json", ".kml"];
    let lower = filename.to_lowercase();
    
    for ext in extensions {
        if lower.ends_with(ext) {
            return filename[..filename.len() - ext.len()].to_string();
        }
    }
    filename.to_string()
}

/// Get human-readable format name
fn get_format_name(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".parquet") || lower.ends_with(".geoparquet") {
        "GeoParquet"
    } else if lower.ends_with(".gpkg") {
        "GeoPackage"
    } else if lower.ends_with(".shp") {
        "Shapefile"
    } else if lower.ends_with(".geojson") || lower.ends_with(".json") {
        "GeoJSON"
    } else if lower.ends_with(".kml") {
        "KML"
    } else {
        "Unknown"
    }
}

/// Delete a dataset
pub async fn delete_dataset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    tokio::task::spawn_blocking(move || {
        state.db.delete_dataset(id)
    })
    .await??;
    Ok(StatusCode::NO_CONTENT)
}

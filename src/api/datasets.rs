use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use uuid::Uuid;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::models::{CreateFromSqlRequest, Dataset, DatasetListResponse, PreviewSqlRequest, SqlPreviewResponse};
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

/// Upload one chunk of a large file.
/// Client splits the file into ≤25MB chunks and POSTs each with:
///   session_id, chunk_index, total_chunks, filename, data (bytes)
/// Returns 202 Accepted while chunks are still missing, 200 OK + Dataset when complete.
pub async fn upload_chunk(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut session_id: Option<String> = None;
    let mut chunk_index: Option<u32> = None;
    let mut total_chunks: Option<u32> = None;
    let mut file_name: Option<String> = None;
    let mut chunk_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "session_id" => {
                session_id = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?);
            }
            "chunk_index" => {
                let t = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                chunk_index = Some(t.parse::<u32>().map_err(|_| AppError::BadRequest("Invalid chunk_index".to_string()))?);
            }
            "total_chunks" => {
                let t = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                total_chunks = Some(t.parse::<u32>().map_err(|_| AppError::BadRequest("Invalid total_chunks".to_string()))?);
            }
            "filename" => {
                file_name = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?);
            }
            "data" => {
                let bytes = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                chunk_data = Some(bytes.to_vec());
            }
            _ => { let _ = field.bytes().await; }
        }
    }

    let session_id   = session_id.ok_or_else(|| AppError::BadRequest("Missing session_id".to_string()))?;
    let chunk_index  = chunk_index.ok_or_else(|| AppError::BadRequest("Missing chunk_index".to_string()))?;
    let total_chunks = total_chunks.ok_or_else(|| AppError::BadRequest("Missing total_chunks".to_string()))?;
    let file_name    = file_name.ok_or_else(|| AppError::BadRequest("Missing filename".to_string()))?;
    let chunk_data   = chunk_data.ok_or_else(|| AppError::BadRequest("Missing data".to_string()))?;

    // Basic validation
    if total_chunks == 0 || chunk_index >= total_chunks {
        return Err(AppError::BadRequest(format!("Invalid chunk_index {} for total_chunks {}", chunk_index, total_chunks)));
    }
    if session_id.len() > 64 || !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::BadRequest("Invalid session_id".to_string()));
    }

    // Write chunk to disk
    let chunks_dir = std::path::Path::new("data").join("chunks").join(&session_id);
    fs::create_dir_all(&chunks_dir).await?;
    let chunk_path = chunks_dir.join(format!("{:08}", chunk_index));
    let mut f = fs::File::create(&chunk_path).await?;
    f.write_all(&chunk_data).await?;
    f.flush().await?;
    drop(f);

    // Count received chunks
    let mut dir = fs::read_dir(&chunks_dir).await?;
    let mut received: u32 = 0;
    while let Some(_entry) = dir.next_entry().await? {
        received += 1;
    }

    tracing::info!("Chunk {}/{} received for session {} ({})", chunk_index + 1, total_chunks, session_id, file_name);

    // Still waiting for more chunks
    if received < total_chunks {
        return Ok((StatusCode::ACCEPTED, Json(serde_json::json!({
            "session_id": session_id,
            "received": received,
            "total": total_chunks
        }))).into_response());
    }

    // All chunks received — reassemble
    tracing::info!("All {} chunks received for session {}, reassembling '{}'", total_chunks, session_id, file_name);
    let data_dir = std::path::Path::new("data");
    fs::create_dir_all(data_dir).await?;
    let output_path = data_dir.join(&file_name);

    let mut output_file = fs::File::create(&output_path).await?;
    for i in 0..total_chunks {
        let cp = chunks_dir.join(format!("{:08}", i));
        let bytes = fs::read(&cp).await.map_err(|e| {
            AppError::Internal(format!("Failed to read chunk {}: {}", i, e))
        })?;
        output_file.write_all(&bytes).await?;
    }
    output_file.flush().await?;
    drop(output_file);

    // Clean up chunk directory (best-effort)
    if let Err(e) = fs::remove_dir_all(&chunks_dir).await {
        tracing::warn!("Failed to clean chunk dir for session {}: {}", session_id, e);
    }

    // Process the reassembled file
    let dataset_name = remove_spatial_extension(&file_name);
    let output_path_str = output_path.to_string_lossy().to_string();
    let fmt = get_format_name(&file_name);

    let dataset = tokio::task::spawn_blocking(move || {
        state.db.load_spatial_file(&output_path_str, &dataset_name)
    })
    .await??;

    tracing::info!("Loaded chunked dataset: {} ({} features, {})", dataset.name, dataset.feature_count, fmt);

    Ok((StatusCode::OK, Json(dataset)).into_response())
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

/// Create a new dataset from an arbitrary DuckDB SQL query.
/// Body: { "sql": "SELECT ...", "name": "my_dataset" }
pub async fn create_from_sql(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateFromSqlRequest>,
) -> AppResult<Json<Dataset>> {
    if req.sql.trim().is_empty() {
        return Err(AppError::BadRequest("SQL query is required".to_string()));
    }
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Dataset name is required".to_string()));
    }
    let sql = req.sql.clone();
    let name = req.name.trim().to_string();
    let dataset = tokio::task::spawn_blocking(move || {
        state.db.create_dataset_from_sql(&sql, &name)
    })
    .await??;
    tracing::info!("Created SQL dataset: {} ({} rows)", dataset.name, dataset.feature_count);
    Ok(Json(dataset))
}

/// Preview the first rows of an arbitrary DuckDB SQL query (no persistence).
/// Body: { "sql": "SELECT ..." }
/// Returns column metadata and up to 200 rows as JSON.
pub async fn preview_sql(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PreviewSqlRequest>,
) -> AppResult<Json<SqlPreviewResponse>> {
    if req.sql.trim().is_empty() {
        return Err(AppError::BadRequest("SQL query is required".to_string()));
    }
    let sql = req.sql.clone();
    let result = tokio::task::spawn_blocking(move || {
        state.db.preview_sql(&sql, 200)
    })
    .await??;
    Ok(Json(result))
}

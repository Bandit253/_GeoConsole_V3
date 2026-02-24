use anyhow::Result;
use axum::{
    response::IntoResponse,
    routing::{get, post, delete},
    Json, Router,
};
use std::sync::Arc;
use std::path::PathBuf;
use axum::extract::DefaultBodyLimit;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use axum::http::header::{HeaderName, HeaderValue};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use geoconsole_v3::{api, db::DuckDbManager, AppState};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,geoconsole_v3=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting GeoConsole V3");

    // Initialize DuckDB
    let db = DuckDbManager::new()?;
    tracing::info!("DuckDB initialized with spatial extension");

    let http_client = reqwest::Client::new();
    let state = Arc::new(AppState { db, http_client });

    // Static file serving (frontend build output)
    let static_dir = std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| "frontend/dist".to_string());
    let static_path = PathBuf::from(&static_dir);
    
    if !static_path.exists() {
        tracing::warn!("Static directory '{}' not found. Frontend will not be served.", static_dir);
        tracing::warn!("Run 'npm run build' in frontend/ to generate static files.");
    } else {
        tracing::info!("Serving static files from: {}", static_path.display());
    }

    // API router
    let api_router = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Dataset API
        .route("/api/datasets", get(api::datasets::list_datasets))
        .route("/api/datasets", post(api::datasets::upload_dataset))
        .route("/api/datasets/:id", get(api::datasets::get_dataset))
        .route("/api/datasets/:id", delete(api::datasets::delete_dataset))
        // Spatial data API (Arrow IPC)
        .route("/api/datasets/:id/features", get(api::spatial::get_features_arrow))
        .route("/api/datasets/:id/geojson", get(api::spatial::get_features_geojson))
        .route("/api/datasets/:id/bounds", get(api::spatial::get_bounds))
        // Map configuration API
        .route("/api/maps", get(api::maps::list_maps))
        .route("/api/maps", post(api::maps::create_map))
        .route("/api/maps/:id", get(api::maps::get_map))
        .route("/api/maps/:id", post(api::maps::update_map))
        .route("/api/maps/:id", delete(api::maps::delete_map))
        .with_state(state);

    // Build main app with API routes + static file fallback
    let app = Router::new()
        .merge(api_router)
        // Serve static files for all other routes (SPA fallback)
        .fallback_service(ServeDir::new(static_path).append_index_html_on_directories(true))
        // Body size limit (100MB for large spatial files)
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        // COOP/COEP headers (required for DuckDB-WASM SharedArrayBuffer)
        // Cloudflare must be configured to pass these through
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("cross-origin-opener-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("cross-origin-embedder-policy"),
            HeaderValue::from_static("require-corp"),
        ))
        // CORS
        .layer(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
            .expose_headers([
                "X-Total-Count".parse().unwrap(),
                "X-Truncated".parse().unwrap(),
            ]))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    // Bind address from env or default to 127.0.0.1:3003 (localhost only)
    // Use HOST=0.0.0.0 env var if you need direct external access
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3003".to_string());
    let bind_addr = format!("{}:{}", host, port);
    
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Server listening on http://{}", bind_addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "geoconsole-v3",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

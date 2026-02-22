use anyhow::Result;
use axum::{
    response::IntoResponse,
    routing::{get, post, delete},
    Json, Router,
};
use std::sync::Arc;
use axum::extract::DefaultBodyLimit;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
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

    // Build router
    let app = Router::new()
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
        // Routing API (proxies to Valhalla)
        .route("/api/routing/route", post(api::routing::calculate_route))
        .route("/api/routing/isochrone", post(api::routing::calculate_isochrone))
        // Body size limit (100MB for large spatial files)
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
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
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003").await?;
    tracing::info!("Server listening on http://localhost:3003");
    
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

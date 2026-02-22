pub mod api;
pub mod db;
pub mod error;
pub mod models;

use db::DuckDbManager;

pub struct AppState {
    pub db: DuckDbManager,
    pub http_client: reqwest::Client,
}

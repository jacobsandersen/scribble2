use std::sync::Arc;

use moka::future::Cache;

use crate::{config::ScribbleConfig, indieauth::{IndieAuthError, TokenInfo}, micropub::storage::job::JobQueue, path_pattern::PathPattern};

pub mod config;
pub mod git;
pub mod indieauth;
pub mod microformats;
pub mod micropub;
pub mod path_pattern;
pub mod telemetry;

pub struct AppState {
    pub config: Arc<ScribbleConfig>,
    pub path_pattern: PathPattern,
    pub reqwest: reqwest::Client,
    pub job_queue: Arc<JobQueue>,
    pub auth_cache: Cache<String, Result<TokenInfo, IndieAuthError>>
}

pub trait MapToResponse {
  fn map_to_response(self) -> axum::response::Response;
}
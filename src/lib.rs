use std::sync::Arc;

use crate::{config::ScribbleConfig, micropub::storage::job::JobQueue};

pub mod indieauth;
pub mod micropub;
pub mod config;
pub mod microformats;
pub mod git;

pub struct AppState {
  pub config: ScribbleConfig,
  pub reqwest: reqwest::Client,
  pub job_queue: Arc<JobQueue>
}
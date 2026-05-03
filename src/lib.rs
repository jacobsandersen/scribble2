use tokio::sync::mpsc;

use crate::{config::ScribbleConfig, micropub::storage::WriteJob};

pub mod indieauth;
pub mod micropub;
pub mod config;
pub mod microformats;
pub mod git;
pub mod util;

pub struct AppState {
  pub config: ScribbleConfig,
  pub reqwest: reqwest::Client,
  pub writer_tx: mpsc::Sender<WriteJob>
}
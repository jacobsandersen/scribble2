use crate::config::ScribbleConfig;

pub mod indieauth;
pub mod micropub;
pub mod config;
pub mod microformats;

pub struct AppState {
  pub config: ScribbleConfig,
  pub reqwest: reqwest::Client
}
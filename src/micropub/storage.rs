pub mod job;

use std::path::{Path, PathBuf};
use async_tempfile::TempDir;
use chrono::Datelike;

use thiserror::Error;
use tokio::{fs, io};
use tracing::debug;
use uuid::Uuid;

use crate::{microformats::{Mf2Object, Mf2Value}};

#[derive(Debug, Error)]
pub enum WriteError {
  #[error("serialization error during write: {0}")]
  Serde(#[from] serde_json::Error),

  #[error("io error during write: {0}")]
  Io(#[from] io::Error)
}

fn build_content_path(payload: &Mf2Object, path_pattern: String, workdir: &TempDir) -> (String, PathBuf) {
  let slug = if let Some(Mf2Value::String(slug)) = payload.first_prop("mp-slug") {
    slug::slugify(slug)
  } else {
    uuid()
  };

  let mut path = create_path_from_pattern(&path_pattern, &slug);
  let mut abs_path = workdir.join(&path);

  while abs_path.exists() {
    path = create_path_from_pattern(&path_pattern, &format!("{slug}-{}", uuid()));
    abs_path = workdir.join(&path);
  } 

  (path, abs_path)
}

fn create_path_from_pattern(pattern: &str, slug: &str) -> String {
  let now = chrono::Utc::now();
  let year = format!("{}", now.year());
  let month = format!("{:02}", now.month());
  let day = format!("{:02}", now.day());

  pattern
    .replace("{year}", year.as_str())
    .replace("{month}", month.as_str())
    .replace("{day}", day.as_str())
    .replace("{slug}", slug)
}

async fn write_to_file(payload: &Mf2Object, path: &PathBuf) -> Result<(), WriteError> {
  debug!("serializing payload...");
  let payload_json = serde_json::to_string_pretty(&payload)
    .map_err(|e| WriteError::Serde(e))?;

  debug!("writing payload to file...");
  let parent_paths = path.parent().unwrap_or(Path::new(""));

  fs::create_dir_all(parent_paths).await
    .map_err(|e| WriteError::Io(e))?;

  fs::write(path, payload_json).await
    .map_err(|e| WriteError::Io(e))?;

  Ok(())
}

fn uuid() -> String {
  Uuid::new_v4().as_hyphenated().to_string()
}
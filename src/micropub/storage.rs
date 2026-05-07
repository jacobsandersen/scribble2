pub mod job;

use async_tempfile::TempDir;
use std::{collections::HashMap, path::{Path, PathBuf}};

use thiserror::Error;
use tokio::{fs, io};
use tracing::debug;
use uuid::Uuid;

use crate::{
    microformats::Mf2Object,
    path_pattern::PathPattern,
};

#[derive(Debug, Error)]
pub(in crate::micropub) enum StorageError {
    #[error("serialization error during storage operatoin: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("io error during storage operation: {0}")]
    Io(#[from] io::Error),
}

fn create_content_path(
    slug: Option<String>,
    path_pattern: &PathPattern,
    workdir: &TempDir,
) -> (String, String, PathBuf) {
    let slug = if let Some(slug) = slug {
      slug::slugify(slug)
    } else {
      uuid()
    };

    let mut ctx = path_pattern.new_context(&slug);

    build_content_path(slug, path_pattern, workdir, &mut ctx)
}

fn build_content_path(
  slug: String,
  path_pattern: &PathPattern,
  workdir: &TempDir,
  ctx: &mut HashMap<&str, String>
) -> (String, String, PathBuf) {
    let mut path = path_pattern.resolve(&ctx);
    let mut abs_path = workdir.join(&path);

    while abs_path.exists() {
        ctx.insert("slug", format!("{slug}-{}", uuid()));
        path = path_pattern.resolve(&ctx);
        abs_path = workdir.join(&path);
    }

    (slug, path, abs_path)
}

async fn write_to_file(payload: &Mf2Object, path: &PathBuf) -> Result<(), StorageError> {
    debug!("finalizing path...");
    let path = path.with_extension("json");

    debug!("serializing payload...");
    let payload_json = serde_json::to_string_pretty(&payload).map_err(|e| StorageError::Serde(e))?;

    debug!("writing payload to file...");
    let parent_paths = path.parent().unwrap_or(Path::new(""));

    fs::create_dir_all(parent_paths)
        .await
        .map_err(|e| StorageError::Io(e))?;

    fs::write(path, payload_json)
        .await
        .map_err(|e| StorageError::Io(e))?;

    Ok(())
}

async fn read_to_object(path: &PathBuf) -> Result<Mf2Object, StorageError> {
  debug!("finalizing path...");
  let path = path.with_extension("json");

  debug!("reading file to string...");
  let content = fs::read_to_string(path).await?;

  debug!("converting string to object...");
  Ok(serde_json::from_str::<Mf2Object>(&content)?)
}

async fn delete_file(path: &PathBuf) -> Result<(), StorageError> {
  debug!("finalizing path...");
  let path = path.with_extension("json");

  debug!("deleting file...");
  fs::remove_file(path).await?;
  
  Ok(())
}

fn uuid() -> String {
    Uuid::new_v4().as_hyphenated().to_string()
}

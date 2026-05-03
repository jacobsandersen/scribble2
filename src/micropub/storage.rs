use std::{path::{Path, PathBuf}, sync::Arc};
use async_tempfile::TempDir;
use chrono::Datelike;

use tokio::{fs, sync::oneshot};
use tracing::debug;
use uuid::Uuid;

use crate::{AppState, git, microformats::{Mf2Object, Mf2Value}};

#[derive(Debug)]
pub enum WriteError {
  Process(String),
  Git(git2::Error)
}

pub struct WriteJob {
  pub state: Arc<AppState>,
  pub payload: Mf2Object,
  pub respond_to: oneshot::Sender<Result<String, WriteError>>
}

pub async fn store_object(job: &WriteJob) -> Result<String, WriteError> {
  debug!("cloning content repository...");
  let (repo, workdir) = git::clone_repo(&job.state).await
    .map_err(|e| WriteError::Process(format!("failed to clone git repository: {e:?}")))?;

  debug!("creating content path...");
  let (path, abs_path) = build_content_path(&job, &workdir);

  debug!("writing content to file...");
  write_to_file(&job.payload, &abs_path).await?;

  debug!("committing file to git...");
  git::add_and_commit(&repo, &path).map_err(|e| WriteError::Git(e))?;

  debug!("pushing repository to remote...");
  let branch = job.state.config.micropub.storage.git.branch.as_deref().unwrap_or("main");
  git::push(&job.state, &repo, branch).map_err(|e| WriteError::Git(e))?;

  Ok(path.to_string())
}

fn build_content_path(job: &WriteJob, workdir: &TempDir) -> (String, PathBuf) {
  let slug = if let Some(Mf2Value::String(slug)) = job.payload.first_prop("mp-slug") {
    slug::slugify(slug)
  } else {
    uuid()
  };

  let path_pattern = &job.state.config.micropub.storage.path_pattern;

  let now = chrono::Utc::now();
  let year = format!("{}", now.year());
  let month = format!("{:02}", now.month());
  let day = format!("{:02}", now.day());

  let mut path = path_pattern
    .replace("{year}", year.as_str())
    .replace("{month}", month.as_str())
    .replace("{day}", day.as_str())
    .replace("{slug}", &slug);

  let mut abs_path = workdir.join(&path);

  while abs_path.exists() {
    path = format!("{}-{}", path, uuid());
    abs_path = workdir.join(&path);
  } 

  (path, abs_path)
}

async fn write_to_file(payload: &Mf2Object, path: &PathBuf) -> Result<(), WriteError> {
  debug!("serializing payload...");
  let payload_json = serde_json::to_string_pretty(&payload)
    .map_err(|e| WriteError::Process(format!("failed to serialize payload: {e:?}")))?;

  debug!("writing payload to file...");
  let parent_paths = path.parent().unwrap_or(Path::new(""));

  fs::create_dir_all(parent_paths).await
    .map_err(|e| WriteError::Process(format!("failed to create parent directories: {e:?}")))?;

  fs::write(path, payload_json).await
    .map_err(|e| WriteError::Process(format!("failed to persist payload to file: {e:?}")))?;

  Ok(())
}

fn uuid() -> String {
  Uuid::new_v4().as_hyphenated().to_string()
}
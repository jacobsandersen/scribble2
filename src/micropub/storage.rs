use std::{path::Path, sync::Arc};
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

  debug!("creating slug");
  let slug = if let Some(Mf2Value::String(slug)) = job.payload.first_prop("mp-slug") {
    slug::slugify(slug)
  } else {
    uuid()
  };

  debug!("creating content path...");
  let path = build_content_path(&job.state, &slug);
  let path = path.strip_prefix("/").unwrap_or(&path);
  let mut abs_path = workdir.join(&path);

  debug!("ensuring unique path...");
  while abs_path.exists() {
    abs_path = Path::new(&format!("{}-{}", abs_path.to_string_lossy(), uuid())).to_path_buf();
  } 

  debug!("serializing payload...");
  let payload_json = serde_json::to_string_pretty(&job.payload)
    .map_err(|e| WriteError::Process(format!("failed to serialize payload: {e:?}")))?;

  debug!("writing payload to file...");
  let parent_paths = abs_path.parent().unwrap_or(Path::new(""));

  debug!("going to create parent directories for {}: {parent_paths:?}", abs_path.to_string_lossy());

  fs::create_dir_all(parent_paths).await
    .map_err(|e| WriteError::Process(format!("failed to create parent directories: {e:?}")))?;

  fs::write(abs_path, payload_json).await
    .map_err(|e| WriteError::Process(format!("failed to persist payload to file: {e:?}")))?;

  debug!("committing file to git...");
  let mut idx = repo.index().map_err(|e| WriteError::Git(e))?;
  idx.add_path(Path::new(&path)).map_err(|e| WriteError::Git(e))?;
  idx.write().map_err(|e| WriteError::Git(e))?;
  let oid = idx.write_tree().map_err(|e| WriteError::Git(e))?;
  let tree = repo.find_tree(oid).map_err(|e| WriteError::Git(e))?;
  let parents = match repo.head() {
    Ok(head) => vec![head.peel_to_commit().map_err(|e| WriteError::Git(e))?],
    Err(_) => vec![]
  };
  let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

  let sig = git2::Signature::now("scribble", "scribble@indieweb")
    .map_err(|e| WriteError::Git(e))?;

  let _ = repo.commit(
    Some("HEAD"), 
    &sig, 
    &sig, 
    "scribble: add new file", 
    &tree,
    &parent_refs
  ).map_err(|e| WriteError::Git(e))?;

  debug!("pushing repository to remote...");
  let mut remote = repo.find_remote("origin")
    .map_err(|e| WriteError::Git(e))?;

  let branch = job.state.config.micropub.storage.git.branch.clone().unwrap_or(String::from("main"));
  let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

  let mut po = git2::PushOptions::new();
  po.remote_callbacks(git::get_remote_callbacks(&job.state));

  remote.push(&[&refspec], Some(&mut po))
    .map_err(|e| WriteError::Git(e))?;

  Ok(path.to_string())
}

pub fn build_content_path(state: &Arc<AppState>, slug: &str) -> String {
  let path_pattern = &state.config.micropub.storage.path_pattern;

  let now = chrono::Utc::now();
  let year = format!("{}", now.year());
  let month = format!("{:02}", now.month());
  let day = format!("{:02}", now.day());

  path_pattern
    .replace("{year}", year.as_str())
    .replace("{month}", month.as_str())
    .replace("{day}", day.as_str())
    .replace("{slug}", slug)
}

fn uuid() -> String {
  Uuid::new_v4().as_hyphenated().to_string()
}
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_tempfile::TempDir;
use futures::future::BoxFuture;
use thiserror::Error;
use tokio::{fs, io, sync::oneshot::{self, Receiver}};
use tower_http::BoxError;
use tracing::debug;

use crate::{
    AppState, MapToResponse, git, microformats::{Mf2Object, Mf2Value}, micropub::{
        error::{not_found, system_error}, post::update::{Deletion, UpdatePayload}, storage::{self, StorageError, job::Job}
    }, path_pattern::PathPattern
};

#[derive(Debug, Error)]
pub(in crate::micropub) enum UpdateError {
    #[error("repository clone failed: {0}")]
    Clone(#[from] git::CloneError),

    #[error("invalid url {0}: does not belong to this instance")]
    ForeignUrl(String),

    #[error("url {0}: content not found")]
    NotFound(String),

    #[error("io error: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("storage operation failed: {0}")]
    Storage(#[from] storage::StorageError),

    #[error("git operation failed: {0}")]
    Git2(#[from] git2::Error),
}

impl MapToResponse for UpdateError {
  fn map_to_response(self) -> axum::response::Response {
      match self {
        Self::NotFound(msg) => {
          not_found(&msg)
        },
        _ => {
          system_error(&format!("failed to update post: {self}"))
        }
      }
  }
}

pub(in crate::micropub) struct UpdateJob {
    pub state: Arc<AppState>,
    pub payload: UpdatePayload,
    pub respond_to: oneshot::Sender<Result<(String, bool), UpdateError>>,
}

impl UpdateJob {
  pub fn new(state: Arc<AppState>, payload: UpdatePayload) -> (UpdateJob, Receiver<Result<(String, bool), UpdateError>>) {
    let (respond_to, rx) = oneshot::channel();
    (UpdateJob { state, payload, respond_to }, rx)
  }
}

impl Job for UpdateJob {
    fn execute(self) -> BoxFuture<'static, Result<(), BoxError>> {
        Box::pin(async move {
            let run = async {
                debug!("cloning content repository...");
                let (repo, workdir) = git::clone_repo(&self.state).await?;

                debug!("checking for existing content at path...");
                let public_url = &self.state.config.micropub.content.public_url;
                let payload_url = &self.payload.url;

                let path = self
                    .payload
                    .url
                    .strip_prefix(public_url)
                    .ok_or(UpdateError::ForeignUrl(payload_url.to_string()))?
                    .to_string();

                let mut parts = self.state.path_pattern.extract(&path).ok_or_else(|| {
                    debug!("received url with invalid path pattern (unable to extract)");
                    UpdateError::NotFound(payload_url.clone())
                })?;


                debug!("patching content...");
                let repo_path = workdir.join(&path);
                let mut object = storage::read_to_object(&repo_path).await?;
                patch_object(self.payload, &mut object);

                debug!("updating slug (if needed)...");
                let (path, repo_path, changed) = update_slug_and_path(
                    &mut parts,
                    &mut object,
                    path,
                    repo_path,
                    &workdir,
                    &self.state.path_pattern,
                )
                .await?;

                debug!("saving patched content to file...");
                storage::write_to_file(&object, &repo_path).await?;

                debug!("committing file to git...");
                git::add_all_and_commit(&repo, "update post")?;

                debug!("pushing repository to remote...");
                let branch = &self
                    .state
                    .config
                    .micropub
                    .content
                    .git
                    .branch
                    .as_deref()
                    .unwrap_or("main");

                git::push(&self.state, &repo, branch)?;

                Ok((String::from(path), changed))
            };

            let _ = self.respond_to.send(run.await);
            Ok(())
        })
    }
}

fn patch_object(payload: UpdatePayload, object: &mut Mf2Object) {
    for (key, val) in payload.replace {
        object.set_props(key, val);
    }

    for (key, val) in payload.add {
        object.add_props(key, val);
    }

    match payload.delete {
        Deletion::Complete(deletions) => {
            for deletion in deletions {
                object.delete_prop(deletion);
            }
        }

        Deletion::Partial(deletions) => {
            for (key, value) in deletions {
                object.delete_prop_values(key, value);
            }
        }
    }
}

async fn update_slug_and_path(
    parts: &mut HashMap<&str, String>,
    object: &mut Mf2Object,
    path: String,
    repo_path: PathBuf,
    workdir: &TempDir,
    path_pattern: &PathPattern,
) -> Result<(String, PathBuf, bool), UpdateError> {
    let slug = parts.get("slug").unwrap().clone();

    if let Some(Mf2Value::String(new_slug)) = object.first_prop("mp-slug") {
        if new_slug != slug {
            debug!("deleting old file...");
            storage::delete_file(&repo_path).await?;

            debug!("updating path...");
            let new_slug = slug::slugify(new_slug);
            parts.insert("slug", new_slug.clone());
            let (slug, path, repo_path) =
                storage::build_content_path(new_slug, path_pattern, &workdir, parts);
            debug!("new path: {}", repo_path.to_string_lossy());

            object.set_props(
                String::from("mp-slug"),
                vec![Mf2Value::String(slug.clone())],
            );

            return Ok((path, repo_path, true));
        }
    }

    Ok((path, repo_path, false))
}

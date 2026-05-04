use std::sync::Arc;

use futures::future::BoxFuture;
use thiserror::Error;
use tokio::sync::oneshot::{self, Receiver};
use tower_http::BoxError;
use tracing::debug;

use crate::{AppState, MapToResponse, git, microformats::{Mf2Object, Mf2Value}, micropub::{error::system_error, storage::{self, job::Job}}};

#[derive(Debug, Error)]
pub(in crate::micropub) enum CreateError {
  #[error("repository clone failed: {0}")]
  Clone(#[from] git::CloneError),

  #[error("file write operation failed: {0}")]
  Write(#[from] storage::WriteError),

  #[error("git operation failed: {0}")]
  Git2(#[from] git2::Error)
}

impl MapToResponse for CreateError {
  fn map_to_response(self) -> axum::response::Response {
    system_error(&format!("failed to create post: {self}"))
  }
}

pub(in crate::micropub) struct CreateJob {
  pub state: Arc<AppState>,
  pub payload: Mf2Object,
  pub respond_to: oneshot::Sender<Result<String, CreateError>>
}

impl CreateJob {
  pub fn new(state: Arc<AppState>, payload: Mf2Object) -> (CreateJob, Receiver<Result<String, CreateError>>) {
    let (respond_to, rx) = oneshot::channel();
    (CreateJob { state, payload, respond_to }, rx)
  }
}

impl Job for CreateJob {
  fn execute(mut self) -> BoxFuture<'static, Result<(), BoxError>> {
    Box::pin(async move {
      let run = async {
        debug!("cloning content repository...");
        let (repo, workdir) = git::clone_repo(&self.state).await?;

        debug!("creating content path...");
        let slug = self.payload.first_string_prop("mp-slug");
        let (slug, path, abs_path) = storage::create_content_path(slug, &self.state.path_pattern, &workdir);

        debug!("persisting object slug...");
        self.payload.set_props(String::from("mp-slug"), vec![Mf2Value::String(slug.clone())]);

        debug!("writing content to file...");
        storage::write_to_file(&self.payload, &abs_path).await?;

        debug!("committing file to git...");
        git::add_and_commit(&repo, &path, &format!("add new post: {slug}"))?;

        debug!("pushing repository to remote...");
        let branch = &self.state.config.micropub.storage.git.branch.as_deref().unwrap_or("main");
        git::push(&self.state, &repo, branch)?;

        Ok(path)
      };

      let _ = self.respond_to.send(run.await);
      Ok(())
    })
  }
}
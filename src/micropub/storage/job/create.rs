use std::sync::Arc;

use futures::future::BoxFuture;
use thiserror::Error;
use tokio::sync::oneshot;
use tower_http::BoxError;
use tracing::debug;

use crate::{AppState, git, microformats::Mf2Object, micropub::storage::{self, job::Job}};

#[derive(Debug, Error)]
pub enum CreateError {
  #[error("repository clone failed while creating post: {0}")]
  Clone(#[from] git::CloneError),

  #[error("file write operation failed while creating post: {0}")]
  Write(#[from] storage::WriteError),

  #[error("git operation failed while creating post: {0}")]
  Git2(#[from] git2::Error)
}

pub struct CreateJob {
  pub state: Arc<AppState>,
  pub payload: Mf2Object,
  pub respond_to: oneshot::Sender<Result<String, CreateError>>
}

impl Job for CreateJob {
  fn execute(self) -> BoxFuture<'static, Result<(), BoxError>> {
    Box::pin(async move {
      let run = async {
        debug!("cloning content repository...");
        let (repo, workdir) = git::clone_repo(&self.state).await?;

        debug!("creating content path...");
        let path_pattern = &self.state.config.micropub.storage.path_pattern;
        let (path, abs_path) = storage::build_content_path(&self.payload, path_pattern.to_owned(), &workdir);

        debug!("writing content to file...");
        storage::write_to_file(&self.payload, &abs_path).await?;

        debug!("committing file to git...");
        git::add_and_commit(&repo, &path)?;

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
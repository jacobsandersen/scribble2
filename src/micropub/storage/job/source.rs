use std::sync::Arc;

use futures::future::BoxFuture;
use thiserror::Error;
use tokio::sync::oneshot::{self, Receiver};
use tower_http::BoxError;
use tracing::debug;

use crate::{
    AppState, MapToResponse,
    git::{self, CloneError},
    microformats::{Mf2Object, Mf2Value},
    micropub::{
        error::{not_found, system_error},
        storage::{self, StorageError, job::Job},
    },
};

#[derive(Debug, Error)]
pub(in crate::micropub) enum SourceError {
    #[error("invalid url '{0}': does not belong to this instance")]
    ForeignUrl(String),

    #[error("clone failed: {0}")]
    Clone(#[from] CloneError),

    #[error("storage operation failed: {0}")]
    Storage(#[from] StorageError),

    #[error("request content was not found")]
    NotFound
}

impl MapToResponse for SourceError {
    fn map_to_response(self) -> axum::response::Response {
        match self {
          Self::NotFound => not_found(&format!("{self}")),
          _ => system_error(&format!("failed to source post: {self}"))
        }
    }
}

pub(in crate::micropub) struct SourceJob {
    pub state: Arc<AppState>,
    pub url: String,
    pub respond_to: oneshot::Sender<Result<Mf2Object, SourceError>>,
}

impl SourceJob {
    pub fn new(
        state: Arc<AppState>,
        url: String,
    ) -> (SourceJob, Receiver<Result<Mf2Object, SourceError>>) {
        let (respond_to, rx) = oneshot::channel();
        (
            SourceJob {
                state,
                url,
                respond_to,
            },
            rx,
        )
    }
}

impl Job for SourceJob {
    fn execute(self) -> BoxFuture<'static, Result<(), BoxError>> {
        Box::pin(async move {
            let run = async {
                debug!("cloning content repository...");
                let (_, workdir) = git::clone_repo(&self.state).await?;

                debug!("checking for existing content at path...");
                let public_url = &self.state.config.micropub.content.public_url;
                let payload_url = &self.url;

                let path = self
                    .url
                    .strip_prefix(public_url)
                    .ok_or(SourceError::ForeignUrl(payload_url.to_string()))?
                    .to_string();

                debug!("reading content...");
                let repo_path = workdir.join(&path);
                let object = storage::read_to_object(&repo_path).await?;

                if let Some(Mf2Value::Boolean(deleted)) = object.first_prop("deleted") {
                  if deleted {
                    debug!("requested content is marked deleted, refusing to return source");
                    return Err(SourceError::NotFound)
                  }
                }

                Ok(object)
            };

            let _ = self.respond_to.send(run.await);
            Ok(())
        })
    }
}

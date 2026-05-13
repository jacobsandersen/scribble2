use std::sync::Arc;

use futures::future::LocalBoxFuture;
use thiserror::Error;
use tokio::sync::oneshot::{self, Receiver};
use tower_http::BoxError;
use tracing::{Instrument, error, info, info_span};

use crate::{
    AppState, MapToResponse, git,
    microformats::{Mf2Object, Mf2Value},
    micropub::{
        error::system_error,
        post::{UploadedFile, media},
        storage::{self, job::Job},
    },
};

#[derive(Debug, Error)]
pub(in crate::micropub) enum CreateError {
    #[error("repository clone failed: {0}")]
    Clone(#[from] git::CloneError),

    #[error("file upload failed: {0}")]
    FileUpload(#[from] media::MediaError),

    #[error("file write operation failed: {0}")]
    Write(#[from] storage::StorageError),

    #[error("git operation failed: {0}")]
    Git2(#[from] git2::Error),
}

impl MapToResponse for CreateError {
    fn map_to_response(self) -> axum::response::Response {
        system_error(&format!("failed to create post: {self}"))
    }
}

pub(in crate::micropub) struct CreateJob {
    pub state: Arc<AppState>,
    pub files: Vec<UploadedFile>,
    pub payload: Mf2Object,
    pub respond_to: oneshot::Sender<Result<String, CreateError>>,
    pub span: tracing::Span,
}

impl CreateJob {
    pub fn new(
        state: Arc<AppState>,
        files: Vec<UploadedFile>,
        payload: Mf2Object,
    ) -> (CreateJob, Receiver<Result<String, CreateError>>) {
        let (respond_to, rx) = oneshot::channel();
        (
            CreateJob {
                state,
                files,
                payload,
                respond_to,
                span: tracing::Span::current(),
            },
            rx,
        )
    }
}

impl Job for CreateJob {
    fn execute(mut self, repo: &git2::Repository) -> LocalBoxFuture<'_, Result<(), BoxError>> {
        Box::pin(async move {
            let _ = self.respond_to.send(
                async {
                    info!("cleaning repo...");
                    git::clean_repo(&repo).await?;

                    info!("getting repo workdir...");
                    let workdir = repo
                        .workdir()
                        .ok_or(git2::Error::from_str("unable to get repo workdir"))?;

                    info!("creating content path...");
                    let slug = self.payload.first_string_prop("mp-slug");
                    let (slug, path, abs_path) =
                        storage::create_content_path(slug, &self.state.path_pattern, &workdir);

                    info!("persisting object slug...");
                    self.payload
                        .set_props("mp-slug", vec![Mf2Value::String(slug.clone())]);

                    if !self.files.is_empty() {
                        info!("uploading files...");
                        let s3 = media::get_s3(&self.state)?;

                        for file in self.files {
                            info!(
                                "...uploading {} to field {}",
                                &file.filename, &file.field_name
                            );
                            let url = media::persist_file(&self.state, &s3, &file).await?;
                            self.payload
                                .add_props(&file.field_name, vec![Mf2Value::String(url)]);
                            info!("...ok");
                        }
                    }

                    info!("ensuring datetime fields...");
                    let now = Mf2Value::String(chrono::Local::now().to_rfc3339());
                    self.payload
                        .set_prop_if_not_exists("published", now.clone());
                    self.payload.set_prop_if_not_exists("updated", now);

                    info!("writing content to file...");
                    storage::write_to_file(&self.payload, &abs_path).await?;

                    info!("committing file to git...");
                    git::add_all_and_commit(&repo, &format!("add new post: {slug}"))?;

                    Ok(path)
                }
                .instrument(info_span!(parent: self.span.clone(), "create_job"))
                .await,
            );

            info_span!(parent: self.span.clone(), "create_job_tail").in_scope(|| {
                info!("pushing repository to remote...");
                let branch = &self
                    .state
                    .config
                    .micropub
                    .content
                    .git
                    .branch
                    .as_deref()
                    .unwrap_or("main");
                let _ = git::push(&self.state, &repo, branch).inspect_err(|e| {
                    error!("failed to push git repository: {e}");
                });
            });

            Ok(())
        })
    }
}

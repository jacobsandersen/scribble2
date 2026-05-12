pub(in crate::micropub) mod create;
pub(in crate::micropub) mod update;
pub(in crate::micropub) mod source;

use std::pin::Pin;

use axum::{response::Response};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tower_http::BoxError;

use crate::{MapToResponse, micropub::error::system_error};

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("job queue is closed")]
    Closed,
}

pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

pub trait Job: Send + 'static {
    fn execute(self, repo: &git2::Repository) -> LocalBoxFuture<'_, Result<(), BoxError>>;
}

pub type JobFn = Box<dyn FnOnce(&git2::Repository) -> LocalBoxFuture<Result<(), BoxError>> + Send>;

pub struct JobQueue {
    tx: mpsc::Sender<JobFn>,
}

impl JobQueue {
    pub fn new(tx: mpsc::Sender<JobFn>) -> JobQueue {
        JobQueue { tx }
    }

    pub async fn enqueue<J: Job>(&self, job: J) -> Result<(), QueueError> {
        let job_fn: JobFn = Box::new(move |repo| job.execute(repo));
        self.tx.send(job_fn).await.map_err(|_| QueueError::Closed)
    }

    pub async fn enqueue_and_wait<J, T, E>(
        &self,
        job: J,
        rx: oneshot::Receiver<Result<T, E>>,
    ) -> Result<T, Response>
    where
        J: Job,
        E: std::error::Error + MapToResponse,
    {
        self.enqueue(job)
            .await
            .map_err(|_| system_error("job submission failed"))?;

        rx.await
            .map_err(|_| system_error("unknown error awaiting job completion"))?
            .map_err(|e| e.map_to_response())
    }
}

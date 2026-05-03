pub mod create;

use futures::future::BoxFuture;
use tokio::sync::mpsc;
use tower_http::BoxError;

pub enum QueueError {
  Closed
}

pub trait Job: Send + 'static {
  fn execute(self) -> BoxFuture<'static, Result<(), BoxError>>;
}

pub type JobFn = Box<dyn FnOnce() -> BoxFuture<'static, Result<(), BoxError>> + Send>;

pub struct JobQueue {
  tx: mpsc::Sender<JobFn>
}

impl JobQueue {
  pub fn new(tx: mpsc::Sender<JobFn>) -> JobQueue {
    JobQueue { tx }
  }

  pub async fn enqueue<J: Job>(&self, job: J) -> Result<(), QueueError> {
    let job_fn: JobFn = Box::new(move || job.execute());
    self.tx.send(job_fn).await.map_err(|_| QueueError::Closed)
  }
}
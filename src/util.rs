use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::debug;

use crate::{AppState, microformats::Mf2Object, micropub::storage::WriteJob};

pub async fn submit_write_job(state: &Arc<AppState>, payload: Mf2Object) -> Result<String, String> {
  let (respond_to, rx) = oneshot::channel(); 

  debug!("sending write job...");
  state.writer_tx.send(WriteJob {
    state: state.clone(),
    payload,
    respond_to
  }).await.map_err(|e| format!("failed to send write job: {e:?}"))?;

  debug!("awaiting write job completion...");
  let path = rx.await
    .map_err(|e| format!("unknown error while awaiting write job completion: {e:?}"))?
    .map_err(|e| format!("failed to write post: {e:?}"))?;

  debug!("write job complete");
  Ok(path)
}
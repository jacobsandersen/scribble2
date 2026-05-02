use std::sync::Arc;

use axum::{body::Body, response::Response};
use reqwest::{StatusCode, header};
use tokio::sync::oneshot;
use tracing::debug;

use crate::{AppState, indieauth::TokenScope, microformats::Mf2Object, micropub::{error::{insufficient_scope, invalid_request, system_error}, post::MicropubBody, storage::WriteJob}};

pub async fn handle(state: Arc<AppState>, body: MicropubBody) -> Result<Response, Response> {
  debug!("checking scope");
  if !body.token.scope().contains(&TokenScope::Create) {
    return Err(insufficient_scope("missing create scope"));
  }

  debug!("converting payload to mf2");
  let obj = Mf2Object::try_from(body.payload).map_err(|e| {
    invalid_request(&format!("failed to read mf2 object for creation: {e:?}"))
  })?;

  let (respond_to, rx) = oneshot::channel(); 

  debug!("sending write job...");
  state.writer_tx.send(WriteJob {
    state: state.clone(),
    payload: obj,
    respond_to
  }).await.map_err(|e| {
    debug!("failed to send write job: {e:?}");
    system_error("failed to schedule write job for post")
  })?;

  debug!("awaiting write job completion...");
  let path = rx.await
    .map_err(|e| {
      debug!("failed to receive write job response: {e:?}");
      system_error("unknown error while awaiting write job completion")
    })?
    .map_err(|e| {
      debug!("failed to write post: {e:?}");
      system_error("failed to write post to backing storage")
    })?;
  
  debug!("returning accepted response");
  Ok(Response::builder()
    .status(StatusCode::ACCEPTED)
    .header(header::LOCATION, state.config.micropub.get_content_url(&path))
    .body(Body::empty())
    .unwrap())
}

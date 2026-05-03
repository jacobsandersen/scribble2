use std::sync::Arc;

use axum::{body::Body, response::Response};
use reqwest::{StatusCode, header};
use tokio::sync::oneshot;
use tracing::debug;

use crate::{
    AppState,
    indieauth::TokenScope,
    microformats::Mf2Object,
    micropub::{
        error::{insufficient_scope, invalid_request, system_error},
        post::MicropubBody, storage::job::create::CreateJob
    }
};

pub async fn handle(state: Arc<AppState>, body: MicropubBody) -> Result<Response, Response> {
    debug!("checking scope");
    if !body.token.scope().contains(&TokenScope::Create) {
        return Err(insufficient_scope("missing create scope"));
    }

    debug!("converting payload to mf2...");
    let obj = Mf2Object::try_from(body.payload)
        .map_err(|e| invalid_request(&format!("failed to read mf2 object for creation: {e:?}")))?;

    debug!("submitting write job...");
    let (tx, rx) = oneshot::channel();
    state.job_queue.enqueue(CreateJob {
      state: state.clone(),
      payload: obj,
      respond_to: tx
    }).await.map_err(|_e| {
      debug!("failed to enqueue write job");
      system_error("write job submission failed")
    })?;

    debug!("awaiting write job completion...");
    let path = rx.await
      .map_err(|e| {
        debug!("failed to receive write job result: {e}");
        system_error("unknown error awaiting write job completion")
      })?
      .map_err(|e| {
        debug!("failed to create post: {e}");
        system_error("failed to create post in backing storage")
      })?;

    debug!("returning accepted response...");
    Ok(Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(
            header::LOCATION,
            state.config.micropub.get_content_url(&path),
        )
        .body(Body::empty())
        .unwrap())
}

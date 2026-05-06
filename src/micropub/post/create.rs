use std::sync::Arc;

use axum::{body::Body, response::Response};
use reqwest::{StatusCode, header};
use tracing::debug;

use crate::{
    AppState,
    indieauth::TokenScope,
    microformats::Mf2Object,
    micropub::{
        error::{insufficient_scope, invalid_request},
        post::MicropubBody,
        storage::job::create::CreateJob,
    },
};

pub async fn handle(state: Arc<AppState>, body: MicropubBody) -> Result<Response, Response> {
    debug!("checking scope");
    if !body.token.scope().contains(&TokenScope::Create) {
        return Err(insufficient_scope("missing create scope"));
    }

    debug!("converting payload to mf2...");
    let obj = Mf2Object::try_from(body.payload)
        .map_err(|e| invalid_request(&format!("failed to read mf2 object for creation: {e:?}")))?;

    debug!("waiting for creation to complete...");
    let (job, rx) = CreateJob::new(state.clone(), body.files, obj);
    let path = state.job_queue.enqueue_and_wait(job, rx).await?;

    debug!("returning accepted response...");
    Ok(Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(
            header::LOCATION,
            state.config.micropub.content.get_content_url(&path),
        )
        .body(Body::empty())
        .unwrap())
}

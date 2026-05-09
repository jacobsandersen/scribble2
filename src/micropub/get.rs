use std::sync::Arc;

use axum::{Json, extract::{FromRequest, Request, State}, response::{IntoResponse, Response}};
use axum_extra::extract::Query;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::{AppState, micropub::{error::invalid_request, storage::job::source::SourceJob}};

#[derive(Debug, Deserialize)]
pub struct QueryBaseRequest {
  q: String
}

#[derive(Debug, Deserialize)]
struct SourceRequest {
  url: String,

  #[serde(default)]
  properties: Vec<String>
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct MicropubConfigPayload {
  media_endpoint: String,
  syndicate_to: Vec<SyndicationTarget>
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct SyndicateToPayload {
  syndicate_to: Vec<SyndicationTarget>
}

#[derive(Serialize)]
struct SyndicationTarget {
  uid: String,
  name: String,
  service: Option<SyndicationTargetContext>,
  user: Option<SyndicationTargetContext>
}

#[derive(Serialize)]
struct SyndicationTargetContext {
  name: String,
  url: Option<String>,
  photo: Option<String>
}

#[instrument(skip(state))]
pub async fn handle(State(state): State<Arc<AppState>>, params: Query<QueryBaseRequest>, req: Request) -> Result<Response, Response> {
  match params.q.as_str() {
    "config" => handle_config(&state),
    "source" => handle_source(&state, req).await,
    "syndicate-to" => handle_syndicate_to(&state),
    _ => Err(invalid_request("Unknown query parameter"))
  }
}

#[instrument(skip(state))]
fn handle_config(state: &Arc<AppState>) -> Result<Response, Response> {
  let payload = MicropubConfigPayload {
    media_endpoint: format!("{}micropub/media", state.config.server.public_url),
    syndicate_to: Vec::new()
  };

  Ok(Json(payload).into_response())
}

#[instrument(skip(state))]
async fn handle_source(state: &Arc<AppState>, req: Request) -> Result<Response, Response> {
  let Query(params) = Query::<SourceRequest>::from_request(req, state)
    .await
    .map_err(|e| {
      invalid_request(&format!("invalid source request: {e}"))
    })?;

  let url = params.url;
  if url.is_empty() {
    return Err(invalid_request("`url` parameter must contain a value"));
  }

  info!("waiting for source lookup to complete...");
  let (job, rx) = SourceJob::new(state.clone(), url.to_string());
  let mut obj = state.job_queue.enqueue_and_wait(job, rx).await?;

  if !params.properties.is_empty() {
    info!("applying filters...");
    for (key, _) in obj.clone().properties {
      if !params.properties.contains(&key) {
        obj.delete_prop(key);
      }
    }
  }

  info!("returning source object...");
  Ok(Json(obj).into_response())
}

#[instrument(skip(_state))]
fn handle_syndicate_to(_state: &Arc<AppState>) -> Result<Response, Response> {
  let payload = SyndicateToPayload { syndicate_to: Vec::new() };
  Ok(Json(payload).into_response())
}

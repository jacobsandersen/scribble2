use std::{collections::HashMap, sync::Arc};

use axum::response::Response;
use serde::Deserialize;
use tracing::debug;

use crate::{AppState, indieauth::TokenScope, micropub::{error::{insufficient_scope, invalid_request}, post::{MicropubBody, MicropubPayload}}};

#[derive(Debug, Deserialize)]
pub(in crate::micropub) enum Deletion {
  Complete(Vec<String>),
  Partial(HashMap<String, Vec<String>>)
}

#[derive(Debug, Deserialize)]
pub(in crate::micropub) struct UpdatePayload {
  pub url: String,
  pub replace: HashMap<String, Vec<String>>,
  pub add: HashMap<String, Vec<String>>,
  pub delete: Deletion
}

pub async fn handle(state: Arc<AppState>, body: MicropubBody) -> Result<Response, Response> {
  debug!("converting raw payload to micropub update schema...");
  let json = match body.payload {
    MicropubPayload::Json(json) => json,
    MicropubPayload::Form(_) => return Err(invalid_request("updates must use JSON"))
  };

  let payload = serde_json::from_value::<UpdatePayload>(json)
    .map_err(|e| {
      debug!("failed to deserialize JSON to micropub update payload: {e:?}");
      invalid_request("invalid micropub update payload")
    })?;

  debug!("loading existing post...");
  
  debug!("checking scope");
  if !body.token.scope().contains(&TokenScope::Update) {
      return Err(insufficient_scope("missing update scope"));
  }

  Ok(invalid_request("hi"))
}
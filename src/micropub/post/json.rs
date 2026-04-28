use axum::{body::Bytes, response::Response};
use tracing::debug;

use crate::{microformats::Mf2Object, micropub::{error, post::common}};

pub async fn handle(body: Bytes) -> Result<Response, Response> {
  let value: Mf2Object = serde_json::from_slice(&body).map_err(|e| {
    debug!("failed to parse json body: {e:?}");
    error::invalid_request("failed to parse json body")
  })?;

  common::handle(value).await
}
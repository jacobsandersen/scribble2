use axum::{body::Bytes, response::Response};
use tracing::debug;

use crate::{microformats::Mf2Object, micropub::{error, post::common}};

pub async fn handle(body: Bytes) -> Result<Response, Response> {
  let value = Mf2Object::from_form(body).map_err(|e| {
    debug!("failed to parse urlencoded form body: {e:?}");
    error::invalid_request("failed to parse urlencoded form body")
  })?;

  common::handle(value).await
}
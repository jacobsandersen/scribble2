use axum::response::Response;
use tracing::debug;

use crate::{microformats::Mf2Object, micropub::error};

pub async fn handle(payload: Mf2Object) -> Result<Response, Response> {
  debug!("{payload:#?}");
  Ok(error::forbidden("hi"))
}
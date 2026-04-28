mod json;
mod form_data;
mod multipart;
mod common;

use axum::{body::Bytes, http::{HeaderMap, header}, response::Response};
use tracing::debug;

use crate::micropub::error;

pub async fn handle(headers: HeaderMap, body: Bytes) -> Result<Response, Response> {
  let content_type_mime = headers
    .get(header::CONTENT_TYPE.as_str())
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
    .parse::<mime::Mime>()
    .map_err(|e| {
      debug!("invalid content type \"{e:?}\"");
      error::invalid_request("invalid content type")
    })?;

  let content_type = content_type_mime.essence_str();

  match content_type {
    "application/json" => json::handle(body).await,
    "application/x-www-form-urlencoded" => form_data::handle(body).await,
    "multipart/form-data" => multipart::handle(body).await,
    _ => {
      debug!("illegal content type \"{}\" for micropub POST", content_type);
      return Err(error::invalid_request("illegal content type for micropub POST"));
    }
  }
}

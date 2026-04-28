use axum::{body::Bytes, response::Response};

use crate::micropub::error::forbidden;

pub async fn handle(_body: Bytes) -> Result<Response, Response> {
  Ok(forbidden("hi"))
}
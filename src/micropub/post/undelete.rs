use std::sync::Arc;

use axum::response::Response;

use crate::{AppState, micropub::{error::invalid_request, post::MicropubBody}};

pub async fn handle(_state: Arc<AppState>, _body: MicropubBody) -> Result<Response, Response> {
  Ok(invalid_request("hi"))
}
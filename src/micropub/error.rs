use axum::{Json, response::{IntoResponse, Response}};
use reqwest::StatusCode;
use serde_json::json;

pub fn unauthorized(message: &str) -> Response {
  error(StatusCode::UNAUTHORIZED, "unauthorized", message)
}

pub fn forbidden(message: &str) -> Response {
  error(StatusCode::FORBIDDEN, "forbidden", message)
}

pub fn invalid_request(message: &str) -> Response {
  error(StatusCode::BAD_REQUEST, "invalid_request", message)
}

// pub fn insufficient_scope(message: &str) -> Response {
//   error(StatusCode::FORBIDDEN, "insufficient_scope", message)
// }

fn error(status: StatusCode, error: &str, message: &str) -> Response {
  (status, Json(json!({
    "error": error,
    "error_description": message
  }))).into_response()
}
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

pub fn bad_request(message: &str) -> Response {
  error(StatusCode::BAD_REQUEST, "bad_request", message)
}

pub fn insufficient_scope(message: &str) -> Response {
  error(StatusCode::FORBIDDEN, "insufficient_scope", message)
}

pub fn system_error(message: &str) -> Response {
  error(StatusCode::INTERNAL_SERVER_ERROR, "system_error", message)
}

pub fn not_found(message: &str) -> Response {
  error(StatusCode::NOT_FOUND, "not_found", message)
}

fn error(status: StatusCode, error: &str, message: &str) -> Response {
  (status, Json(json!({
    "error": error,
    "error_description": message
  }))).into_response()
}
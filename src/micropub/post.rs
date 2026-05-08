pub(in crate::micropub) mod create;
pub(in crate::micropub) mod update;
pub(in crate::micropub) mod delete;
pub(in crate::micropub) mod media;

use std::{collections::HashMap, sync::Arc};

use async_tempfile::TempFile;
use axum::{Extension, body::{Body, Bytes}, extract::State, http::{HeaderMap, header}, response::Response};
use http_body_util::BodyExt;
use mime::Mime;
use multer::Multipart;
use reqwest::StatusCode;
use serde_json::Value;
use tracing::debug;
use tokio::io::AsyncWriteExt;

use crate::{AppState, indieauth::{self, TokenInfo}, micropub::{error::{self, forbidden, invalid_request, system_error, unauthorized}, post::delete::DeletionMode}};

/// MicropubBody represents an intermediate Micropub payload before any
/// particular request intent has been established. It contains the content-type
/// agnostic payload (JSON/Form data), uploaded files (if any) that have already
/// been streamed to disk for later use, and the token which has been extracted
/// from the headers (in middleware) or from the body (if form data), with
/// header preference.
pub struct MicropubBody {
  pub payload: MicropubPayload,
  pub files: Vec<UploadedFile>,
  pub token: TokenInfo
}

/// MicropubPayload contains either the raw Json or Form data body of the Micropub
/// request, which has not yet been converted to a stricter type (i.e. for creation
/// or updating, etc).
pub enum MicropubPayload {
  Json(serde_json::Value),
  Form(HashMap<String, Vec<String>>)
}

impl MicropubPayload {
  /// Helper method to extract a single value from a MicropubPayload
  /// If there are multiple data items to extract, it is better to define
  /// a struct and use try_from.
  pub fn get_string(&self, key: &str) -> Option<String> {
    match self {
      MicropubPayload::Json(json) => {
        if !json.is_object() {
          return None;
        }

        json[key].as_str().map(|s| s.to_string())
      },

      MicropubPayload::Form(form) => {
        form
          .get(key)
          .and_then(|v| v.first()
          .map(|s| s.to_string()))
      }
    }
  }
}

/// Uploaded file contains the (declared) name of an uploaded file and its temporary
/// location on disk while waiting to be saved elsewhere.
/// 
/// Note: TempFile cleans up any data on disk when it drops.
#[derive(Debug)]
pub struct UploadedFile {
  pub field_name: String,
  pub filename: String,
  pub file: TempFile
}

/// Action declares the types of (standard) actions a Micropub POST request can take,
/// including Create, Update, Delete, and Undelete.
pub enum Action {
  Create,
  Update,
  Delete,
  Undelete
}

impl From<&str> for Action {
  fn from(value: &str) -> Self {
      match value {
        "create" => Self::Create,
        "update" => Self::Update,
        "delete" => Self::Delete,
        "undelete" => Self::Undelete,
        _ => Self::Create
      }
  }
}

pub async fn handle(State(state): State<Arc<AppState>>, token: Option<Extension<TokenInfo>>, headers: HeaderMap, body: Body) -> Result<Response, Response> {
  let content_type = get_content_type(headers)?;
  let body = decode_body(&state, token, content_type, body).await?;

  let action = match &body.payload {
    MicropubPayload::Json(json) => {
      if let Some(action) = json["action"].as_str() {
        Action::from(action)
      } else {
        Action::Create
      }
    },

    MicropubPayload::Form(form) => {
      form.get("action")
        .and_then(|v| v.first())
        .map(|s| s.as_str())
        .unwrap_or("create")
        .into()
    }
  };

  match action {
    Action::Create => create::handle(state, body).await,
    Action::Update => update::handle(state, body).await,
    Action::Delete => delete::handle(state, body, DeletionMode::Delete).await,
    Action::Undelete => delete::handle(state, body, DeletionMode::Undelete).await
  }
}

pub async fn handle_media(State(state): State<Arc<AppState>>, token: Option<Extension<TokenInfo>>, headers: HeaderMap, body: Body) -> Result<Response, Response> {
  let content_type = get_content_type(headers)?;
  if content_type.essence_str() != "multipart/form-data" {
    return Err(invalid_request("media endpoint only supports multipart/form-data content type"));
  }

  let body = decode_multipart(&state, token, content_type, body, Some(String::from("file"))).await?;
  if body.files.is_empty() {
    return Err(invalid_request("received media upload request with zero files (media endpoint accepts one file, in field named `file`)"));
  }

  let s3 = media::get_s3(&state).map_err(|e| {
    debug!("failed to get s3: {e:?}");
    system_error("failed to get s3 connection (bad credentials?)")
  })?;

  let url = media::persist_file(&state, &s3, &body.files[0]).await
    .map_err(|e| {
      debug!("file upload failed {e:?}");
      system_error("failed to upload file")
    })?;

  Ok(Response::builder()
      .status(StatusCode::CREATED)
      .header(header::LOCATION, url)
      .body(Body::empty())
      .unwrap())
}

fn get_content_type(headers: HeaderMap) -> Result<Mime, Response> {
  headers
    .get(header::CONTENT_TYPE.as_str())
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
    .parse::<mime::Mime>()
    .map_err(|e| {
      debug!("invalid content type \"{e:?}\"");
      error::invalid_request("invalid content type")
    })
}

async fn decode_body(state: &Arc<AppState>, token: Option<Extension<TokenInfo>>, content_type: Mime, body: Body) -> Result<MicropubBody, Response> {
  match content_type.essence_str() {
    "application/json" => Ok(decode_json(token, body).await?),
    "application/x-www-form-urlencoded" => Ok(decode_form(state, token, body).await?),
    "multipart/form-data" => Ok(decode_multipart(state, token, content_type, body, None).await?), 
    _ => {
      debug!("illegal content type \"{}\" for micropub POST", content_type);
      return Err(error::invalid_request("illegal content type for micropub POST"));
    }
  }
}

async fn decode_json(token: Option<Extension<TokenInfo>>, body: Body) -> Result<MicropubBody, Response> {
  let token = token
    .ok_or_else(|| unauthorized("Bearer token is required when sending JSON requests"))?.0;

  let body = collect_body(body).await?;

  let json = serde_json::from_slice::<Value>(&body).map_err(|e| {
    debug!("failed to read JSON body: {e:?}");
    invalid_request("invalid JSON body")
  })?;

  Ok(MicropubBody { 
    payload: MicropubPayload::Json(json), 
    files: Vec::new(),
    token
  })
}

async fn decode_form(state: &Arc<AppState>, token: Option<Extension<TokenInfo>>, body: Body) -> Result<MicropubBody, Response> {
  let body  = collect_body(body).await?;
  let data: Vec<(String, String)> = serde_urlencoded::from_bytes(&body).map_err(|e| {
    debug!("failed to read form encoded body: {e:?}");
    invalid_request("invalid form encoded body")
  })?;

  let mut map = HashMap::new();
  for (key, value) in data {
    let key = key.strip_suffix("[]").unwrap_or(&key).to_string();
    map.entry(key).or_insert_with(Vec::new).push(value);
  }

  let token = get_token_header_first(state, token, &mut map).await?;
  Ok(MicropubBody { payload: MicropubPayload::Form(map), files: Vec::new(), token })
}

async fn decode_multipart(state: &Arc<AppState>, token: Option<Extension<TokenInfo>>, content_type: Mime, body: Body, field_name_filter: Option<String>) -> Result<MicropubBody, Response> {
  let body = body.into_data_stream();
  let boundary = multer::parse_boundary(content_type).map_err(|e| {
    debug!("failed to parse multipart boundary: {e:?}");
    invalid_request("invalid multipart body; failed to parse multipart boundary")
  })?;

  let mut map: HashMap<String, Vec<String>> = HashMap::new();
  let mut files: Vec<UploadedFile> = Vec::new();

  let mut multipart = Multipart::new(body, boundary);
  while let Some(mut field) = multipart.next_field().await.map_err(|e| {
    debug!("failed to read multipart field: {e:?}");
    system_error("error while reading multipart fields")
  })? {
    let Some(field_name) = field.name().map(|s| s.to_string()) else { 
      debug!("skipping nameless field in multipart body");
      continue 
    };

    if let Some(ref field_name_filter) = field_name_filter {
      if field_name_filter != &field_name {
        debug!("skipping field: field name {field_name} did not match filter {field_name_filter}");
        continue
      }
    }

    match field.file_name().map(|s| s.to_string()) {
      None => {
        let Some(value) = field.text().await.ok() else { 
          debug!("skipping value-less field in multipart body");
          continue 
        };

        map.entry(field_name).or_default().push(value);
      },

      Some(filename) => {
        let mut file = TempFile::new()
          .await
          .map_err(|e| {
            debug!("failed to create tempfile to store streamed file: {e:?}");
            system_error("failed to read multipart request; unable to create file on disk")
          })?
          .open_rw()
          .await
          .map_err(|e| {
            debug!("failed to open tempfile for writing: {e:?}");
            system_error("failed to read multipart request; unable to open file for writing")
          })?;

        while let Some(chunk) = field.chunk().await.map_err(|e| {
          debug!("failed to read multipart file chunk: {e:?}");
          system_error("error while reading multipart file chunk")
        })? {
          file.write_all(&chunk).await.map_err(|e| {
            debug!("failed to write multipart file chunk to disk: {e:?}");
            system_error("failed to write multipart file chunk to disk")
          })?;
        }

        files.push(UploadedFile { field_name, filename, file });
      }
    }
  }

  let token = get_token_header_first(state, token, &mut map).await?;
  Ok(MicropubBody { payload: MicropubPayload::Form(map), files, token })
}

async fn collect_body(body: Body) -> Result<Bytes, Response> {
  Ok(body
    .collect()
    .await
    .map_err(|e| {
      debug!("failed to collect Body to Bytes: {e:?}");
      system_error("failed to collect request body")
    })?
    .to_bytes())
}

async fn get_token_header_first(state: &Arc<AppState>, maybe_token: Option<Extension<TokenInfo>>, form: &mut HashMap<String, Vec<String>>) -> Result<TokenInfo, Response> {
  let token_result = match maybe_token {
    Some(token) => Ok(token.0),
    None => extract_and_validate_token_from_form(state, form).await
  };

  form.remove("access_token");

  token_result
}

async fn extract_and_validate_token_from_form(state: &Arc<AppState>, form: &HashMap<String, Vec<String>>) -> Result<TokenInfo, Response> {
  let access_token = form.get("access_token")
    .and_then(|v| v.first())
    .map(|s| s.as_str())
    .ok_or_else(|| unauthorized("access_token parameter is required if not provided in header"))?;

  indieauth::validate_token(&state, access_token)
    .await
    .map_err(|e| unauthorized(&e))
}
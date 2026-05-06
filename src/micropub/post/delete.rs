use std::{collections::HashMap, fmt::Display, sync::Arc};

use axum::{body::Body, response::Response};
use reqwest::{StatusCode, header};
use tracing::debug;

use crate::{AppState, indieauth::TokenScope, microformats::Mf2Value, micropub::{error::{insufficient_scope, invalid_request}, post::{MicropubBody, update::{Deletion, UpdatePayload}}, storage::job::update::UpdateJob}};

#[derive(Debug, PartialEq)]
pub enum DeletionMode {
  Delete,
  Undelete
}

impl Display for DeletionMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DeletionMode::Delete => write!(f, "delete"),
      DeletionMode::Undelete => write!(f, "undelete")
    }
  }
}

impl From<&DeletionMode> for TokenScope {
  fn from(value: &DeletionMode) -> Self {
      match value {
        DeletionMode::Delete => TokenScope::Delete,
        DeletionMode::Undelete => TokenScope::Undelete
      }
  }
}

pub async fn handle(state: Arc<AppState>, body: MicropubBody, mode: DeletionMode) -> Result<Response, Response> {
    debug!("checking scope...");
    if !body.token.scope().contains(&TokenScope::from(&mode)) {
        return Err(insufficient_scope(&format!("missing {mode} scope")));
    }

    debug!("getting url for delete/undelete operation...");
    let url = body.payload.get_string("url");
    if url.is_none() {
      return Err(invalid_request("missing required parameter `url`"));
    }

    debug!("building delegated update payload for delete/undelete...");
    let mut replace = HashMap::new();
    replace.insert("deleted".to_string(), vec![Mf2Value::String(format!("{}", mode == DeletionMode::Delete))]);

    let payload = UpdatePayload {
      add: HashMap::new(),
      replace,
      delete: Deletion::Complete(Vec::new()),
      url: url.unwrap()
    };

    debug!("waiting for delegated update to complete...");
    let (job, rx) = UpdateJob::new(state.clone(), payload);
    let (path, changed) = state.job_queue.enqueue_and_wait(job, rx).await?;

    debug!("returning response...");
    let builder = match mode {
      DeletionMode::Delete => {
        Response::builder().status(StatusCode::NO_CONTENT)
      },

      DeletionMode::Undelete => {
        if changed {
          Response::builder().status(StatusCode::CREATED).header(
              header::LOCATION,
              state.config.micropub.content.get_content_url(&path),
          )
        } else {
            Response::builder().status(StatusCode::OK)
        }
      }
    };

    Ok(builder.body(Body::empty()).unwrap())
}
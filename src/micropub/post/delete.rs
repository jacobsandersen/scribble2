use std::{collections::HashMap, fmt::Display, sync::Arc};

use axum::{body::Body, response::Response};
use reqwest::{StatusCode, header};
use tracing::{info, instrument};

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

#[instrument(skip(state))]
pub async fn handle(state: Arc<AppState>, body: MicropubBody, mode: DeletionMode) -> Result<Response, Response> {
    info!("checking scope...");
    if !body.token.scope().contains(&TokenScope::from(&mode)) {
        return Err(insufficient_scope(&format!("missing {mode} scope")));
    }

    info!("getting url for delete/undelete operation...");
    let url = body.payload.get_string("url");
    if url.is_none() {
      return Err(invalid_request("missing required parameter `url`"));
    }

    info!("building delegated update payload for delete/undelete...");
    let payload = match mode {
      DeletionMode::Delete => {
        let mut replace = HashMap::new();
        replace.insert("deleted".to_string(), vec![Mf2Value::Boolean(mode == DeletionMode::Delete)]);

        UpdatePayload {
          add: HashMap::new(),
          replace,
          delete: Deletion::Complete(Vec::new()),
          url: url.unwrap()
        }
      },

      DeletionMode::Undelete => {
        let delete = Deletion::Complete(vec!["deleted".to_string()]);
        
        UpdatePayload {
          add: HashMap::new(),
          replace: HashMap::new(),
          delete,
          url: url.unwrap()
        }
      }
    };

    info!("waiting for delegated update to complete...");
    let (job, rx) = UpdateJob::new(state.clone(), payload);
    let (path, changed) = state.job_queue.enqueue_and_wait(job, rx).await?;

    info!("returning response...");
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
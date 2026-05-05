use std::{collections::HashMap, sync::Arc};

use axum::{body::Body, response::Response};
use reqwest::{StatusCode, header};
use serde::Deserialize;
use tracing::debug;

use crate::{
    AppState,
    indieauth::TokenScope,
    microformats::Mf2Value,
    micropub::{
        error::{insufficient_scope, invalid_request},
        post::{MicropubBody, MicropubPayload},
        storage::job::update::UpdateJob,
    },
};

enum PropertyPolicy {
  ReplaceOnly { max_values: usize },
}

fn get_property_policy(prop: &str) -> Option<PropertyPolicy> {
  match prop {
    "mp-slug" => Some(PropertyPolicy::ReplaceOnly { max_values: 1 }),
    _ => None
  }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::micropub) enum Deletion {
    Complete(Vec<String>),
    Partial(HashMap<String, Vec<Mf2Value>>),
}

impl Default for Deletion {
    fn default() -> Self {
        Deletion::Complete(Vec::new())
    }
}

#[derive(Debug, Deserialize)]
pub(in crate::micropub) struct UpdatePayload {
    pub url: String,
    #[serde(default)]
    pub replace: HashMap<String, Vec<Mf2Value>>,
    #[serde(default)]
    pub add: HashMap<String, Vec<Mf2Value>>,
    #[serde(default)]
    pub delete: Deletion,
}

impl UpdatePayload {
    fn validate_payload(self) -> Result<Self, String> {
      for (key, values) in &self.replace {
        if let Some(policy) = get_property_policy(key) {
          match policy {
            PropertyPolicy::ReplaceOnly { max_values } if values.len() > max_values => {
              let values = values.len();
              return Err(format!("{key}: too many values ({values} > max {max_values})"))
            },

            _ => {}
          }
        }
      }

      for key in self.add.keys() {
        if let Some(policy) = get_property_policy(key) {
          match policy {
            PropertyPolicy::ReplaceOnly { .. } => {
              return Err(format!("{key}: this property may only be set with replace"))
            }
          }
        }
      }

      match &self.delete {
        Deletion::Complete(deletion) => {
          for key in deletion {
            if let Some(_) = get_property_policy(&key) {
              return Err(format!("{key}: this property cannot be deleted"));
            }
          }
        },

        Deletion::Partial(deletion) => {
          for (key, _) in deletion {
            if let Some(_) = get_property_policy(&key) {
              return Err(format!("{key}: this property cannot be deleted"));
            }
          }
        }
      }

      Ok(self)
    }
}

impl TryFrom<MicropubPayload> for UpdatePayload {
  type Error = Response;

  fn try_from(value: MicropubPayload) -> Result<Self, Self::Error> {
    match value {
      MicropubPayload::Json(json) => {
        Ok(serde_json::from_value::<UpdatePayload>(json).map_err(|e| {
            debug!("failed to deserialize JSON to micropub update payload: {e:?}");
            invalid_request("invalid micropub update payload")
        })?)
      },

      MicropubPayload::Form(_) => {
        Err(invalid_request("micropub update is defined for JSON bodies only"))
      }
    }
  }
}

pub async fn handle(state: Arc<AppState>, body: MicropubBody) -> Result<Response, Response> {
    debug!("checking scope");
    if !body.token.scope().contains(&TokenScope::Update) {
        return Err(insufficient_scope("missing update scope"));
    }

    debug!("converting micropub payload to update payload...");
    let payload = UpdatePayload::try_from(body.payload)?
      .validate_payload()
      .map_err(|e| {
        debug!("micropub update payload validation failed: {e}");
        invalid_request(&e)
      })?;

    debug!("waiting for update to complete...");
    let (job, rx) = UpdateJob::new(state.clone(), payload);
    let (path, changed) = state.job_queue.enqueue_and_wait(job, rx).await?;

    debug!("returning response...");
    let builder = if changed {
        Response::builder().status(StatusCode::CREATED).header(
            header::LOCATION,
            state.config.micropub.get_content_url(&path),
        )
    } else {
        Response::builder().status(StatusCode::OK)
    };

    Ok(builder.body(Body::empty()).unwrap())
}

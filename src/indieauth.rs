use reqwest::StatusCode;
use serde::Deserialize;
use tracing::debug;

use crate::AppState;

#[derive(Deserialize, Clone)]
pub struct TokenInfo {
    pub me: String,
    pub client_id: Option<String>,
    scope: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum TokenScope {
  Create,
  Draft,
  Update,
  Delete,
  Undelete,
  Media,
  Unknown(String)
}

impl From<String> for TokenScope {
  fn from(value: String) -> Self {
    match value.as_str() {
      "create" => TokenScope::Create,
      "draft" => TokenScope::Draft,
      "update" => TokenScope::Update,
      "delete" => TokenScope::Delete,
      "undelete" => TokenScope::Undelete,
      "media" => TokenScope::Media,
      unknown => TokenScope::Unknown(unknown.to_string())
    }
  }
}

impl TokenInfo {
  pub fn scope(&self) -> Vec<TokenScope> {
    match &self.scope {
      Some(scope) => {
        scope.split(" ").map(|s| TokenScope::from(s.to_string())).collect()
      },

      None => Vec::new()
    }
  }
}

/// validate_token attempts to validate a given token against the configured token validation endpoint.
/// First, it will attempt the standardized token validation method by sending a POST request to the
/// validation endpoint with the token as a form-encoded variable called `token`. If, in trying this,
/// we receive an HTTP error (like not found, or something else), we fall back to an older, nonstandard
/// token validation technique. In this case, we will send a GET request to the same endpoint with the
/// token as a Bearer auth header. In either case, if we get a successful response, we decode the body
/// into a TokenInfo struct. Otherwise, we send back a String containing a more specific error message.
/// With a valid TokenInfo, we compare the token's `me` claim to the instance's configured me_url. If
/// these values do not match, the token is rejected. Otherwise, the token is accepted.
pub async fn validate_token(state: &AppState, token: &str) -> Result<TokenInfo, String> {
  let token = validate_token_inner(state, token).await.map_err(|e| {
    format!("failed to validate token: {}", e)
  })?;

  if token.me != state.config.auth.me_url {
    return Err(format!("failed to validate token: configured me_url does not match token's `me` claim"));
  }

  Ok(token)
}

async fn validate_token_inner(
    state: &AppState,
    token: &str,
) -> Result<TokenInfo, reqwest::Error> {
    let url = &state.config.auth.validate_token_url[..];

    let payload = [("token", token)];

    let response = state
        .reqwest
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&payload)
        .send()
        .await
        .inspect_err(|e| debug!("error sending POST request for token validation: {e:#?}"))?;
    
    if response.status() == StatusCode::OK {
      let maybe_info = response.json::<TokenInfo>().await;
      if maybe_info.is_ok() {
        return Ok(maybe_info.unwrap());
      }
    }

    debug!("modern token validation method failed, trying legacy token validation method as fallback");

    // This is an old validation routine that is no longer standardized. We send a GET request to the token endpoint
    // with the token in the bearer auth header. IndieAuth works in this manner, despite this being nonstandard.
    Ok(
      state
        .reqwest
        .get(url)
        .header("Accept", "application/json")
        .bearer_auth(token)
        .send()
        .await
        .inspect_err(|e| debug!("error sending GET request for fallback token validation: {e:#?}"))?
        .json::<TokenInfo>()
        .await
        .inspect_err(|e| debug!("error deserializing fallback token validation response to TokenInfo: {e:#?}"))?
    )
}

use serde::Deserialize;
use tracing::debug;

use crate::AppState;

#[derive(Deserialize, Clone)]
pub struct TokenInfo {
    pub active: bool,
    pub me: String,
    pub client_id: Option<String>,
    pub scope: Option<String>,
    pub exp: Option<u32>,
    pub iat: Option<u32>,
}

/// validate_token attempts to validate a given token against the configured token validation endpoint.
/// First, it will attempt the standardized token validation method by sending a POST request to the
/// validation endpoint with the token as a form-encoded variable called `token`. If, in trying this,
/// we receive an HTTP error (like not found, or something else), we fall back to an older, nonstandard
/// token validation technique. In this case, we will send a GET request to the same endpoint with the
/// token as a Bearer auth header. In either case, if we get a successful response, we decode the body
/// into a TokenInfo struct and pass that along. Otherwise, we send back a ValidationError.
pub async fn validate_token(
    state: &AppState,
    token: &str,
) -> Result<TokenInfo, reqwest::Error> {
    let url = &state.config.auth.validate_token_url[..];

    let payload = [("token", token)];
    let result = state
        .reqwest
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&payload)
        .send()
        .await?
        .json::<TokenInfo>()
        .await
        .inspect_err(|e| debug!("error deserializing standard token validation response to TokenInfo: {e:#?}"));

    match result {
        Ok(response) => Ok(response),
        Err(err) => {
            debug!("token endpoint returned an error for the standard token validation routine, trying nonstandard validation: {err:#?}");

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
    }
}

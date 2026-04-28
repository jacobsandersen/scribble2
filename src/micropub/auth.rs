use std::{fmt::Display, sync::Arc};

use axum::{
    extract::{Request, State},
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};
use tracing::debug;

use crate::{AppState, indieauth, micropub::error};

#[derive(Debug)]
enum AuthError {
    MalformedHeader,
    HeaderNotBearer,
}

impl Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match &self {
            AuthError::MalformedHeader => "Malformed Bearer token header",
            AuthError::HeaderNotBearer => "Header is not Bearer formatted",
        };

        write!(f, "{}", message)
    }
}

pub async fn authorize(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    if let Some(header) = request.headers().get(header::AUTHORIZATION.as_str()) {
        let token =
            extract_token_from_header(header).map_err(|e| error::forbidden(&e.to_string()))?;

        debug!("extracted token value from header, validating with indieauth");

        let info = indieauth::validate_token(&state, token)
            .await
            .map_err(|e| {
                let err_msg = e.to_string();
                debug!("token validation failed: {}", err_msg);
                error::unauthorized(&format!("failed to validate token: {}", err_msg))
            })?;

        debug!("saving token info to request");

        request.extensions_mut().insert(info);
    }

    Ok(next.run(request).await)
}

fn extract_token_from_header(header: &HeaderValue) -> Result<&str, AuthError> {
    debug!("attempting to extract token from authorization header");

    let value = header.to_str().map_err(|e| {
        debug!("failed to convert header to &str: {e:?}");
        AuthError::MalformedHeader
    })?;

    let parts: Vec<&str> = value.split(" ").collect();
    if parts.len() != 2 {
        debug!("malformed authorization header (len != 2)");
        return Err(AuthError::MalformedHeader);
    }

    if parts[0].to_lowercase().trim() != "bearer" {
        debug!("malformed authorization header (not Bearer)");
        return Err(AuthError::HeaderNotBearer);
    }

    Ok(parts[1].trim())
}

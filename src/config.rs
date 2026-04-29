use std::sync::LazyLock;

use regex::Regex;

use serde::Deserialize;
use validator::{Validate};

static RE_IPV4: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(\b25[0-5]|\b2[0-4][0-9]|\b[01]?[0-9][0-9]?)(\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)){3}").unwrap()
});

#[derive(Debug, Validate, Deserialize)]
pub struct ScribbleConfig {
  /// Server configuration, including http binding information, payload sizes, etc.
  #[validate(nested)]
  pub server: Server,

  /// Authorization information, including the user's "me" URL and token introspection 
  /// URL for token validation.
  #[validate(nested)]
  pub auth: Auth
}

#[derive(Debug, Validate, Deserialize)]
pub struct Server {
  /// The ip/port pair to bind the http server to
  #[validate(nested)]
  pub binding: Binding
}

#[derive(Debug, Validate, Deserialize)]
pub struct Binding {
  /// The IP address to bind the http server to
  #[validate(regex(path = *RE_IPV4))]
  pub ip: String,

  /// The port to bind the http server to
  pub port: u16
}

impl std::fmt::Display for Binding { 
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      write!(f, "{}:{}", self.ip, self.port)
  }
}

#[derive(Debug, Validate, Deserialize)]
pub struct Auth {
  /// The user's "me" URL, which is usually their website. A presented token's
  /// "me" field must much this value for the token to be considered "for" this
  /// server.
  #[validate(url)]
  pub me_url: String,

  /// The token introspection URL. The server will attempt to POST a token here to
  /// validate it. Failing that, it will attempt an older validation method by
  /// GET'ing this URL with the token as a form body parameter.
  #[validate(url)]
  pub validate_token_url: String
}

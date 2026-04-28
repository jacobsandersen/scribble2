use std::sync::LazyLock;

use regex::Regex;

use serde::Deserialize;
use validator::{Validate};

static RE_IPV4: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(\b25[0-5]|\b2[0-4][0-9]|\b[01]?[0-9][0-9]?)(\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)){3}").unwrap()
});

#[derive(Debug, Validate, Deserialize)]
pub struct ScribbleConfig {
  pub server: Server,
  pub auth: Auth
}

#[derive(Debug, Validate, Deserialize)]
pub struct Server {
  pub binding: Binding
}

#[derive(Debug, Validate, Deserialize)]
pub struct Binding {
  #[validate(regex(path = *RE_IPV4))]
  pub ip: String,
  pub port: u16
}

impl std::fmt::Display for Binding { 
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      write!(f, "{}:{}", self.ip, self.port)
  }
}

#[derive(Debug, Validate, Deserialize)]
pub struct Auth {
  #[validate(url)]
  pub me_url: String,

  #[validate(url)]
  pub validate_token_url: String
}

use std::sync::LazyLock;

use regex::Regex;

use serde::Deserialize;
use validator::{Validate, ValidationError};

static RE_IPV4: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(\b25[0-5]|\b2[0-4][0-9]|\b[01]?[0-9][0-9]?)(\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)){3}").unwrap()
});

static RE_PATH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"^(\{(year|month|day|slug)+\})(\/(\{(year|month|day|slug)+\})+)+(\.json)?$").unwrap()
});

#[derive(Debug, Validate, Deserialize)]
pub struct ScribbleConfig {
  /// Server configuration, including http binding information, payload sizes, etc.
  #[validate(nested)]
  pub server: Server,

  /// Authorization information, including the user's "me" URL and token introspection 
  /// URL for token validation.
  #[validate(nested)]
  pub auth: Auth,

  /// Micropub specific settings, including the public content URL and path pattern configuration.
  #[validate(nested)]
  pub micropub: Micropub
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

#[derive(Debug, Validate, Deserialize)]
pub struct Micropub {
  /// The storage information controlling how media is stored.
  #[validate(nested)]
  pub media: MicropubMedia,

  /// The storage information controlling how content is stored.
  #[validate(nested)]
  pub content: MicropubContent
}

#[derive(Debug, Validate, Deserialize)]
pub struct MicropubMedia {
  /// The public URL where media uploaded by this server is available.
  /// This value is combined with the object path (randomized) to create the media permalink.
  #[validate(url, custom(function = "has_trailing_slash"))]
  pub public_url: String,

  /// The configuration for the s3 storage, where this server will upload any
  /// media provided to it.
  #[validate(nested)]
  pub s3: MicropubMediaS3
}

impl MicropubMedia {
  pub fn get_media_url(&self, path: &str) -> String {
    format!("{}{}", self.public_url, path)
  }
}

#[derive(Debug, Validate, Deserialize)]
pub struct MicropubMediaS3 {
  /// The access key id for S3
  pub access_key_id: String,

  /// The secret access key for S3
  pub secret_access_key: String,

  /// The region for S3 (i.e., ap-southeast-1)
  pub region: Option<String>,

  /// The bucket name in S3
  pub bucket: String,

  /// The S3 endpoint (allows using other S3 compatible services)
  pub endpoint: String
}

#[derive(Debug, Validate, Deserialize)]
pub struct MicropubContent {
  /// The public URL where content published by this server is available.
  /// This value is combined with the path_pattern to create the content permalink.
  #[validate(url, custom(function = "has_trailing_slash"))]
  pub public_url: String,

  /// The path pattern for content stored by this server.
  /// The content will then be available at {public_url}/{path_pattern}.
  /// The path pattern supports replacement of {year}, {month}, {day}, and
  /// {slug} variables.
  #[validate(regex(path = *RE_PATH_PATTERN))]
  pub path_pattern: String,

  /// The configuration for git storage, where this server will upload any 
  /// content provided to it.
  #[validate(nested)]
  pub git: MicropubContentGit
}

impl MicropubContent {
  pub fn get_content_url(&self, path: &str) -> String {
    format!("{}{}", self.public_url, path)
  }
}

#[derive(Debug, Validate, Deserialize)]
pub struct MicropubContentGit {
  /// The repository that this server will store its data.
  pub repository: String,

  /// The path to the private SSH key used for authentication
  /// (only if the repository remote is SSH).
  pub key_path: Option<String>,

  /// The username to use for authentication. Default ~ (null)
  /// when using SSH, as the username for SSH is typically 
  /// specified in the URL (i.e., as `git`).
  pub username: Option<String>,

  /// The password to use for authentication. For http(s) repositories,
  /// this is the account password or app password (e.g., for GitHub).
  /// For ssh repositories, this is the key passphrase. May be ~ (null) 
  /// when using SSH and the key has no passphrase.
  pub password: Option<String>,

  /// The branch to push to on the remote. If not specified, "main" is used.
  pub branch: Option<String>
}

fn has_trailing_slash(url: &str) -> Result<(), ValidationError> {
  if !url.ends_with("/") {
    return Err(ValidationError::new("missing_trailing_slash"));
  }

  Ok(())
}
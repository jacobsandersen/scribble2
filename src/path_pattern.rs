use std::collections::HashMap;

use chrono::Datelike;
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum PatternError {
  #[error("missing required parameter {0}")]
  MissingRequiredParam(String)
}

pub struct PathPattern {
  raw: String
}

impl PathPattern {
  pub fn new(path: &str) -> Result<PathPattern, PatternError> {
    if !path.contains("{slug}") {
      return Err(PatternError::MissingRequiredParam(String::from("{slug}")));
    }

    Ok(PathPattern { raw: path.to_owned() })
  }

  pub fn new_context(&self, slug: &str) -> HashMap<&str, String> {
    let now = chrono::Utc::now();
    let mut map = HashMap::new();

    let year = format!("{}", now.year());
    map.insert("year", year);

    let month = format!("{:02}", now.month());
    map.insert("month", month);

    let day = format!("{:02}", now.day());
    map.insert("day", day);

    map.insert("slug", slug.to_string());

    map
  }

  pub fn resolve(&self, map: &HashMap<&str, String>) -> String {
    let mut result = self.raw.clone();

    for (key, value) in map {
      let placeholder = &format!("{{{}}}", key);
      result = result.replace(placeholder, value);
    }
    
    result
  }

  pub fn extract<'a>(&self, path: &str) -> Option<HashMap<&str, String>> {
    let path = path.strip_suffix(".json").unwrap_or(path);
    let raw = self.raw.strip_suffix(".json").unwrap_or(&self.raw);

    let pattern_parts: Vec<&str> = raw.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    if pattern_parts.len() != path_parts.len() {
      debug!("pattern_parts and path_parts are different lengths");
      return None;
    }

    let mut map = HashMap::new();
    for (pat, val) in pattern_parts.iter().zip(path_parts.iter()) {
        match *pat {
            "{year}"  => { map.insert("year", val.to_string()); }
            "{month}" => { map.insert("month", val.to_string()); }
            "{day}"   => { map.insert("day", val.to_string()); }
            "{slug}"  => { map.insert("slug", val.to_string()); }
            literal if literal == *val => {}
            x => {
              debug!("unknown path var {x}, bailing");
              return None;
            }
        }
    }

    Some(map)
  }
}
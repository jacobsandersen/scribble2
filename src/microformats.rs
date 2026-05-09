use std::collections::HashMap;

use serde::{Deserialize, Serialize, de::Visitor};
use serde_valid::Validate;
use tracing::warn;

use crate::micropub::post::MicropubPayload;

#[derive(Debug, PartialEq, Clone)] 
pub enum Mf2Value {
  String(String),
  Embedded(serde_json::Value),
  Object(Mf2Object)
}

impl Serialize for Mf2Value {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
          S: serde::Serializer {
      match &self {
        Mf2Value::String(str) => {
          str.serialize(serializer)
        },

        Mf2Value::Embedded(obj) => {
          obj.serialize(serializer)
        },

        Mf2Value::Object (obj) => {
           obj.serialize(serializer)
        }
      }
  }
}

struct Mf2ValueVisitor;

impl<'de> Visitor<'de> for Mf2ValueVisitor {
  type Value = Mf2Value;

  fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
      write!(formatter, "An Mf2Value variant (String, Markup, Object)")
  } 

  fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where E: serde::de::Error, {

    Ok(Mf2Value::String(v.to_string()))
  }

  fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where A: serde::de::MapAccess<'de>, {
    
    let mut r#type: Option<Vec<String>> = None;
    let mut properties: Option<HashMap<String, Vec<Mf2Value>>> = None;
    let mut children: Option<Vec<Mf2Object>> = None;
    let mut other: HashMap<String, serde_json::Value> = HashMap::new();

    while let Some(key) = map.next_key::<String>()? {
      match key.as_str() {
        "type" => r#type = Some(map.next_value()?),
        "properties" => properties = Some(map.next_value()?),
        "children" => children = Some(map.next_value()?),
        _ => { other.insert(key, map.next_value()?); }
      }
    }

    if let (Some(r#type), Some(properties)) = (r#type, properties) {
      Ok(Mf2Value::Object(Mf2Object { 
        r#type, 
        properties, 
        children
      }))
    } else {
      Ok(Mf2Value::Embedded(serde_json::Value::Object(
        other.into_iter().collect()
      )))
    }
  }
}

/// Note: This Deserialize implementation is using `deserialize_any`,
/// which requires a self-describing format like JSON. This cannot be
/// used to construct an Mf2Value from a form-encoded byte stream. 
/// For that, use Mf2Value::from_form.
impl<'de> Deserialize<'de> for Mf2Value {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
    
    deserializer.deserialize_any(Mf2ValueVisitor)
  }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Validate)]
pub struct Mf2Object {
  #[validate(min_items = 1)]
  r#type: Vec<String>,
  pub properties: HashMap<String, Vec<Mf2Value>>,
  pub children: Option<Vec<Mf2Object>>
}

impl Mf2Object {
  pub fn first_type(&self) -> String {
    self.r#type.first().cloned().unwrap() // serde_valid ensures there is always one item
  }

  pub fn first_prop(&self, prop: &str) -> Option<Mf2Value> {
    self.properties
      .get(&prop.to_string())
      .and_then(|v| v.first().cloned())
  }

  pub fn first_string_prop(&self, prop: &str) -> Option<String> {
    if let Some(Mf2Value::String(value)) = self.first_prop(prop) {
      Some(value)
    } else {
      None
    }
  }

  pub fn add_props(&mut self, key: String, props: Vec<Mf2Value>) {
    let entry = self.properties.entry(key).or_insert_with(Vec::new);
    for prop in props {
      if !entry.contains(&prop) {
        entry.push(prop);
      }
    }
  }

  pub fn set_props(&mut self, key: String, props: Vec<Mf2Value>) {
    self.properties.insert(key, props);
  }

  pub fn delete_prop(&mut self, key: String) {
    self.properties.remove(&key);
  }

  pub fn delete_prop_values(&mut self, key: String, values: Vec<Mf2Value>) {
    let entry = self.properties.get_mut(&key);
    if let Some(entry) = entry {
      entry.retain(|v| !values.contains(v));
    }
  }

  /// This function converts a `HashMap<String, Vec<String>>` (form data) payload into an `Mf2Object`.
  /// 
  /// Note: If the form data does not contain an `h` property, or if the `h` property is empty,
  /// the default `type` of the Mf2Object is `h-entry`.
  pub fn from_form(form: HashMap<String, Vec<String>>) -> Mf2Object {
    let mut r#type = String::from("h-entry");
    let mut properties = HashMap::new();

    for (key, value) in form {
      if key == "h" {
        if let Some(first) = value.first().cloned() {
          r#type = format!("h-{first}");
        } else {
          warn!("received empty h parameter -- object type will default to h-entry");
        }
      } else {
        properties.insert(key, value.into_iter().map(|v| Mf2Value::String(v)).collect());
      }
    }

    Mf2Object { r#type: vec![r#type], properties, children: None }
  }
}

impl TryFrom<MicropubPayload> for Mf2Object {
  type Error = serde_json::Error;
  
  fn try_from(value: MicropubPayload) -> Result<Self, Self::Error> {
      match value {
        MicropubPayload::Json(json) => {
          serde_json::from_value::<Mf2Object>(json)
        }

        MicropubPayload::Form(form) => {
          Ok(Mf2Object::from_form(form))
        }
      }
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;
  use super::*;

  #[test]
  fn test_serde_value_string() {
    let value = Mf2Value::String(String::from("test string"));
    let serialized_value = serde_json::to_value(&value).unwrap();
    let macro_value = json!("test string");
    assert_eq!(serialized_value, macro_value);

    let deserialized_value: Mf2Value = serde_json::from_value(macro_value).unwrap();
    assert_eq!(deserialized_value, value);
  }

  #[test]
  fn test_serde_value_object() {
    let mut value_properties = HashMap::new();
    value_properties.insert(String::from("name"), vec![Mf2Value::String(String::from("John Smith"))]);

    let value = Mf2Value::Object(Mf2Object { 
      r#type: vec![String::from("h-entry")], 
      properties: value_properties.clone(),
      children: Some(vec![
        Mf2Object {
          r#type: vec![String::from("subtype")],
          properties: value_properties,
          children: None
        }
      ])
    });

    let serialized_value = serde_json::to_value(&value).unwrap();

    let macro_value = json!({
      "type": ["h-entry"],
      "properties": {
        "name": ["John Smith"]
      },
      "children": [
        {
          "type": ["subtype"],
          "properties": {
            "name": ["John Smith"]
          },
          "children": null
        }
      ]
    });

    assert_eq!(serialized_value, macro_value);

    let deserialized_value: Mf2Value = serde_json::from_value(macro_value).unwrap();

    assert_eq!(deserialized_value, value);
  }
}
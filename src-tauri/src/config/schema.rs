use serde_json::{Map, Value};

use super::toml_types::ConfigToml;

/// Generate a JSON Schema for ConfigToml by introspecting its serialized shape.
/// This is a lightweight alternative to schemars — it produces a schema from
/// the default value so that frontends can discover available keys.
pub fn config_schema() -> Value {
    let default_val =
        serde_json::to_value(ConfigToml::default()).unwrap_or(Value::Object(Map::new()));
    build_schema_from_value(&default_val)
}

/// Validate that a JSON value conforms to the known ConfigToml keys.
/// Returns a list of unknown top-level keys.
pub fn validate_config_keys(value: &Value) -> Vec<String> {
    let schema = serde_json::to_value(ConfigToml::default()).unwrap_or(Value::Object(Map::new()));
    let known_keys: Vec<String> = match &schema {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => vec![],
    };

    match value {
        Value::Object(map) => map
            .keys()
            .filter(|k| !known_keys.contains(k))
            .cloned()
            .collect(),
        _ => vec![],
    }
}

/// Render the config schema as pretty-printed JSON bytes.
pub fn config_schema_json() -> Result<Vec<u8>, serde_json::Error> {
    let schema = config_schema();
    serde_json::to_vec_pretty(&schema)
}

fn build_schema_from_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut properties = Map::new();
            for (key, val) in map {
                properties.insert(key.clone(), build_schema_from_value(val));
            }
            serde_json::json!({
                "type": "object",
                "properties": properties
            })
        }
        Value::Array(_) => serde_json::json!({ "type": "array" }),
        Value::String(_) => serde_json::json!({ "type": "string" }),
        Value::Number(_) => serde_json::json!({ "type": "number" }),
        Value::Bool(_) => serde_json::json!({ "type": "boolean" }),
        Value::Null => serde_json::json!({ "type": "null" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_object() {
        let schema = config_schema();
        assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn schema_json_roundtrip() {
        let bytes = config_schema_json().unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.get("type").and_then(|v| v.as_str()), Some("object"));
    }

    #[test]
    fn validate_known_keys() {
        let val = serde_json::json!({ "model": "gpt-4" });
        let unknown = validate_config_keys(&val);
        assert!(unknown.is_empty());
    }

    #[test]
    fn validate_unknown_keys() {
        let val = serde_json::json!({ "model": "gpt-4", "bogus_key": true });
        let unknown = validate_config_keys(&val);
        assert_eq!(unknown, vec!["bogus_key".to_string()]);
    }
}

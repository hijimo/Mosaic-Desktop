use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use toml::Value as TomlValue;

use super::layer_stack::ConfigLayerMeta;

use std::collections::HashMap;

/// Record which config layer set each leaf key. Walks the TOML tree and
/// inserts dotted-path → metadata entries into `origins`.
pub fn record_origins(
    value: &TomlValue,
    meta: &ConfigLayerMeta,
    path: &mut Vec<String>,
    origins: &mut HashMap<String, ConfigLayerMeta>,
) {
    match value {
        TomlValue::Table(table) => {
            for (key, val) in table {
                path.push(key.clone());
                record_origins(val, meta, path, origins);
                path.pop();
            }
        }
        TomlValue::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                path.push(idx.to_string());
                record_origins(item, meta, path, origins);
                path.pop();
            }
        }
        _ => {
            if !path.is_empty() {
                origins.insert(path.join("."), meta.clone());
            }
        }
    }
}

/// Compute a SHA-256 fingerprint of a TOML value for change detection.
pub fn version_for_toml(value: &TomlValue) -> String {
    let json = serde_json::to_value(value).unwrap_or(JsonValue::Null);
    let canonical = canonical_json(&json);
    let serialized = serde_json::to_vec(&canonical).unwrap_or_default();
    let hash = Sha256::digest(&serialized);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

fn canonical_json(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            for key in keys {
                if let Some(val) = map.get(&key) {
                    sorted.insert(key, canonical_json(val));
                }
            }
            JsonValue::Object(sorted)
        }
        JsonValue::Array(items) => {
            JsonValue::Array(items.iter().map(canonical_json).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_deterministic() {
        let val: TomlValue = toml::from_str("model = \"gpt-4\"").unwrap();
        let v1 = version_for_toml(&val);
        let v2 = version_for_toml(&val);
        assert_eq!(v1, v2);
        assert!(v1.starts_with("sha256:"));
    }

    #[test]
    fn different_values_different_fingerprint() {
        let a: TomlValue = toml::from_str("model = \"gpt-4\"").unwrap();
        let b: TomlValue = toml::from_str("model = \"gpt-3.5\"").unwrap();
        assert_ne!(version_for_toml(&a), version_for_toml(&b));
    }

    #[test]
    fn record_origins_tracks_leaves() {
        let val: TomlValue = toml::from_str("model = \"gpt-4\"\n[tui]\ntheme = \"dark\"").unwrap();
        let meta = ConfigLayerMeta {
            name: "user".into(),
            version: "v1".into(),
        };
        let mut origins = HashMap::new();
        let mut path = Vec::new();
        record_origins(&val, &meta, &mut path, &mut origins);
        assert!(origins.contains_key("model"));
        assert!(origins.contains_key("tui.theme"));
    }
}

pub mod edit;
pub mod layer_stack;
pub mod toml_types;

pub use edit::ConfigEdit;
pub use layer_stack::{ConfigLayer, ConfigLayerStack};
pub use toml_types::{ConfigProfile, ConfigToml, McpServerConfig, McpServerTransportConfig, McpToolFilter};

use crate::protocol::error::{CodexError, ErrorCode};

/// Serialize a ConfigToml to a TOML string.
pub fn serialize_toml(config: &ConfigToml) -> Result<String, CodexError> {
    toml::to_string(config).map_err(|e| {
        CodexError::new(
            ErrorCode::ConfigurationError,
            format!("failed to serialize config to TOML: {e}"),
        )
    })
}

/// Deserialize a TOML string into a ConfigToml.
pub fn deserialize_toml(content: &str) -> Result<ConfigToml, CodexError> {
    toml::from_str(content).map_err(|e| {
        CodexError::new(
            ErrorCode::ConfigurationError,
            format!("failed to parse TOML config: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::Effort;

    #[test]
    fn serialize_deserialize_roundtrip() {
        let config = ConfigToml {
            model: Some("gpt-4".to_string()),
            ..Default::default()
        };
        let toml_str = serialize_toml(&config).unwrap();
        let decoded = deserialize_toml(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn invalid_toml_returns_descriptive_error() {
        let err = deserialize_toml("{{{{invalid toml").unwrap_err();
        assert_eq!(err.code, ErrorCode::ConfigurationError);
        assert!(err.message.contains("failed to parse TOML config"));
    }

    #[test]
    fn empty_toml_parses_to_default() {
        let config = deserialize_toml("").unwrap();
        assert_eq!(config, ConfigToml::default());
    }

    #[test]
    fn kebab_case_keys_in_toml_output() {
        let config = ConfigToml {
            model_reasoning_effort: Some(Effort::High),
            ..Default::default()
        };
        let toml_str = serialize_toml(&config).unwrap();
        assert!(toml_str.contains("model_reasoning_effort"));
    }
}

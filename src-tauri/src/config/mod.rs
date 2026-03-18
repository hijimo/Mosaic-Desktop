pub mod config_requirements;
pub mod constraint;
pub mod diagnostics;
pub mod edit;
pub mod fingerprint;
pub mod layer_stack;
pub mod merge;
pub mod overrides;
pub mod permissions;
pub mod schema;
pub mod service;
pub mod toml_types;

pub use config_requirements::{
    ConfigRequirements, ConfigRequirementsToml, ConstrainedWithSource, McpServerIdentity,
    McpServerRequirement, NetworkConstraints, RequirementSource, ResidencyRequirement,
    SandboxModeRequirement, Sourced, WebSearchModeRequirement,
};
pub use constraint::{Constrained, ConstraintError, ConstraintResult};
pub use diagnostics::{
    ConfigError, ConfigLoadError, TextPosition, TextRange, config_error_from_toml,
    config_error_from_typed_toml, format_config_error, format_config_error_with_source,
    io_error_from_config_error, validate_config_file,
};
pub use edit::ConfigEdit;
pub use fingerprint::{record_origins, version_for_toml};
pub use layer_stack::{ConfigLayer, ConfigLayerMeta, ConfigLayerStack};
pub use merge::merge_toml_values;
pub use overrides::build_cli_overrides_layer;
pub use permissions::{NetworkMode, NetworkToml, PermissionsToml};
pub use schema::{config_schema, config_schema_json, validate_config_keys};
pub use service::{ConfigService, ConfigServiceError};
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

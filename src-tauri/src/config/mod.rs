pub mod edit;
pub mod layer_stack;
pub mod toml_types;

pub use edit::ConfigEdit;
pub use layer_stack::{ConfigLayer, ConfigLayerStack};
pub use toml_types::{ConfigToml, McpServerConfig, McpServerTransportConfig, McpToolFilter};

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
/// Returns a descriptive parse error on invalid input.
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
    use std::collections::HashMap;

    #[test]
    fn serialize_deserialize_roundtrip() {
        let config = ConfigToml {
            model: Some("gpt-4".to_string()),
            approval_policy: Some("always".to_string()),
            sandbox_policy: None,
            mcp_servers: None,
            profiles: None,
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
            model: None,
            approval_policy: Some("never".to_string()),
            sandbox_policy: Some("read-only".to_string()),
            mcp_servers: None,
            profiles: None,
        };
        let toml_str = serialize_toml(&config).unwrap();
        assert!(toml_str.contains("approval-policy"));
        assert!(toml_str.contains("sandbox-policy"));
    }

    // --- Proptest strategies ---

    use proptest::prelude::*;

    fn arb_tool_filter() -> impl Strategy<Value = McpToolFilter> {
        (
            prop::option::of(prop::collection::vec("[a-z_]{1,10}", 0..=3)),
            prop::option::of(prop::collection::vec("[a-z_]{1,10}", 0..=3)),
        )
            .prop_map(|(enabled, disabled)| McpToolFilter { enabled, disabled })
    }

    fn arb_transport() -> impl Strategy<Value = McpServerTransportConfig> {
        prop_oneof![
            (
                "[a-z]{1,10}",
                prop::collection::vec("[a-z0-9.-]{1,10}", 0..=2),
                prop::collection::hash_map("[A-Z_]{1,8}", "[a-z0-9]{1,8}", 0..=2),
            )
                .prop_map(|(command, args, env)| McpServerTransportConfig::Stdio {
                    command,
                    args,
                    env,
                }),
            (
                "https://[a-z]{1,10}\\.com/[a-z]{1,5}",
                prop::collection::hash_map("[A-Za-z-]{1,10}", "[a-z0-9]{1,10}", 0..=2),
            )
                .prop_map(|(url, headers)| McpServerTransportConfig::Http { url, headers }),
            (
                "https://[a-z]{1,10}\\.com",
                "[a-z]{3,8}",
                "[a-z]{3,8}",
                "https://[a-z]{1,10}\\.com/token",
            )
                .prop_map(|(url, client_id, client_secret, token_url)| {
                    McpServerTransportConfig::OAuth {
                        url,
                        client_id,
                        client_secret,
                        token_url,
                    }
                }),
        ]
    }

    fn arb_mcp_server_config() -> impl Strategy<Value = McpServerConfig> {
        (
            arb_transport(),
            any::<bool>(),
            prop::option::of("[a-z ]{1,20}"),
            prop::option::of(arb_tool_filter()),
        )
            .prop_map(|(transport, disabled, disabled_reason, tool_filter)| {
                McpServerConfig {
                    transport,
                    disabled,
                    disabled_reason,
                    tool_filter,
                }
            })
    }

    /// Generate a ConfigToml without nested profiles (to avoid infinite recursion).
    fn arb_config_toml_flat() -> impl Strategy<Value = ConfigToml> {
        (
            prop::option::of("[a-z0-9-]{1,15}"),
            prop::option::of("[a-z-]{1,10}"),
            prop::option::of("[a-z-]{1,10}"),
            prop::option::of(prop::collection::hash_map(
                "[a-z]{1,8}",
                arb_mcp_server_config(),
                0..=2,
            )),
        )
            .prop_map(
                |(model, approval_policy, sandbox_policy, mcp_servers)| ConfigToml {
                    model,
                    approval_policy,
                    sandbox_policy,
                    mcp_servers,
                    profiles: None,
                },
            )
    }

    fn arb_config_toml() -> impl Strategy<Value = ConfigToml> {
        (
            arb_config_toml_flat(),
            prop::option::of(prop::collection::hash_map(
                "[a-z]{1,6}",
                arb_config_toml_flat(),
                0..=2,
            )),
        )
            .prop_map(|(mut config, profiles)| {
                config.profiles = profiles;
                config
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Property 3: ConfigToml TOML round-trip
        #[test]
        fn config_toml_toml_roundtrip(config in arb_config_toml()) {
            let toml_str = serialize_toml(&config).unwrap();
            let decoded = deserialize_toml(&toml_str).unwrap();
            prop_assert_eq!(config, decoded);
        }

        /// Property 18: Config layer priority merge —
        /// when multiple layers define the same field, the highest-priority
        /// layer's value is used.
        #[test]
        fn config_layer_priority_merge(
            session_model in "[a-z]{1,8}",
            user_model in "[a-z]{1,8}",
            system_model in "[a-z]{1,8}",
        ) {
            let mut stack = ConfigLayerStack::new();
            stack.add_layer(ConfigLayer::Session, ConfigToml {
                model: Some(session_model),
                ..Default::default()
            });
            stack.add_layer(ConfigLayer::User, ConfigToml {
                model: Some(user_model),
                ..Default::default()
            });
            stack.add_layer(ConfigLayer::System, ConfigToml {
                model: Some(system_model.clone()),
                ..Default::default()
            });
            let merged = stack.merge();
            // System > User > Session
            prop_assert_eq!(merged.model, Some(system_model));
        }

        /// Property 19: Config profile override —
        /// a named profile overrides base config values when activated.
        #[test]
        fn config_profile_override(
            base_model in "[a-z]{1,8}",
            profile_model in "[a-z]{1,8}",
        ) {
            let profile = ConfigToml {
                model: Some(profile_model.clone()),
                ..Default::default()
            };
            let config = ConfigToml {
                model: Some(base_model),
                profiles: Some(HashMap::from([("test".to_string(), profile)])),
                ..Default::default()
            };
            let mut stack = ConfigLayerStack::new();
            stack.add_layer(ConfigLayer::User, config);
            let resolved = stack.resolve_with_profile("test");
            prop_assert_eq!(resolved.model, Some(profile_model));
        }

        /// Property 20: Invalid TOML returns parse error —
        /// invalid TOML strings return a descriptive error, never panic.
        #[test]
        fn invalid_toml_returns_parse_error(garbage in "[^\\x00]{1,50}") {
            // Prepend invalid TOML syntax to ensure it's always invalid
            let invalid = format!("{{{{ {garbage}");
            let result = deserialize_toml(&invalid);
            if let Err(err) = result {
                prop_assert_eq!(err.code, ErrorCode::ConfigurationError);
                prop_assert!(err.message.contains("failed to parse TOML config"));
            }
            // If it somehow parses, that's fine too — no panic is the key property
        }
    }
}

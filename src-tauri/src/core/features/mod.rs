//! Centralized feature flags and metadata.
//!
//! This module defines toggles that gate experimental and optional behavior.
//! Call sites consult a single `Features` container instead of wiring
//! individual booleans through multiple types.

mod legacy;
#[allow(unused_imports)]
pub(crate) use legacy::LegacyFeatureToggles;
#[allow(unused_imports)]
pub(crate) use legacy::legacy_feature_keys;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// High-level lifecycle stage for a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Still under development, not ready for external use.
    UnderDevelopment,
    /// Available to users through an experimental menu.
    Experimental {
        name: &'static str,
        description: &'static str,
    },
    /// Stable — flag kept for ad-hoc toggling.
    Stable,
    /// Deprecated, should not be used.
    Deprecated,
    /// Flag is useless but kept for backward compatibility.
    Removed,
}

/// Unique features toggled via configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    // Stable
    GhostCommit,
    ShellTool,

    // Experimental / Under development
    JsRepl,
    JsReplToolsOnly,
    UnifiedExec,
    ShellZshFork,
    ApplyPatchFreeform,
    RequestPermissions,
    WebSearchRequest,
    WebSearchCached,
    SearchTool,
    UseLinuxSandboxBwrap,
    RequestRule,
    WindowsSandbox,
    WindowsSandboxElevated,
    RemoteModels,
    ShellSnapshot,
    CodexGitCommit,
    RuntimeMetrics,
    Sqlite,
    MemoryTool,
    ChildAgentsMd,
    PowershellUtf8,
    EnableRequestCompression,
    Collab,
    Apps,
    Plugins,
    AppsMcpGateway,
    SkillMcpDependencyInstall,
    SkillEnvVarDependencyPrompt,
    Steer,
    DefaultModeRequestUserInput,
    CollaborationModes,
    Personality,
    Artifact,
    FastMode,
    VoiceTranscription,
    RealtimeConversation,
    PreventIdleSleep,
    ResponsesWebsockets,
    ResponsesWebsocketsV2,
}

impl Feature {
    pub fn key(self) -> &'static str {
        self.spec().key
    }

    pub fn stage(self) -> Stage {
        self.spec().stage
    }

    pub fn default_enabled(self) -> bool {
        self.spec().default_enabled
    }

    fn spec(self) -> &'static FeatureSpec {
        FEATURES
            .iter()
            .find(|s| s.id == self)
            .expect("missing FeatureSpec")
    }
}

/// Single registry entry.
#[derive(Debug, Clone, Copy)]
pub struct FeatureSpec {
    pub id: Feature,
    pub key: &'static str,
    pub stage: Stage,
    pub default_enabled: bool,
}

/// All known features.
pub const FEATURES: &[FeatureSpec] = &[
    FeatureSpec { id: Feature::GhostCommit, key: "undo", stage: Stage::Stable, default_enabled: false },
    FeatureSpec { id: Feature::ShellTool, key: "shell_tool", stage: Stage::Stable, default_enabled: true },
    FeatureSpec { id: Feature::UnifiedExec, key: "unified_exec", stage: Stage::Stable, default_enabled: !cfg!(windows) },
    FeatureSpec { id: Feature::ShellZshFork, key: "shell_zsh_fork", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::ShellSnapshot, key: "shell_snapshot", stage: Stage::Stable, default_enabled: true },
    FeatureSpec {
        id: Feature::JsRepl, key: "js_repl",
        stage: Stage::Experimental { name: "JavaScript REPL", description: "Enable a persistent Node-backed JavaScript REPL." },
        default_enabled: false,
    },
    FeatureSpec { id: Feature::JsReplToolsOnly, key: "js_repl_tools_only", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::WebSearchRequest, key: "web_search_request", stage: Stage::Deprecated, default_enabled: false },
    FeatureSpec { id: Feature::WebSearchCached, key: "web_search_cached", stage: Stage::Deprecated, default_enabled: false },
    FeatureSpec { id: Feature::SearchTool, key: "search_tool", stage: Stage::Removed, default_enabled: false },
    FeatureSpec { id: Feature::CodexGitCommit, key: "codex_git_commit", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::RuntimeMetrics, key: "runtime_metrics", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::Sqlite, key: "sqlite", stage: Stage::Stable, default_enabled: true },
    FeatureSpec { id: Feature::MemoryTool, key: "memories", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::ChildAgentsMd, key: "child_agents_md", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::ApplyPatchFreeform, key: "apply_patch_freeform", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::RequestPermissions, key: "request_permissions", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::UseLinuxSandboxBwrap, key: "use_linux_sandbox_bwrap", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::RequestRule, key: "request_rule", stage: Stage::Removed, default_enabled: false },
    FeatureSpec { id: Feature::WindowsSandbox, key: "experimental_windows_sandbox", stage: Stage::Removed, default_enabled: false },
    FeatureSpec { id: Feature::WindowsSandboxElevated, key: "elevated_windows_sandbox", stage: Stage::Removed, default_enabled: false },
    FeatureSpec { id: Feature::RemoteModels, key: "remote_models", stage: Stage::Removed, default_enabled: false },
    FeatureSpec { id: Feature::PowershellUtf8, key: "powershell_utf8", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::EnableRequestCompression, key: "enable_request_compression", stage: Stage::Stable, default_enabled: true },
    FeatureSpec {
        id: Feature::Collab, key: "multi_agent",
        stage: Stage::Experimental { name: "Multi-agents", description: "Spawn multiple agents to parallelize work." },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::Apps, key: "apps",
        stage: Stage::Experimental { name: "Apps", description: "Use connected ChatGPT Apps via $ mentions." },
        default_enabled: false,
    },
    FeatureSpec { id: Feature::Plugins, key: "plugins", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::AppsMcpGateway, key: "apps_mcp_gateway", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::SkillMcpDependencyInstall, key: "skill_mcp_dependency_install", stage: Stage::Stable, default_enabled: true },
    FeatureSpec { id: Feature::SkillEnvVarDependencyPrompt, key: "skill_env_var_dependency_prompt", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::Steer, key: "steer", stage: Stage::Removed, default_enabled: true },
    FeatureSpec { id: Feature::DefaultModeRequestUserInput, key: "default_mode_request_user_input", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::CollaborationModes, key: "collaboration_modes", stage: Stage::Removed, default_enabled: true },
    FeatureSpec { id: Feature::Personality, key: "personality", stage: Stage::Stable, default_enabled: true },
    FeatureSpec { id: Feature::Artifact, key: "artifact", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::FastMode, key: "fast_mode", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::VoiceTranscription, key: "voice_transcription", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::RealtimeConversation, key: "realtime_conversation", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec {
        id: Feature::PreventIdleSleep, key: "prevent_idle_sleep",
        stage: if cfg!(any(target_os = "macos", target_os = "linux", target_os = "windows")) {
            Stage::Experimental { name: "Prevent sleep while running", description: "Keep computer awake while running." }
        } else {
            Stage::UnderDevelopment
        },
        default_enabled: false,
    },
    FeatureSpec { id: Feature::ResponsesWebsockets, key: "responses_websockets", stage: Stage::UnderDevelopment, default_enabled: false },
    FeatureSpec { id: Feature::ResponsesWebsocketsV2, key: "responses_websockets_v2", stage: Stage::UnderDevelopment, default_enabled: false },
];

/// Deserializable features table for TOML config.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct FeaturesToml {
    #[serde(flatten)]
    pub entries: BTreeMap<String, bool>,
}

/// Holds the effective set of enabled features.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Features {
    enabled: BTreeSet<Feature>,
}

impl Features {
    /// Start with built-in defaults.
    pub fn with_defaults() -> Self {
        let mut set = BTreeSet::new();
        for spec in FEATURES {
            if spec.default_enabled {
                set.insert(spec.id);
            }
        }
        Self { enabled: set }
    }

    pub fn enabled(&self, f: Feature) -> bool {
        self.enabled.contains(&f)
    }

    pub fn enable(&mut self, f: Feature) -> &mut Self {
        self.enabled.insert(f);
        self
    }

    pub fn disable(&mut self, f: Feature) -> &mut Self {
        self.enabled.remove(&f);
        self
    }

    /// Apply a table of key -> bool toggles (e.g. from TOML `[features]`).
    pub fn apply_map(&mut self, m: &BTreeMap<String, bool>) {
        for (k, v) in m {
            match feature_for_key(k) {
                Some(feat) => {
                    if *v { self.enable(feat); } else { self.disable(feat); }
                }
                None => {
                    eprintln!("unknown feature key in config: {k}");
                }
            }
        }
    }

    /// Build from a merged ConfigToml.
    pub fn from_config(cfg: &crate::config::ConfigToml) -> Self {
        let mut features = Features::with_defaults();

        // Apply [features] table if present.
        if let Some(ref val) = cfg.features {
            if let Ok(ft) = serde_json::from_value::<FeaturesToml>(val.clone()) {
                features.apply_map(&ft.entries);
            }
        }

        // Enforce dependency: js_repl_tools_only requires js_repl.
        if features.enabled(Feature::JsReplToolsOnly) && !features.enabled(Feature::JsRepl) {
            features.disable(Feature::JsReplToolsOnly);
        }

        features
    }

    pub fn enabled_features(&self) -> Vec<Feature> {
        self.enabled.iter().copied().collect()
    }
}

/// Resolve a key to a Feature, checking canonical keys then legacy aliases.
fn feature_for_key(key: &str) -> Option<Feature> {
    for spec in FEATURES {
        if spec.key == key {
            return Some(spec.id);
        }
    }
    legacy::feature_for_key(key)
}

/// Returns `true` if the key matches any known feature toggle.
pub fn is_known_feature_key(key: &str) -> bool {
    feature_for_key(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_development_features_disabled_by_default() {
        for spec in FEATURES {
            if matches!(spec.stage, Stage::UnderDevelopment) {
                assert!(
                    !spec.default_enabled,
                    "feature `{}` is under development but enabled by default",
                    spec.key
                );
            }
        }
    }

    #[test]
    fn default_enabled_features_are_stable_or_removed() {
        for spec in FEATURES {
            if spec.default_enabled {
                assert!(
                    matches!(spec.stage, Stage::Stable | Stage::Removed),
                    "feature `{}` enabled by default but stage is {:?}",
                    spec.key,
                    spec.stage
                );
            }
        }
    }

    #[test]
    fn with_defaults_enables_stable_defaults() {
        let f = Features::with_defaults();
        assert!(f.enabled(Feature::ShellTool));
        assert!(f.enabled(Feature::Sqlite));
        assert!(!f.enabled(Feature::JsRepl));
        assert!(!f.enabled(Feature::MemoryTool));
    }

    #[test]
    fn enable_disable_toggle() {
        let mut f = Features::with_defaults();
        assert!(!f.enabled(Feature::JsRepl));
        f.enable(Feature::JsRepl);
        assert!(f.enabled(Feature::JsRepl));
        f.disable(Feature::JsRepl);
        assert!(!f.enabled(Feature::JsRepl));
    }

    #[test]
    fn apply_map_sets_features() {
        let mut f = Features::with_defaults();
        let mut m = BTreeMap::new();
        m.insert("js_repl".to_string(), true);
        m.insert("shell_tool".to_string(), false);
        f.apply_map(&m);
        assert!(f.enabled(Feature::JsRepl));
        assert!(!f.enabled(Feature::ShellTool));
    }

    #[test]
    fn legacy_alias_collab() {
        assert_eq!(feature_for_key("multi_agent"), Some(Feature::Collab));
        assert_eq!(feature_for_key("collab"), Some(Feature::Collab));
    }

    #[test]
    fn from_config_parses_features_json() {
        let cfg = crate::config::ConfigToml {
            features: Some(serde_json::json!({"js_repl": true, "shell_tool": false})),
            ..Default::default()
        };
        let f = Features::from_config(&cfg);
        assert!(f.enabled(Feature::JsRepl));
        assert!(!f.enabled(Feature::ShellTool));
    }

    #[test]
    fn from_config_enforces_js_repl_dependency() {
        let cfg = crate::config::ConfigToml {
            features: Some(serde_json::json!({"js_repl_tools_only": true})),
            ..Default::default()
        };
        let f = Features::from_config(&cfg);
        assert!(!f.enabled(Feature::JsReplToolsOnly));
    }

    #[test]
    fn feature_key_roundtrip() {
        for spec in FEATURES {
            assert_eq!(spec.id.key(), spec.key);
            assert_eq!(spec.id.stage(), spec.stage);
            assert_eq!(spec.id.default_enabled(), spec.default_enabled);
        }
    }

    #[test]
    fn is_known_feature_key_works() {
        assert!(is_known_feature_key("js_repl"));
        assert!(is_known_feature_key("collab")); // legacy alias
        assert!(!is_known_feature_key("nonexistent_feature"));
    }

    #[test]
    fn enabled_features_returns_all_enabled() {
        let mut f = Features::with_defaults();
        f.enable(Feature::JsRepl);
        let list = f.enabled_features();
        assert!(list.contains(&Feature::JsRepl));
        assert!(list.contains(&Feature::ShellTool));
    }
}

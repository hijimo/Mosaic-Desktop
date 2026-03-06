use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::protocol::types::{
    AskForApproval, Effort, ForcedLoginMethod, Personality, ReasoningSummary, SandboxMode,
    ServiceTier, Verbosity, WebSearchMode,
};

/// Top-level TOML configuration structure matching reference `ConfigToml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct ConfigToml {
    // ── Model ────────────────────────────────────────────────────
    pub model: Option<String>,
    pub review_model: Option<String>,
    pub model_provider: Option<String>,
    pub model_context_window: Option<i64>,
    pub model_auto_compact_token_limit: Option<i64>,
    pub model_reasoning_effort: Option<Effort>,
    pub plan_mode_reasoning_effort: Option<Effort>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_verbosity: Option<Verbosity>,
    pub model_supports_reasoning_summaries: Option<bool>,
    pub model_instructions_file: Option<PathBuf>,
    pub model_catalog_json: Option<PathBuf>,

    // ── Policies ─────────────────────────────────────────────────
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_workspace_write: Option<SandboxWorkspaceWrite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsToml>,
    pub allow_login_shell: Option<bool>,
    #[serde(default)]
    pub shell_environment_policy: ShellEnvironmentPolicyToml,

    // ── Instructions ─────────────────────────────────────────────
    pub instructions: Option<String>,
    pub developer_instructions: Option<String>,
    pub compact_prompt: Option<String>,

    // ── Personality / mode ───────────────────────────────────────
    pub personality: Option<Personality>,
    pub service_tier: Option<ServiceTier>,

    // ── Notifications ────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<Vec<String>>,

    // ── MCP ──────────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    pub mcp_oauth_credentials_store: Option<String>,
    pub mcp_oauth_callback_port: Option<u16>,
    pub mcp_oauth_callback_url: Option<String>,

    // ── Model providers ──────────────────────────────────────────
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model_providers: HashMap<String, serde_json::Value>,

    // ── Auth ─────────────────────────────────────────────────────
    pub forced_login_method: Option<ForcedLoginMethod>,
    pub forced_chatgpt_workspace_id: Option<String>,
    pub cli_auth_credentials_store: Option<String>,
    pub commit_attribution: Option<String>,

    // ── History / state ──────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryToml>,
    pub sqlite_home: Option<PathBuf>,
    pub log_dir: Option<PathBuf>,

    // ── Project docs ─────────────────────────────────────────────
    pub project_doc_max_bytes: Option<usize>,
    pub project_doc_fallback_filenames: Option<Vec<String>>,
    pub tool_output_token_limit: Option<usize>,

    // ── Shell / tools ────────────────────────────────────────────
    pub background_terminal_max_timeout: Option<u64>,
    pub js_repl_node_path: Option<PathBuf>,
    pub js_repl_node_module_dirs: Option<Vec<PathBuf>>,
    pub zsh_path: Option<PathBuf>,
    pub web_search: Option<WebSearchMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsToml>,

    // ── TUI ──────────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tui: Option<TuiToml>,
    pub hide_agent_reasoning: Option<bool>,
    pub show_raw_agent_reasoning: Option<bool>,
    pub file_opener: Option<serde_json::Value>,

    // ── Agents / memories / skills ───────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentsToml>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memories: Option<MemoriesToml>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub plugins: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<serde_json::Value>,

    // ── Realtime ─────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<serde_json::Value>,
    pub experimental_realtime_ws_base_url: Option<String>,
    pub experimental_realtime_ws_model: Option<String>,
    pub experimental_realtime_ws_backend_prompt: Option<String>,
    pub chatgpt_base_url: Option<String>,

    // ── Projects ─────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<HashMap<String, serde_json::Value>>,
    pub suppress_unstable_features_warning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghost_snapshot: Option<serde_json::Value>,
    pub project_root_markers: Option<Vec<String>>,

    // ── Profiles ─────────────────────────────────────────────────
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub profiles: HashMap<String, ConfigProfile>,
}

/// Subset of ConfigToml used as a named profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct ConfigProfile {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    pub model_reasoning_effort: Option<Effort>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub personality: Option<Personality>,
    pub service_tier: Option<ServiceTier>,
    pub instructions: Option<String>,
    pub developer_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

// ── Nested config structs ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct SandboxWorkspaceWrite {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writable_roots: Vec<PathBuf>,
    #[serde(default)]
    pub network_access: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct PermissionsToml {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct ShellEnvironmentPolicyToml {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherit: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub set: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct HistoryToml {
    pub persistence: Option<String>,
    pub max_entries: Option<usize>,
    pub save_on_exit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct ToolsToml {
    pub enable_apply_patch: Option<bool>,
    pub enable_web_search: Option<bool>,
    pub enable_js_repl: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct TuiToml {
    pub alt_screen: Option<String>,
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct AgentsToml {
    pub max_concurrent_threads: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct MemoriesToml {
    pub enabled: Option<bool>,
    pub max_entries: Option<usize>,
}

// ── MCP server config ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum McpServerTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    OAuth {
        url: String,
        client_id: String,
        client_secret: String,
        token_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct McpServerConfig {
    pub transport: McpServerTransportConfig,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_filter: Option<McpToolFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct McpToolFilter {
    pub enabled: Option<Vec<String>>,
    pub disabled: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_toml_roundtrip() {
        let config = ConfigToml::default();
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn config_with_core_fields() {
        let config = ConfigToml {
            model: Some("gpt-4".into()),
            model_reasoning_effort: Some(Effort::High),
            model_reasoning_summary: Some(ReasoningSummary::Concise),
            sandbox_mode: Some(SandboxMode::WorkspaceWrite),
            personality: Some(Personality::Friendly),
            web_search: Some(WebSearchMode::Live),
            instructions: Some("be helpful".into()),
            ..Default::default()
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn kebab_case_keys() {
        let config = ConfigToml {
            model_reasoning_effort: Some(Effort::Medium),
            sandbox_mode: Some(SandboxMode::ReadOnly),
            ..Default::default()
        };
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("model-reasoning-effort"));
        assert!(toml_str.contains("sandbox-mode"));
    }

    #[test]
    fn profile_roundtrip() {
        let profile = ConfigProfile {
            model: Some("gpt-3.5".into()),
            ..Default::default()
        };
        let config = ConfigToml {
            model: Some("gpt-4".into()),
            profiles: HashMap::from([("fast".into(), profile)]),
            ..Default::default()
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn mcp_server_roundtrip() {
        let server = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".into(),
                args: vec!["server.js".into()],
                env: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };
        let config = ConfigToml {
            mcp_servers: HashMap::from([("test".into(), server)]),
            ..Default::default()
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }
}

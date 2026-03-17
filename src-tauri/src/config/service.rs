use std::path::{Path, PathBuf};

use crate::protocol::error::{CodexError, ErrorCode};

use super::layer_stack::{ConfigLayer, ConfigLayerStack};
use super::toml_types::ConfigToml;
use super::{deserialize_toml, serialize_toml};

const CONFIG_TOML_FILE: &str = "config.toml";

/// Errors specific to the config service.
#[derive(Debug)]
pub enum ConfigServiceError {
    Io {
        context: &'static str,
        source: std::io::Error,
    },
    Config(CodexError),
}

impl std::fmt::Display for ConfigServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Config(e) => write!(f, "{}", e.message),
        }
    }
}

impl std::error::Error for ConfigServiceError {}

impl From<CodexError> for ConfigServiceError {
    fn from(e: CodexError) -> Self {
        Self::Config(e)
    }
}

/// Service for loading and persisting configuration from/to disk.
///
/// Manages the codex home directory (`~/.codex/`) and project-level
/// config files, assembling them into a [`ConfigLayerStack`].
#[derive(Debug, Clone)]
pub struct ConfigService {
    codex_home: PathBuf,
}

impl ConfigService {
    pub fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    /// Resolve the codex home directory (`~/.codex` or `$CODEX_HOME`).
    pub fn find_codex_home() -> Option<PathBuf> {
        if let Ok(val) = std::env::var("CODEX_HOME") {
            let p = PathBuf::from(val);
            if p.is_absolute() {
                return Some(p);
            }
        }
        dirs::home_dir().map(|h| h.join(".codex"))
    }

    /// Load a full [`ConfigLayerStack`] from disk, merging:
    /// 1. User config (`~/.codex/config.toml`)
    /// 2. Project config (`.codex/config.toml` relative to `project_root`)
    pub fn load_layers(&self, project_root: Option<&Path>) -> Result<ConfigLayerStack, ConfigServiceError> {
        let mut stack = ConfigLayerStack::new();

        // User layer
        let user_path = self.codex_home.join(CONFIG_TOML_FILE);
        if let Some(cfg) = self.read_toml_file(&user_path)? {
            stack.add_layer(ConfigLayer::User, cfg);
        }

        // Project layer
        if let Some(root) = project_root {
            let project_path = root.join(".codex").join(CONFIG_TOML_FILE);
            if let Some(cfg) = self.read_toml_file(&project_path)? {
                stack.add_layer(ConfigLayer::Project, cfg);
            }
        }

        Ok(stack)
    }

    /// Write a [`ConfigToml`] to the user-level config file.
    pub fn save_user_config(&self, config: &ConfigToml) -> Result<(), ConfigServiceError> {
        let path = self.codex_home.join(CONFIG_TOML_FILE);
        self.write_toml_file(&path, config)
    }

    /// Write a [`ConfigToml`] to the project-level config file.
    pub fn save_project_config(
        &self,
        project_root: &Path,
        config: &ConfigToml,
    ) -> Result<(), ConfigServiceError> {
        let dir = project_root.join(".codex");
        std::fs::create_dir_all(&dir).map_err(|e| ConfigServiceError::Io {
            context: "create project config dir",
            source: e,
        })?;
        let path = dir.join(CONFIG_TOML_FILE);
        self.write_toml_file(&path, config)
    }

    /// Read a single config key from the merged stack.
    pub fn read_value(
        &self,
        stack: &ConfigLayerStack,
        key: &str,
    ) -> Option<serde_json::Value> {
        let merged = stack.merge();
        let json = serde_json::to_value(&merged).ok()?;
        json.get(key).cloned()
    }

    // ── internal helpers ──────────────────────────────────────────

    fn read_toml_file(&self, path: &Path) -> Result<Option<ConfigToml>, ConfigServiceError> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let cfg = deserialize_toml(&content)?;
                Ok(Some(cfg))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ConfigServiceError::Io {
                context: "read config file",
                source: e,
            }),
        }
    }

    fn write_toml_file(&self, path: &Path, config: &ConfigToml) -> Result<(), ConfigServiceError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ConfigServiceError::Io {
                context: "create config parent dir",
                source: e,
            })?;
        }
        let content = serialize_toml(config)?;
        std::fs::write(path, content).map_err(|e| ConfigServiceError::Io {
            context: "write config file",
            source: e,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, ConfigService) {
        let tmp = TempDir::new().unwrap();
        let svc = ConfigService::new(tmp.path().to_path_buf());
        (tmp, svc)
    }

    #[test]
    fn load_empty_returns_empty_stack() {
        let (_tmp, svc) = setup();
        let stack = svc.load_layers(None).unwrap();
        assert_eq!(stack.merge(), ConfigToml::default());
    }

    #[test]
    fn save_and_load_user_config() {
        let (_tmp, svc) = setup();
        let cfg = ConfigToml {
            model: Some("gpt-4o".into()),
            ..Default::default()
        };
        svc.save_user_config(&cfg).unwrap();
        let stack = svc.load_layers(None).unwrap();
        assert_eq!(stack.merge().model, Some("gpt-4o".into()));
    }

    #[test]
    fn project_overrides_user() {
        let (tmp, svc) = setup();
        let user_cfg = ConfigToml {
            model: Some("user-model".into()),
            ..Default::default()
        };
        svc.save_user_config(&user_cfg).unwrap();

        let project_root = tmp.path().join("my-project");
        let project_cfg = ConfigToml {
            model: Some("project-model".into()),
            ..Default::default()
        };
        svc.save_project_config(&project_root, &project_cfg).unwrap();

        let stack = svc.load_layers(Some(&project_root)).unwrap();
        assert_eq!(stack.merge().model, Some("project-model".into()));
    }

    #[test]
    fn read_value_extracts_key() {
        let (_tmp, svc) = setup();
        let cfg = ConfigToml {
            model: Some("test-model".into()),
            ..Default::default()
        };
        svc.save_user_config(&cfg).unwrap();
        let stack = svc.load_layers(None).unwrap();
        let val = svc.read_value(&stack, "model");
        assert_eq!(val, Some(serde_json::json!("test-model")));
    }

    #[test]
    fn find_codex_home_returns_some() {
        // Should at least return ~/.codex when HOME is set
        let home = ConfigService::find_codex_home();
        assert!(home.is_some());
    }
}

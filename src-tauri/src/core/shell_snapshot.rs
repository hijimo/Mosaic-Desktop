//! Shell environment snapshot — captures the shell's exported variables and cwd
//! so that commands can be executed with the user's environment context.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use tracing::warn;

use super::shell::{Shell, ShellType};

/// Snapshot of the shell environment at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSnapshot {
    /// Path to the snapshot file on disk (if persisted).
    pub path: Option<PathBuf>,
    /// Working directory at snapshot time.
    pub cwd: PathBuf,
    /// Exported environment variables.
    pub env: HashMap<String, String>,
}

const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const SNAPSHOT_DIR: &str = "shell_snapshots";

/// Variables excluded from snapshots (they change per-session).
const EXCLUDED_VARS: &[&str] = &["PWD", "OLDPWD", "_", "SHLVL"];

impl ShellSnapshot {
    /// Capture a snapshot of the current shell environment.
    pub async fn capture(cwd: &Path, shell: &Shell) -> Result<Self, String> {
        let script = match shell.shell_type {
            ShellType::Zsh | ShellType::Bash | ShellType::Sh => "env -0",
            ShellType::PowerShell => {
                "Get-ChildItem Env: | ForEach-Object { \"$($_.Name)=$($_.Value)\" }"
            }
            ShellType::Cmd => "set",
        };

        let args = shell.exec_args(script, false);
        let program = args.first().ok_or("empty args")?;

        let output = tokio::time::timeout(
            SNAPSHOT_TIMEOUT,
            Command::new(program)
                .args(&args[1..])
                .current_dir(cwd)
                .output(),
        )
        .await
        .map_err(|_| "snapshot timeout".to_string())?
        .map_err(|e| format!("snapshot exec failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "snapshot command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let env = parse_env(&stdout, shell.shell_type);

        Ok(Self {
            path: None,
            cwd: cwd.to_path_buf(),
            env,
        })
    }

    /// Persist the snapshot to disk under `mosaic_home/shell_snapshots/`.
    pub async fn persist(&mut self, mosaic_home: &Path, session_id: &str) -> Result<(), String> {
        let dir = mosaic_home.join(SNAPSHOT_DIR);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| format!("create snapshot dir: {e}"))?;

        let filename = format!("{session_id}.env");
        let path = dir.join(&filename);

        let content: String = self
            .env
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n");

        tokio::fs::write(&path, content)
            .await
            .map_err(|e| format!("write snapshot: {e}"))?;

        self.path = Some(path);
        Ok(())
    }

    /// Load a snapshot from a persisted file.
    pub async fn load(path: &Path) -> Result<Self, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("read snapshot: {e}"))?;

        let mut env = HashMap::new();
        for line in content.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if !EXCLUDED_VARS.contains(&k) {
                    env.insert(k.to_string(), v.to_string());
                }
            }
        }

        Ok(Self {
            path: Some(path.to_path_buf()),
            cwd: env
                .get("PWD")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/")),
            env,
        })
    }

    /// Clean up old snapshots beyond the retention period.
    pub async fn cleanup(mosaic_home: &Path, retention: Duration) {
        let dir = mosaic_home.join(SNAPSHOT_DIR);
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return;
        };
        let cutoff = std::time::SystemTime::now() - retention;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        let _ = tokio::fs::remove_file(entry.path()).await;
                    }
                }
            }
        }
    }
}

fn parse_env(stdout: &str, shell_type: ShellType) -> HashMap<String, String> {
    let mut env = HashMap::new();

    match shell_type {
        ShellType::Zsh | ShellType::Bash | ShellType::Sh => {
            // `env -0` uses NUL separators
            for entry in stdout.split('\0') {
                if let Some((k, v)) = entry.split_once('=') {
                    if !k.is_empty() && !EXCLUDED_VARS.contains(&k) {
                        env.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        ShellType::PowerShell | ShellType::Cmd => {
            for line in stdout.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    if !k.is_empty() && !EXCLUDED_VARS.contains(&k) {
                        env.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
    }

    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_nul_separated() {
        let input = "HOME=/Users/test\0PATH=/usr/bin\0PWD=/tmp\0";
        let env = parse_env(input, ShellType::Bash);
        assert_eq!(env.get("HOME").unwrap(), "/Users/test");
        assert_eq!(env.get("PATH").unwrap(), "/usr/bin");
        assert!(!env.contains_key("PWD")); // excluded
    }

    #[test]
    fn parse_env_newline_separated() {
        let input = "HOME=/Users/test\nPATH=/usr/bin\nOLDPWD=/old\n";
        let env = parse_env(input, ShellType::PowerShell);
        assert_eq!(env.get("HOME").unwrap(), "/Users/test");
        assert!(!env.contains_key("OLDPWD")); // excluded
    }

    #[tokio::test]
    async fn persist_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut snapshot = ShellSnapshot {
            path: None,
            cwd: PathBuf::from("/tmp"),
            env: HashMap::from([
                ("HOME".into(), "/Users/test".into()),
                ("LANG".into(), "en_US.UTF-8".into()),
            ]),
        };

        snapshot.persist(tmp.path(), "test-session").await.unwrap();
        assert!(snapshot.path.is_some());

        let loaded = ShellSnapshot::load(snapshot.path.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(loaded.env.get("HOME").unwrap(), "/Users/test");
        assert_eq!(loaded.env.get("LANG").unwrap(), "en_US.UTF-8");
    }
}

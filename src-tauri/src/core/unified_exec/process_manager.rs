//! Process lifecycle management with output buffering and limits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::warn;
use uuid::Uuid;

use super::{ExecCommand, EXEC_ENV, MAX_PROCESSES, OUTPUT_MAX_BYTES};

/// Result of a completed command execution.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Handle to a running managed process.
#[derive(Debug)]
pub struct ProcessHandle {
    pub id: String,
    pub child: Child,
    pub command: Vec<String>,
    pub cwd: PathBuf,
}

/// Manages a pool of child processes with limits and output capture.
pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, ProcessHandle>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute a command and wait for completion, capturing output.
    pub async fn exec(&self, cmd: &ExecCommand) -> Result<ExecResult, String> {
        let processes = self.processes.lock().await;
        if processes.len() >= MAX_PROCESSES {
            return Err(format!("process limit reached ({MAX_PROCESSES})"));
        }
        drop(processes);

        let program = cmd.command.first().ok_or("empty command")?;
        let args = &cmd.command[1..];

        let mut child = Command::new(program)
            .args(args)
            .current_dir(&cmd.cwd)
            .envs(EXEC_ENV.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let read_stream = |mut stream: Option<tokio::process::ChildStdout>| async move {
            let Some(ref mut s) = stream else {
                return String::new();
            };
            let mut buf = Vec::with_capacity(4096);
            let _ = s.read_to_end(&mut buf).await;
            if buf.len() > OUTPUT_MAX_BYTES {
                buf.truncate(OUTPUT_MAX_BYTES);
            }
            String::from_utf8_lossy(&buf).into_owned()
        };

        let read_stderr = |mut stream: Option<tokio::process::ChildStderr>| async move {
            let Some(ref mut s) = stream else {
                return String::new();
            };
            let mut buf = Vec::with_capacity(4096);
            let _ = s.read_to_end(&mut buf).await;
            if buf.len() > OUTPUT_MAX_BYTES {
                buf.truncate(OUTPUT_MAX_BYTES);
            }
            String::from_utf8_lossy(&buf).into_owned()
        };

        let (stdout_str, stderr_str) =
            tokio::join!(read_stream(stdout_handle), read_stderr(stderr_handle));

        let wait_result = if let Some(t) = cmd.timeout {
            timeout(t, child.wait()).await.map_err(|_| "timeout".to_string())?
        } else {
            child.wait().await
        };

        let status = wait_result.map_err(|e| format!("wait failed: {e}"))?;

        Ok(ExecResult {
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_str,
            stderr: stderr_str,
        })
    }

    /// Spawn a long-running background process.
    pub async fn spawn(&self, cmd: &ExecCommand) -> Result<String, String> {
        let mut processes = self.processes.lock().await;
        if processes.len() >= MAX_PROCESSES {
            return Err(format!("process limit reached ({MAX_PROCESSES})"));
        }

        let program = cmd.command.first().ok_or("empty command")?;
        let args = &cmd.command[1..];

        let child = Command::new(program)
            .args(args)
            .current_dir(&cmd.cwd)
            .envs(EXEC_ENV.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;

        let id = Uuid::new_v4().to_string();
        processes.insert(
            id.clone(),
            ProcessHandle {
                id: id.clone(),
                child,
                command: cmd.command.clone(),
                cwd: cmd.cwd.clone(),
            },
        );
        Ok(id)
    }

    /// Kill a managed process by ID.
    pub async fn kill(&self, id: &str) -> Result<(), String> {
        let mut processes = self.processes.lock().await;
        let Some(mut handle) = processes.remove(id) else {
            return Err(format!("process {id} not found"));
        };
        handle
            .child
            .kill()
            .await
            .map_err(|e| format!("kill failed: {e}"))
    }

    /// List active process IDs.
    pub async fn list(&self) -> Vec<String> {
        self.processes.lock().await.keys().cloned().collect()
    }

    /// Number of active processes.
    pub async fn count(&self) -> usize {
        self.processes.lock().await.len()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exec_echo() {
        let mgr = ProcessManager::new();
        let result = mgr
            .exec(&ExecCommand {
                command: vec!["echo".into(), "hello".into()],
                cwd: std::env::temp_dir(),
                timeout: Some(Duration::from_secs(5)),
            })
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.trim().contains("hello"));
    }

    #[tokio::test]
    async fn exec_failure() {
        let mgr = ProcessManager::new();
        let result = mgr
            .exec(&ExecCommand {
                command: vec!["false".into()],
                cwd: std::env::temp_dir(),
                timeout: Some(Duration::from_secs(5)),
            })
            .await
            .unwrap();
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn spawn_and_kill() {
        let mgr = ProcessManager::new();
        let id = mgr
            .spawn(&ExecCommand {
                command: vec!["sleep".into(), "60".into()],
                cwd: std::env::temp_dir(),
                timeout: None,
            })
            .await
            .unwrap();
        assert_eq!(mgr.count().await, 1);
        mgr.kill(&id).await.unwrap();
        assert_eq!(mgr.count().await, 0);
    }

    #[tokio::test]
    async fn kill_nonexistent() {
        let mgr = ProcessManager::new();
        assert!(mgr.kill("nonexistent").await.is_err());
    }
}

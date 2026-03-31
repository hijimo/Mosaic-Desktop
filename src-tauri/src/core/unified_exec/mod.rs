//! Unified execution engine — manages interactive processes with output buffering,
//! approval orchestration, and sandboxing.
//!
//! Flow: build request → spawn process → stream output → collect response.

pub mod async_watcher;
pub mod errors;
pub mod head_tail_buffer;
pub mod process;
pub mod process_manager;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

pub use errors::UnifiedExecError;
pub use head_tail_buffer::HeadTailBuffer;
pub use process::UnifiedExecProcess;

// ── Constants ────────────────────────────────────────────────────

pub const MAX_PROCESSES: usize = 64;
pub const WARNING_PROCESSES: usize = 60;
pub const MIN_YIELD_TIME_MS: u64 = 250;
pub const MAX_YIELD_TIME_MS: u64 = 30_000;
pub const MIN_EMPTY_YIELD_TIME_MS: u64 = 5_000;
pub const DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_MAX_OUTPUT_TOKENS: usize = 10_000;
pub const OUTPUT_MAX_BYTES: usize = 1024 * 1024; // 1 MiB
pub const OUTPUT_MAX_TOKENS: usize = OUTPUT_MAX_BYTES / 4;

/// Environment variables set for all managed processes.
pub const EXEC_ENV: [(&str, &str); 10] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("LANG", "C.UTF-8"),
    ("LC_CTYPE", "C.UTF-8"),
    ("LC_ALL", "C.UTF-8"),
    ("COLORTERM", ""),
    ("PAGER", "cat"),
    ("GIT_PAGER", "cat"),
    ("GH_PAGER", "cat"),
    ("MOSAIC_CI", "1"),
];

// ── Request / Response types ─────────────────────────────────────

/// A command execution request (simple, non-interactive).
#[derive(Debug, Clone)]
pub struct ExecCommand {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Option<Duration>,
}

/// Full unified exec command request (interactive PTY sessions).
#[derive(Debug)]
pub struct ExecCommandRequest {
    pub command: Vec<String>,
    pub process_id: String,
    pub yield_time_ms: u64,
    pub max_output_tokens: Option<usize>,
    pub workdir: Option<PathBuf>,
    pub tty: bool,
    pub justification: Option<String>,
    pub prefix_rule: Option<Vec<String>>,
}

/// Write to stdin of an existing process.
#[derive(Debug)]
pub struct WriteStdinRequest<'a> {
    pub process_id: &'a str,
    pub input: &'a str,
    pub yield_time_ms: u64,
    pub max_output_tokens: Option<usize>,
}

/// Response from a unified exec operation.
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedExecResponse {
    pub event_call_id: String,
    pub chunk_id: String,
    pub wall_time: Duration,
    pub output: String,
    pub raw_output: Vec<u8>,
    pub process_id: Option<String>,
    pub exit_code: Option<i32>,
    pub original_token_count: Option<usize>,
    pub session_command: Option<Vec<String>>,
}

// ── Process store ────────────────────────────────────────────────

/// Tracks active processes and reserved IDs.
#[derive(Default)]
pub struct ProcessStore {
    pub processes: HashMap<String, ProcessEntry>,
    pub reserved_process_ids: HashSet<String>,
}

impl ProcessStore {
    pub fn remove(&mut self, process_id: &str) -> Option<ProcessEntry> {
        self.reserved_process_ids.remove(process_id);
        self.processes.remove(process_id)
    }
}

/// Entry for a managed process in the store.
pub struct ProcessEntry {
    pub process: Arc<UnifiedExecProcess>,
    pub call_id: String,
    pub process_id: String,
    pub command: Vec<String>,
    pub tty: bool,
    pub last_used: tokio::time::Instant,
}

// ── Process Manager ──────────────────────────────────────────────

/// Manages the lifecycle of unified exec processes.
pub struct UnifiedExecProcessManager {
    pub(crate) process_store: Mutex<ProcessStore>,
    pub(crate) max_write_stdin_yield_time_ms: u64,
}

impl UnifiedExecProcessManager {
    pub fn new(max_write_stdin_yield_time_ms: u64) -> Self {
        Self {
            process_store: Mutex::new(ProcessStore::default()),
            max_write_stdin_yield_time_ms: max_write_stdin_yield_time_ms
                .max(MIN_EMPTY_YIELD_TIME_MS),
        }
    }
}

impl Default for UnifiedExecProcessManager {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS)
    }
}

// ── Legacy ProcessManager (simple exec without PTY) ──────────────

/// Simple process manager for non-interactive commands.
pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, ProcessHandle>>>,
}

/// Handle to a running managed process (legacy, non-PTY).
#[derive(Debug)]
pub struct ProcessHandle {
    pub id: String,
    pub child: tokio::process::Child,
    pub command: Vec<String>,
    pub cwd: PathBuf,
}

/// Result of a completed command execution.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Alias for legacy code.
pub type ProcessMgrHandle = ProcessHandle;

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn exec(&self, cmd: &ExecCommand) -> Result<ExecResult, String> {
        let processes = self.processes.lock().await;
        if processes.len() >= MAX_PROCESSES {
            return Err(format!("process limit reached ({MAX_PROCESSES})"));
        }
        drop(processes);

        let program = cmd.command.first().ok_or("empty command")?;
        let args = &cmd.command[1..];

        let mut child = tokio::process::Command::new(program)
            .args(args)
            .current_dir(&cmd.cwd)
            .envs(EXEC_ENV.iter().copied())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let read_stdout = async move {
            let Some(mut s) = stdout_handle else {
                return String::new();
            };
            let mut buf = Vec::with_capacity(4096);
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut s, &mut buf).await;
            if buf.len() > OUTPUT_MAX_BYTES {
                buf.truncate(OUTPUT_MAX_BYTES);
            }
            String::from_utf8_lossy(&buf).into_owned()
        };
        let read_stderr = async move {
            let Some(mut s) = stderr_handle else {
                return String::new();
            };
            let mut buf = Vec::with_capacity(4096);
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut s, &mut buf).await;
            if buf.len() > OUTPUT_MAX_BYTES {
                buf.truncate(OUTPUT_MAX_BYTES);
            }
            String::from_utf8_lossy(&buf).into_owned()
        };

        let (stdout_str, stderr_str) = tokio::join!(read_stdout, read_stderr);

        let wait_result = if let Some(t) = cmd.timeout {
            tokio::time::timeout(t, child.wait())
                .await
                .map_err(|_| "timeout".to_string())?
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

    pub async fn spawn(&self, cmd: &ExecCommand) -> Result<String, String> {
        let mut processes = self.processes.lock().await;
        if processes.len() >= MAX_PROCESSES {
            return Err(format!("process limit reached ({MAX_PROCESSES})"));
        }
        let program = cmd.command.first().ok_or("empty command")?;
        let args = &cmd.command[1..];
        let child = tokio::process::Command::new(program)
            .args(args)
            .current_dir(&cmd.cwd)
            .envs(EXEC_ENV.iter().copied())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;
        let id = uuid::Uuid::new_v4().to_string();
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

    pub async fn list(&self) -> Vec<String> {
        self.processes.lock().await.keys().cloned().collect()
    }

    pub async fn count(&self) -> usize {
        self.processes.lock().await.len()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ──────────────────────────────────────────────────────

pub fn clamp_yield_time(yield_time_ms: u64) -> u64 {
    yield_time_ms.clamp(MIN_YIELD_TIME_MS, MAX_YIELD_TIME_MS)
}

pub fn resolve_max_tokens(max_tokens: Option<usize>) -> usize {
    max_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
}

pub fn generate_chunk_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..6)
        .map(|_| format!("{:x}", rng.random_range(0..16u8)))
        .collect()
}

pub fn apply_exec_env(mut env: HashMap<String, String>) -> HashMap<String, String> {
    for (key, value) in EXEC_ENV {
        env.insert(key.to_string(), value.to_string());
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn legacy_exec_echo() {
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
    async fn legacy_exec_failure() {
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
    async fn legacy_spawn_and_kill() {
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
    async fn legacy_kill_nonexistent() {
        let mgr = ProcessManager::new();
        assert!(mgr.kill("nonexistent").await.is_err());
    }

    #[test]
    fn exec_env_injects_defaults() {
        let env = apply_exec_env(HashMap::new());
        assert_eq!(env.get("NO_COLOR"), Some(&"1".to_string()));
        assert_eq!(env.get("MOSAIC_CI"), Some(&"1".to_string()));
        assert_eq!(env.len(), 10);
    }

    #[test]
    fn exec_env_overrides_existing() {
        let mut base = HashMap::new();
        base.insert("NO_COLOR".to_string(), "0".to_string());
        base.insert("PATH".to_string(), "/usr/bin".to_string());
        let env = apply_exec_env(base);
        assert_eq!(env.get("NO_COLOR"), Some(&"1".to_string()));
        assert_eq!(env.get("PATH"), Some(&"/usr/bin".to_string()));
    }
}

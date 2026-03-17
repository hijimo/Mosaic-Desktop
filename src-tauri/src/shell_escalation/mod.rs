//! Unix shell-escalation protocol implementation.
//!
//! A patched shell invokes an exec wrapper on every `exec()` attempt. The wrapper sends an
//! `EscalateRequest` over the inherited `CODEX_ESCALATE_SOCKET`, and the server decides whether to
//! run the command directly (`Run`) or execute it on the server side (`Escalate`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── Environment variable names ───────────────────────────────────

/// Exec wrappers read this to find the inherited FD for the escalation socket.
pub const ESCALATE_SOCKET_ENV_VAR: &str = "CODEX_ESCALATE_SOCKET";

/// Patched shells use this to wrap exec() calls.
pub const EXEC_WRAPPER_ENV_VAR: &str = "EXEC_WRAPPER";

/// Compatibility alias for older patched bash builds.
pub const LEGACY_BASH_EXEC_WRAPPER_ENV_VAR: &str = "BASH_EXEC_WRAPPER";

// ── Protocol messages ────────────────────────────────────────────

/// The client sends this to the server to request an exec() call.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct EscalateRequest {
    pub file: PathBuf,
    pub argv: Vec<String>,
    pub workdir: PathBuf,
    pub env: HashMap<String, String>,
}

/// The server sends this to the client to respond to an exec() request.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct EscalateResponse {
    pub action: EscalateAction,
}

/// Action the server tells the client to take.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum EscalateAction {
    Run,
    Escalate,
    Deny { reason: Option<String> },
}

/// The server's internal decision for an escalation request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EscalationDecision {
    Run,
    Escalate(EscalationExecution),
    Deny { reason: Option<String> },
}

impl EscalationDecision {
    pub fn run() -> Self {
        Self::Run
    }
    pub fn escalate(execution: EscalationExecution) -> Self {
        Self::Escalate(execution)
    }
    pub fn deny(reason: Option<String>) -> Self {
        Self::Deny { reason }
    }
}

/// How an escalated command should be re-executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EscalationExecution {
    Unsandboxed,
    TurnDefault,
}

// ── Policy trait ─────────────────────────────────────────────────

/// Decides what action to take in response to an execve request from a client.
#[async_trait::async_trait]
pub trait EscalationPolicy: Send + Sync {
    async fn determine_action(
        &self,
        file: &Path,
        argv: &[String],
        workdir: &Path,
    ) -> anyhow::Result<EscalationDecision>;
}

// ── Exec types ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecParams {
    pub command: String,
    pub workdir: String,
    pub timeout_ms: Option<u64>,
    pub login: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub output: String,
    pub duration: Duration,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedExec {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub arg0: Option<String>,
}

// ── Stopwatch ────────────────────────────────────────────────────

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

/// A pausable stopwatch that fires a `CancellationToken` after a time limit.
#[derive(Clone, Debug)]
pub struct Stopwatch {
    limit: Duration,
    inner: Arc<Mutex<StopwatchState>>,
    notify: Arc<Notify>,
}

#[derive(Debug)]
struct StopwatchState {
    elapsed: Duration,
    running_since: Option<Instant>,
    active_pauses: u32,
}

impl Stopwatch {
    pub fn new(limit: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StopwatchState {
                elapsed: Duration::ZERO,
                running_since: Some(Instant::now()),
                active_pauses: 0,
            })),
            notify: Arc::new(Notify::new()),
            limit,
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        let limit = self.limit;
        let token = CancellationToken::new();
        let cancel = token.clone();
        let inner = Arc::clone(&self.inner);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(async move {
            loop {
                let (remaining, running) = {
                    let guard = inner.lock().await;
                    let elapsed = guard.elapsed
                        + guard
                            .running_since
                            .map(|since| since.elapsed())
                            .unwrap_or_default();
                    if elapsed >= limit {
                        break;
                    }
                    (limit - elapsed, guard.running_since.is_some())
                };
                if !running {
                    notify.notified().await;
                    continue;
                }
                let sleep = tokio::time::sleep(remaining);
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => break,
                    _ = notify.notified() => continue,
                }
            }
            cancel.cancel();
        });
        token
    }

    pub async fn pause_for<F: std::future::Future>(&self, fut: F) -> F::Output {
        self.pause().await;
        let result = fut.await;
        self.resume().await;
        result
    }

    async fn pause(&self) {
        let mut guard = self.inner.lock().await;
        guard.active_pauses += 1;
        if guard.active_pauses == 1 {
            if let Some(since) = guard.running_since.take() {
                guard.elapsed += since.elapsed();
                self.notify.notify_waiters();
            }
        }
    }

    async fn resume(&self) {
        let mut guard = self.inner.lock().await;
        if guard.active_pauses == 0 {
            return;
        }
        guard.active_pauses -= 1;
        if guard.active_pauses == 0 && guard.running_since.is_none() {
            guard.running_since = Some(Instant::now());
            self.notify.notify_waiters();
        }
    }
}

// ── FD message types (for the super-exec protocol) ───────────────

/// The client sends this to the server to forward its open FDs.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SuperExecMessage {
    pub fds: Vec<i32>,
}

/// The server responds when the exec()'d command has exited.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SuperExecResult {
    pub exit_code: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_decision_constructors() {
        assert_eq!(EscalationDecision::run(), EscalationDecision::Run);
        assert_eq!(
            EscalationDecision::deny(Some("nope".into())),
            EscalationDecision::Deny {
                reason: Some("nope".into())
            }
        );
        assert_eq!(
            EscalationDecision::escalate(EscalationExecution::Unsandboxed),
            EscalationDecision::Escalate(EscalationExecution::Unsandboxed)
        );
    }

    #[test]
    fn escalate_request_round_trips_json() {
        let req = EscalateRequest {
            file: PathBuf::from("/bin/echo"),
            argv: vec!["echo".into(), "hello".into()],
            workdir: PathBuf::from("/tmp"),
            env: HashMap::from([("KEY".into(), "VALUE".into())]),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: EscalateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn escalate_action_deny_serializes() {
        let action = EscalateAction::Deny {
            reason: Some("blocked".into()),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: EscalateAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, decoded);
    }

    #[tokio::test]
    async fn stopwatch_fires_after_limit() {
        let sw = Stopwatch::new(Duration::from_millis(50));
        let token = sw.cancellation_token();
        let start = tokio::time::Instant::now();
        token.cancelled().await;
        assert!(start.elapsed() >= Duration::from_millis(50));
    }
}

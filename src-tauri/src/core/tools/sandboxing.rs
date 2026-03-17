//! Sandboxing and approval primitives for tool runtimes.
//!
//! Adapted from Codex `tools/sandboxing.rs`. Provides `ApprovalStore`,
//! `ToolRuntime` trait, and `SandboxAttempt` for sandbox orchestration.

use serde::Serialize;
use std::collections::HashMap;

use crate::protocol::error::CodexError;

/// Cached approval decisions keyed by serialized request.
#[derive(Clone, Default, Debug)]
pub struct ApprovalStore {
    map: HashMap<String, ApprovalDecision>,
}

/// Possible approval decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    ApprovedForSession,
    Denied,
}

impl ApprovalStore {
    pub fn get<K: Serialize>(&self, key: &K) -> Option<&ApprovalDecision> {
        let s = serde_json::to_string(key).ok()?;
        self.map.get(&s)
    }

    pub fn put<K: Serialize>(&mut self, key: K, value: ApprovalDecision) {
        if let Ok(s) = serde_json::to_string(&key) {
            self.map.insert(s, value);
        }
    }
}

/// Approval requirement for executing a tool.
#[derive(Debug, Clone)]
pub enum ExecApprovalRequirement {
    Skip,
    Forbidden { reason: String },
    NeedsApproval { reason: Option<String> },
}

/// Error type for tool execution failures.
#[derive(Debug)]
pub enum ToolError {
    Codex(CodexError),
    Rejected(String),
}

impl From<CodexError> for ToolError {
    fn from(e: CodexError) -> Self {
        Self::Codex(e)
    }
}

/// Sandbox type for a tool execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxType {
    None,
    Default,
}

/// Context for a single sandbox attempt.
pub struct SandboxAttempt {
    pub sandbox: SandboxType,
}

/// Trait for tool runtimes that support sandbox orchestration.
#[async_trait::async_trait]
pub trait ToolRuntime: Send + Sync {
    /// Execute the tool within the given sandbox attempt.
    async fn run(
        &self,
        args: serde_json::Value,
        attempt: &SandboxAttempt,
    ) -> Result<serde_json::Value, ToolError>;

    /// Whether to retry with an escalated sandbox on failure.
    fn escalate_on_failure(&self) -> bool {
        false
    }
}

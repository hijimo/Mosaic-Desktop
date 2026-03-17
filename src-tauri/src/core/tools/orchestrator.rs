//! Tool orchestrator — approval + sandbox + retry.
//!
//! Adapted from Codex `tools/orchestrator.rs`. Simplified: no real sandbox
//! manager or network proxy; provides the structural skeleton for future
//! integration.

use crate::protocol::error::{CodexError, ErrorCode};

/// Approval requirement for a tool execution.
#[derive(Debug, Clone)]
pub enum ExecApprovalRequirement {
    /// No approval needed.
    Skip,
    /// Execution is forbidden.
    Forbidden { reason: String },
    /// User approval is required.
    NeedsApproval { reason: Option<String> },
}

/// Result of an orchestrated tool run.
pub struct OrchestratorRunResult {
    pub output: serde_json::Value,
    pub approved: bool,
}

/// Central orchestrator for tool execution with approval and sandbox support.
pub struct ToolOrchestrator;

impl ToolOrchestrator {
    pub fn new() -> Self {
        Self
    }

    /// Run a tool with approval checks.
    ///
    /// In the full implementation this would:
    /// 1. Check approval requirement
    /// 2. Select sandbox strategy
    /// 3. Execute the tool
    /// 4. Retry with escalated sandbox on denial
    pub async fn run(
        &self,
        tool_name: &str,
        args: serde_json::Value,
        requirement: ExecApprovalRequirement,
        execute: impl std::future::Future<Output = Result<serde_json::Value, CodexError>>,
    ) -> Result<OrchestratorRunResult, CodexError> {
        match requirement {
            ExecApprovalRequirement::Forbidden { reason } => {
                return Err(CodexError::new(ErrorCode::ToolExecutionFailed, reason));
            }
            ExecApprovalRequirement::NeedsApproval { reason } => {
                // In the full implementation, this would prompt the user.
                // For now, auto-approve.
                let _ = reason;
            }
            ExecApprovalRequirement::Skip => {}
        }

        let output = execute.await?;
        Ok(OrchestratorRunResult {
            output,
            approved: true,
        })
    }
}

impl Default for ToolOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct MultiAgentHandler;

pub const MIN_WAIT_TIMEOUT_MS: i64 = 10_000;
pub const DEFAULT_WAIT_TIMEOUT_MS: i64 = 30_000;
pub const MAX_WAIT_TIMEOUT_MS: i64 = 3600 * 1000;

#[derive(Debug, Deserialize)]
struct SpawnAgentArgs {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    items: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    fork_context: bool,
}

#[derive(Debug, Deserialize)]
struct SendInputArgs {
    id: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    items: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ResumeAgentArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
struct WaitArgs {
    #[serde(default)]
    agent_ids: Option<Vec<String>>,
    #[serde(default)]
    timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CloseAgentArgs {
    id: String,
}

#[derive(Debug, Serialize)]
struct SpawnAgentResult {
    id: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct AgentStatusEntry {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_output: Option<String>,
}

#[async_trait]
impl ToolHandler for MultiAgentHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if matches!(n.as_str(),
            "spawn_agent" | "send_input" | "resume_agent" | "wait" | "close_agent"
        ))
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("spawn_agent".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Determine tool name from args or context
        // In the full implementation, tool_name comes from ToolInvocation
        let tool_name = args.get("__tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("spawn_agent");

        match tool_name {
            "spawn_agent" => {
                let _params: SpawnAgentArgs = serde_json::from_value(args).map_err(|e| {
                    CodexError::new(ErrorCode::InvalidInput, format!("invalid spawn_agent args: {e}"))
                })?;
                // Full implementation: check thread depth limit, build agent config, spawn
                // TODO: wire to AgentControl system
                Err(CodexError::new(ErrorCode::ToolExecutionFailed, "spawn_agent requires the agent subsystem"))
            }
            "send_input" => {
                let _params: SendInputArgs = serde_json::from_value(args).map_err(|e| {
                    CodexError::new(ErrorCode::InvalidInput, format!("invalid send_input args: {e}"))
                })?;
                Err(CodexError::new(ErrorCode::ToolExecutionFailed, "send_input requires the agent subsystem"))
            }
            "resume_agent" => {
                let _params: ResumeAgentArgs = serde_json::from_value(args).map_err(|e| {
                    CodexError::new(ErrorCode::InvalidInput, format!("invalid resume_agent args: {e}"))
                })?;
                Err(CodexError::new(ErrorCode::ToolExecutionFailed, "resume_agent requires the agent subsystem"))
            }
            "wait" => {
                let params: WaitArgs = serde_json::from_value(args).map_err(|e| {
                    CodexError::new(ErrorCode::InvalidInput, format!("invalid wait args: {e}"))
                })?;
                let timeout = params.timeout_ms.unwrap_or(DEFAULT_WAIT_TIMEOUT_MS);
                if timeout < MIN_WAIT_TIMEOUT_MS {
                    return Err(CodexError::new(
                        ErrorCode::InvalidInput,
                        format!("timeout_ms must be at least {MIN_WAIT_TIMEOUT_MS}"),
                    ));
                }
                if timeout > MAX_WAIT_TIMEOUT_MS {
                    return Err(CodexError::new(
                        ErrorCode::InvalidInput,
                        format!("timeout_ms must be at most {MAX_WAIT_TIMEOUT_MS}"),
                    ));
                }
                Err(CodexError::new(ErrorCode::ToolExecutionFailed, "wait requires the agent subsystem"))
            }
            "close_agent" => {
                let _params: CloseAgentArgs = serde_json::from_value(args).map_err(|e| {
                    CodexError::new(ErrorCode::InvalidInput, format!("invalid close_agent args: {e}"))
                })?;
                Err(CodexError::new(ErrorCode::ToolExecutionFailed, "close_agent requires the agent subsystem"))
            }
            other => Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("unsupported collab tool {other}"),
            )),
        }
    }
}

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::core::agent::control::{AgentControl, SpawnAgentOptions};
use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::AgentStatus;

pub const MIN_WAIT_TIMEOUT_MS: i64 = 10_000;
pub const DEFAULT_WAIT_TIMEOUT_MS: i64 = 30_000;
pub const MAX_WAIT_TIMEOUT_MS: i64 = 3600 * 1000;

// ── Args ─────────────────────────────────────────────────────────

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

// ── Results ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct SpawnAgentResult {
    id: String,
    nickname: String,
    status: String,
}

#[derive(Debug, Serialize)]
pub struct AgentStatusEntry {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_output: Option<String>,
}

// ── Handler ──────────────────────────────────────────────────────

pub struct MultiAgentHandler {
    agent_control: Arc<AgentControl>,
    /// Current depth of the calling agent (0 for root).
    current_depth: usize,
}

impl MultiAgentHandler {
    pub fn new(agent_control: Arc<AgentControl>, current_depth: usize) -> Self {
        Self { agent_control, current_depth }
    }
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
        let tool_name = args.get("__tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("spawn_agent");

        match tool_name {
            "spawn_agent" => self.handle_spawn(args).await,
            "send_input" => self.handle_send_input(args).await,
            "resume_agent" => self.handle_resume(args).await,
            "wait" => self.handle_wait(args).await,
            "close_agent" => self.handle_close(args).await,
            other => Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("unsupported collab tool {other}"),
            )),
        }
    }
}

impl MultiAgentHandler {
    async fn handle_spawn(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: SpawnAgentArgs = serde_json::from_value(args)
            .map_err(|e| CodexError::new(ErrorCode::InvalidInput, format!("invalid spawn_agent args: {e}")))?;

        // Validate: message and items are mutually exclusive
        if params.message.is_some() && params.items.is_some() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "spawn_agent: provide either 'message' or 'items', not both",
            ));
        }
        let prompt = params.message
            .or_else(|| params.items.map(|items| serde_json::to_string(&items).unwrap_or_default()))
            .unwrap_or_default();
        if prompt.trim().is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "spawn_agent: message or items must be non-empty",
            ));
        }

        let options = SpawnAgentOptions {
            fork: params.fork_context,
            agent_type: params.agent_type,
            ..Default::default()
        };

        let (instance, guards) = self.agent_control
            .spawn_agent(options, self.current_depth)
            .await?;

        let result = SpawnAgentResult {
            id: instance.thread_id.clone(),
            nickname: guards.nickname,
            status: "running".into(),
        };
        serde_json::to_value(result)
            .map_err(|e| CodexError::new(ErrorCode::ToolExecutionFailed, e.to_string()))
    }

    async fn handle_send_input(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: SendInputArgs = serde_json::from_value(args)
            .map_err(|e| CodexError::new(ErrorCode::InvalidInput, format!("invalid send_input args: {e}")))?;

        if params.message.is_some() && params.items.is_some() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "send_input: provide either 'message' or 'items', not both",
            ));
        }
        let text = params.message
            .or_else(|| params.items.map(|items| serde_json::to_string(&items).unwrap_or_default()))
            .unwrap_or_default();
        if text.trim().is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "send_input: message or items must be non-empty",
            ));
        }

        let input = crate::protocol::types::UserInput::Text {
            text,
            text_elements: vec![],
        };
        self.agent_control.send_input(&params.id, input).await?;

        Ok(serde_json::json!({ "status": "sent" }))
    }

    async fn handle_resume(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: ResumeAgentArgs = serde_json::from_value(args)
            .map_err(|e| CodexError::new(ErrorCode::InvalidInput, format!("invalid resume_agent args: {e}")))?;

        self.agent_control.resume_agent(&params.id).await?;
        Ok(serde_json::json!({ "status": "resumed" }))
    }

    async fn handle_wait(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: WaitArgs = serde_json::from_value(args)
            .map_err(|e| CodexError::new(ErrorCode::InvalidInput, format!("invalid wait args: {e}")))?;

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

        let agent_ids = params.agent_ids.unwrap_or_default();
        if agent_ids.is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "wait: agent_ids must be non-empty",
            ));
        }

        let mut results = Vec::new();
        for id in &agent_ids {
            let wait_result = tokio::time::timeout(
                Duration::from_millis(timeout as u64),
                self.agent_control.wait(id),
            ).await;

            let entry = match wait_result {
                Ok(Ok(output)) => AgentStatusEntry {
                    id: id.clone(),
                    status: "completed".into(),
                    last_output: Some(output.to_string()),
                },
                Ok(Err(e)) => AgentStatusEntry {
                    id: id.clone(),
                    status: "errored".into(),
                    last_output: Some(e.to_string()),
                },
                Err(_) => AgentStatusEntry {
                    id: id.clone(),
                    status: "timed_out".into(),
                    last_output: None,
                },
            };
            results.push(entry);
        }

        serde_json::to_value(&results)
            .map_err(|e| CodexError::new(ErrorCode::ToolExecutionFailed, e.to_string()))
    }

    async fn handle_close(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: CloseAgentArgs = serde_json::from_value(args)
            .map_err(|e| CodexError::new(ErrorCode::InvalidInput, format!("invalid close_agent args: {e}")))?;

        self.agent_control.close_agent(&params.id).await?;
        Ok(serde_json::json!({ "status": "closed" }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::SandboxPolicy;
    use std::path::PathBuf;

    fn test_sandbox() -> SandboxPolicy {
        SandboxPolicy::DangerFullAccess
    }

    fn make_handler() -> MultiAgentHandler {
        let (tx, _rx) = async_channel::unbounded();
        let control = Arc::new(AgentControl::new(
            3, PathBuf::from("/tmp"), test_sandbox(), tx,
        ));
        MultiAgentHandler::new(control, 0)
    }

    #[tokio::test]
    async fn spawn_returns_id_and_nickname() {
        let handler = make_handler();
        let result = handler.handle(serde_json::json!({
            "__tool_name": "spawn_agent",
            "message": "do something"
        })).await.unwrap();
        assert!(result.get("id").is_some());
        assert!(result.get("nickname").is_some());
        assert_eq!(result["status"], "running");
    }

    #[tokio::test]
    async fn spawn_rejects_empty_message() {
        let handler = make_handler();
        let err = handler.handle(serde_json::json!({
            "__tool_name": "spawn_agent",
            "message": "  "
        })).await.unwrap_err();
        assert!(err.to_string().contains("non-empty"));
    }

    #[tokio::test]
    async fn spawn_rejects_both_message_and_items() {
        let handler = make_handler();
        let err = handler.handle(serde_json::json!({
            "__tool_name": "spawn_agent",
            "message": "hello",
            "items": [{"text": "world"}]
        })).await.unwrap_err();
        assert!(err.to_string().contains("not both"));
    }

    #[tokio::test]
    async fn send_input_to_spawned_agent() {
        let handler = make_handler();
        let spawn_result = handler.handle(serde_json::json!({
            "__tool_name": "spawn_agent",
            "message": "task"
        })).await.unwrap();
        let agent_id = spawn_result["id"].as_str().unwrap();

        let result = handler.handle(serde_json::json!({
            "__tool_name": "send_input",
            "id": agent_id,
            "message": "more input"
        })).await.unwrap();
        assert_eq!(result["status"], "sent");
    }

    #[tokio::test]
    async fn send_input_to_unknown_agent_errors() {
        let handler = make_handler();
        let err = handler.handle(serde_json::json!({
            "__tool_name": "send_input",
            "id": "nonexistent",
            "message": "hello"
        })).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn close_agent_succeeds() {
        let handler = make_handler();
        let spawn_result = handler.handle(serde_json::json!({
            "__tool_name": "spawn_agent",
            "message": "task"
        })).await.unwrap();
        let agent_id = spawn_result["id"].as_str().unwrap();

        let result = handler.handle(serde_json::json!({
            "__tool_name": "close_agent",
            "id": agent_id
        })).await.unwrap();
        assert_eq!(result["status"], "closed");
    }

    #[tokio::test]
    async fn wait_rejects_empty_ids() {
        let handler = make_handler();
        let err = handler.handle(serde_json::json!({
            "__tool_name": "wait",
            "agent_ids": []
        })).await.unwrap_err();
        assert!(err.to_string().contains("non-empty"));
    }

    #[tokio::test]
    async fn wait_rejects_low_timeout() {
        let handler = make_handler();
        let err = handler.handle(serde_json::json!({
            "__tool_name": "wait",
            "agent_ids": ["a"],
            "timeout_ms": 100
        })).await.unwrap_err();
        assert!(err.to_string().contains("at least"));
    }

    #[tokio::test]
    async fn spawn_respects_depth_limit() {
        let (tx, _rx) = async_channel::unbounded();
        let control = Arc::new(AgentControl::new(
            1, PathBuf::from("/tmp"), test_sandbox(), tx,
        ));
        // depth=1, max=1 → should fail
        let handler = MultiAgentHandler::new(control, 1);
        let err = handler.handle(serde_json::json!({
            "__tool_name": "spawn_agent",
            "message": "task"
        })).await.unwrap_err();
        assert!(err.to_string().contains("depth"));
    }

    #[tokio::test]
    async fn unsupported_tool_errors() {
        let handler = make_handler();
        let err = handler.handle(serde_json::json!({
            "__tool_name": "unknown_tool"
        })).await.unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }
}

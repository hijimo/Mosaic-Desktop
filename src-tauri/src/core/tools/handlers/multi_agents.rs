use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::core::agent::control::AgentControl;
use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::thread_id::ThreadId;
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
    config: crate::config::ConfigLayerStack,
}

impl MultiAgentHandler {
    pub fn new(agent_control: Arc<AgentControl>, config: crate::config::ConfigLayerStack) -> Self {
        Self {
            agent_control,
            config,
        }
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
        let tool_name = args
            .get("__tool_name")
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

fn parse_thread_id(id: &str) -> Result<ThreadId, CodexError> {
    id.try_into().map_err(|_| {
        CodexError::new(
            ErrorCode::InvalidInput,
            format!("invalid thread id: {id}"),
        )
    })
}

impl MultiAgentHandler {
    async fn handle_spawn(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: SpawnAgentArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid spawn_agent args: {e}"),
            )
        })?;

        if params.message.is_some() && params.items.is_some() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "spawn_agent: provide either 'message' or 'items', not both",
            ));
        }
        let prompt = params
            .message
            .or_else(|| {
                params
                    .items
                    .map(|items| serde_json::to_string(&items).unwrap_or_default())
            })
            .unwrap_or_default();
        if prompt.trim().is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "spawn_agent: message or items must be non-empty",
            ));
        }

        let input = crate::protocol::types::UserInput::Text {
            text: prompt,
            text_elements: vec![],
        };

        // spawn_agent now takes config + Vec<UserInput> + Option<SessionSource>
        let thread_id = self
            .agent_control
            .spawn_agent(self.config.clone(), vec![input], None)
            .await?;

        let (nickname, _role) = self
            .agent_control
            .get_agent_nickname_and_role(thread_id)
            .await
            .unwrap_or((None, None));

        let result = SpawnAgentResult {
            id: thread_id.to_string(),
            nickname: nickname.unwrap_or_else(|| thread_id.to_string()),
            status: "running".into(),
        };
        serde_json::to_value(result)
            .map_err(|e| CodexError::new(ErrorCode::ToolExecutionFailed, e.to_string()))
    }

    async fn handle_send_input(
        &self,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        let params: SendInputArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid send_input args: {e}"),
            )
        })?;

        if params.message.is_some() && params.items.is_some() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "send_input: provide either 'message' or 'items', not both",
            ));
        }
        let text = params
            .message
            .or_else(|| {
                params
                    .items
                    .map(|items| serde_json::to_string(&items).unwrap_or_default())
            })
            .unwrap_or_default();
        if text.trim().is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "send_input: message or items must be non-empty",
            ));
        }

        let thread_id = parse_thread_id(&params.id)?;
        let input = crate::protocol::types::UserInput::Text {
            text,
            text_elements: vec![],
        };
        self.agent_control
            .send_input(thread_id, vec![input])
            .await?;

        Ok(serde_json::json!({ "status": "sent" }))
    }

    async fn handle_resume(
        &self,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        let params: ResumeAgentArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid resume_agent args: {e}"),
            )
        })?;

        let thread_id = parse_thread_id(&params.id)?;
        self.agent_control.interrupt_agent(thread_id).await?;
        Ok(serde_json::json!({ "status": "resumed" }))
    }

    async fn handle_wait(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
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

        let agent_ids = params.agent_ids.unwrap_or_default();
        if agent_ids.is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "wait: agent_ids must be non-empty",
            ));
        }

        let mut results = Vec::new();
        for id_str in &agent_ids {
            let thread_id = parse_thread_id(id_str)?;
            let wait_result = tokio::time::timeout(
                Duration::from_millis(timeout as u64),
                self.agent_control.wait(thread_id),
            )
            .await;

            let entry = match wait_result {
                Ok(Ok(status)) => {
                    let (status_str, output) = match &status {
                        AgentStatus::Completed(msg) => ("completed", msg.clone()),
                        AgentStatus::Errored(msg) => ("errored", Some(msg.clone())),
                        AgentStatus::Shutdown => ("shutdown", None),
                        _ => ("unknown", None),
                    };
                    AgentStatusEntry {
                        id: id_str.clone(),
                        status: status_str.into(),
                        last_output: output,
                    }
                }
                Ok(Err(e)) => AgentStatusEntry {
                    id: id_str.clone(),
                    status: "errored".into(),
                    last_output: Some(e.to_string()),
                },
                Err(_) => AgentStatusEntry {
                    id: id_str.clone(),
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
        let params: CloseAgentArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid close_agent args: {e}"),
            )
        })?;

        let thread_id = parse_thread_id(&params.id)?;
        self.agent_control.shutdown_agent(thread_id).await?;
        Ok(serde_json::json!({ "status": "closed" }))
    }
}

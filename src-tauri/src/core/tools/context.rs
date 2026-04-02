//! Tool invocation context types.
//!
//! Matches Codex `tools/context.rs` — all public types, method signatures,
//! and enum variants are identical to the upstream implementation.

use crate::core::session::{Session, TurnContext};
use crate::core::turn_diff_tracker::TurnDiffTracker;
use crate::protocol::types::{
    CallToolResult, FunctionCallOutputBody, FunctionCallOutputPayload, ResponseInputItem,
};
use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared mutable tracker for file diffs accumulated during a turn.
pub type SharedTurnDiffTracker = Arc<Mutex<TurnDiffTracker>>;

/// Where a tool call originated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallSource {
    Direct,
    JsRepl,
}

/// Per-invocation context passed through the tool dispatch pipeline.
/// Matches Codex `ToolInvocation` — holds Arc references to Session and TurnContext.
#[derive(Clone)]
pub struct ToolInvocation {
    pub session: Arc<Session>,
    pub turn: Arc<TurnContext>,
    pub tracker: SharedTurnDiffTracker,
    pub call_id: String,
    pub tool_name: String,
    pub payload: ToolPayload,
}

/// Describes the payload of a single tool invocation.
/// Matches Codex `ToolPayload` variants exactly.
#[derive(Clone, Debug)]
pub enum ToolPayload {
    /// Standard JSON function call.
    Function { arguments: String },
    /// Freeform (custom) tool input (e.g. `apply_patch`, `js_repl`).
    Custom { input: String },
    /// Local shell call with structured parameters.
    LocalShell { params: ShellToolCallParams },
    /// MCP tool call.
    Mcp {
        server: String,
        tool: String,
        raw_arguments: String,
    },
}

impl ToolPayload {
    /// Return a loggable representation of the payload for telemetry.
    pub fn log_payload(&self) -> Cow<'_, str> {
        match self {
            ToolPayload::Function { arguments } => Cow::Borrowed(arguments),
            ToolPayload::Custom { input } => Cow::Borrowed(input),
            ToolPayload::LocalShell { params } => Cow::Owned(params.command.join(" ")),
            ToolPayload::Mcp { raw_arguments, .. } => Cow::Borrowed(raw_arguments),
        }
    }
}

/// Parameters for a local shell tool call.
/// Matches Codex `codex_protocol::models::ShellToolCallParams`.
#[derive(Clone, Debug)]
pub struct ShellToolCallParams {
    pub command: Vec<String>,
    pub workdir: Option<String>,
    pub timeout_ms: Option<u64>,
    pub sandbox_permissions: Option<SandboxPermissions>,
    pub additional_permissions: Option<serde_json::Value>,
    pub prefix_rule: Option<String>,
    pub justification: Option<String>,
}

/// Sandbox permission level for a tool call.
/// Matches Codex `SandboxPermissions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxPermissions {
    UseDefault,
    WithAdditionalPermissions,
    RequireEscalated,
}

/// Result of executing a tool.
/// Matches Codex `ToolOutput` — Function variant holds `FunctionCallOutputBody`,
/// Mcp variant holds `Result<CallToolResult, String>`.
#[derive(Clone)]
pub enum ToolOutput {
    Function {
        body: FunctionCallOutputBody,
        success: Option<bool>,
    },
    Mcp {
        result: Result<CallToolResult, String>,
    },
}

impl ToolOutput {
    /// Return a loggable preview of the output for telemetry.
    pub fn log_preview(&self) -> String {
        match self {
            ToolOutput::Function { body, .. } => {
                let text = match body {
                    FunctionCallOutputBody::Text(s) => s.clone(),
                    FunctionCallOutputBody::ContentItems(items) => items
                        .iter()
                        .filter_map(|item| match item {
                            crate::protocol::types::FunctionCallOutputContentItem::InputText {
                                text,
                            } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                crate::core::tools::telemetry_preview(&text)
            }
            ToolOutput::Mcp { result } => format!("{result:?}"),
        }
    }

    /// Whether the output indicates success for logging purposes.
    pub fn success_for_logging(&self) -> bool {
        match self {
            ToolOutput::Function { success, .. } => success.unwrap_or(true),
            ToolOutput::Mcp { result } => result.is_ok(),
        }
    }

    /// Convert this output into a `ResponseInputItem` for sending back to the model.
    /// Matches Codex `ToolOutput::into_response()`.
    pub fn into_response(self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        match self {
            ToolOutput::Function { body, success } => {
                if matches!(payload, ToolPayload::Custom { .. }) {
                    return ResponseInputItem::CustomToolCallOutput {
                        call_id: call_id.to_string(),
                        output: FunctionCallOutputPayload { body, success },
                    };
                }
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.to_string(),
                    output: FunctionCallOutputPayload { body, success },
                }
            }
            ToolOutput::Mcp { result } => ResponseInputItem::McpToolCallOutput {
                call_id: call_id.to_string(),
                result,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_tool_calls_should_roundtrip_as_custom_outputs() {
        let payload = ToolPayload::Custom {
            input: "patch".to_string(),
        };
        let response = ToolOutput::Function {
            body: FunctionCallOutputBody::Text("patched".to_string()),
            success: Some(true),
        }
        .into_response("call-42", &payload);

        match response {
            ResponseInputItem::CustomToolCallOutput { call_id, output } => {
                assert_eq!(call_id, "call-42");
                assert_eq!(output.text_content(), Some("patched"));
                assert_eq!(output.success, Some(true));
            }
            other => panic!("expected CustomToolCallOutput, got {other:?}"),
        }
    }

    #[test]
    fn function_payloads_remain_function_outputs() {
        let payload = ToolPayload::Function {
            arguments: "{}".to_string(),
        };
        let response = ToolOutput::Function {
            body: FunctionCallOutputBody::Text("ok".to_string()),
            success: Some(true),
        }
        .into_response("fn-1", &payload);

        match response {
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                assert_eq!(call_id, "fn-1");
                assert_eq!(output.text_content(), Some("ok"));
                assert_eq!(output.success, Some(true));
            }
            other => panic!("expected FunctionCallOutput, got {other:?}"),
        }
    }

    #[test]
    fn log_payload_returns_correct_representation() {
        assert_eq!(
            ToolPayload::Function {
                arguments: "{\"x\":1}".into()
            }
            .log_payload()
            .as_ref(),
            "{\"x\":1}"
        );
        assert_eq!(
            ToolPayload::Custom {
                input: "patch data".into()
            }
            .log_payload()
            .as_ref(),
            "patch data"
        );
        assert_eq!(
            ToolPayload::LocalShell {
                params: ShellToolCallParams {
                    command: vec!["echo".into(), "hi".into()],
                    workdir: None,
                    timeout_ms: None,
                    sandbox_permissions: None,
                    additional_permissions: None,
                    prefix_rule: None,
                    justification: None,
                }
            }
            .log_payload()
            .as_ref(),
            "echo hi"
        );
        assert_eq!(
            ToolPayload::Mcp {
                server: "s".into(),
                tool: "t".into(),
                raw_arguments: "{\"a\":1}".into()
            }
            .log_payload()
            .as_ref(),
            "{\"a\":1}"
        );
    }

    #[test]
    fn mcp_output_into_response() {
        let payload = ToolPayload::Mcp {
            server: "s".into(),
            tool: "t".into(),
            raw_arguments: "{}".into(),
        };
        let response = ToolOutput::Mcp {
            result: Err("fail".to_string()),
        }
        .into_response("mcp-1", &payload);

        match response {
            ResponseInputItem::McpToolCallOutput { call_id, result } => {
                assert_eq!(call_id, "mcp-1");
                assert!(result.is_err());
            }
            other => panic!("expected McpToolCallOutput, got {other:?}"),
        }
    }

    #[test]
    fn success_for_logging_defaults() {
        let ok = ToolOutput::Function {
            body: FunctionCallOutputBody::Text("ok".into()),
            success: None,
        };
        assert!(ok.success_for_logging()); // None defaults to true

        let fail = ToolOutput::Function {
            body: FunctionCallOutputBody::Text("err".into()),
            success: Some(false),
        };
        assert!(!fail.success_for_logging());

        let mcp_ok = ToolOutput::Mcp {
            result: Ok(CallToolResult {
                content: None,
                is_error: None,
            }),
        };
        assert!(mcp_ok.success_for_logging());

        let mcp_err = ToolOutput::Mcp {
            result: Err("err".into()),
        };
        assert!(!mcp_err.success_for_logging());
    }
}

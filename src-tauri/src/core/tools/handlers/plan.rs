use async_trait::async_trait;
use serde::Deserialize;
use std::sync::LazyLock;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct PlanHandler;

/// Static tool spec for the plan tool, matching source Codex PLAN_TOOL.
pub static PLAN_TOOL: LazyLock<PlanToolSpec> = LazyLock::new(|| {
    PlanToolSpec {
        name: "update_plan".to_string(),
        description: "Updates the task plan.\nProvide an optional explanation and a list of plan items, each with a step and status.\nAt most one step can be in_progress at a time.\n".to_string(),
        parameters: PlanToolParameters {
            required: vec!["plan".to_string()],
            properties: PlanToolProperties {
                explanation: PropertySpec { r#type: "string".to_string(), description: None },
                plan: PropertySpec {
                    r#type: "array".to_string(),
                    description: Some("The list of steps".to_string()),
                },
            },
        },
    }
});

#[derive(Debug, Clone)]
pub struct PlanToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: PlanToolParameters,
}

#[derive(Debug, Clone)]
pub struct PlanToolParameters {
    pub required: Vec<String>,
    pub properties: PlanToolProperties,
}

#[derive(Debug, Clone)]
pub struct PlanToolProperties {
    pub explanation: PropertySpec,
    pub plan: PropertySpec,
}

#[derive(Debug, Clone)]
pub struct PropertySpec {
    pub r#type: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
struct UpdatePlanArgs {
    #[serde(default)]
    explanation: Option<String>,
    #[serde(default)]
    plan: Option<Vec<PlanItem>>,
}

#[derive(Deserialize)]
struct PlanItem {
    step: String,
    status: String,
}

#[async_trait]
impl ToolHandler for PlanHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "update_plan")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("update_plan".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: UpdatePlanArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid plan args: {e}"))
        })?;

        // Source Codex rejects update_plan in Plan mode (it's a TODO/checklist tool)
        // TODO: wire current_mode from actual session collaboration_mode
        let current_mode = super::request_user_input::ModeKind::Default;
        if current_mode == super::request_user_input::ModeKind::Plan {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "update_plan is a TODO/checklist tool and is not allowed in Plan mode",
            ));
        }

        // Validate: at most one step can be in_progress
        if let Some(ref plan) = params.plan {
            let in_progress_count = plan.iter().filter(|item| item.status == "in_progress").count();
            if in_progress_count > 1 {
                return Err(CodexError::new(
                    ErrorCode::InvalidInput,
                    "at most one step can be in_progress at a time",
                ));
            }
        }

        // In the full implementation, this emits an EventMsg::PlanUpdate event
        // so clients can render the plan. The handler's value is in its structured inputs.
        // TODO: emit PlanUpdate event via session.send_event()

        Ok(serde_json::json!({"status": "Plan updated"}))
    }
}

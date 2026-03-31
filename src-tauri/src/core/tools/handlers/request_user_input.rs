use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::ModeKind;

fn request_user_input_is_available(mode: ModeKind, default_mode_enabled: bool) -> bool {
    mode.allows_request_user_input() || (default_mode_enabled && mode == ModeKind::Default)
}

pub fn request_user_input_unavailable_message(
    mode: ModeKind,
    default_mode_enabled: bool,
) -> Option<String> {
    if request_user_input_is_available(mode, default_mode_enabled) {
        None
    } else {
        Some(format!(
            "request_user_input is unavailable in {} mode",
            mode.display_name()
        ))
    }
}

pub fn request_user_input_tool_description(default_mode_enabled: bool) -> String {
    let modes: Vec<&str> = [
        ModeKind::Default,
        ModeKind::Plan,
        ModeKind::Execute,
        ModeKind::PairProgramming,
    ]
    .iter()
    .filter(|m| request_user_input_is_available(**m, default_mode_enabled))
    .map(|m| m.display_name())
    .collect();
    let allowed = match modes.as_slice() {
        [] => "no modes".to_string(),
        [m] => format!("{m} mode"),
        [a, b] => format!("{a} or {b} mode"),
        _ => format!("modes: {}", modes.join(",")),
    };
    format!("Request user input for one to three short questions and wait for the response. This tool is only available in {allowed}.")
}

pub struct RequestUserInputHandler {
    pub default_mode_request_user_input: bool,
}

impl Default for RequestUserInputHandler {
    fn default() -> Self {
        Self {
            default_mode_request_user_input: false,
        }
    }
}

#[derive(Deserialize)]
struct RequestUserInputArgs {
    questions: Vec<QuestionArg>,
}

#[derive(Deserialize)]
struct QuestionArg {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    options: Option<Vec<String>>,
    #[serde(default)]
    is_other: bool,
}

#[async_trait]
impl ToolHandler for RequestUserInputHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "request_user_input")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("request_user_input".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Mode availability check (default to Plan mode for now)
        let current_mode = ModeKind::Default; // TODO: wire to actual session mode
        if let Some(msg) = request_user_input_unavailable_message(
            current_mode,
            self.default_mode_request_user_input,
        ) {
            return Err(CodexError::new(ErrorCode::ToolExecutionFailed, msg));
        }

        let mut params: RequestUserInputArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid request_user_input args: {e}"),
            )
        })?;

        // Validate: all questions must have non-empty options
        let missing_options = params
            .questions
            .iter()
            .any(|q| q.options.as_ref().map_or(true, |opts| opts.is_empty()));
        if missing_options {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "request_user_input requires non-empty options for every question",
            ));
        }

        // Set is_other = true for all questions (matches source Codex behavior)
        for question in &mut params.questions {
            question.is_other = true;
        }

        // In the full implementation, this emits a RequestUserInput event and waits.
        // TODO: emit event via session and await user response
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            "request_user_input requires UI integration to emit events and await responses",
        ))
    }
}

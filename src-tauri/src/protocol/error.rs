use serde::{Deserialize, Serialize};
use std::fmt;

/// Error codes for all Mosaic system errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    InvalidInput,
    ToolExecutionFailed,
    McpServerUnavailable,
    ConfigurationError,
    SandboxViolation,
    ApprovalDenied,
    SessionError,
    InternalError,
}

/// Unified error type shared across all Mosaic modules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl CodexError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details),
        }
    }
}

impl fmt::Display for CodexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for CodexError {}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_error_code() -> impl Strategy<Value = ErrorCode> {
        prop_oneof![
            Just(ErrorCode::InvalidInput),
            Just(ErrorCode::ToolExecutionFailed),
            Just(ErrorCode::McpServerUnavailable),
            Just(ErrorCode::ConfigurationError),
            Just(ErrorCode::SandboxViolation),
            Just(ErrorCode::ApprovalDenied),
            Just(ErrorCode::SessionError),
            Just(ErrorCode::InternalError),
        ]
    }

    /// Non-null JSON values only — `Some(Null)` round-trips to `None`
    /// via serde's `Option` handling, so we exclude it.
    fn arb_json_value_non_null() -> impl Strategy<Value = serde_json::Value> {
        prop_oneof![
            any::<bool>().prop_map(serde_json::Value::Bool),
            any::<i64>().prop_map(|n| serde_json::Value::Number(n.into())),
            "[a-zA-Z0-9 _-]{0,50}".prop_map(serde_json::Value::String),
        ]
    }

    fn arb_codex_error() -> impl Strategy<Value = CodexError> {
        (
            arb_error_code(),
            "[a-zA-Z0-9 _.-]{0,100}",
            prop::option::of(arb_json_value_non_null()),
        )
            .prop_map(|(code, message, details)| CodexError {
                code,
                message,
                details,
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn error_code_json_roundtrip(code in arb_error_code()) {
            let json = serde_json::to_string(&code).unwrap();
            let decoded: ErrorCode = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(code, decoded);
        }

        #[test]
        fn codex_error_json_roundtrip(error in arb_codex_error()) {
            let json = serde_json::to_string(&error).unwrap();
            let decoded: CodexError = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(error, decoded);
        }
    }
}

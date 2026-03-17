use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct JsReplHandler;
pub struct JsReplResetHandler;

/// Prefix for pragma lines in js_repl input.
pub const JS_REPL_PRAGMA_PREFIX: &str = "//pragma:";

/// Join stdout and stderr into a single output string.
fn join_outputs(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() { stderr.to_string() }
    else if stderr.is_empty() { stdout.to_string() }
    else { format!("{stdout}\n{stderr}") }
}

/// Build a structured exec output for js_repl results.
fn build_js_repl_exec_output(
    output: &str,
    error: Option<&str>,
    duration: std::time::Duration,
) -> serde_json::Value {
    let stderr = error.unwrap_or("");
    let aggregated = join_outputs(output, stderr);
    serde_json::json!({
        "exit_code": if error.is_some() { 1 } else { 0 },
        "stdout": output,
        "stderr": stderr,
        "aggregated_output": aggregated,
        "duration_ms": duration.as_millis() as u64,
        "timed_out": false,
    })
}

/// Emit a shell-like begin event for js_repl execution.
/// In the full implementation, this sends a ToolEmitter::shell begin event.
async fn emit_js_repl_exec_begin(_call_id: &str) {
    // TODO: wire to actual event system
    // ToolEmitter::shell(vec!["js_repl"], cwd, ExecCommandSource::Agent, false).begin(ctx).await;
}

/// Emit a shell-like end event for js_repl execution.
/// In the full implementation, this sends a ToolEmitter::shell finish event.
async fn emit_js_repl_exec_end(
    _call_id: &str,
    _output: &str,
    _error: Option<&str>,
    _duration: std::time::Duration,
) {
    // TODO: wire to actual event system
    // let exec_output = build_js_repl_exec_output(output, error, duration);
    // emitter.finish(ctx, stage).await;
}

#[derive(Debug, Deserialize)]
struct JsReplArgs {
    /// The JavaScript code to execute. May contain pragma lines.
    code: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Parse pragma directives from the code input.
fn parse_pragmas(code: &str) -> (Vec<String>, String) {
    let mut pragmas = Vec::new();
    let mut clean_lines = Vec::new();
    for line in code.lines() {
        if let Some(pragma) = line.trim().strip_prefix(JS_REPL_PRAGMA_PREFIX) {
            pragmas.push(pragma.trim().to_string());
        } else {
            clean_lines.push(line);
        }
    }
    (pragmas, clean_lines.join("\n"))
}

#[async_trait]
impl ToolHandler for JsReplHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "js_repl")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("js_repl".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Feature flag check (matches source: session.features().enabled(Feature::JsRepl))
        // TODO: wire to actual feature flag system

        let params: JsReplArgs = serde_json::from_value(args.clone()).map_err(|_| {
            // Also support freeform string input (Custom payload)
            CodexError::new(ErrorCode::InvalidInput, "js_repl expects {code} or freeform string")
        }).or_else(|_| {
            // Try freeform string
            args.as_str().map(|s| JsReplArgs { code: s.to_string(), timeout_ms: None })
                .ok_or_else(|| CodexError::new(ErrorCode::InvalidInput, "js_repl expects {code} or freeform string"))
        })?;

        let (pragmas, clean_code) = parse_pragmas(&params.code);

        if clean_code.trim().is_empty() {
            return Err(CodexError::new(ErrorCode::InvalidInput, "js_repl code must not be empty"));
        }

        // Full implementation:
        // 1. Get or create persistent Node.js process via js_repl module
        // 2. Send code to the process
        // 3. Emit shell begin/end events
        // 4. Return output with exit_code
        // TODO: wire to actual js_repl runtime

        let _ = pragmas; // Will be used when runtime is wired

        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            "js_repl requires the JavaScript REPL runtime (persistent Node.js process)",
        ))
    }
}

#[async_trait]
impl ToolHandler for JsReplResetHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "js_repl_reset")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("js_repl_reset".to_string())
    }

    async fn handle(&self, _args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Full implementation: kill and restart the Node.js process
        // TODO: wire to actual js_repl runtime
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            "js_repl_reset requires the JavaScript REPL runtime",
        ))
    }
}

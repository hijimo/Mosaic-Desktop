pub mod handlers;
pub mod router;

// Infrastructure modules
pub mod context;
pub mod events;
pub mod js_repl;
pub mod network_approval;
pub mod orchestrator;
pub mod parallel;
pub mod sandboxing;
pub mod spec;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

use crate::core::truncation::{formatted_truncate_text, TruncationPolicy};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::ResponseInputItem;
pub use context::ToolInvocation;
pub use router::ToolRouter;
pub use spec::{
    build_specs, AssembledToolRuntime, ConfiguredToolSpec, ToolRegistryBuilder, ToolsConfig,
};

// ---------------------------------------------------------------------------
// FunctionCallError — matches Codex function_tool.rs exactly
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum FunctionCallError {
    #[allow(dead_code)]
    RespondToModel(String),
    #[allow(dead_code)]
    MissingLocalShellCallId,
    #[allow(dead_code)]
    Fatal(String),
}

impl fmt::Display for FunctionCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RespondToModel(msg) => write!(f, "{msg}"),
            Self::MissingLocalShellCallId => {
                write!(f, "LocalShellCall without call_id or id")
            }
            Self::Fatal(msg) => write!(f, "Fatal error: {msg}"),
        }
    }
}

impl std::error::Error for FunctionCallError {}

// ---------------------------------------------------------------------------
// StreamOutput + ExecToolCallOutput — matches Codex exec.rs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StreamOutput<T: Clone> {
    pub text: T,
    pub truncated_after_lines: Option<u32>,
}

impl<T: Clone> StreamOutput<T> {
    pub fn new(text: T) -> Self {
        Self {
            text,
            truncated_after_lines: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExecToolCallOutput {
    pub exit_code: i32,
    pub stdout: StreamOutput<String>,
    pub stderr: StreamOutput<String>,
    pub aggregated_output: StreamOutput<String>,
    pub duration: Duration,
    pub timed_out: bool,
}

impl Default for ExecToolCallOutput {
    fn default() -> Self {
        Self {
            exit_code: 0,
            stdout: StreamOutput::new(String::new()),
            stderr: StreamOutput::new(String::new()),
            aggregated_output: StreamOutput::new(String::new()),
            duration: Duration::ZERO,
            timed_out: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Telemetry preview constants — matches Codex tools/mod.rs
// ---------------------------------------------------------------------------

pub(crate) const TELEMETRY_PREVIEW_MAX_BYTES: usize = 2 * 1024;
pub(crate) const TELEMETRY_PREVIEW_MAX_LINES: usize = 64;
pub(crate) const TELEMETRY_PREVIEW_TRUNCATION_NOTICE: &str =
    "[... telemetry preview truncated ...]";

// ---------------------------------------------------------------------------
// Exec output formatting — matches Codex tools/mod.rs signatures exactly
// ---------------------------------------------------------------------------

/// Format the combined exec output for sending back to the model.
/// Includes exit code and duration metadata; truncates large bodies safely.
pub fn format_exec_output_for_model_structured(
    exec_output: &ExecToolCallOutput,
    truncation_policy: TruncationPolicy,
) -> String {
    #[derive(Serialize)]
    struct ExecMetadata {
        exit_code: i32,
        duration_seconds: f32,
    }

    #[derive(Serialize)]
    struct ExecOutput<'a> {
        output: &'a str,
        metadata: ExecMetadata,
    }

    let duration_seconds = ((exec_output.duration.as_secs_f32()) * 10.0).round() / 10.0;
    let formatted_output = format_exec_output_str(exec_output, truncation_policy);

    let payload = ExecOutput {
        output: &formatted_output,
        metadata: ExecMetadata {
            exit_code: exec_output.exit_code,
            duration_seconds,
        },
    };

    serde_json::to_string(&payload).expect("serialize ExecOutput")
}

pub fn format_exec_output_for_model_freeform(
    exec_output: &ExecToolCallOutput,
    truncation_policy: TruncationPolicy,
) -> String {
    let duration_seconds = ((exec_output.duration.as_secs_f32()) * 10.0).round() / 10.0;
    let content = build_content_with_timeout(exec_output);
    let total_lines = content.lines().count();
    let formatted_output = truncate_text(&content, truncation_policy.clone());

    let mut sections = Vec::new();
    sections.push(format!("Exit code: {}", exec_output.exit_code));
    sections.push(format!("Wall time: {duration_seconds} seconds"));
    if total_lines != formatted_output.lines().count() {
        sections.push(format!("Total output lines: {total_lines}"));
    }
    sections.push("Output:".to_string());
    sections.push(formatted_output);
    sections.join("\n")
}

pub fn format_exec_output_str(
    exec_output: &ExecToolCallOutput,
    truncation_policy: TruncationPolicy,
) -> String {
    let content = build_content_with_timeout(exec_output);
    formatted_truncate_text(&content, truncation_policy)
}

fn build_content_with_timeout(exec_output: &ExecToolCallOutput) -> String {
    if exec_output.timed_out {
        format!(
            "command timed out after {} milliseconds\n{}",
            exec_output.duration.as_millis(),
            exec_output.aggregated_output.text
        )
    } else {
        exec_output.aggregated_output.text.clone()
    }
}

fn truncate_text(content: &str, policy: TruncationPolicy) -> String {
    formatted_truncate_text(content, policy)
}

/// Truncate content for telemetry preview (max bytes + max lines).
/// Matches Codex `telemetry_preview` in tools/context.rs.
pub(crate) fn telemetry_preview(content: &str) -> String {
    let byte_limit = content.len().min(TELEMETRY_PREVIEW_MAX_BYTES);
    let truncated_slice = &content[..find_char_boundary(content, byte_limit)];
    let truncated_by_bytes = truncated_slice.len() < content.len();

    let mut preview = String::new();
    let mut lines_iter = truncated_slice.lines();
    for idx in 0..TELEMETRY_PREVIEW_MAX_LINES {
        match lines_iter.next() {
            Some(line) => {
                if idx > 0 {
                    preview.push('\n');
                }
                preview.push_str(line);
            }
            None => break,
        }
    }
    let truncated_by_lines = lines_iter.next().is_some();

    if !truncated_by_bytes && !truncated_by_lines {
        return content.to_string();
    }

    if preview.len() < truncated_slice.len()
        && truncated_slice
            .as_bytes()
            .get(preview.len())
            .is_some_and(|byte| *byte == b'\n')
    {
        preview.push('\n');
    }

    if !preview.is_empty() && !preview.ends_with('\n') {
        preview.push('\n');
    }
    preview.push_str(TELEMETRY_PREVIEW_TRUNCATION_NOTICE);
    preview
}

fn find_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Identifies the kind of tool — used for registry lookup and dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    /// A built-in tool identified by name.
    Builtin(String),
    /// An MCP tool qualified as `mcp__{server}__{tool}`.
    Mcp { server: String, tool: String },
    /// A dynamically registered tool.
    Dynamic(String),
}

impl ToolKind {
    /// Returns the display name for this tool kind.
    pub fn name(&self) -> String {
        match self {
            ToolKind::Builtin(name) => name.clone(),
            ToolKind::Mcp { server, tool } => format!("mcp__{server}__{tool}"),
            ToolKind::Dynamic(name) => name.clone(),
        }
    }
}

/// Tool metadata returned by listing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub kind: ToolKind,
    pub input_schema: Option<serde_json::Value>,
}

/// Async trait that all tool handlers must implement.
///
/// - `matches_kind`: returns true if this handler can service the given `ToolKind`.
/// - `kind`: returns the canonical `ToolKind` for this handler.
/// - `handle`: executes the tool with the provided JSON arguments.
/// - `tool_spec`: returns the Responses API tool definition for this handler.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn matches_kind(&self, kind: &ToolKind) -> bool;
    fn kind(&self) -> ToolKind;
    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError>;
    /// Returns the tool definition in Responses API format for sending to the model.
    /// Default returns None (tool is not advertised to the model).
    fn tool_spec(&self) -> Option<serde_json::Value> {
        None
    }
}

/// Registry of built-in tool handlers.
///
/// Supports runtime registration and dispatch by `ToolKind`.
pub struct ToolRegistry {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a new tool handler.
    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        self.handlers.push(handler);
    }

    /// Dispatch a tool call to the first handler whose `matches_kind` returns true.
    pub async fn dispatch(
        &self,
        kind: &ToolKind,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        for handler in &self.handlers {
            if handler.matches_kind(kind) {
                return handler.handle(args).await;
            }
        }
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("no handler registered for tool kind: {}", kind.name()),
        ))
    }

    /// Find a handler matching the given kind.
    pub fn find(&self, kind: &ToolKind) -> Option<&dyn ToolHandler> {
        self.handlers
            .iter()
            .find(|h| h.matches_kind(kind))
            .map(|h| h.as_ref())
    }

    /// List all registered handler kinds.
    pub fn registered_kinds(&self) -> Vec<ToolKind> {
        self.handlers.iter().map(|h| h.kind()).collect()
    }

    /// Collect all tool specs from registered handlers for sending to the model API.
    pub fn collect_tool_specs(&self) -> Vec<serde_json::Value> {
        self.handlers.iter().filter_map(|h| h.tool_spec()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Codex-aligned registry types (tools/registry.rs equivalent)
//
// These types match Codex's ToolHandler/ToolRegistry signatures exactly.
// The legacy ToolHandler trait above is preserved for backward compatibility
// with existing handlers. New handlers should implement `registry::ToolHandler`.
// ===========================================================================

pub mod registry {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::context::{ToolInvocation, ToolOutput, ToolPayload};
    use super::FunctionCallError;
    use crate::protocol::types::ResponseInputItem;

    /// Tool kind for registry dispatch — matches Codex `registry::ToolKind`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum ToolKind {
        Function,
        Mcp,
    }

    /// Tool handler trait — matches Codex `registry::ToolHandler` exactly.
    #[async_trait]
    pub trait ToolHandler: Send + Sync {
        fn kind(&self) -> ToolKind;

        fn matches_kind(&self, payload: &ToolPayload) -> bool {
            matches!(
                (self.kind(), payload),
                (ToolKind::Function, ToolPayload::Function { .. })
                    | (ToolKind::Function, ToolPayload::Custom { .. })
                    | (ToolKind::Function, ToolPayload::LocalShell { .. })
                    | (ToolKind::Mcp, ToolPayload::Mcp { .. })
            )
        }

        /// Returns `true` if the invocation might mutate the user's environment.
        async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
            false
        }

        /// Execute the tool and return output for the model.
        async fn handle(
            &self,
            invocation: ToolInvocation,
        ) -> Result<ToolOutput, FunctionCallError>;
    }

    /// Tool registry — matches Codex `registry::ToolRegistry`.
    /// Dispatches by tool name, returns `ResponseInputItem`.
    pub struct ToolRegistry {
        handlers: HashMap<String, Arc<dyn ToolHandler>>,
    }

    impl ToolRegistry {
        pub fn new(handlers: HashMap<String, Arc<dyn ToolHandler>>) -> Self {
            Self { handlers }
        }

        pub fn handler(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
            self.handlers.get(name).map(Arc::clone)
        }

        pub async fn dispatch(
            &self,
            invocation: ToolInvocation,
        ) -> Result<ResponseInputItem, FunctionCallError> {
            let tool_name = invocation.tool_name.clone();
            let call_id = invocation.call_id.clone();
            let payload = invocation.payload.clone();

            let handler = match self.handler(&tool_name) {
                Some(h) => h,
                None => {
                    let message = match &payload {
                        ToolPayload::Custom { .. } => {
                            format!("unsupported custom tool call: {tool_name}")
                        }
                        _ => format!("unsupported call: {tool_name}"),
                    };
                    return Err(FunctionCallError::RespondToModel(message));
                }
            };

            if !handler.matches_kind(&payload) {
                return Err(FunctionCallError::Fatal(format!(
                    "tool {tool_name} invoked with incompatible payload"
                )));
            }

            let output = handler.handle(invocation).await?;
            Ok(output.into_response(&call_id, &payload))
        }
    }

    /// Configured tool spec — matches Codex `registry::ConfiguredToolSpec`.
    #[derive(Debug, Clone)]
    pub struct ConfiguredToolSpec {
        pub spec: crate::core::tools::spec::ToolSpec,
        pub supports_parallel_tool_calls: bool,
    }

    impl ConfiguredToolSpec {
        pub fn new(
            spec: crate::core::tools::spec::ToolSpec,
            supports_parallel_tool_calls: bool,
        ) -> Self {
            Self {
                spec,
                supports_parallel_tool_calls,
            }
        }
    }

    /// Builder for constructing a ToolRegistry — matches Codex `ToolRegistryBuilder`.
    pub struct ToolRegistryBuilder {
        handlers: HashMap<String, Arc<dyn ToolHandler>>,
        specs: Vec<ConfiguredToolSpec>,
    }

    impl ToolRegistryBuilder {
        pub fn new() -> Self {
            Self {
                handlers: HashMap::new(),
                specs: Vec::new(),
            }
        }

        pub fn push_spec(&mut self, spec: crate::core::tools::spec::ToolSpec) {
            self.push_spec_with_parallel_support(spec, false);
        }

        pub fn push_spec_with_parallel_support(
            &mut self,
            spec: crate::core::tools::spec::ToolSpec,
            supports_parallel_tool_calls: bool,
        ) {
            self.specs
                .push(ConfiguredToolSpec::new(spec, supports_parallel_tool_calls));
        }

        pub fn register_handler(
            &mut self,
            name: impl Into<String>,
            handler: Arc<dyn ToolHandler>,
        ) {
            self.handlers.insert(name.into(), handler);
        }

        pub fn build(self) -> (Vec<ConfiguredToolSpec>, ToolRegistry) {
            let registry = ToolRegistry::new(self.handlers);
            (self.specs, registry)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl ToolHandler for EchoTool {
        fn matches_kind(&self, kind: &ToolKind) -> bool {
            matches!(kind, ToolKind::Builtin(name) if name == "echo")
        }

        fn kind(&self) -> ToolKind {
            ToolKind::Builtin("echo".to_string())
        }

        async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
            Ok(args)
        }
    }

    #[tokio::test]
    async fn register_and_dispatch() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));

        let kind = ToolKind::Builtin("echo".to_string());
        let input = serde_json::json!({"msg": "hello"});
        let result = registry.dispatch(&kind, input.clone()).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn dispatch_unknown_returns_error() {
        let registry = ToolRegistry::new();
        let kind = ToolKind::Builtin("nonexistent".to_string());
        let result = registry.dispatch(&kind, serde_json::Value::Null).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
    }

    #[test]
    fn find_returns_matching_handler() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));

        let kind = ToolKind::Builtin("echo".to_string());
        assert!(registry.find(&kind).is_some());

        let missing = ToolKind::Builtin("missing".to_string());
        assert!(registry.find(&missing).is_none());
    }

    #[test]
    fn registered_kinds_lists_all() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let kinds = registry.registered_kinds();
        assert_eq!(kinds.len(), 1);
        assert_eq!(kinds[0], ToolKind::Builtin("echo".to_string()));
    }

    #[test]
    fn tool_kind_name_formats_correctly() {
        assert_eq!(ToolKind::Builtin("read".to_string()).name(), "read");
        assert_eq!(
            ToolKind::Mcp {
                server: "srv".to_string(),
                tool: "fetch".to_string()
            }
            .name(),
            "mcp__srv__fetch"
        );
        assert_eq!(ToolKind::Dynamic("custom".to_string()).name(), "custom");
    }

    #[test]
    fn tools_module_exposes_runtime_assembly_types() {
        let assembled: AssembledToolRuntime = build_specs(&ToolsConfig::default(), false);
        let builder = ToolRegistryBuilder::new();
        let configured = ConfiguredToolSpec::new(
            assembled.configured_specs[0].spec.clone(),
            assembled.configured_specs[0].supports_parallel_tool_calls,
        );

        assert!(assembled
            .configured_specs
            .iter()
            .any(|spec| spec.spec.name() == "shell"));
        assert!(configured.supports_parallel_tool_calls);
        let _ = builder;
    }

    // ---- Codex-aligned registry tests ----

    #[test]
    fn registry_handler_lookup() {
        let handlers = std::collections::HashMap::new();
        let reg = registry::ToolRegistry::new(handlers);
        assert!(reg.handler("nonexistent").is_none());
    }

    #[test]
    fn registry_builder_builds_specs_and_registry() {
        let mut builder = registry::ToolRegistryBuilder::new();
        builder.push_spec(crate::core::tools::spec::ToolSpec::Function {
            name: "test_tool".into(),
            description: "desc".into(),
            strict: false,
            parameters: crate::core::tools::spec::JsonSchema::Object {
                properties: std::collections::BTreeMap::new(),
                required: None,
                additional_properties: None,
            },
        });
        let (specs, _registry) = builder.build();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].spec.name(), "test_tool");
        assert!(!specs[0].supports_parallel_tool_calls);
    }

    #[test]
    fn registry_configured_tool_spec_parallel_flag() {
        let spec = registry::ConfiguredToolSpec::new(
            crate::core::tools::spec::ToolSpec::Function {
                name: "parallel_tool".into(),
                description: "".into(),
                strict: false,
                parameters: crate::core::tools::spec::JsonSchema::Object {
                    properties: std::collections::BTreeMap::new(),
                    required: None,
                    additional_properties: None,
                },
            },
            true,
        );
        assert!(spec.supports_parallel_tool_calls);
    }

    #[test]
    fn registry_tool_kind_variants() {
        let f = registry::ToolKind::Function;
        let m = registry::ToolKind::Mcp;
        assert_ne!(f, m);
        assert_eq!(f, registry::ToolKind::Function);
    }
}

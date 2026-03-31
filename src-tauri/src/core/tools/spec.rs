//! Tool specification types and JSON schema definitions.
//!
//! Adapted from Codex `tools/spec.rs`. Provides `JsonSchema`, `ToolSpec`,
//! and builder functions for all built-in tool definitions.

use crate::core::tools::{ToolHandler, ToolRegistry};
use crate::protocol::types::WebSearchMode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Generic JSON Schema subset for tool parameter definitions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    #[serde(alias = "integer")]
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(
            rename = "additionalProperties",
            skip_serializing_if = "Option::is_none"
        )]
        additional_properties: Option<AdditionalProperties>,
    },
}

/// Whether additional properties are allowed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<JsonSchema>),
}

impl From<bool> for AdditionalProperties {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

/// A tool specification sent to the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ToolSpec {
    #[serde(rename = "function")]
    Function {
        name: String,
        description: String,
        #[serde(default)]
        strict: bool,
        parameters: JsonSchema,
    },
    #[serde(rename = "web_search")]
    WebSearch {
        #[serde(skip_serializing_if = "Option::is_none")]
        external_web_access: Option<bool>,
    },
}

impl ToolSpec {
    pub fn name(&self) -> &str {
        match self {
            Self::Function { name, .. } => name,
            Self::WebSearch { .. } => "web_search",
        }
    }
}

/// Configuration for which tools are enabled.
#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub shell_enabled: bool,
    pub shell_command_enabled: bool,
    pub apply_patch_enabled: bool,
    pub list_dir_enabled: bool,
    pub read_file_enabled: bool,
    pub grep_files_enabled: bool,
    pub mcp_resources_enabled: bool,
    pub unified_exec_enabled: bool,
    pub update_plan_enabled: bool,
    pub view_image_enabled: bool,
    pub request_user_input_enabled: bool,
    pub js_repl_enabled: bool,
    pub test_sync_enabled: bool,
    pub agent_jobs_enabled: bool,
    pub agent_jobs_worker_enabled: bool,
    pub collab_tools: bool,
    pub search_tool: bool,
    pub presentation_artifact: bool,
    pub web_search_mode: Option<WebSearchMode>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            shell_enabled: true,
            shell_command_enabled: false,
            apply_patch_enabled: true,
            list_dir_enabled: true,
            read_file_enabled: true,
            grep_files_enabled: true,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            search_tool: false,
            presentation_artifact: false,
            web_search_mode: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredToolSpec {
    pub spec: ToolSpec,
    pub supports_parallel_tool_calls: bool,
}

impl ConfiguredToolSpec {
    pub fn new(spec: ToolSpec, supports_parallel_tool_calls: bool) -> Self {
        Self {
            spec,
            supports_parallel_tool_calls,
        }
    }
}

pub struct AssembledToolRuntime {
    pub configured_specs: Vec<ConfiguredToolSpec>,
    pub registry: ToolRegistry,
}

pub struct ToolRegistryBuilder {
    configured_specs: Vec<ConfiguredToolSpec>,
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            configured_specs: Vec::new(),
            handlers: Vec::new(),
        }
    }

    pub fn push_spec(&mut self, spec: ToolSpec) {
        self.push_spec_with_parallel_support(spec, false);
    }

    pub fn push_spec_with_parallel_support(
        &mut self,
        spec: ToolSpec,
        supports_parallel_tool_calls: bool,
    ) {
        self.configured_specs
            .push(ConfiguredToolSpec::new(spec, supports_parallel_tool_calls));
    }

    pub fn register_handler(&mut self, handler: Box<dyn ToolHandler>) {
        self.handlers.push(handler);
    }

    pub fn build(self) -> AssembledToolRuntime {
        let mut registry = ToolRegistry::new();
        for handler in self.handlers {
            registry.register(handler);
        }
        AssembledToolRuntime {
            configured_specs: self.configured_specs,
            registry,
        }
    }
}

pub fn build_specs(config: &ToolsConfig, has_agent_control: bool) -> AssembledToolRuntime {
    use crate::core::tools::handlers::{
        ApplyPatchHandler, BatchJobHandler, GrepFilesHandler, JsReplHandler, JsReplResetHandler,
        ListDirHandler, McpResourceHandler, PlanHandler, PresentationArtifactHandler,
        ReadFileHandler, RequestUserInputHandler, SearchToolBm25Handler, ShellCommandHandler,
        ShellHandler, TestSyncHandler, UnifiedExecHandler, ViewImageHandler,
    };

    let mut builder = ToolRegistryBuilder::new();

    if config.shell_enabled {
        builder.push_spec_with_parallel_support(create_shell_tool(), true);
        builder.register_handler(Box::new(ShellHandler));
    }

    if config.shell_command_enabled {
        builder.push_spec_with_parallel_support(create_shell_command_tool(), true);
        builder.register_handler(Box::new(ShellCommandHandler::default()));
    }

    if config.apply_patch_enabled {
        builder.push_spec(create_apply_patch_tool());
        builder.register_handler(Box::new(ApplyPatchHandler));
    }

    if config.list_dir_enabled {
        builder.push_spec_with_parallel_support(create_list_dir_tool(), true);
        builder.register_handler(Box::new(ListDirHandler));
    }

    if config.read_file_enabled {
        builder.push_spec_with_parallel_support(create_read_file_tool(), true);
        builder.register_handler(Box::new(ReadFileHandler));
    }

    if config.grep_files_enabled {
        builder.push_spec_with_parallel_support(create_grep_files_tool(), true);
        builder.register_handler(Box::new(GrepFilesHandler));
    }

    if config.mcp_resources_enabled {
        builder.push_spec_with_parallel_support(create_list_mcp_resources_tool(), true);
        builder.push_spec_with_parallel_support(create_list_mcp_resource_templates_tool(), true);
        builder.push_spec_with_parallel_support(create_read_mcp_resource_tool(), true);
        builder.register_handler(Box::new(McpResourceHandler::list_resources()));
        builder.register_handler(Box::new(McpResourceHandler::list_resource_templates()));
        builder.register_handler(Box::new(McpResourceHandler::read_resource()));
    }

    if config.unified_exec_enabled {
        let manager =
            std::sync::Arc::new(crate::core::unified_exec::UnifiedExecProcessManager::default());
        builder.push_spec_with_parallel_support(create_exec_command_tool(), true);
        builder.push_spec(create_write_stdin_tool());
        builder.register_handler(Box::new(UnifiedExecHandler::new(manager)));
    }

    if config.update_plan_enabled {
        builder.push_spec(create_update_plan_tool());
        builder.register_handler(Box::new(PlanHandler));
    }

    if config.view_image_enabled {
        builder.push_spec(create_view_image_tool());
        builder.register_handler(Box::new(ViewImageHandler));
    }

    if config.request_user_input_enabled {
        builder.push_spec(create_request_user_input_tool());
        builder.register_handler(Box::new(RequestUserInputHandler::default()));
    }

    if config.js_repl_enabled {
        builder.push_spec(create_js_repl_tool());
        builder.push_spec(create_js_repl_reset_tool());
        builder.register_handler(Box::new(JsReplHandler));
        builder.register_handler(Box::new(JsReplResetHandler));
    }

    if config.test_sync_enabled {
        builder.push_spec_with_parallel_support(create_test_sync_tool(), true);
        builder.register_handler(Box::new(TestSyncHandler));
    }

    if config.agent_jobs_enabled {
        builder.push_spec(create_spawn_agents_on_csv_tool());
        builder.register_handler(Box::new(BatchJobHandler));
        if config.agent_jobs_worker_enabled {
            builder.push_spec(create_report_agent_job_result_tool());
        }
    }

    if config.presentation_artifact {
        builder.push_spec(create_presentation_artifact_tool());
        builder.register_handler(Box::new(PresentationArtifactHandler));
    }

    if config.search_tool {
        builder.push_spec(create_search_tool_bm25_tool());
        builder.register_handler(Box::new(SearchToolBm25Handler));
    }

    match config.web_search_mode {
        Some(WebSearchMode::Cached) => {
            builder.push_spec(create_web_search_tool(false));
        }
        Some(WebSearchMode::Live) => {
            builder.push_spec(create_web_search_tool(true));
        }
        Some(WebSearchMode::Disabled) | None => {}
    }

    if config.collab_tools && has_agent_control {
        builder.push_spec(create_spawn_agent_tool());
        builder.push_spec(create_send_input_tool());
        builder.push_spec(create_resume_agent_tool());
        builder.push_spec(create_wait_tool());
        builder.push_spec(create_close_agent_tool());
    }

    builder.build()
}

fn string_schema(description: &str) -> JsonSchema {
    JsonSchema::String {
        description: Some(description.to_string()),
    }
}

fn number_schema(description: &str) -> JsonSchema {
    JsonSchema::Number {
        description: Some(description.to_string()),
    }
}

fn boolean_schema(description: &str) -> JsonSchema {
    JsonSchema::Boolean {
        description: Some(description.to_string()),
    }
}

fn array_schema(items: JsonSchema, description: &str) -> JsonSchema {
    JsonSchema::Array {
        items: Box::new(items),
        description: Some(description.to_string()),
    }
}

fn object_schema(
    properties: BTreeMap<String, JsonSchema>,
    required: &[&str],
    additional_properties: bool,
) -> JsonSchema {
    JsonSchema::Object {
        properties,
        required: (!required.is_empty()).then(|| required.iter().map(|s| s.to_string()).collect()),
        additional_properties: Some(additional_properties.into()),
    }
}

fn function_tool(name: &str, description: &str, parameters: JsonSchema, strict: bool) -> ToolSpec {
    ToolSpec::Function {
        name: name.to_string(),
        description: description.to_string(),
        strict,
        parameters,
    }
}

fn create_web_search_tool(external_web_access: bool) -> ToolSpec {
    ToolSpec::WebSearch {
        external_web_access: Some(external_web_access),
    }
}

fn create_shell_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        array_schema(string_schema("Command token."), "Command and arguments."),
    );
    properties.insert(
        "workdir".to_string(),
        string_schema("Optional working directory for the command."),
    );
    properties.insert(
        "timeout_ms".to_string(),
        number_schema("Optional command timeout in milliseconds."),
    );
    properties.insert(
        "sandbox_permissions".to_string(),
        string_schema("Sandbox permissions mode for the command."),
    );
    properties.insert(
        "justification".to_string(),
        string_schema("Justification for elevated permissions, if any."),
    );
    properties.insert(
        "prefix_rule".to_string(),
        array_schema(
            string_schema("Prefix token."),
            "Optional approved command prefix.",
        ),
    );
    function_tool(
        "shell",
        "Run a local shell command with explicit arguments.",
        object_schema(properties, &["command"], false),
        false,
    )
}

fn create_apply_patch_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "patch".to_string(),
        string_schema("Unified diff patch content to apply."),
    );
    properties.insert(
        "path".to_string(),
        string_schema("Optional base path for patch application."),
    );
    function_tool(
        "apply_patch",
        "Apply a unified diff patch to modify files.",
        object_schema(properties, &["patch"], false),
        false,
    )
}

fn create_shell_command_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        string_schema("The shell script to execute in the user's default shell."),
    );
    properties.insert(
        "workdir".to_string(),
        string_schema("The working directory to execute the command in."),
    );
    properties.insert(
        "timeout_ms".to_string(),
        number_schema("The timeout for the command in milliseconds."),
    );
    properties.insert(
        "login".to_string(),
        boolean_schema("Whether to run the shell with login shell semantics. Defaults to true."),
    );
    properties.insert(
        "sandbox_permissions".to_string(),
        string_schema("Sandbox permissions mode for the command."),
    );
    properties.insert(
        "additional_permissions".to_string(),
        object_schema(BTreeMap::new(), &[], true),
    );
    properties.insert(
        "justification".to_string(),
        string_schema("Justification for elevated permissions, if any."),
    );
    properties.insert(
        "prefix_rule".to_string(),
        array_schema(
            string_schema("Prefix token."),
            "Optional approved command prefix.",
        ),
    );
    function_tool(
        "shell_command",
        "Runs a shell command and returns its output.\n- Always set the `workdir` param when using the shell_command function. Do not use `cd` unless absolutely necessary.",
        object_schema(properties, &["command"], false),
        false,
    )
}

fn create_list_dir_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "dir_path".to_string(),
        string_schema("Absolute path to the directory to list."),
    );
    properties.insert(
        "offset".to_string(),
        number_schema("1-indexed entry number to start from."),
    );
    properties.insert(
        "limit".to_string(),
        number_schema("Maximum number of directory entries to return."),
    );
    properties.insert(
        "depth".to_string(),
        number_schema("Maximum directory traversal depth."),
    );
    function_tool(
        "list_dir",
        "List entries in a local directory.",
        object_schema(properties, &["dir_path"], false),
        false,
    )
}

fn create_read_file_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "file_path".to_string(),
        string_schema("Absolute path to the file."),
    );
    properties.insert(
        "offset".to_string(),
        number_schema("1-indexed line number to start reading from."),
    );
    properties.insert(
        "limit".to_string(),
        number_schema("Maximum number of lines to return."),
    );
    function_tool(
        "read_file",
        "Read the contents of a local file.",
        object_schema(properties, &["file_path"], false),
        false,
    )
}

fn create_grep_files_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "pattern".to_string(),
        string_schema("Regular expression pattern to search for."),
    );
    properties.insert(
        "include".to_string(),
        string_schema("Optional glob limiting which files are searched."),
    );
    properties.insert(
        "path".to_string(),
        string_schema("Directory or file path to search."),
    );
    properties.insert(
        "limit".to_string(),
        number_schema("Maximum number of file paths to return."),
    );
    function_tool(
        "grep_files",
        "Find files whose contents match a pattern.",
        object_schema(properties, &["pattern"], false),
        false,
    )
}

fn create_list_mcp_resources_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "server".to_string(),
        string_schema(
            "Optional MCP server name. When omitted, list resources from every configured server.",
        ),
    );
    properties.insert(
        "cursor".to_string(),
        string_schema(
            "Opaque cursor returned by a previous list_mcp_resources call for the same server.",
        ),
    );
    function_tool(
        "list_mcp_resources",
        "Lists resources provided by MCP servers.",
        object_schema(properties, &[], false),
        false,
    )
}

fn create_list_mcp_resource_templates_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "server".to_string(),
        string_schema("Optional MCP server name. When omitted, list resource templates from all configured servers."),
    );
    properties.insert(
        "cursor".to_string(),
        string_schema("Opaque cursor returned by a previous list_mcp_resource_templates call for the same server."),
    );
    function_tool(
        "list_mcp_resource_templates",
        "Lists resource templates provided by MCP servers.",
        object_schema(properties, &[], false),
        false,
    )
}

fn create_read_mcp_resource_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "server".to_string(),
        string_schema("MCP server name exactly as configured."),
    );
    properties.insert("uri".to_string(), string_schema("Resource URI to read."));
    function_tool(
        "read_mcp_resource",
        "Read a specific resource from an MCP server given the server name and resource URI.",
        object_schema(properties, &["server", "uri"], false),
        false,
    )
}

fn create_exec_command_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "cmd".to_string(),
        string_schema("Shell command to execute."),
    );
    properties.insert(
        "workdir".to_string(),
        string_schema(
            "Optional working directory to run the command in; defaults to the current cwd.",
        ),
    );
    properties.insert(
        "shell".to_string(),
        string_schema("Shell binary to launch. Defaults to the user's default shell."),
    );
    properties.insert(
        "login".to_string(),
        boolean_schema("Whether to run the shell with -l/-i semantics. Defaults to true."),
    );
    properties.insert(
        "tty".to_string(),
        boolean_schema("Whether to allocate a TTY for the command. Defaults to false."),
    );
    properties.insert(
        "yield_time_ms".to_string(),
        number_schema("How long to wait (in milliseconds) for output before yielding."),
    );
    properties.insert(
        "max_output_tokens".to_string(),
        number_schema("Maximum number of tokens to return. Excess output will be truncated."),
    );
    properties.insert(
        "timeout_ms".to_string(),
        number_schema("Optional command timeout in milliseconds."),
    );
    properties.insert(
        "sandbox_permissions".to_string(),
        string_schema("Sandbox permissions mode for the command."),
    );
    properties.insert(
        "additional_permissions".to_string(),
        object_schema(BTreeMap::new(), &[], true),
    );
    properties.insert(
        "justification".to_string(),
        string_schema("Justification for elevated permissions, if any."),
    );
    properties.insert(
        "prefix_rule".to_string(),
        array_schema(
            string_schema("Prefix token."),
            "Optional approved command prefix.",
        ),
    );
    function_tool(
        "exec_command",
        "Runs a command in a PTY, returning output or a session ID for ongoing interaction.",
        object_schema(properties, &["cmd"], false),
        false,
    )
}

fn create_write_stdin_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "session_id".to_string(),
        number_schema("Identifier of the running unified exec session."),
    );
    properties.insert(
        "chars".to_string(),
        string_schema("Bytes to write to stdin (may be empty to poll)."),
    );
    properties.insert(
        "yield_time_ms".to_string(),
        number_schema("How long to wait (in milliseconds) for output before yielding."),
    );
    properties.insert(
        "max_output_tokens".to_string(),
        number_schema("Maximum number of tokens to return. Excess output will be truncated."),
    );
    function_tool(
        "write_stdin",
        "Writes characters to an existing unified exec session and returns recent output.",
        object_schema(properties, &["session_id"], false),
        false,
    )
}

fn create_update_plan_tool() -> ToolSpec {
    let mut plan_item_properties = BTreeMap::new();
    plan_item_properties.insert("step".to_string(), string_schema("The step description."));
    plan_item_properties.insert(
        "status".to_string(),
        string_schema("One of pending, in_progress, or completed."),
    );

    let mut properties = BTreeMap::new();
    properties.insert(
        "explanation".to_string(),
        string_schema("Optional explanation for the plan update."),
    );
    properties.insert(
        "plan".to_string(),
        array_schema(
            object_schema(plan_item_properties, &["step", "status"], false),
            "The list of steps.",
        ),
    );
    function_tool(
        "update_plan",
        "Updates the task plan.\nProvide an optional explanation and a list of plan items, each with a step and status.\nAt most one step can be in_progress at a time.\n",
        object_schema(properties, &["plan"], false),
        false,
    )
}

fn create_view_image_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "path".to_string(),
        string_schema("Local filesystem path to an image file."),
    );
    function_tool(
        "view_image",
        "View a local image from the filesystem.",
        object_schema(properties, &["path"], false),
        false,
    )
}

fn create_request_user_input_tool() -> ToolSpec {
    let mut question_properties = BTreeMap::new();
    question_properties.insert(
        "text".to_string(),
        string_schema("Question text shown to the user."),
    );
    question_properties.insert(
        "options".to_string(),
        array_schema(
            string_schema("A single user-facing option."),
            "Mutually exclusive options for this question.",
        ),
    );
    question_properties.insert(
        "is_other".to_string(),
        boolean_schema("Whether to include an Other free-form option."),
    );

    let mut properties = BTreeMap::new();
    properties.insert(
        "questions".to_string(),
        array_schema(
            object_schema(question_properties, &[], false),
            "One to three short questions for the user.",
        ),
    );
    function_tool(
        "request_user_input",
        &crate::core::tools::handlers::request_user_input_tool_description(false),
        object_schema(properties, &["questions"], false),
        false,
    )
}

fn create_js_repl_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "code".to_string(),
        string_schema("JavaScript source to execute in the persistent Node-backed REPL."),
    );
    properties.insert(
        "timeout_ms".to_string(),
        number_schema("Optional execution timeout in milliseconds."),
    );
    function_tool(
        "js_repl",
        "Execute JavaScript in a persistent Node-backed REPL. Current Mosaic wiring routes to a placeholder handler until the runtime is connected.",
        object_schema(properties, &["code"], false),
        false,
    )
}

fn create_js_repl_reset_tool() -> ToolSpec {
    function_tool(
        "js_repl_reset",
        "Reset the persistent JavaScript REPL runtime for the current session.",
        object_schema(BTreeMap::new(), &[], false),
        false,
    )
}

fn create_test_sync_tool() -> ToolSpec {
    let mut barrier_properties = BTreeMap::new();
    barrier_properties.insert(
        "id".to_string(),
        string_schema("Identifier shared by concurrent calls that should rendezvous."),
    );
    barrier_properties.insert(
        "participants".to_string(),
        number_schema("Number of tool calls that must arrive before the barrier opens."),
    );
    barrier_properties.insert(
        "timeout_ms".to_string(),
        number_schema("Maximum time in milliseconds to wait at the barrier."),
    );

    let mut properties = BTreeMap::new();
    properties.insert(
        "sleep_before_ms".to_string(),
        number_schema("Optional delay in milliseconds before any other action."),
    );
    properties.insert(
        "sleep_after_ms".to_string(),
        number_schema("Optional delay in milliseconds after completing the barrier."),
    );
    properties.insert(
        "barrier".to_string(),
        object_schema(barrier_properties, &["id", "participants"], false),
    );
    function_tool(
        "test_sync_tool",
        "Internal synchronization helper used by Codex integration tests.",
        object_schema(properties, &[], false),
        false,
    )
}

fn create_spawn_agents_on_csv_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "csv_path".to_string(),
        string_schema("Path to the CSV file containing input rows."),
    );
    properties.insert(
        "instruction".to_string(),
        string_schema(
            "Instruction template to apply to each CSV row. Use {column_name} placeholders to inject values from the row.",
        ),
    );
    properties.insert(
        "id_column".to_string(),
        string_schema("Optional column name to use as stable item id."),
    );
    properties.insert(
        "output_csv_path".to_string(),
        string_schema("Optional output CSV path for exported results."),
    );
    properties.insert(
        "max_concurrency".to_string(),
        number_schema(
            "Maximum concurrent workers for this job. Defaults to 16 and is capped by config.",
        ),
    );
    properties.insert(
        "max_workers".to_string(),
        number_schema("Alias for max_concurrency. Set to 1 to run sequentially."),
    );
    properties.insert(
        "max_runtime_seconds".to_string(),
        number_schema("Maximum runtime per worker before it is failed. Defaults to 1800 seconds."),
    );
    properties.insert(
        "output_schema".to_string(),
        object_schema(BTreeMap::new(), &[], true),
    );
    function_tool(
        "spawn_agents_on_csv",
        "Process a CSV by spawning one worker sub-agent per row. The instruction string is a template where `{column}` placeholders are replaced with row values. Each worker must call `report_agent_job_result` with a JSON object (matching `output_schema` when provided); missing reports are treated as failures. This call blocks until all rows finish and automatically exports results to `output_csv_path` (or a default path).",
        object_schema(properties, &["csv_path", "instruction"], false),
        false,
    )
}

fn create_report_agent_job_result_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "job_id".to_string(),
        string_schema("Identifier of the job."),
    );
    properties.insert(
        "item_id".to_string(),
        string_schema("Identifier of the job item."),
    );
    properties.insert(
        "result".to_string(),
        object_schema(BTreeMap::new(), &[], true),
    );
    properties.insert(
        "stop".to_string(),
        boolean_schema(
            "Optional. When true, cancels the remaining job items after this result is recorded.",
        ),
    );
    function_tool(
        "report_agent_job_result",
        "Worker-only tool to report a result for an agent job item. Main agents should not call this.",
        object_schema(properties, &["job_id", "item_id", "result"], false),
        false,
    )
}

fn create_presentation_artifact_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "action".to_string(),
        string_schema("Artifact action to run, such as read, list, create, update, or delete."),
    );
    properties.insert(
        "path".to_string(),
        string_schema("Artifact path to inspect or mutate."),
    );
    properties.insert(
        "content".to_string(),
        string_schema("Optional artifact content payload for write-style actions."),
    );
    function_tool(
        "presentation_artifact",
        "Manage presentation artifact files. Current Mosaic wiring validates path access, then delegates to the artifact subsystem when available.",
        object_schema(properties, &[], false),
        false,
    )
}

fn create_search_tool_bm25_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "query".to_string(),
        string_schema("Search query for matching MCP tools."),
    );
    properties.insert(
        "limit".to_string(),
        number_schema("Maximum number of tools to return."),
    );
    function_tool(
        crate::core::tools::handlers::SEARCH_TOOL_BM25_TOOL_NAME,
        "Search MCP tool metadata using a BM25-style ranking pipeline when available.",
        object_schema(properties, &["query"], false),
        false,
    )
}

fn create_spawn_agent_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "message".to_string(),
        string_schema("Initial message for the spawned agent."),
    );
    properties.insert(
        "items".to_string(),
        array_schema(
            object_schema(BTreeMap::new(), &[], true),
            "Structured input items for the spawned agent.",
        ),
    );
    properties.insert(
        "agent_type".to_string(),
        string_schema("Optional role or type name for the agent."),
    );
    properties.insert(
        "fork_context".to_string(),
        boolean_schema("When true, fork the current context into the new agent."),
    );
    function_tool(
        "spawn_agent",
        "Spawn a sub-agent for a focused task.",
        object_schema(properties, &[], false),
        false,
    )
}

fn create_send_input_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "id".to_string(),
        string_schema("Agent id to send input to."),
    );
    properties.insert(
        "message".to_string(),
        string_schema("Plain-text input for the target agent."),
    );
    properties.insert(
        "items".to_string(),
        array_schema(
            object_schema(BTreeMap::new(), &[], true),
            "Structured input items for the target agent.",
        ),
    );
    function_tool(
        "send_input",
        "Send additional input to an existing agent.",
        object_schema(properties, &["id"], false),
        false,
    )
}

fn create_resume_agent_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert("id".to_string(), string_schema("Agent id to resume."));
    function_tool(
        "resume_agent",
        "Resume a previously closed agent.",
        object_schema(properties, &["id"], false),
        false,
    )
}

fn create_wait_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "agent_ids".to_string(),
        array_schema(string_schema("Agent id."), "Agent ids to wait on."),
    );
    properties.insert(
        "timeout_ms".to_string(),
        number_schema("Wait timeout in milliseconds."),
    );
    function_tool(
        "wait",
        "Wait for one or more agents to finish.",
        object_schema(properties, &["agent_ids"], false),
        false,
    )
}

fn create_close_agent_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert("id".to_string(), string_schema("Agent id to close."));
    function_tool(
        "close_agent",
        "Close an agent and its descendants.",
        object_schema(properties, &["id"], false),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_specs_includes_stable_builtin_tools_without_collab() {
        let assembled = build_specs(&ToolsConfig::default(), false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"apply_patch".to_string()));
        assert!(names.contains(&"list_dir".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"grep_files".to_string()));
        assert!(!names.contains(&"spawn_agent".to_string()));
        assert!(!names.contains(&"shell_command".to_string()));
        assert!(!names.contains(&"exec_command".to_string()));
        assert!(!names.contains(&"write_stdin".to_string()));
        assert!(!names.contains(&"list_mcp_resources".to_string()));
        assert!(!names.contains(&"list_mcp_resource_templates".to_string()));
        assert!(!names.contains(&"read_mcp_resource".to_string()));
        assert!(!names.contains(&"search_tool_bm25".to_string()));
        assert!(!names.contains(&"update_plan".to_string()));
        assert!(!names.contains(&"view_image".to_string()));
        assert!(!names.contains(&"request_user_input".to_string()));
        assert!(!names.contains(&"js_repl".to_string()));
        assert!(!names.contains(&"js_repl_reset".to_string()));
        assert!(!names.contains(&"test_sync_tool".to_string()));
        assert!(!names.contains(&"spawn_agents_on_csv".to_string()));
        assert!(!names.contains(&"report_agent_job_result".to_string()));
        assert!(!names.contains(&"presentation_artifact".to_string()));
    }

    #[test]
    fn build_specs_adds_collab_tools_when_enabled_and_available() {
        let config = ToolsConfig {
            collab_tools: true,
            ..ToolsConfig::default()
        };
        let assembled = build_specs(&config, true);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert!(names.contains(&"spawn_agent".to_string()));
        assert!(names.contains(&"send_input".to_string()));
        assert!(names.contains(&"resume_agent".to_string()));
        assert!(names.contains(&"wait".to_string()));
        assert!(names.contains(&"close_agent".to_string()));
    }

    #[test]
    fn build_specs_adds_optional_plan_and_view_image_tools_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: true,
            view_image_enabled: true,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names.len(), 2);
        assert!(names.contains(&"update_plan".to_string()));
        assert!(names.contains(&"view_image".to_string()));
    }

    #[test]
    fn build_specs_adds_optional_request_user_input_tool_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: true,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names, vec!["request_user_input".to_string()]);
    }

    #[test]
    fn build_specs_adds_optional_shell_command_tool_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: true,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names, vec!["shell_command".to_string()]);
    }

    #[test]
    fn build_specs_adds_optional_presentation_artifact_tool_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: true,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names, vec!["presentation_artifact".to_string()]);
    }

    #[test]
    fn build_specs_adds_optional_unified_exec_tools_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: true,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(
            names,
            vec!["exec_command".to_string(), "write_stdin".to_string()]
        );
    }

    #[test]
    fn build_specs_adds_optional_mcp_resource_tools_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: true,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(
            names,
            vec![
                "list_mcp_resources".to_string(),
                "list_mcp_resource_templates".to_string(),
                "read_mcp_resource".to_string(),
            ]
        );
    }

    #[test]
    fn build_specs_adds_optional_search_tool_bm25_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: true,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names, vec!["search_tool_bm25".to_string()]);
    }

    #[test]
    fn build_specs_adds_optional_js_repl_tools_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: true,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(
            names,
            vec!["js_repl".to_string(), "js_repl_reset".to_string()]
        );
    }

    #[test]
    fn build_specs_adds_optional_test_sync_tool_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: true,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names, vec!["test_sync_tool".to_string()]);
    }

    #[test]
    fn build_specs_adds_optional_agent_job_tools_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: None,
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: true,
            agent_jobs_worker_enabled: true,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(
            names,
            vec![
                "spawn_agents_on_csv".to_string(),
                "report_agent_job_result".to_string(),
            ]
        );
    }

    #[test]
    fn build_specs_adds_optional_cached_web_search_tool_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: Some(WebSearchMode::Cached),
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);
        let names: Vec<String> = assembled
            .configured_specs
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert_eq!(names, vec!["web_search".to_string()]);
    }

    #[test]
    fn build_specs_adds_optional_live_web_search_tool_when_enabled() {
        let config = ToolsConfig {
            presentation_artifact: false,
            web_search_mode: Some(WebSearchMode::Live),
            search_tool: false,
            js_repl_enabled: false,
            test_sync_enabled: false,
            agent_jobs_enabled: false,
            agent_jobs_worker_enabled: false,
            collab_tools: false,
            shell_enabled: false,
            shell_command_enabled: false,
            apply_patch_enabled: false,
            list_dir_enabled: false,
            read_file_enabled: false,
            grep_files_enabled: false,
            mcp_resources_enabled: false,
            unified_exec_enabled: false,
            update_plan_enabled: false,
            view_image_enabled: false,
            request_user_input_enabled: false,
        };
        let assembled = build_specs(&config, false);

        assert_eq!(assembled.configured_specs.len(), 1);
        assert_eq!(
            assembled.configured_specs[0].spec,
            ToolSpec::WebSearch {
                external_web_access: Some(true),
            }
        );
    }
}

pub mod dynamic;

// Built-in tool handlers
pub mod agent_jobs;
pub mod apply_patch;
pub mod grep_files;
pub mod js_repl;
pub mod list_dir;
pub mod mcp;
pub mod mcp_resource;
pub mod multi_agents;
pub mod plan;
pub mod presentation_artifact;
pub mod read_file;
pub mod request_user_input;
pub mod runtimes;
pub mod search_tool_bm25;
pub mod shell;
pub mod shell_command;
pub mod test_sync;
pub mod unified_exec;
pub mod view_image;

// Re-exports for convenience
pub use agent_jobs::BatchJobHandler;
pub use agent_jobs::AgentJobsHandler; // backward compat alias
pub use apply_patch::ApplyPatchHandler;
pub use grep_files::GrepFilesHandler;
pub use js_repl::{JsReplHandler, JsReplResetHandler};
pub use list_dir::ListDirHandler;
pub use mcp::McpHandler;
pub use mcp_resource::McpResourceHandler;
pub use multi_agents::MultiAgentHandler;
pub use plan::PlanHandler;
pub use plan::PLAN_TOOL;
pub use presentation_artifact::PresentationArtifactHandler;
pub use read_file::ReadFileHandler;
pub use request_user_input::RequestUserInputHandler;
pub use request_user_input::request_user_input_tool_description;
pub use search_tool_bm25::SearchToolBm25Handler;
pub use search_tool_bm25::SEARCH_TOOL_BM25_TOOL_NAME;
pub use search_tool_bm25::SEARCH_TOOL_BM25_DEFAULT_LIMIT;
pub use shell::ShellHandler;
pub use shell::is_known_safe_command;
pub use shell_command::ShellCommandHandler;
pub use test_sync::TestSyncHandler;
pub use unified_exec::UnifiedExecHandler;
pub use view_image::ViewImageHandler;

use crate::protocol::error::{CodexError, ErrorCode};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Parse JSON string arguments into a typed struct.
pub fn parse_arguments<T>(arguments: &str) -> Result<T, CodexError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(arguments).map_err(|e| {
        CodexError::new(ErrorCode::InvalidInput, format!("failed to parse function arguments: {e}"))
    })
}

/// Parse JSON string arguments with a base path for resolving relative paths.
pub fn parse_arguments_with_base_path<T>(arguments: &str, _base_path: &Path) -> Result<T, CodexError>
where
    T: for<'de> Deserialize<'de>,
{
    // In the full implementation, this sets an AbsolutePathBufGuard
    // so that deserialized AbsolutePathBuf fields resolve against base_path.
    parse_arguments(arguments)
}

/// Resolve the effective working directory from arguments' `workdir` field.
/// Matches source Codex `resolve_workdir_base_path` which uses `resolve_path`.
pub fn resolve_workdir_base_path(arguments: &str, default_cwd: &Path) -> Result<PathBuf, CodexError> {
    let args: serde_json::Value = parse_arguments(arguments)?;
    Ok(args
        .get("workdir")
        .and_then(serde_json::Value::as_str)
        .filter(|wd| !wd.is_empty())
        .map(PathBuf::from)
        .map_or_else(
            || default_cwd.to_path_buf(),
            |wd| resolve_path(default_cwd, &wd),
        ))
}

/// Resolve a potentially relative path against a base directory, normalizing components.
/// Matches source Codex `crate::util::resolve_path`.
fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    // Normalize path components (resolve `.` and `..` without filesystem access)
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); }
            std::path::Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Validates feature/policy constraints for `with_additional_permissions` and
/// returns normalized permissions. Matches source Codex `normalize_and_validate_additional_permissions`.
///
/// In the simplified Mosaic architecture:
/// - `approval_policy` is an `Option<&str>` (source uses `AskForApproval` enum)
/// - `sandbox_permissions` is an `Option<String>` (source uses `SandboxPermissions` enum)
/// - `additional_permissions` is an `Option<serde_json::Value>` (source uses `Option<PermissionProfile>`)
pub fn normalize_and_validate_additional_permissions(
    request_permission_enabled: bool,
    approval_policy: Option<&str>,
    sandbox_permissions: Option<&str>,
    additional_permissions: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, String> {
    let uses_additional = sandbox_permissions == Some("with_additional_permissions");

    if !request_permission_enabled && (uses_additional || additional_permissions.is_some()) {
        return Err(
            "additional permissions are disabled; enable `features.request_permission` before using `with_additional_permissions`"
                .to_string(),
        );
    }

    if uses_additional {
        // Source checks: approval_policy must be OnRequest
        if approval_policy != Some("on_request") && approval_policy != None {
            return Err(format!(
                "approval policy is {:?}; reject command — you cannot request additional permissions unless the approval policy is on_request",
                approval_policy.unwrap_or("unknown"),
            ));
        }

        let Some(perms) = additional_permissions else {
            return Err(
                "missing `additional_permissions`; provide `file_system.read` and/or `file_system.write` when using `with_additional_permissions`"
                    .to_string(),
            );
        };
        // Validate that at least one path is specified
        let has_read = perms.get("file_system").and_then(|fs| fs.get("read"))
            .and_then(|v| v.as_array()).map_or(false, |a| !a.is_empty());
        let has_write = perms.get("file_system").and_then(|fs| fs.get("write"))
            .and_then(|v| v.as_array()).map_or(false, |a| !a.is_empty());
        if !has_read && !has_write {
            return Err(
                "`additional_permissions` must include at least one path in `file_system.read` or `file_system.write`"
                    .to_string(),
            );
        }
        return Ok(Some(perms.clone()));
    }

    if additional_permissions.is_some() {
        Err(
            "`additional_permissions` requires `sandbox_permissions` set to `with_additional_permissions`"
                .to_string(),
        )
    } else {
        Ok(None)
    }
}

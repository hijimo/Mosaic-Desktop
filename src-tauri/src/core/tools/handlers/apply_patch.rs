use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ApplyPatchHandler;

/// Lark grammar for freeform apply_patch validation (embedded from source).
pub const APPLY_PATCH_LARK_GRAMMAR: &str = include_str!("tool_apply_patch.lark");

#[derive(Deserialize)]
struct ApplyPatchArgs {
    input: String,
}

/// Represents a parsed file change from the patch.
#[derive(Debug, Clone)]
pub enum PatchFileChange {
    Add { path: String },
    Delete { path: String },
    Update { path: String, move_to: Option<String> },
}

#[async_trait]
impl ToolHandler for ApplyPatchHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "apply_patch")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("apply_patch".to_string())
    }

    fn tool_spec(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "name": "apply_patch",
            "description": "Apply a unified diff patch to modify files. Use this to make targeted edits to existing files.",
            "parameters": {
                "type": "object",
                "properties": {
                    "patch": {
                        "type": "string",
                        "description": "The unified diff patch content to apply"
                    },
                    "path": {
                        "type": "string",
                        "description": "Base path for the patch"
                    }
                },
                "required": ["patch"],
                "additionalProperties": false
            }
        }))
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Support both freeform (string) and JSON object input
        let patch_text = if let Some(s) = args.as_str() {
            s.to_string()
        } else {
            let params: ApplyPatchArgs = serde_json::from_value(args).map_err(|e| {
                CodexError::new(ErrorCode::InvalidInput, format!("invalid apply_patch args: {e}"))
            })?;
            params.input
        };

        if patch_text.trim().is_empty() {
            return Err(CodexError::new(ErrorCode::InvalidInput, "empty patch input"));
        }

        // Parse the patch to extract file paths for approval tracking
        let file_changes = parse_patch_file_changes(&patch_text);
        let file_paths: Vec<String> = file_changes.iter().flat_map(|c| match c {
            PatchFileChange::Add { path } => vec![path.clone()],
            PatchFileChange::Delete { path } => vec![path.clone()],
            PatchFileChange::Update { path, move_to } => {
                let mut paths = vec![path.clone()];
                if let Some(dest) = move_to { paths.push(dest.clone()); }
                paths
            }
        }).collect();

        // Apply the patch using git apply
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let result = apply_patch_via_git(&patch_text, &cwd).await?;

        Ok(serde_json::json!({
            "success": result.success,
            "output": result.output,
            "file_paths": file_paths,
        }))
    }
}

struct ApplyResult {
    success: bool,
    output: String,
}

async fn apply_patch_via_git(patch_text: &str, cwd: &Path) -> Result<ApplyResult, CodexError> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["apply", "--verbose", "-"]);
    cmd.current_dir(cwd);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        CodexError::new(ErrorCode::ToolExecutionFailed, format!("failed to spawn git apply: {e}"))
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(patch_text.as_bytes()).await;
        drop(stdin);
    }

    let output = child.wait_with_output().await.map_err(|e| {
        CodexError::new(ErrorCode::ToolExecutionFailed, format!("git apply error: {e}"))
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.is_empty() { stdout.to_string() } else { format!("{stdout}\n{stderr}") };

    Ok(ApplyResult {
        success: output.status.success(),
        output: combined,
    })
}

/// Parse patch text to extract file changes (Add/Delete/Update with optional Move).
pub fn parse_patch_file_changes(patch: &str) -> Vec<PatchFileChange> {
    let mut changes = Vec::new();
    let mut lines = patch.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(path) = trimmed.strip_prefix("*** Add File: ") {
            changes.push(PatchFileChange::Add { path: path.trim().to_string() });
        } else if let Some(path) = trimmed.strip_prefix("*** Delete File: ") {
            changes.push(PatchFileChange::Delete { path: path.trim().to_string() });
        } else if let Some(path) = trimmed.strip_prefix("*** Update File: ") {
            let path = path.trim().to_string();
            // Check for optional Move to
            let move_to = if let Some(next) = lines.peek() {
                if let Some(dest) = next.trim().strip_prefix("*** Move to: ") {
                    let dest = dest.trim().to_string();
                    lines.next(); // consume the Move to line
                    Some(dest)
                } else {
                    None
                }
            } else {
                None
            };
            changes.push(PatchFileChange::Update { path, move_to });
        }
    }

    changes
}

/// Intercept apply_patch commands from shell/exec handlers.
/// Matches source Codex `intercept_apply_patch` — checks for apply_patch invocations,
/// validates the patch, and returns the result directly if intercepted.
///
/// Returns `Ok(Some(result))` if intercepted, `Ok(None)` if not an apply_patch command,
/// or `Err` if the patch is malformed.
pub async fn intercept_apply_patch(
    command: &[String],
    cwd: &Path,
    _timeout_ms: Option<u64>,
) -> Result<Option<serde_json::Value>, CodexError> {
    // Check if this is an apply_patch command
    if !is_apply_patch_command(command) {
        return Ok(None);
    }

    // Extract the patch text from the command
    let patch_text = extract_patch_text(command);
    let Some(patch_text) = patch_text else {
        return Ok(None);
    };

    // Validate the patch structure
    match validate_patch_structure(&patch_text) {
        PatchValidation::Valid => {}
        PatchValidation::CorrectnessError(err) => {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                format!("apply_patch verification failed: {err}"),
            ));
        }
        PatchValidation::NotApplyPatch => return Ok(None),
    }

    // Apply the patch
    let result = apply_patch_via_git(&patch_text, cwd).await?;
    let file_changes = parse_patch_file_changes(&patch_text);
    let file_paths: Vec<String> = file_changes.iter().flat_map(|c| match c {
        PatchFileChange::Add { path } => vec![path.clone()],
        PatchFileChange::Delete { path } => vec![path.clone()],
        PatchFileChange::Update { path, move_to } => {
            let mut paths = vec![path.clone()];
            if let Some(dest) = move_to { paths.push(dest.clone()); }
            paths
        }
    }).collect();

    Ok(Some(serde_json::json!({
        "success": result.success,
        "output": result.output,
        "file_paths": file_paths,
    })))
}

/// Extract patch text from a command array.
fn extract_patch_text(command: &[String]) -> Option<String> {
    match command.first().map(|s| s.as_str()) {
        Some("apply_patch") => command.get(1).cloned(),
        Some(prog) => {
            let base = prog.rsplit('/').next().unwrap_or(prog);
            if matches!(base, "bash" | "zsh" | "sh") {
                command.iter().position(|a| a == "-c")
                    .and_then(|pos| command.get(pos + 1))
                    .and_then(|cmd| {
                        if cmd.starts_with("apply_patch ") {
                            Some(cmd.strip_prefix("apply_patch ")?.to_string())
                        } else {
                            None
                        }
                    })
            } else {
                None
            }
        }
        None => None,
    }
}

/// Patch validation result, matching source Codex `MaybeApplyPatchVerified` variants.
enum PatchValidation {
    Valid,
    CorrectnessError(String),
    NotApplyPatch,
}

/// Validate the structural correctness of a patch.
/// Checks for `*** Begin Patch` / `*** End Patch` envelope and valid file operations.
fn validate_patch_structure(patch: &str) -> PatchValidation {
    let trimmed = patch.trim();
    if !trimmed.starts_with("*** Begin Patch") {
        return PatchValidation::NotApplyPatch;
    }
    if !trimmed.ends_with("*** End Patch") {
        return PatchValidation::CorrectnessError(
            "patch must end with '*** End Patch'".to_string(),
        );
    }
    // Check that there's at least one file operation
    let has_file_op = trimmed.contains("*** Add File:") 
        || trimmed.contains("*** Delete File:") 
        || trimmed.contains("*** Update File:");
    if !has_file_op {
        return PatchValidation::CorrectnessError(
            "patch must contain at least one file operation (Add/Delete/Update)".to_string(),
        );
    }
    PatchValidation::Valid
}

/// Checks for both direct `apply_patch` invocations and `git apply` variants.
pub fn is_apply_patch_command(command: &[String]) -> bool {
    match command.first().map(|s| s.as_str()) {
        Some("apply_patch") => true,
        Some(prog) => {
            let base = prog.rsplit('/').next().unwrap_or(prog);
            // Detect: git apply ..., or shell -c "git apply ..."
            if base == "git" {
                command.get(1).map(|s| s.as_str()) == Some("apply")
            } else if matches!(base, "bash" | "zsh" | "sh") {
                // Check -c argument for "apply_patch" or "git apply"
                command.iter().position(|a| a == "-c")
                    .and_then(|pos| command.get(pos + 1))
                    .map(|cmd| cmd.starts_with("apply_patch") || cmd.starts_with("git apply"))
                    .unwrap_or(false)
            } else {
                false
            }
        }
        None => false,
    }
}

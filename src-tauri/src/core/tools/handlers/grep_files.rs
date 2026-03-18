use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct GrepFilesHandler;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 2000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

fn default_limit() -> usize { DEFAULT_LIMIT }

#[derive(Deserialize)]
struct GrepFilesArgs {
    pattern: String,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[async_trait]
impl ToolHandler for GrepFilesHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "grep_files")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("grep_files".to_string())
    }

    fn tool_spec(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "name": "grep_files",
            "description": "Search for a pattern in files using regex. Returns matching lines with file paths and line numbers.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in"
                    },
                    "include": {
                        "type": "string",
                        "description": "Glob pattern for files to include (e.g. '*.rs')"
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }
        }))
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: GrepFilesArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid grep_files args: {e}"))
        })?;

        let pattern = params.pattern.trim();
        if pattern.is_empty() {
            return Err(CodexError::new(ErrorCode::InvalidInput, "pattern must not be empty"));
        }
        if params.limit == 0 {
            return Err(CodexError::new(ErrorCode::InvalidInput, "limit must be greater than zero"));
        }

        let limit = params.limit.min(MAX_LIMIT);
        let cwd = std::env::current_dir().unwrap_or_default();
        let search_path = params.path.as_deref().unwrap_or(".");

        // Verify path exists (resolve against cwd for relative paths)
        let resolved = if Path::new(search_path).is_absolute() {
            std::path::PathBuf::from(search_path)
        } else {
            cwd.join(search_path)
        };
        verify_path_exists(&resolved).await?;

        let include = params.include.as_deref()
            .map(str::trim)
            .and_then(|v| if v.is_empty() { None } else { Some(v.to_string()) });

        let results = run_rg_search(pattern, include.as_deref(), search_path, limit, &cwd).await?;

        if results.is_empty() {
            Ok(serde_json::json!({
                "matches": [],
                "message": "No matches found.",
            }))
        } else {
            Ok(serde_json::json!({
                "matches": results,
            }))
        }
    }
}

async fn verify_path_exists(path: &Path) -> Result<(), CodexError> {
    tokio::fs::metadata(path).await.map_err(|e| {
        CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("unable to access `{}`: {e}", path.display()),
        )
    })?;
    Ok(())
}

async fn run_rg_search(
    pattern: &str,
    include: Option<&str>,
    search_path: &str,
    limit: usize,
    cwd: &Path,
) -> Result<Vec<String>, CodexError> {
    let mut command = tokio::process::Command::new("rg");
    command
        .current_dir(cwd)
        .arg("--files-with-matches")
        .arg("--sortr=modified")
        .arg("--regexp")
        .arg(pattern)
        .arg("--no-messages");

    if let Some(glob) = include {
        command.arg("--glob").arg(glob);
    }

    command.arg("--").arg(search_path);

    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| CodexError::new(ErrorCode::ToolExecutionFailed, "rg timed out after 30 seconds"))?
        .map_err(|e| CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("failed to launch rg: {e}. Ensure ripgrep is installed and on PATH."),
        ))?;

    match output.status.code() {
        Some(0) => Ok(parse_results(&output.stdout, limit)),
        Some(1) => Ok(Vec::new()), // no matches
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CodexError::new(ErrorCode::ToolExecutionFailed, format!("rg failed: {stderr}")))
        }
    }
}

fn parse_results(stdout: &[u8], limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    for line in stdout.split(|b| *b == b'\n') {
        if line.is_empty() { continue; }
        if let Ok(text) = std::str::from_utf8(line) {
            if text.is_empty() { continue; }
            results.push(text.to_string());
            if results.len() >= limit { break; }
        }
    }
    results
}

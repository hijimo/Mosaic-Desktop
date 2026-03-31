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

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

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
            "description": "Finds files whose contents match the pattern and lists them by modification time.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regular expression pattern to search for."
                    },
                    "include": {
                        "type": "string",
                        "description": "Optional glob that limits which files are searched (e.g. \"*.rs\" or \"*.{ts,tsx}\")."
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file path to search. Defaults to the session's working directory."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of file paths to return (defaults to 100)."
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }
        }))
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: GrepFilesArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid grep_files args: {e}"),
            )
        })?;

        let pattern = params.pattern.trim();
        if pattern.is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "pattern must not be empty",
            ));
        }
        if params.limit == 0 {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "limit must be greater than zero",
            ));
        }

        let limit = params.limit.min(MAX_LIMIT);
        let cwd = std::env::current_dir().unwrap_or_default();
        let search_path = params.path.as_deref().unwrap_or(".");

        let resolved = if Path::new(search_path).is_absolute() {
            std::path::PathBuf::from(search_path)
        } else {
            cwd.join(search_path)
        };
        verify_path_exists(&resolved).await?;

        let include = params.include.as_deref().map(str::trim).and_then(|v| {
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        });

        // Try rg first; fall back to built-in regex+ignore search
        let results =
            match run_rg_search(pattern, include.as_deref(), search_path, limit, &cwd).await {
                Ok(r) => r,
                Err(_) => run_builtin_search(pattern, include.as_deref(), &resolved, limit).await?,
            };

        if results.is_empty() {
            Ok(serde_json::json!("No matches found."))
        } else {
            Ok(serde_json::Value::String(results.join("\n")))
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

// ── Primary: rg (matches source codex-main) ──────────────────────

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
        .map_err(|_| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                "rg timed out after 30 seconds",
            )
        })?
        .map_err(|e| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("rg not available: {e}"),
            )
        })?;

    match output.status.code() {
        Some(0) => Ok(parse_results(&output.stdout, limit)),
        Some(1) => Ok(Vec::new()),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("rg failed: {stderr}"),
            ))
        }
    }
}

fn parse_results(stdout: &[u8], limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    for line in stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(line) {
            if text.is_empty() {
                continue;
            }
            results.push(text.to_string());
            if results.len() == limit {
                break;
            }
        }
    }
    results
}

// ── Fallback: built-in regex + ignore (no external dependency) ───

async fn run_builtin_search(
    pattern: &str,
    include: Option<&str>,
    search_path: &Path,
    limit: usize,
) -> Result<Vec<String>, CodexError> {
    let pattern = pattern.to_string();
    let include = include.map(String::from);
    let search_path = search_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        builtin_grep(&pattern, include.as_deref(), &search_path, limit)
    })
    .await
    .map_err(|e| {
        CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("search task failed: {e}"),
        )
    })?
}

fn builtin_grep(
    pattern: &str,
    include: Option<&str>,
    search_path: &Path,
    limit: usize,
) -> Result<Vec<String>, CodexError> {
    let re = regex::Regex::new(pattern)
        .map_err(|e| CodexError::new(ErrorCode::InvalidInput, format!("invalid regex: {e}")))?;

    let mut builder = ignore::WalkBuilder::new(search_path);
    builder.hidden(false).git_ignore(true).git_global(true);

    if let Some(glob) = include {
        let mut overrides = ignore::overrides::OverrideBuilder::new(search_path);
        overrides.add(glob).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid glob '{glob}': {e}"),
            )
        })?;
        let built = overrides.build().map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("glob build error: {e}"))
        })?;
        builder.overrides(built);
    }

    // Collect matching files with mtime for sorting
    let mut hits: Vec<(std::time::SystemTime, String)> = Vec::new();

    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().map_or(true, |ft| !ft.is_file()) {
            continue;
        }

        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if re.is_match(&content) {
            let mtime = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let display = path
                .strip_prefix(search_path)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            hits.push((mtime, display));
        }
    }

    // Sort by modification time descending (matches rg --sortr=modified)
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(hits.into_iter().take(limit).map(|(_, p)| p).collect())
}

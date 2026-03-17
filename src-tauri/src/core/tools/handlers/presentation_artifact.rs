use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct PresentationArtifactHandler;

#[derive(Debug, Deserialize)]
struct PresentationArtifactArgs {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum PathAccessKind {
    Read,
    Write,
}

#[async_trait]
impl ToolHandler for PresentationArtifactHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "presentation_artifact")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("presentation_artifact".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Feature flag check (matches source: session.enabled(Feature::Artifact))
        // TODO: wire to actual feature flag system
        let artifact_enabled = true;
        if !artifact_enabled {
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                "presentation_artifact is disabled by feature flag",
            ));
        }

        let params: PresentationArtifactArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid presentation_artifact args: {e}"))
        })?;

        // Determine access kind based on action (read vs write)
        let access_kind = match params.action.as_deref() {
            Some("read") | Some("list") => PathAccessKind::Read,
            _ => PathAccessKind::Write, // create/update/delete default to write
        };

        // Path access authorization check — matches source's
        // `request.required_path_accesses(&turn.cwd)` then `authorize_path_access` per path
        let required_accesses = required_path_accesses(&params, access_kind);
        for (path, kind) in &required_accesses {
            authorize_path_access(path, *kind)?;
        }

        // Full implementation calls session.execute_presentation_artifact()
        // TODO: wire to actual presentation artifact execution
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            "presentation_artifact execution requires the artifact subsystem",
        ))
    }
}

/// Extract required path accesses from the request, matching source's
/// `PresentationArtifactToolRequest::required_path_accesses`.
fn required_path_accesses(params: &PresentationArtifactArgs, default_kind: PathAccessKind) -> Vec<(String, PathAccessKind)> {
    let mut accesses = Vec::new();
    if let Some(ref path) = params.path {
        accesses.push((path.clone(), default_kind));
    }
    accesses
}

fn authorize_path_access(path: &str, kind: PathAccessKind) -> Result<(), CodexError> {
    let p = std::path::Path::new(path);

    // Reject path traversal
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                "path traversal (..) is not allowed in presentation_artifact",
            ));
        }
    }

    // Resolve effective path for access check
    let effective_path = effective_path(p, kind);

    // Check if path is accessible under current sandbox policy.
    // Source checks path_is_readable/path_is_writable against sandbox_policy readable/writable roots.
    // In Mosaic, we check against a configurable set of allowed roots.
    // TODO: wire to actual sandbox_policy when available
    if !path_is_allowed(&effective_path, kind) {
        return Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!(
                "{} path `{}` is outside the current sandbox policy",
                match kind { PathAccessKind::Read => "read", PathAccessKind::Write => "write" },
                p.display(),
            ),
        ));
    }

    Ok(())
}

/// Check if a path is allowed under the current sandbox policy.
/// Source checks `sandbox_policy.get_readable_roots_with_cwd` / `get_writable_roots_with_cwd`.
/// Simplified: allow paths under cwd.
fn path_is_allowed(path: &std::path::Path, kind: PathAccessKind) -> bool {
    let cwd = std::env::current_dir().unwrap_or_default();
    match kind {
        PathAccessKind::Read => {
            // Read: allow anything under cwd (source checks readable_roots)
            path.starts_with(&cwd)
        }
        PathAccessKind::Write => {
            // Write: allow anything under cwd (source checks writable_roots)
            path.starts_with(&cwd)
        }
    }
}

/// Resolve the effective path for access checking.
/// For reads, resolve symlinks to check the real target.
/// For writes, use the parent directory (the file may not exist yet).
fn effective_path(path: &std::path::Path, kind: PathAccessKind) -> std::path::PathBuf {
    match kind {
        PathAccessKind::Read => {
            // Try to resolve symlinks for read access
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        PathAccessKind::Write => {
            // For write, check the parent directory
            path.parent().map(|p| {
                std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
            }).unwrap_or_else(|| path.to_path_buf())
        }
    }
}

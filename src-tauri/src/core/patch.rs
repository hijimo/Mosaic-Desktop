use std::collections::HashMap;
use std::path::PathBuf;

use crate::protocol::types::FileChange;

/// Apply a set of file changes (add/delete/update) to the filesystem.
pub async fn apply_patch(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &std::path::Path,
) -> Result<PatchResult, crate::protocol::error::CodexError> {
    let mut applied = Vec::new();
    let mut failed = Vec::new();

    for (path, change) in changes {
        let full_path = if path.is_absolute() {
            path.clone()
        } else {
            cwd.join(path)
        };

        let result = match change {
            FileChange::Add { content } => {
                if let Some(parent) = full_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                tokio::fs::write(&full_path, content).await
            }
            FileChange::Delete { .. } => tokio::fs::remove_file(&full_path).await,
            FileChange::Update { unified_diff: _, move_path: _ } => {
                // TODO: apply unified diff properly
                // For now, just acknowledge the patch
                Ok(())
            }
        };

        match result {
            Ok(()) => applied.push(path.clone()),
            Err(e) => failed.push((path.clone(), e.to_string())),
        }
    }

    Ok(PatchResult { applied, failed })
}

/// Result of applying a patch.
#[derive(Debug)]
pub struct PatchResult {
    pub applied: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

impl PatchResult {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }
}

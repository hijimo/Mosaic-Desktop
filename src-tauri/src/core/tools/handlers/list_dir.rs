use async_trait::async_trait;
use serde::Deserialize;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::FileType;
use std::path::{Path, PathBuf};

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ListDirHandler;

const MAX_ENTRY_LENGTH: usize = 500;
const INDENTATION_SPACES: usize = 2;

fn default_offset() -> usize { 1 }
fn default_limit() -> usize { 25 }
fn default_depth() -> usize { 2 }

#[derive(Deserialize)]
struct ListDirArgs {
    dir_path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_depth")]
    depth: usize,
}

#[async_trait]
impl ToolHandler for ListDirHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "list_dir")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("list_dir".to_string())
    }

    fn tool_spec(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "name": "list_dir",
            "description": "Lists entries in a local directory with 1-indexed entry numbers and simple type labels.",
            "parameters": {
                "type": "object",
                "properties": {
                    "dir_path": {
                        "type": "string",
                        "description": "Absolute path to the directory to list."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "The entry number to start listing from. Must be 1 or greater."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "The maximum number of entries to return."
                    },
                    "depth": {
                        "type": "integer",
                        "description": "The maximum directory depth to traverse. Must be 1 or greater."
                    }
                },
                "required": ["dir_path"],
                "additionalProperties": false
            }
        }))
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: ListDirArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid list_dir args: {e}"))
        })?;

        if params.offset == 0 {
            return Err(CodexError::new(ErrorCode::InvalidInput, "offset must be a 1-indexed entry number"));
        }
        if params.limit == 0 {
            return Err(CodexError::new(ErrorCode::InvalidInput, "limit must be greater than zero"));
        }
        if params.depth == 0 {
            return Err(CodexError::new(ErrorCode::InvalidInput, "depth must be greater than zero"));
        }

        let path = PathBuf::from(&params.dir_path);
        if !path.is_absolute() {
            return Err(CodexError::new(ErrorCode::InvalidInput, "dir_path must be an absolute path"));
        }

        let entries = list_dir_slice(&path, params.offset, params.limit, params.depth).await?;
        let mut output = Vec::with_capacity(entries.len() + 1);
        output.push(format!("Absolute path: {}", path.display()));
        output.extend(entries);

        Ok(serde_json::json!({
            "content": output.join("\n"),
        }))
    }
}

async fn list_dir_slice(
    path: &Path,
    offset: usize,
    limit: usize,
    depth: usize,
) -> Result<Vec<String>, CodexError> {
    let mut entries = Vec::new();
    collect_entries(path, Path::new(""), depth, &mut entries).await?;

    if entries.is_empty() {
        return Ok(Vec::new());
    }

    entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    let start_index = offset - 1;
    if start_index >= entries.len() {
        return Err(CodexError::new(ErrorCode::InvalidInput, "offset exceeds directory entry count"));
    }

    let remaining = entries.len() - start_index;
    let capped_limit = limit.min(remaining);
    let end_index = start_index + capped_limit;
    let selected = &entries[start_index..end_index];
    let mut formatted = Vec::with_capacity(selected.len());

    for entry in selected {
        formatted.push(format_entry_line(entry));
    }

    if end_index < entries.len() {
        formatted.push(format!("More than {capped_limit} entries found"));
    }

    Ok(formatted)
}

async fn collect_entries(
    dir_path: &Path,
    relative_prefix: &Path,
    depth: usize,
    entries: &mut Vec<DirEntry>,
) -> Result<(), CodexError> {
    let mut queue = VecDeque::new();
    queue.push_back((dir_path.to_path_buf(), relative_prefix.to_path_buf(), depth));

    while let Some((current_dir, prefix, remaining_depth)) = queue.pop_front() {
        let mut read_dir = tokio::fs::read_dir(&current_dir).await.map_err(|e| {
            CodexError::new(ErrorCode::ToolExecutionFailed, format!("failed to read directory: {e}"))
        })?;

        let mut dir_entries = Vec::new();

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| {
            CodexError::new(ErrorCode::ToolExecutionFailed, format!("failed to read directory: {e}"))
        })? {
            let file_type = entry.file_type().await.map_err(|e| {
                CodexError::new(ErrorCode::ToolExecutionFailed, format!("failed to inspect entry: {e}"))
            })?;

            let file_name = entry.file_name();
            let relative_path = if prefix.as_os_str().is_empty() {
                PathBuf::from(&file_name)
            } else {
                prefix.join(&file_name)
            };

            let display_name = format_entry_component(&file_name);
            let display_depth = prefix.components().count();
            let sort_key = format_entry_name(&relative_path);
            let kind = DirEntryKind::from(&file_type);
            dir_entries.push((
                entry.path(),
                relative_path,
                kind,
                DirEntry { name: sort_key, display_name, depth: display_depth, kind },
            ));
        }

        dir_entries.sort_unstable_by(|a, b| a.3.name.cmp(&b.3.name));

        for (entry_path, relative_path, kind, dir_entry) in dir_entries {
            if kind == DirEntryKind::Directory && remaining_depth > 1 {
                queue.push_back((entry_path, relative_path, remaining_depth - 1));
            }
            entries.push(dir_entry);
        }
    }

    Ok(())
}

fn format_entry_name(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.len() > MAX_ENTRY_LENGTH {
        let mut end = MAX_ENTRY_LENGTH;
        while end > 0 && !normalized.is_char_boundary(end) { end -= 1; }
        normalized[..end].to_string()
    } else {
        normalized
    }
}

fn format_entry_component(name: &OsStr) -> String {
    let normalized = name.to_string_lossy();
    if normalized.len() > MAX_ENTRY_LENGTH {
        let mut end = MAX_ENTRY_LENGTH;
        while end > 0 && !normalized.is_char_boundary(end) { end -= 1; }
        normalized[..end].to_string()
    } else {
        normalized.to_string()
    }
}

fn format_entry_line(entry: &DirEntry) -> String {
    let indent = " ".repeat(entry.depth * INDENTATION_SPACES);
    let mut name = entry.display_name.clone();
    match entry.kind {
        DirEntryKind::Directory => name.push('/'),
        DirEntryKind::Symlink => name.push('@'),
        DirEntryKind::Other => name.push('?'),
        DirEntryKind::File => {}
    }
    format!("{indent}{name}")
}

#[derive(Clone)]
struct DirEntry {
    name: String,
    display_name: String,
    depth: usize,
    kind: DirEntryKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DirEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

impl From<&FileType> for DirEntryKind {
    fn from(ft: &FileType) -> Self {
        if ft.is_symlink() { DirEntryKind::Symlink }
        else if ft.is_dir() { DirEntryKind::Directory }
        else if ft.is_file() { DirEntryKind::File }
        else { DirEntryKind::Other }
    }
}

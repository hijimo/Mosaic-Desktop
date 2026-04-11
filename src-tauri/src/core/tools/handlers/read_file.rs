use async_trait::async_trait;
use serde::Deserialize;
use std::collections::VecDeque;
use std::path::PathBuf;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ReadFileHandler;

const MAX_LINE_LENGTH: usize = 500;
const TAB_WIDTH: usize = 4;
const COMMENT_PREFIXES: &[&str] = &["#", "//", "--"];

#[derive(Deserialize)]
struct ReadFileArgs {
    file_path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    mode: ReadMode,
    #[serde(default)]
    indentation: Option<IndentationArgs>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum ReadMode {
    #[default]
    Slice,
    Indentation,
}

#[derive(Deserialize, Clone)]
struct IndentationArgs {
    #[serde(default)]
    anchor_line: Option<usize>,
    #[serde(default)]
    max_levels: usize,
    #[serde(default)]
    include_siblings: bool,
    #[serde(default = "default_include_header")]
    include_header: bool,
    #[serde(default)]
    max_lines: Option<usize>,
}

impl Default for IndentationArgs {
    fn default() -> Self {
        Self {
            anchor_line: None,
            max_levels: 0,
            include_siblings: false,
            include_header: true,
            max_lines: None,
        }
    }
}

fn default_offset() -> usize {
    1
}
fn default_limit() -> usize {
    2000
}
fn default_include_header() -> bool {
    true
}

#[derive(Clone, Debug)]
struct LineRecord {
    number: usize,
    raw: String,
    display: String,
    indent: usize,
}

impl LineRecord {
    fn is_blank(&self) -> bool {
        self.raw.trim().is_empty()
    }
    fn is_comment(&self) -> bool {
        COMMENT_PREFIXES
            .iter()
            .any(|p| self.raw.trim().starts_with(p))
    }
}

#[async_trait]
impl ToolHandler for ReadFileHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "read_file")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("read_file".to_string())
    }

    fn tool_spec(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "name": "read_file",
            "description": "Read the contents of a file at the given path. Use this to examine source code, configuration files, or any text file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "The line number to start reading from. Must be 1 or greater."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "The maximum number of lines to return."
                    }
                },
                "required": ["file_path"],
                "additionalProperties": false
            }
        }))
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: ReadFileArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid read_file args: {e}"),
            )
        })?;

        if params.offset == 0 {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "offset must be a 1-indexed line number",
            ));
        }
        if params.limit == 0 {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "limit must be greater than zero",
            ));
        }

        let path = PathBuf::from(&params.file_path);
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };

        let collected = match params.mode {
            ReadMode::Slice => slice_read(&path, params.offset, params.limit).await?,
            ReadMode::Indentation => {
                let opts = params.indentation.unwrap_or_default();
                indentation_read(&path, params.offset, params.limit, opts).await?
            }
        };

        Ok(serde_json::json!({
            "content": collected.join("\n"),
        }))
    }
}

fn format_line(raw: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(raw);
    if decoded.len() > MAX_LINE_LENGTH {
        // UTF-8 safe truncation: find char boundary
        let mut end = MAX_LINE_LENGTH;
        while end > 0 && !decoded.is_char_boundary(end) {
            end -= 1;
        }
        decoded[..end].to_string()
    } else {
        decoded.into_owned()
    }
}

fn measure_indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

async fn read_all_lines(path: &PathBuf) -> Result<Vec<LineRecord>, CodexError> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("failed to read file: {e}"),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut lines = Vec::new();
    let mut number = 0usize;
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).await.map_err(|e| {
            CodexError::new(ErrorCode::ToolExecutionFailed, format!("read error: {e}"))
        })?;
        if n == 0 {
            break;
        }
        if buf.last() == Some(&b'\n') {
            buf.pop();
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        number += 1;
        let raw = String::from_utf8_lossy(&buf).into_owned();
        let indent = measure_indent(&raw);
        let display = format_line(&buf);
        lines.push(LineRecord {
            number,
            raw,
            display,
            indent,
        });
    }
    Ok(lines)
}

async fn slice_read(
    path: &PathBuf,
    offset: usize,
    limit: usize,
) -> Result<Vec<String>, CodexError> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("failed to read file: {e}"),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut collected = Vec::new();
    let mut seen = 0usize;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).await.map_err(|e| {
            CodexError::new(ErrorCode::ToolExecutionFailed, format!("read error: {e}"))
        })?;
        if n == 0 {
            break;
        }
        if buf.last() == Some(&b'\n') {
            buf.pop();
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        seen += 1;
        if seen < offset {
            continue;
        }
        if collected.len() >= limit {
            break;
        }
        let formatted = format_line(&buf);
        collected.push(format!("L{seen}: {formatted}"));
    }
    if seen < offset {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "offset exceeds file length",
        ));
    }
    Ok(collected)
}

async fn indentation_read(
    path: &PathBuf,
    offset: usize,
    limit: usize,
    opts: IndentationArgs,
) -> Result<Vec<String>, CodexError> {
    let anchor_line = opts.anchor_line.unwrap_or(offset);
    if anchor_line == 0 {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "anchor_line must be a 1-indexed line number",
        ));
    }
    let guard_limit = opts.max_lines.unwrap_or(limit);
    if guard_limit == 0 {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "max_lines must be greater than zero",
        ));
    }

    let all_lines = read_all_lines(path).await?;
    if all_lines.is_empty() || anchor_line > all_lines.len() {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "anchor_line exceeds file length",
        ));
    }

    let anchor_index = anchor_line - 1;
    let effective_indents = compute_effective_indents(&all_lines);
    let anchor_indent = effective_indents[anchor_index];

    let min_indent = if opts.max_levels == 0 {
        0
    } else {
        anchor_indent.saturating_sub(opts.max_levels * TAB_WIDTH)
    };

    let final_limit = limit.min(guard_limit).min(all_lines.len());

    if final_limit == 1 {
        return Ok(vec![format!(
            "L{}: {}",
            all_lines[anchor_index].number, all_lines[anchor_index].display
        )]);
    }

    let mut i: isize = anchor_index as isize - 1;
    let mut j: usize = anchor_index + 1;
    let mut i_counter_min_indent = 0;
    let mut j_counter_min_indent = 0;

    let mut out: VecDeque<&LineRecord> = VecDeque::with_capacity(limit);
    out.push_back(&all_lines[anchor_index]);

    while out.len() < final_limit {
        let mut progressed = 0;

        // Up
        if i >= 0 {
            let iu = i as usize;
            if effective_indents[iu] >= min_indent {
                out.push_front(&all_lines[iu]);
                progressed += 1;
                i -= 1;
                if effective_indents[iu] == min_indent && !opts.include_siblings {
                    let allow_header_comment = opts.include_header && all_lines[iu].is_comment();
                    let can_take = allow_header_comment || i_counter_min_indent == 0;
                    if can_take {
                        i_counter_min_indent += 1;
                    } else {
                        out.pop_front();
                        progressed -= 1;
                        i = -1;
                    }
                }
                if out.len() >= final_limit {
                    break;
                }
            } else {
                i = -1;
            }
        }

        // Down
        if j < all_lines.len() {
            if effective_indents[j] >= min_indent {
                out.push_back(&all_lines[j]);
                progressed += 1;
                j += 1;
                if j > 0 && effective_indents[j - 1] == min_indent && !opts.include_siblings {
                    if j_counter_min_indent > 0 {
                        out.pop_back();
                        progressed -= 1;
                        j = all_lines.len();
                    }
                    j_counter_min_indent += 1;
                }
            } else {
                j = all_lines.len();
            }
        }

        if progressed == 0 {
            break;
        }
    }

    // Trim empty lines from front/back
    while matches!(out.front(), Some(l) if l.raw.trim().is_empty()) {
        out.pop_front();
    }
    while matches!(out.back(), Some(l) if l.raw.trim().is_empty()) {
        out.pop_back();
    }

    Ok(out
        .into_iter()
        .map(|r| format!("L{}: {}", r.number, r.display))
        .collect())
}

fn compute_effective_indents(records: &[LineRecord]) -> Vec<usize> {
    let mut effective = Vec::with_capacity(records.len());
    let mut prev = 0usize;
    for r in records {
        if r.is_blank() {
            effective.push(prev);
        } else {
            prev = r.indent;
            effective.push(prev);
        }
    }
    effective
}

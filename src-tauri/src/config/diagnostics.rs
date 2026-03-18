use std::fmt;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use super::toml_types::ConfigToml;

/// 1-based line/column position in a text file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPosition {
    pub line: usize,
    pub column: usize,
}

/// Text range in 1-based line/column coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub start: TextPosition,
    pub end: TextPosition,
}

/// A configuration error with file location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub path: PathBuf,
    pub range: TextRange,
    pub message: String,
}

impl ConfigError {
    pub fn new(path: PathBuf, range: TextRange, message: impl Into<String>) -> Self {
        Self {
            path,
            range,
            message: message.into(),
        }
    }
}

/// Wrapper that pairs a [`ConfigError`] with an optional TOML parse error source.
#[derive(Debug)]
pub struct ConfigLoadError {
    error: ConfigError,
    source: Option<toml::de::Error>,
}

impl ConfigLoadError {
    pub fn new(error: ConfigError, source: Option<toml::de::Error>) -> Self {
        Self { error, source }
    }

    pub fn config_error(&self) -> &ConfigError {
        &self.error
    }
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.error.path.display(),
            self.error.range.start.line,
            self.error.range.start.column,
            self.error.message
        )
    }
}

impl std::error::Error for ConfigLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e as &dyn std::error::Error)
    }
}

pub fn io_error_from_config_error(
    kind: std::io::ErrorKind,
    error: ConfigError,
    source: Option<toml::de::Error>,
) -> std::io::Error {
    std::io::Error::new(kind, ConfigLoadError::new(error, source))
}

/// Build a [`ConfigError`] from a raw TOML parse error.
pub fn config_error_from_toml(
    path: impl AsRef<Path>,
    contents: &str,
    err: toml::de::Error,
) -> ConfigError {
    let range = err
        .span()
        .map(|span| text_range_from_span(contents, span))
        .unwrap_or_else(default_range);
    ConfigError::new(path.as_ref().to_path_buf(), range, err.message())
}

/// Try to deserialize `contents` as `T`; return the first error with location info.
pub fn config_error_from_typed_toml<T: DeserializeOwned>(
    path: impl AsRef<Path>,
    contents: &str,
) -> Option<ConfigError> {
    match toml::from_str::<T>(contents) {
        Ok(_) => None,
        Err(err) => Some(config_error_from_toml(path, contents, err)),
    }
}

/// Validate a config file on disk, returning the first error if any.
pub fn validate_config_file(path: &Path) -> Option<ConfigError> {
    let contents = std::fs::read_to_string(path).ok()?;
    config_error_from_typed_toml::<ConfigToml>(path, &contents)
}

/// Format a [`ConfigError`] with source context (line highlight + caret).
pub fn format_config_error(error: &ConfigError, contents: &str) -> String {
    let mut output = String::new();
    let start = error.range.start;
    let _ = writeln!(
        output,
        "{}:{}:{}: {}",
        error.path.display(),
        start.line,
        start.column,
        error.message
    );

    let line_index = start.line.saturating_sub(1);
    let line = match contents.lines().nth(line_index) {
        Some(l) => l.trim_end_matches('\r'),
        None => return output.trim_end().to_string(),
    };

    let line_number = start.line;
    let gutter = line_number.to_string().len();
    let _ = writeln!(output, "{:width$} |", "", width = gutter);
    let _ = writeln!(output, "{line_number:>gutter$} | {line}");

    let highlight_len = if error.range.end.line == error.range.start.line
        && error.range.end.column >= error.range.start.column
    {
        error.range.end.column - error.range.start.column + 1
    } else {
        1
    };
    let spaces = " ".repeat(start.column.saturating_sub(1));
    let carets = "^".repeat(highlight_len.max(1));
    let _ = writeln!(output, "{:width$} | {spaces}{carets}", "", width = gutter);
    output.trim_end().to_string()
}

/// Format a config error, reading the source file from disk.
pub fn format_config_error_with_source(error: &ConfigError) -> String {
    match std::fs::read_to_string(&error.path) {
        Ok(contents) => format_config_error(error, &contents),
        Err(_) => format_config_error(error, ""),
    }
}

// ── helpers ──────────────────────────────────────────────────────

fn text_range_from_span(contents: &str, span: std::ops::Range<usize>) -> TextRange {
    let start = position_for_offset(contents, span.start);
    let end_index = if span.end > span.start {
        span.end - 1
    } else {
        span.end
    };
    let end = position_for_offset(contents, end_index);
    TextRange { start, end }
}

fn position_for_offset(contents: &str, index: usize) -> TextPosition {
    let bytes = contents.as_bytes();
    if bytes.is_empty() {
        return TextPosition { line: 1, column: 1 };
    }
    let safe_index = index.min(bytes.len().saturating_sub(1));
    let line_start = bytes[..safe_index]
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let line = bytes[..line_start]
        .iter()
        .filter(|b| **b == b'\n')
        .count();
    let column = safe_index - line_start;
    TextPosition {
        line: line + 1,
        column: column + 1,
    }
}

fn default_range() -> TextRange {
    let p = TextPosition { line: 1, column: 1 };
    TextRange { start: p, end: p }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_for_first_char() {
        let pos = position_for_offset("hello", 0);
        assert_eq!(pos, TextPosition { line: 1, column: 1 });
    }

    #[test]
    fn position_for_second_line() {
        let pos = position_for_offset("abc\ndef", 4);
        assert_eq!(pos, TextPosition { line: 2, column: 1 });
    }

    #[test]
    fn config_error_from_bad_toml() {
        let err = config_error_from_typed_toml::<ConfigToml>(
            Path::new("test.toml"),
            "model = 123",
        );
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.path, PathBuf::from("test.toml"));
    }

    #[test]
    fn format_error_shows_caret() {
        let error = ConfigError::new(
            PathBuf::from("config.toml"),
            TextRange {
                start: TextPosition { line: 1, column: 3 },
                end: TextPosition { line: 1, column: 5 },
            },
            "bad value",
        );
        let formatted = format_config_error(&error, "model = 123");
        assert!(formatted.contains("^^^"));
        assert!(formatted.contains("bad value"));
    }

    #[test]
    fn valid_toml_returns_none() {
        let err = config_error_from_typed_toml::<ConfigToml>(
            Path::new("ok.toml"),
            "model = \"gpt-4\"",
        );
        assert!(err.is_none());
    }
}

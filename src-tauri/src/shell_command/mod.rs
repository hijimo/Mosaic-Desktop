use std::path::PathBuf;

use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::ParsedCommand;

/// Parse a shell command string into a list of tokens.
///
/// Supports:
/// - Space-separated tokens
/// - Single-quoted strings (no escape processing inside)
/// - Double-quoted strings (with `\"` and `\\` escape sequences)
/// - Backslash escaping outside quotes
pub fn parse_command(input: &str) -> Result<Vec<String>, CodexError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "empty command string",
        ));
    }

    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = trimmed.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '\\' if !in_single_quote => match chars.next() {
                Some(escaped) => current.push(escaped),
                None => {
                    return Err(CodexError::new(
                        ErrorCode::InvalidInput,
                        "trailing backslash in command string",
                    ));
                }
            },
            c if c.is_ascii_whitespace() && !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            other => {
                current.push(other);
            }
        }
    }

    if in_single_quote {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "unterminated single quote in command string",
        ));
    }
    if in_double_quote {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "unterminated double quote in command string",
        ));
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    if tokens.is_empty() {
        return Err(CodexError::new(
            ErrorCode::InvalidInput,
            "command string produced no tokens",
        ));
    }

    Ok(tokens)
}

/// Classify a parsed command into a semantic ParsedCommand type.
///
/// Recognizes common read, list, and search commands.
pub fn classify_command(tokens: &[String]) -> ParsedCommand {
    if tokens.is_empty() {
        return ParsedCommand::Unknown { cmd: String::new() };
    }

    let cmd_name = tokens[0].as_str();
    let full_cmd = tokens.join(" ");

    match cmd_name {
        "cat" | "head" | "tail" | "less" | "more" | "bat" => {
            let path = tokens.get(1).map(PathBuf::from);
            let name = path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            ParsedCommand::Read {
                cmd: full_cmd,
                name,
                path: path.unwrap_or_default(),
            }
        }
        "ls" | "dir" | "tree" | "exa" | "eza" => {
            let path = tokens.get(1).filter(|s| !s.starts_with('-')).cloned();
            ParsedCommand::ListFiles {
                cmd: full_cmd,
                path,
            }
        }
        "grep" | "rg" | "ag" | "find" | "fd" | "fzf" => {
            let query = tokens.get(1).cloned();
            let path = tokens.get(2).filter(|s| !s.starts_with('-')).cloned();
            ParsedCommand::Search {
                cmd: full_cmd,
                query,
                path,
            }
        }
        _ => ParsedCommand::Unknown { cmd: full_cmd },
    }
}

/// Check if a command is considered safe (read-only).
pub fn is_safe_command(tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    matches!(
        tokens[0].as_str(),
        "cat"
            | "head"
            | "tail"
            | "less"
            | "more"
            | "bat"
            | "ls"
            | "dir"
            | "tree"
            | "exa"
            | "eza"
            | "pwd"
            | "whoami"
            | "date"
            | "echo"
            | "which"
            | "type"
            | "file"
            | "wc"
            | "du"
            | "df"
            | "uname"
            | "env"
            | "printenv"
    )
}

/// Check if a command is considered dangerous.
pub fn is_dangerous_command(tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    let cmd = tokens[0].as_str();
    if matches!(cmd, "rm" | "rmdir" | "dd" | "mkfs" | "fdisk" | "format") {
        return true;
    }
    // rm -rf / pattern
    if cmd == "rm" && tokens.iter().any(|t| t.contains("rf") || t == "/") {
        return true;
    }
    // sudo with dangerous subcommand
    if cmd == "sudo" && tokens.len() > 1 {
        return is_dangerous_command(&tokens[1..]);
    }
    false
}

/// Detect the current shell type.
pub fn detect_shell() -> ShellKind {
    if cfg!(target_os = "windows") {
        ShellKind::PowerShell
    } else {
        // Check SHELL env var
        if let Ok(shell) = std::env::var("SHELL") {
            if shell.contains("zsh") {
                return ShellKind::Zsh;
            }
            if shell.contains("fish") {
                return ShellKind::Fish;
            }
        }
        ShellKind::Bash
    }
}

/// Known shell types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_safe_tokens() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-zA-Z0-9_./-]{1,20}", 1..=8)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn shell_command_roundtrip(tokens in arb_safe_tokens()) {
            let command = tokens.join(" ");
            let parsed = parse_command(&command).unwrap();
            prop_assert_eq!(tokens, parsed);
        }
    }

    #[test]
    fn simple_command() {
        let result = parse_command("ls -la /tmp").unwrap();
        assert_eq!(result, vec!["ls", "-la", "/tmp"]);
    }

    #[test]
    fn single_quoted_string() {
        let result = parse_command("echo 'hello world'").unwrap();
        assert_eq!(result, vec!["echo", "hello world"]);
    }

    #[test]
    fn double_quoted_string() {
        let result = parse_command(r#"echo "hello world""#).unwrap();
        assert_eq!(result, vec!["echo", "hello world"]);
    }

    #[test]
    fn escaped_space() {
        let result = parse_command(r"echo hello\ world").unwrap();
        assert_eq!(result, vec!["echo", "hello world"]);
    }

    #[test]
    fn empty_input_returns_error() {
        assert!(parse_command("").is_err());
    }

    #[test]
    fn classify_cat_command() {
        let tokens = vec!["cat".to_string(), "/etc/hosts".to_string()];
        match classify_command(&tokens) {
            ParsedCommand::Read { cmd, path, .. } => {
                assert_eq!(cmd, "cat /etc/hosts");
                assert_eq!(path, PathBuf::from("/etc/hosts"));
            }
            _ => panic!("expected Read"),
        }
    }

    #[test]
    fn classify_ls_command() {
        let tokens = vec!["ls".to_string(), "-la".to_string()];
        match classify_command(&tokens) {
            ParsedCommand::ListFiles { cmd, path } => {
                assert_eq!(cmd, "ls -la");
                assert_eq!(path, None); // -la is a flag, not a path
            }
            _ => panic!("expected ListFiles"),
        }
    }

    #[test]
    fn classify_grep_command() {
        let tokens = vec![
            "grep".to_string(),
            "pattern".to_string(),
            "file.txt".to_string(),
        ];
        match classify_command(&tokens) {
            ParsedCommand::Search { cmd, query, path } => {
                assert_eq!(cmd, "grep pattern file.txt");
                assert_eq!(query, Some("pattern".to_string()));
                assert_eq!(path, Some("file.txt".to_string()));
            }
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn classify_unknown_command() {
        let tokens = vec!["docker".to_string(), "run".to_string()];
        match classify_command(&tokens) {
            ParsedCommand::Unknown { cmd } => {
                assert_eq!(cmd, "docker run");
            }
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn safe_commands() {
        assert!(is_safe_command(&["cat".to_string()]));
        assert!(is_safe_command(&["ls".to_string()]));
        assert!(is_safe_command(&["pwd".to_string()]));
        assert!(!is_safe_command(&["rm".to_string()]));
        assert!(!is_safe_command(&["docker".to_string()]));
    }

    #[test]
    fn dangerous_commands() {
        assert!(is_dangerous_command(&["rm".to_string()]));
        assert!(is_dangerous_command(&["dd".to_string()]));
        assert!(is_dangerous_command(&[
            "sudo".to_string(),
            "rm".to_string(),
            "-rf".to_string(),
            "/".to_string()
        ]));
        assert!(!is_dangerous_command(&["ls".to_string()]));
    }

    #[test]
    fn detect_shell_returns_valid() {
        let shell = detect_shell();
        // Just verify it doesn't panic and returns a valid variant
        assert!(matches!(
            shell,
            ShellKind::Bash | ShellKind::Zsh | ShellKind::Fish | ShellKind::PowerShell
        ));
    }
}

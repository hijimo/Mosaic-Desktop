//! Parse `bash -lc "..."` / `sh -c "..."` style commands into inner commands.

/// Shell prefixes that indicate the last argument is a script to parse.
const SHELL_LC_PREFIXES: &[&[&str]] = &[
    &["bash", "-lc"],
    &["bash", "-c"],
    &["sh", "-lc"],
    &["sh", "-c"],
    &["zsh", "-lc"],
    &["zsh", "-c"],
    &["/bin/bash", "-lc"],
    &["/bin/bash", "-c"],
    &["/bin/sh", "-c"],
    &["/bin/zsh", "-lc"],
    &["/bin/zsh", "-c"],
];

/// If `command` is `["bash", "-lc", "cmd1 && cmd2"]`, parse the script into
/// individual commands. Returns `None` if not a shell -lc invocation or if
/// parsing fails.
pub fn parse_shell_lc_plain_commands(command: &[String]) -> Option<Vec<Vec<String>>> {
    let script = extract_shell_script(command)?;
    let trimmed = script.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Reject heredocs and complex constructs
    if trimmed.contains("<<") {
        return None;
    }
    let parts: Vec<&str> = trimmed.split("&&").collect();
    let mut commands = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // Also split on pipes
        for segment in part.split('|') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            match shlex::split(segment) {
                Some(tokens) if !tokens.is_empty() => commands.push(tokens),
                _ => return None,
            }
        }
    }
    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

/// Fallback: extract a single command prefix from a heredoc-style script.
pub fn parse_shell_lc_single_command_prefix(command: &[String]) -> Option<Vec<String>> {
    let script = extract_shell_script(command)?;
    let trimmed = script.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Take the first "word" sequence before any heredoc/pipe/etc.
    let first_line = trimmed.lines().next()?;
    let clean = first_line
        .split("<<")
        .next()?
        .split('|')
        .next()?
        .trim();
    if clean.is_empty() {
        return None;
    }
    shlex::split(clean).filter(|tokens| !tokens.is_empty())
}

fn extract_shell_script(command: &[String]) -> Option<&str> {
    for prefix in SHELL_LC_PREFIXES {
        if command.len() == prefix.len() + 1
            && command
                .iter()
                .zip(prefix.iter())
                .all(|(a, b)| a.as_str() == *b)
        {
            return Some(&command[prefix.len()]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_simple_bash_lc() {
        let command = cmd(&["bash", "-lc", "cargo build && echo ok"]);
        let parsed = parse_shell_lc_plain_commands(&command).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], cmd(&["cargo", "build"]));
        assert_eq!(parsed[1], cmd(&["echo", "ok"]));
    }

    #[test]
    fn parse_pipe_commands() {
        let command = cmd(&["bash", "-lc", "cat file | grep pattern"]);
        let parsed = parse_shell_lc_plain_commands(&command).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], cmd(&["cat", "file"]));
        assert_eq!(parsed[1], cmd(&["grep", "pattern"]));
    }

    #[test]
    fn rejects_heredoc() {
        let command = cmd(&["bash", "-lc", "python3 <<'PY'\nprint('hi')\nPY"]);
        assert!(parse_shell_lc_plain_commands(&command).is_none());
    }

    #[test]
    fn fallback_extracts_prefix_from_heredoc() {
        let command = cmd(&["bash", "-lc", "python3 <<'PY'\nprint('hi')\nPY"]);
        let prefix = parse_shell_lc_single_command_prefix(&command).unwrap();
        assert_eq!(prefix, cmd(&["python3"]));
    }

    #[test]
    fn non_shell_command_returns_none() {
        let command = cmd(&["cargo", "build"]);
        assert!(parse_shell_lc_plain_commands(&command).is_none());
        assert!(parse_shell_lc_single_command_prefix(&command).is_none());
    }

    #[test]
    fn empty_script_returns_none() {
        let command = cmd(&["bash", "-lc", ""]);
        assert!(parse_shell_lc_plain_commands(&command).is_none());
    }
}

use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ShellHandler;

#[derive(Deserialize)]
struct ShellArgs {
    command: Vec<String>,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    /// Sandbox permissions for the command.
    #[serde(default)]
    sandbox_permissions: Option<String>,
    /// Additional permissions (e.g. file_system read/write paths).
    #[serde(default)]
    additional_permissions: Option<serde_json::Value>,
    /// Justification for elevated permissions.
    #[serde(default)]
    justification: Option<String>,
    /// Prefix rule for command matching.
    #[serde(default)]
    prefix_rule: Option<Vec<String>>,
}

/// Known safe read-only commands that skip approval.
/// Matches source Codex `is_safe_command.rs` logic.

/// Extract the executable base name from a path (e.g., "/usr/bin/ls" → "ls").
fn executable_name(raw: &str) -> Option<String> {
    std::path::Path::new(raw)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_owned())
}

/// Check if a command is known to be safe (read-only).
/// Matches source Codex `is_known_safe_command` with per-command argument checks.
pub fn is_known_safe_command(command: &[String]) -> bool {
    // Normalize zsh → bash for shell -lc parsing
    let command: Vec<String> = command
        .iter()
        .map(|s| {
            if s == "zsh" {
                "bash".to_string()
            } else {
                s.clone()
            }
        })
        .collect();

    if is_safe_to_call_with_exec(&command) {
        return true;
    }

    // Support `bash -lc "cmd1 && cmd2"` — parse and check each sub-command
    if let Some(script) = parse_shell_lc_script(&command) {
        let sub_commands = split_shell_plain_commands(&script);
        if !sub_commands.is_empty()
            && sub_commands
                .iter()
                .all(|cmd| is_safe_to_call_with_exec(cmd))
        {
            return true;
        }
    }

    false
}

fn is_safe_to_call_with_exec(command: &[String]) -> bool {
    let Some(cmd0) = command.first().map(String::as_str) else {
        return false;
    };
    let Some(name) = executable_name(cmd0) else {
        return false;
    };

    match name.as_str() {
        "cat" | "cd" | "cut" | "echo" | "expr" | "false" | "grep" | "head" | "id" | "ls" | "nl"
        | "paste" | "pwd" | "rev" | "seq" | "stat" | "tail" | "tr" | "true" | "uname" | "uniq"
        | "wc" | "which" | "whoami" => true,

        "base64" => !command.iter().skip(1).any(|arg| {
            matches!(arg.as_str(), "-o" | "--output")
                || arg.starts_with("--output=")
                || (arg.starts_with("-o") && arg != "-o")
        }),

        "find" => {
            const UNSAFE: &[&str] = &[
                "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fls", "-fprint", "-fprint0",
                "-fprintf",
            ];
            !command.iter().any(|a| UNSAFE.contains(&a.as_str()))
        }

        "rg" => !command.iter().any(|arg| {
            matches!(arg.as_str(), "--search-zip" | "-z")
                || arg == "--pre"
                || arg.starts_with("--pre=")
                || arg == "--hostname-bin"
                || arg.starts_with("--hostname-bin=")
        }),

        "git" => is_safe_git_command(&command),

        "sed"
            if command.len() <= 4
                && command.get(1).map(String::as_str) == Some("-n")
                && is_valid_sed_n_arg(command.get(2).map(String::as_str)) =>
        {
            true
        }

        _ => false,
    }
}

fn is_safe_git_command(command: &[String]) -> bool {
    // Reject -c config overrides (can execute arbitrary commands)
    if command.iter().any(|arg| {
        matches!(arg.as_str(), "-c" | "--config-env")
            || (arg.starts_with("-c") && arg.len() > 2)
            || arg.starts_with("--config-env=")
    }) {
        return false;
    }

    // Find the git subcommand, skipping global options
    let Some((idx, sub)) = find_git_subcommand(command) else {
        return false;
    };
    let sub_args = &command[idx + 1..];

    const UNSAFE_GIT_FLAGS: &[&str] = &[
        "--output",
        "--ext-diff",
        "--textconv",
        "--exec",
        "--paginate",
    ];
    let args_safe = !sub_args.iter().any(|a| {
        UNSAFE_GIT_FLAGS.contains(&a.as_str())
            || a.starts_with("--output=")
            || a.starts_with("--exec=")
    });

    match sub {
        "status" | "log" | "diff" | "show" => args_safe,
        "branch" => args_safe && git_branch_is_read_only(sub_args),
        _ => false,
    }
}

fn find_git_subcommand(command: &[String]) -> Option<(usize, &str)> {
    const SAFE_SUBS: &[&str] = &["status", "log", "diff", "show", "branch"];
    const GIT_OPTS_WITH_VALUE: &[&str] = &[
        "-C",
        "-c",
        "--config-env",
        "--exec-path",
        "--git-dir",
        "--namespace",
        "--super-prefix",
        "--work-tree",
    ];

    let mut skip_next = false;
    for (idx, arg) in command.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        let a = arg.as_str();
        // Inline value options like -Cpath, --git-dir=...
        if (a.starts_with("-C") || a.starts_with("-c")) && a.len() > 2 {
            continue;
        }
        if a.starts_with("--config-env=")
            || a.starts_with("--exec-path=")
            || a.starts_with("--git-dir=")
            || a.starts_with("--namespace=")
            || a.starts_with("--super-prefix=")
            || a.starts_with("--work-tree=")
        {
            continue;
        }
        if GIT_OPTS_WITH_VALUE.contains(&a) {
            skip_next = true;
            continue;
        }
        if a == "--" || a.starts_with('-') {
            continue;
        }
        if SAFE_SUBS.contains(&a) {
            return Some((idx, a));
        }
        return None; // first positional is the subcommand; if not in our list, stop
    }
    None
}

fn git_branch_is_read_only(args: &[String]) -> bool {
    if args.is_empty() {
        return true;
    }
    let mut saw_flag = false;
    for arg in args.iter().map(String::as_str) {
        match arg {
            "--list" | "-l" | "--show-current" | "-a" | "--all" | "-r" | "--remotes" | "-v"
            | "-vv" | "--verbose" => saw_flag = true,
            _ if arg.starts_with("--format=") => saw_flag = true,
            _ => return false,
        }
    }
    saw_flag
}

fn is_valid_sed_n_arg(arg: Option<&str>) -> bool {
    let Some(s) = arg else { return false };
    let Some(core) = s.strip_suffix('p') else {
        return false;
    };
    let parts: Vec<&str> = core.split(',').collect();
    match parts.as_slice() {
        [n] => !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()),
        [a, b] => {
            !a.is_empty()
                && !b.is_empty()
                && a.chars().all(|c| c.is_ascii_digit())
                && b.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// Extract the script from `bash -lc "..."` or `bash -c "..."` patterns.
fn parse_shell_lc_script(command: &[String]) -> Option<String> {
    let cmd0 = executable_name(command.first()?.as_str())?;
    if !matches!(cmd0.as_str(), "bash" | "sh") {
        return None;
    }
    // Find -c or -lc
    for (i, arg) in command.iter().enumerate().skip(1) {
        if (arg == "-c" || arg == "-lc") && command.len() == i + 2 {
            return Some(command[i + 1].clone());
        }
    }
    None
}

/// Split a shell script on safe operators (&&, ||, ;, |) into individual commands.
/// Returns None-equivalent (empty vec) if unsafe constructs are found.
fn split_shell_plain_commands(script: &str) -> Vec<Vec<String>> {
    // Reject scripts with unsafe shell constructs
    if script.contains('(')
        || script.contains(')')
        || script.contains('>')
        || script.contains('<')
        || script.contains('`')
        || script.contains('$')
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    // Split on &&, ||, ;, |
    let mut current = String::new();
    let chars: Vec<char> = script.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
            if !current.trim().is_empty() {
                result.push(shell_words_parse(current.trim()));
            }
            current.clear();
            i += 2;
            continue;
        }
        if chars[i] == '|' && i + 1 < chars.len() && chars[i + 1] == '|' {
            if !current.trim().is_empty() {
                result.push(shell_words_parse(current.trim()));
            }
            current.clear();
            i += 2;
            continue;
        }
        if chars[i] == '|' || chars[i] == ';' {
            if !current.trim().is_empty() {
                result.push(shell_words_parse(current.trim()));
            }
            current.clear();
            i += 1;
            continue;
        }
        current.push(chars[i]);
        i += 1;
    }
    if !current.trim().is_empty() {
        result.push(shell_words_parse(current.trim()));
    }
    result
}

/// Simple shell word splitting (handles single/double quotes).
fn shell_words_parse(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for ch in s.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[async_trait]
impl ToolHandler for ShellHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "shell" || n == "container.exec" || n == "local_shell")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("shell".to_string())
    }

    fn tool_spec(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "name": "shell",
            "description": "Runs a shell command on the user's machine. Use this for file operations, running scripts, installing packages, or any system task. The command array is executed directly (not through a shell interpreter).",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The command to run as an array of strings (e.g. [\"ls\", \"-la\"])"
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory for the command"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Timeout in milliseconds (default: 120000)"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }
        }))
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: ShellArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid shell args: {e}"))
        })?;

        if params.command.is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "command array must not be empty",
            ));
        }

        // Intercept apply_patch commands (matches source Codex behavior)
        if let Some(result) = super::apply_patch::intercept_apply_patch(
            &params.command,
            &std::env::current_dir().unwrap_or_default(),
            Some(params.timeout_ms.unwrap_or(120_000)),
        )
        .await?
        {
            return Ok(result);
        }

        let program = &params.command[0];
        let cmd_args = &params.command[1..];

        let mut cmd = tokio::process::Command::new(program);
        cmd.args(cmd_args);
        if let Some(ref wd) = params.workdir {
            cmd.current_dir(wd);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let timeout = std::time::Duration::from_millis(params.timeout_ms.unwrap_or(120_000));

        let child = cmd.spawn().map_err(|e| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("failed to spawn command: {e}"),
            )
        })?;

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("command execution error: {e}"),
                ));
            }
            Err(_) => {
                return Ok(serde_json::json!({
                    "exit_code": -1, "stdout": "", "stderr": "command timed out", "timed_out": true,
                }));
            }
        };

        Ok(serde_json::json!({
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }))
    }
}

//! Cross-platform shell detection and management.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Supported shell types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellType {
    Zsh,
    Bash,
    Sh,
    PowerShell,
    Cmd,
}

/// A resolved shell with its type and binary path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shell {
    pub shell_type: ShellType,
    pub shell_path: PathBuf,
}

impl Shell {
    pub fn name(&self) -> &'static str {
        match self.shell_type {
            ShellType::Zsh => "zsh",
            ShellType::Bash => "bash",
            ShellType::Sh => "sh",
            ShellType::PowerShell => "powershell",
            ShellType::Cmd => "cmd",
        }
    }

    /// Build the argument list to execute `command` in this shell.
    pub fn exec_args(&self, command: &str, login: bool) -> Vec<String> {
        match self.shell_type {
            ShellType::Zsh | ShellType::Bash | ShellType::Sh => {
                let flag = if login { "-lc" } else { "-c" };
                vec![
                    self.shell_path.to_string_lossy().into(),
                    flag.into(),
                    command.into(),
                ]
            }
            ShellType::PowerShell => {
                let mut args = vec![self.shell_path.to_string_lossy().into_owned()];
                if !login {
                    args.push("-NoProfile".into());
                }
                args.push("-Command".into());
                args.push(command.into());
                args
            }
            ShellType::Cmd => {
                vec![
                    self.shell_path.to_string_lossy().into(),
                    "/c".into(),
                    command.into(),
                ]
            }
        }
    }
}

/// Detect shell type from a binary path.
pub fn detect_shell_type(path: &Path) -> Option<ShellType> {
    let name = path.file_stem()?.to_str()?.to_ascii_lowercase();
    match name.as_str() {
        "zsh" => Some(ShellType::Zsh),
        "bash" => Some(ShellType::Bash),
        "sh" => Some(ShellType::Sh),
        "pwsh" | "powershell" => Some(ShellType::PowerShell),
        "cmd" => Some(ShellType::Cmd),
        _ => None,
    }
}

/// Resolve a shell by type, searching PATH and common fallback locations.
pub fn get_shell(shell_type: ShellType, explicit_path: Option<&Path>) -> Option<Shell> {
    if let Some(p) = explicit_path {
        if p.is_file() {
            return Some(Shell {
                shell_type,
                shell_path: p.to_path_buf(),
            });
        }
    }

    // Check user's default shell
    #[cfg(unix)]
    if let Some(default) = get_user_shell_path() {
        if detect_shell_type(&default) == Some(shell_type) && default.is_file() {
            return Some(Shell {
                shell_type,
                shell_path: default,
            });
        }
    }

    let binary = match shell_type {
        ShellType::Zsh => "zsh",
        ShellType::Bash => "bash",
        ShellType::Sh => "sh",
        ShellType::PowerShell => "pwsh",
        ShellType::Cmd => "cmd",
    };

    if let Ok(p) = which::which(binary) {
        return Some(Shell {
            shell_type,
            shell_path: p,
        });
    }

    // PowerShell fallback: try "powershell" if "pwsh" not found
    if shell_type == ShellType::PowerShell {
        if let Ok(p) = which::which("powershell") {
            return Some(Shell {
                shell_type,
                shell_path: p,
            });
        }
    }

    // Unix fallback paths
    let fallbacks: &[&str] = match shell_type {
        ShellType::Zsh => &["/bin/zsh"],
        ShellType::Bash => &["/bin/bash"],
        ShellType::Sh => &["/bin/sh"],
        _ => &[],
    };
    for fb in fallbacks {
        let p = PathBuf::from(fb);
        if p.is_file() {
            return Some(Shell {
                shell_type,
                shell_path: p,
            });
        }
    }

    None
}

/// Detect and return the user's default shell.
pub fn default_user_shell() -> Shell {
    #[cfg(unix)]
    {
        if let Some(path) = get_user_shell_path() {
            if let Some(st) = detect_shell_type(&path) {
                if let Some(shell) = get_shell(st, Some(&path)) {
                    return shell;
                }
            }
        }
        // macOS prefers zsh, Linux prefers bash
        let order = if cfg!(target_os = "macos") {
            [ShellType::Zsh, ShellType::Bash]
        } else {
            [ShellType::Bash, ShellType::Zsh]
        };
        for st in order {
            if let Some(shell) = get_shell(st, None) {
                return shell;
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(shell) = get_shell(ShellType::PowerShell, None) {
            return shell;
        }
    }

    ultimate_fallback()
}

/// Resolve a shell from a model-provided path string.
pub fn shell_from_path(path: &Path) -> Shell {
    detect_shell_type(path)
        .and_then(|st| get_shell(st, Some(path)))
        .unwrap_or_else(ultimate_fallback)
}

fn ultimate_fallback() -> Shell {
    if cfg!(windows) {
        Shell {
            shell_type: ShellType::Cmd,
            shell_path: PathBuf::from("cmd.exe"),
        }
    } else {
        Shell {
            shell_type: ShellType::Sh,
            shell_path: PathBuf::from("/bin/sh"),
        }
    }
}

#[cfg(unix)]
fn get_user_shell_path() -> Option<PathBuf> {
    use std::ffi::CStr;
    unsafe {
        let uid = libc::getuid();
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return None;
        }
        let shell = CStr::from_ptr((*pw).pw_shell)
            .to_string_lossy()
            .into_owned();
        Some(PathBuf::from(shell))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_common_shells() {
        assert_eq!(
            detect_shell_type(Path::new("/bin/zsh")),
            Some(ShellType::Zsh)
        );
        assert_eq!(
            detect_shell_type(Path::new("/bin/bash")),
            Some(ShellType::Bash)
        );
        assert_eq!(detect_shell_type(Path::new("/bin/sh")), Some(ShellType::Sh));
        assert_eq!(
            detect_shell_type(Path::new("pwsh")),
            Some(ShellType::PowerShell)
        );
        assert_eq!(
            detect_shell_type(Path::new("powershell.exe")),
            Some(ShellType::PowerShell)
        );
        assert_eq!(
            detect_shell_type(Path::new("cmd.exe")),
            Some(ShellType::Cmd)
        );
        assert_eq!(detect_shell_type(Path::new("fish")), None);
    }

    #[test]
    fn exec_args_bash() {
        let shell = Shell {
            shell_type: ShellType::Bash,
            shell_path: PathBuf::from("/bin/bash"),
        };
        assert_eq!(
            shell.exec_args("echo hi", false),
            vec!["/bin/bash", "-c", "echo hi"]
        );
        assert_eq!(
            shell.exec_args("echo hi", true),
            vec!["/bin/bash", "-lc", "echo hi"]
        );
    }

    #[test]
    fn exec_args_powershell() {
        let shell = Shell {
            shell_type: ShellType::PowerShell,
            shell_path: PathBuf::from("pwsh.exe"),
        };
        assert_eq!(
            shell.exec_args("echo hi", false),
            vec!["pwsh.exe", "-NoProfile", "-Command", "echo hi"]
        );
        assert_eq!(
            shell.exec_args("echo hi", true),
            vec!["pwsh.exe", "-Command", "echo hi"]
        );
    }

    #[test]
    fn exec_args_cmd() {
        let shell = Shell {
            shell_type: ShellType::Cmd,
            shell_path: PathBuf::from("cmd.exe"),
        };
        assert_eq!(
            shell.exec_args("echo hi", false),
            vec!["cmd.exe", "/c", "echo hi"]
        );
    }

    #[test]
    fn fallback_shell() {
        let fb = ultimate_fallback();
        if cfg!(windows) {
            assert_eq!(fb.shell_type, ShellType::Cmd);
        } else {
            assert_eq!(fb.shell_type, ShellType::Sh);
        }
    }

    #[cfg(unix)]
    #[test]
    fn default_shell_resolves() {
        let shell = default_user_shell();
        assert!(shell.shell_path.is_file() || shell.shell_path.to_str() == Some("/bin/sh"));
    }
}

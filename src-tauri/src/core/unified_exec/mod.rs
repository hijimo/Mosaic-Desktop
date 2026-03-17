//! Unified execution engine — manages interactive processes with output buffering.

pub mod process_manager;

use std::path::PathBuf;
use std::time::Duration;

pub use process_manager::{ProcessManager, ProcessHandle, ExecResult};

/// Default constants for process management.
pub const MAX_PROCESSES: usize = 64;
pub const WARNING_PROCESSES: usize = 60;
pub const MIN_YIELD_TIME: Duration = Duration::from_millis(250);
pub const MAX_YIELD_TIME: Duration = Duration::from_secs(30);
pub const OUTPUT_MAX_BYTES: usize = 1024 * 1024; // 1 MiB

/// Environment variables set for all managed processes.
pub const EXEC_ENV: [(&str, &str); 10] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("LANG", "C.UTF-8"),
    ("LC_CTYPE", "C.UTF-8"),
    ("LC_ALL", "C.UTF-8"),
    ("COLORTERM", ""),
    ("PAGER", "cat"),
    ("GIT_PAGER", "cat"),
    ("GH_PAGER", "cat"),
    ("MOSAIC_CI", "1"),
];

/// A command execution request.
#[derive(Debug, Clone)]
pub struct ExecCommand {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Option<Duration>,
}

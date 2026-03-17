//! Tool execution event types.
//!
//! Adapted from Codex `tools/events.rs`. Provides event emission for
//! exec-command begin/end and patch-apply begin/end lifecycle events.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Status of an executed command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExecCommandStatus {
    Completed,
    Failed,
    Declined,
}

/// Status of a patch application.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PatchApplyStatus {
    Completed,
    Failed,
    Declined,
}

/// Source of an exec command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExecCommandSource {
    Shell,
    ShellCommand,
    UnifiedExec,
}

/// Event emitted when a command execution begins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecCommandBeginEvent {
    pub call_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub source: ExecCommandSource,
}

/// Event emitted when a command execution ends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecCommandEndEvent {
    pub call_id: String,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub source: ExecCommandSource,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
    pub status: ExecCommandStatus,
}

/// Event emitted when a patch application begins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchApplyBeginEvent {
    pub call_id: String,
}

/// Event emitted when a patch application ends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchApplyEndEvent {
    pub call_id: String,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub status: PatchApplyStatus,
}

/// Concrete emitter for tool lifecycle events.
///
/// In the full Codex implementation this drives `Session::send_event`.
/// Here it is a data holder; actual emission is deferred to the caller.
pub enum ToolEmitter {
    Shell {
        command: Vec<String>,
        cwd: PathBuf,
        source: ExecCommandSource,
    },
    ApplyPatch {
        call_id: String,
    },
    UnifiedExec {
        command: Vec<String>,
        cwd: PathBuf,
        source: ExecCommandSource,
        process_id: Option<String>,
    },
}

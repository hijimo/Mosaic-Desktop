//! PTY and pipe process management — ported from codex-utils-pty.

pub mod pipe;
pub mod process;
pub mod process_group;
pub mod pty;

pub use pipe::spawn_process as spawn_pipe_process;
pub use pipe::spawn_process_no_stdin as spawn_pipe_process_no_stdin;
pub use process::ProcessHandle;
pub use process::SpawnedProcess;
pub use pty::spawn_process as spawn_pty_process;

/// Backwards-compatible aliases.
pub type ExecCommandSession = ProcessHandle;
pub type SpawnedPty = SpawnedProcess;

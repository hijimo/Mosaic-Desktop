//! Unified exec error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnifiedExecError {
    #[error("Failed to create unified exec process: {message}")]
    CreateProcess { message: String },

    #[error("Unknown process id {process_id}")]
    UnknownProcessId { process_id: String },

    #[error("failed to write to stdin")]
    WriteToStdin,

    #[error("stdin is closed for this session; rerun exec_command with tty=true to keep stdin open")]
    StdinClosed,

    #[error("missing command line for unified exec request")]
    MissingCommandLine,

    #[error("Command denied by sandbox: {message}")]
    SandboxDenied {
        message: String,
        #[source]
        output: Option<ExecToolCallOutputError>,
    },
}

/// Wrapper so we can embed exec output in error chains.
#[derive(Debug)]
pub struct ExecToolCallOutputError {
    pub exit_code: i32,
    pub stderr: String,
    pub aggregated_output: String,
}

impl std::fmt::Display for ExecToolCallOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "exit_code={}, stderr={}", self.exit_code, self.stderr)
    }
}

impl std::error::Error for ExecToolCallOutputError {}

impl UnifiedExecError {
    pub fn create_process(message: String) -> Self {
        Self::CreateProcess { message }
    }

    pub fn sandbox_denied(message: String) -> Self {
        Self::SandboxDenied {
            message,
            output: None,
        }
    }

    pub fn sandbox_denied_with_output(
        message: String,
        exit_code: i32,
        stderr: String,
        aggregated_output: String,
    ) -> Self {
        Self::SandboxDenied {
            message,
            output: Some(ExecToolCallOutputError {
                exit_code,
                stderr,
                aggregated_output,
            }),
        }
    }
}

use std::io::ErrorKind;
use std::path::Path;

use crate::protocol::error::{CodexError, ErrorCode};
use super::SESSIONS_SUBDIR;

/// Map an `anyhow::Error` from session init into a user-friendly [`CodexError`].
pub fn map_session_init_error(err: &anyhow::Error, mosaic_home: &Path) -> CodexError {
    if let Some(mapped) = err
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .find_map(|io_err| map_rollout_io_error(io_err, mosaic_home))
    {
        return mapped;
    }
    CodexError::new(
        ErrorCode::InternalError,
        format!("Failed to initialize session: {err:#}"),
    )
}

fn map_rollout_io_error(io_err: &std::io::Error, mosaic_home: &Path) -> Option<CodexError> {
    let sessions_dir = mosaic_home.join(SESSIONS_SUBDIR);
    let hint = match io_err.kind() {
        ErrorKind::PermissionDenied => format!(
            "Cannot access session files at {} (permission denied). Fix ownership: sudo chown -R $(whoami) {}",
            sessions_dir.display(),
            mosaic_home.display()
        ),
        ErrorKind::NotFound => format!(
            "Session storage missing at {}. Create the directory or choose a different home.",
            sessions_dir.display()
        ),
        ErrorKind::AlreadyExists => format!(
            "Session storage path {} is blocked by an existing file. Remove or rename it.",
            sessions_dir.display()
        ),
        _ => return None,
    };
    Some(CodexError::new(
        ErrorCode::InternalError,
        format!("{hint} (underlying error: {io_err})"),
    ))
}

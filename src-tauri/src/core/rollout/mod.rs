//! Rollout module: persistence and discovery of session rollout files (JSONL).
//!
//! Sessions are recorded as append-only JSONL files under `~/.mosaic/sessions/YYYY/MM/DD/`.
//! Each line is a timestamped [`RolloutLine`] containing either session metadata,
//! a protocol event, or a compaction marker.

pub mod error;
pub mod list;
pub mod metadata;
pub mod policy;
pub mod recorder;
pub mod session_index;
pub mod truncation;

pub const SESSIONS_SUBDIR: &str = "sessions";
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";

pub use list::find_thread_path_by_id_str;
pub use list::find_archived_thread_path_by_id_str;
pub use list::rollout_date_parts;
pub use recorder::RolloutRecorder;
pub use recorder::RolloutRecorderParams;
pub use session_index::append_thread_name;
pub use session_index::find_thread_name_by_id;
pub use session_index::find_thread_path_by_name_str;

//! Session state management — split into service, session, and turn layers.
//!
//! This module mirrors the Codex `core/state/` architecture:
//! - `service` — long-lived session services (MCP, exec, tools, etc.)
//! - `session` — mutable session-scoped state (history, token tracking, etc.)
//! - `turn`    — per-turn state (active tasks, pending approvals/input)

mod service;
mod session;
mod turn;

pub use service::SessionServices;
pub use session::SessionState;
pub use turn::{ActiveTurn, RunningTask, TurnState};

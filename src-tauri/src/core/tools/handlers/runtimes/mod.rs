//! Tool runtime implementations.
//!
//! Each runtime wraps a specific execution backend (shell, apply_patch,
//! unified_exec) and implements the `ToolRuntime` trait from `sandboxing.rs`.

pub mod apply_patch;
pub mod shell;
pub mod unified_exec;

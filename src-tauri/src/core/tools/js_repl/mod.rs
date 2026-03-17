//! JavaScript REPL runtime stub.
//!
//! The full implementation would embed a persistent Node.js kernel
//! (via `deno_core` or a child process) with top-level await support.
//! For now this module re-exports the handler from `handlers/js_repl.rs`.

pub use crate::core::tools::handlers::js_repl::{JsReplHandler, JsReplResetHandler};

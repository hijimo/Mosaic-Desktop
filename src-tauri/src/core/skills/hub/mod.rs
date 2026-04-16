//! Skills Hub — community skill marketplace + security scanning.
//!
//! Mirrors the Hermes Skills Hub ecosystem:
//! - `models`: Data structures (SkillHubMeta, SkillBundle, ScanResult, etc.)
//! - `guard`: Security scanner (threat patterns, structural checks, install policy)
//! - `lock`: Lock file management (provenance tracking)
//! - `dirs`: Hub directory management (quarantine, audit log, cache)
//! - `sources/`: Source adapters (GitHub, skills.sh, ClawHub, etc.)
//! - `search`: Parallel search + unified dedup

pub mod dirs;
pub mod guard;
pub mod models;
pub mod sources;

pub use models::*;

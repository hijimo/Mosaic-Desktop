//! Multi-agent management — spawning, coordinating, lifecycle, guards, and roles.

pub mod control;
pub mod guards;
pub mod role;
pub mod status;

// Re-export primary types.
pub use control::{
    run_batch_jobs, AgentControl, AgentStatusEntry, BatchJobConfig, BatchResult,
    SpawnAgentOptions,
};
pub use guards::{
    exceeds_thread_spawn_depth_limit, next_thread_spawn_depth, session_depth, Guards,
    SpawnReservation,
};
pub use role::{apply_role_to_config, AgentRoleConfig, DEFAULT_ROLE_NAME};
pub use status::{agent_status_from_event, is_final};

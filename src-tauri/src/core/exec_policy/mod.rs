//! Execution policy engine — evaluates commands against rules, heuristics,
//! and sandbox policy to produce approval requirements.

mod bash;
mod heuristics;
mod manager;

pub use manager::{
    ExecApprovalRequirement, ExecPolicyManager, ExecPolicyError, ExecPolicyUpdateError,
    load_exec_policy, collect_policy_files,
};
pub use heuristics::render_decision_for_unmatched_command;

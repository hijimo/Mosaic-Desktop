//! Execution policy engine — evaluates commands against rules, heuristics,
//! and sandbox policy to produce approval requirements.

mod bash;
mod heuristics;
mod manager;

pub use heuristics::render_decision_for_unmatched_command;
pub use manager::{
    collect_policy_files, load_exec_policy, ExecApprovalRequirement, ExecPolicyError,
    ExecPolicyManager, ExecPolicyUpdateError,
};

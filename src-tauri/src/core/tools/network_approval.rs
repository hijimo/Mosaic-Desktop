//! Network approval service for tool executions.
//!
//! Adapted from Codex `tools/network_approval.rs`. Provides types for
//! deferred and immediate network approval flows.

/// Mode of network approval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkApprovalMode {
    /// Approval is resolved before the tool runs.
    Immediate,
    /// Approval is deferred until after the tool completes.
    Deferred,
}

/// Specification for network approval requirements.
#[derive(Clone, Debug)]
pub struct NetworkApprovalSpec {
    pub mode: NetworkApprovalMode,
    pub hosts: Vec<String>,
}

/// A deferred network approval that must be finalized after tool execution.
#[derive(Clone, Debug)]
pub struct DeferredNetworkApproval {
    pub registration_id: String,
    pub hosts: Vec<String>,
}

/// Service for managing network approvals during tool execution.
pub struct NetworkApprovalService;

impl NetworkApprovalService {
    pub fn new() -> Self {
        Self
    }

    /// Begin a network approval flow. Returns `None` if no approval is needed.
    pub async fn begin(&self, _spec: &NetworkApprovalSpec) -> Option<DeferredNetworkApproval> {
        // Stub: no network approval enforcement yet.
        None
    }

    /// Finalize a deferred network approval.
    pub async fn finish(&self, _approval: DeferredNetworkApproval) {
        // Stub: no-op.
    }
}

impl Default for NetworkApprovalService {
    fn default() -> Self {
        Self::new()
    }
}

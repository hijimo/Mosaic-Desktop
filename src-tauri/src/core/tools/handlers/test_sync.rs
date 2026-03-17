use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct TestSyncHandler;

const DEFAULT_TIMEOUT_MS: u64 = 1_000;

static BARRIERS: OnceLock<tokio::sync::Mutex<HashMap<String, BarrierState>>> = OnceLock::new();

struct BarrierState {
    barrier: Arc<tokio::sync::Barrier>,
    participants: usize,
}

#[derive(Debug, Deserialize)]
struct BarrierArgs {
    id: String,
    participants: usize,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct TestSyncArgs {
    #[serde(default)]
    sleep_before_ms: Option<u64>,
    #[serde(default)]
    sleep_after_ms: Option<u64>,
    #[serde(default)]
    barrier: Option<BarrierArgs>,
}

fn default_timeout_ms() -> u64 { DEFAULT_TIMEOUT_MS }

fn barrier_map() -> &'static tokio::sync::Mutex<HashMap<String, BarrierState>> {
    BARRIERS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

#[async_trait]
impl ToolHandler for TestSyncHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "test_sync_tool")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("test_sync_tool".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: TestSyncArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid test_sync args: {e}"))
        })?;

        if let Some(delay) = params.sleep_before_ms {
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }

        if let Some(barrier) = params.barrier {
            wait_on_barrier(barrier).await?;
        }

        if let Some(delay) = params.sleep_after_ms {
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }

        Ok(serde_json::json!({"status": "ok"}))
    }
}

async fn wait_on_barrier(args: BarrierArgs) -> Result<(), CodexError> {
    if args.participants == 0 {
        return Err(CodexError::new(ErrorCode::InvalidInput, "barrier participants must be greater than zero"));
    }
    if args.timeout_ms == 0 {
        return Err(CodexError::new(ErrorCode::InvalidInput, "barrier timeout must be greater than zero"));
    }

    let barrier_id = args.id.clone();
    let barrier = {
        let mut map = barrier_map().lock().await;
        match map.entry(barrier_id.clone()) {
            Entry::Occupied(entry) => {
                let state = entry.get();
                if state.participants != args.participants {
                    let existing = state.participants;
                    return Err(CodexError::new(
                        ErrorCode::InvalidInput,
                        format!("barrier {barrier_id} already registered with {existing} participants"),
                    ));
                }
                state.barrier.clone()
            }
            Entry::Vacant(entry) => {
                let barrier = Arc::new(tokio::sync::Barrier::new(args.participants));
                entry.insert(BarrierState {
                    barrier: barrier.clone(),
                    participants: args.participants,
                });
                barrier
            }
        }
    };

    let timeout = Duration::from_millis(args.timeout_ms);
    let wait_result = tokio::time::timeout(timeout, barrier.wait())
        .await
        .map_err(|_| CodexError::new(ErrorCode::ToolExecutionFailed, "test_sync_tool barrier wait timed out"))?;

    // Leader cleans up the barrier entry
    if wait_result.is_leader() {
        let mut map = barrier_map().lock().await;
        if let Some(state) = map.get(&barrier_id) {
            if Arc::ptr_eq(&state.barrier, &barrier) {
                map.remove(&barrier_id);
            }
        }
    }

    Ok(())
}

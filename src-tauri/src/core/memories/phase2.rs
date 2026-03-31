//! Memory Phase 2: consolidate raw memories into memory_summary.md via a sub-agent.

use crate::config::toml_types::MemoriesConfig;
use crate::core::agent::AgentControl;
use crate::core::memories::memory_root;
use crate::core::memories::prompts::build_consolidation_prompt;
use crate::core::memories::storage::{rebuild_raw_memories_file, sync_rollout_summaries};
use crate::protocol::types::{SandboxPolicy, UserInput};
use crate::protocol::ThreadId;
use crate::state::memories_db::Phase2JobClaimOutcome;
use crate::state::StateDb;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::phase1::ProviderContext;

/// Lease duration (seconds).
const JOB_LEASE_SECONDS: i64 = 3_600;
/// Retry delay (seconds).
const JOB_RETRY_DELAY_SECONDS: i64 = 3_600;
/// Default model for phase-2 consolidation.
const DEFAULT_MODEL: &str = "gpt-4o";

/// Run phase 2: claim the global consolidation job and dispatch a sub-agent.
pub(super) async fn run(
    db: &Arc<Mutex<StateDb>>,
    config: &MemoriesConfig,
    codex_home: &Path,
    current_thread_id: ThreadId,
    agent_control: Option<&AgentControl>,
    _provider_ctx: Option<&ProviderContext>,
) {
    let root = memory_root(codex_home);

    // 1. Claim the job
    let (ownership_token, _input_watermark) = {
        let db = db.lock().await;
        match db.try_claim_global_phase2_job(current_thread_id, JOB_LEASE_SECONDS) {
            Ok(Phase2JobClaimOutcome::Claimed {
                ownership_token,
                input_watermark,
            }) => (ownership_token, input_watermark),
            Ok(_) => return,
            Err(e) => {
                warn!("failed to claim phase-2 job: {e}");
                return;
            }
        }
    };

    // 2. Query memories
    let selection = {
        let db = db.lock().await;
        match db.get_phase2_input_selection(
            config.max_raw_memories_for_consolidation,
            config.max_unused_days,
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("failed to get phase-2 input: {e}");
                let _ = db.mark_global_phase2_job_failed(
                    &ownership_token,
                    "failed_load_inputs",
                    JOB_RETRY_DELAY_SECONDS,
                );
                return;
            }
        }
    };

    let raw_memories = selection.selected.clone();

    // 3. Sync file system artifacts
    if let Err(e) = sync_rollout_summaries(&root, &raw_memories, raw_memories.len()).await {
        warn!("failed syncing rollout summaries: {e}");
        let db = db.lock().await;
        let _ = db.mark_global_phase2_job_failed(
            &ownership_token,
            "failed_sync_artifacts",
            JOB_RETRY_DELAY_SECONDS,
        );
        return;
    }
    if let Err(e) = rebuild_raw_memories_file(&root, &raw_memories, raw_memories.len()).await {
        warn!("failed rebuilding raw memories: {e}");
        let db = db.lock().await;
        let _ = db.mark_global_phase2_job_failed(
            &ownership_token,
            "failed_rebuild_raw",
            JOB_RETRY_DELAY_SECONDS,
        );
        return;
    }

    if raw_memories.is_empty() {
        let db = db.lock().await;
        let _ = db.mark_global_phase2_job_succeeded(&ownership_token, 0, &[]);
        return;
    }

    // 4. Build consolidation prompt
    let prompt = build_consolidation_prompt(&root, &selection);

    // 5. Spawn consolidation agent
    let Some(agent_ctl) = agent_control else {
        warn!("memory phase-2: no agent_control, marking failed");
        let db = db.lock().await;
        let _ = db.mark_global_phase2_job_failed(
            &ownership_token,
            "no_agent_control",
            JOB_RETRY_DELAY_SECONDS,
        );
        return;
    };

    let spawn_opts = crate::core::agent::SpawnAgentOptions {
        model: Some(
            config
                .consolidation_model
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        ),
        sandbox_policy: Some(SandboxPolicy::new_workspace_write_policy()),
        cwd: Some(root.clone()),
        fork: true,
        max_depth: None,
        agent_type: None,
    };

    let (instance, _guards) = match agent_ctl.spawn_agent(spawn_opts, 0).await {
        Ok(r) => r,
        Err(e) => {
            warn!("failed to spawn consolidation agent: {e}");
            let db = db.lock().await;
            let _ = db.mark_global_phase2_job_failed(
                &ownership_token,
                "failed_spawn_agent",
                JOB_RETRY_DELAY_SECONDS,
            );
            return;
        }
    };

    let agent_id = instance.thread_id.clone();

    // Send the consolidation prompt as user input
    if let Err(e) = agent_ctl
        .send_input(
            &agent_id,
            UserInput::Text {
                text: prompt,
                text_elements: vec![],
            },
        )
        .await
    {
        warn!("failed to send input to consolidation agent: {e}");
        let db = db.lock().await;
        let _ = db.mark_global_phase2_job_failed(
            &ownership_token,
            "failed_send_input",
            JOB_RETRY_DELAY_SECONDS,
        );
        return;
    }

    info!(
        "memory phase-2: spawned consolidation agent {} with {} memories",
        agent_id,
        raw_memories.len()
    );

    // 6. Wait for agent completion
    let result = agent_ctl.wait(&agent_id).await;
    let _ = agent_ctl.close_agent(&agent_id).await;

    let db = db.lock().await;
    match result {
        Ok(_) => {
            let watermark = raw_memories
                .iter()
                .map(|m| m.source_updated_at.timestamp())
                .max()
                .unwrap_or(0);
            let _ = db.mark_global_phase2_job_succeeded(&ownership_token, watermark, &raw_memories);
            info!("memory phase-2 consolidation succeeded");
        }
        Err(e) => {
            warn!("consolidation agent failed: {e}");
            let _ = db.mark_global_phase2_job_failed(
                &ownership_token,
                &format!("agent_failed: {}", e.message),
                JOB_RETRY_DELAY_SECONDS,
            );
        }
    }
}

//! Memory Phase 1: extract raw memories from historical rollouts via LLM.

use crate::config::toml_types::MemoriesConfig;
use crate::core::client;
use crate::core::memories::prompts;
use crate::protocol::ThreadId;
use crate::provider::ModelProviderInfo;
use crate::state::memories_db::Stage1JobClaim;
use crate::state::StateDb;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Lease duration (seconds) for phase-1 job ownership.
pub(super) const JOB_LEASE_SECONDS: i64 = 3_600;
/// Backoff delay (seconds) before retrying a failed job.
const JOB_RETRY_DELAY_SECONDS: i64 = 3_600;
/// Default model for phase-1 extraction.
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// Phase 1 model output payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageOneOutput {
    raw_memory: String,
    rollout_summary: String,
    #[serde(default)]
    rollout_slug: Option<String>,
}

/// JSON schema used to constrain phase-1 model output.
fn output_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "rollout_summary": { "type": "string" },
            "rollout_slug": { "type": ["string", "null"] },
            "raw_memory": { "type": "string" }
        },
        "required": ["rollout_summary", "rollout_slug", "raw_memory"],
        "additionalProperties": false
    })
}

/// Provider context needed for LLM calls.
pub(super) struct ProviderContext {
    pub url: String,
    pub api_key: String,
    pub headers: HashMap<String, String>,
}

impl ProviderContext {
    pub fn from_provider_info(info: &ModelProviderInfo) -> Result<Self, String> {
        let provider = info.to_provider();
        let api_key = info
            .api_key()
            .map_err(|e| e.message.clone())?
            .ok_or_else(|| "no API key".to_string())?;
        Ok(Self {
            url: provider.url_for_path("responses"),
            api_key,
            headers: info.resolved_headers(),
        })
    }
}

/// Run phase 1: claim eligible rollout jobs and extract memories.
pub(super) async fn run(
    db: &Arc<Mutex<StateDb>>,
    config: &MemoriesConfig,
    current_thread_id: ThreadId,
    provider_ctx: Option<&ProviderContext>,
) {
    let Some(ctx) = provider_ctx else {
        warn!("memory phase-1: no provider context, skipping");
        return;
    };

    // 1. Claim startup jobs
    let claimed = {
        let db = db.lock().await;
        match db.claim_stage1_jobs_for_startup(
            current_thread_id,
            config.max_rollouts_per_startup,
            config.max_rollout_age_days,
            config.min_rollout_idle_hours,
            JOB_LEASE_SECONDS,
        ) {
            Ok(c) => c,
            Err(e) => {
                warn!("failed to claim stage-1 jobs: {e}");
                return;
            }
        }
    };

    if claimed.is_empty() {
        return;
    }

    let model = config.extract_model.as_deref().unwrap_or(DEFAULT_MODEL);

    info!("memory phase-1: claimed {} job(s)", claimed.len());

    let mut succeeded = 0usize;
    let mut failed = 0usize;

    // 2. Process each claimed job
    for claim in &claimed {
        match process_job(db, claim, ctx, model).await {
            true => succeeded += 1,
            false => failed += 1,
        }
    }

    info!(
        "memory phase-1 complete: {} succeeded, {} failed",
        succeeded, failed
    );
}

/// Process a single stage-1 extraction job. Returns true on success.
async fn process_job(
    db: &Arc<Mutex<StateDb>>,
    claim: &Stage1JobClaim,
    ctx: &ProviderContext,
    model: &str,
) -> bool {
    // Build the stage-1 input message.
    // The rollout content would normally come from loading the rollout file.
    // For now, use a placeholder since Mosaic stores rollouts in SQLite, not JSONL.
    let input_message = prompts::build_stage_one_input(
        &claim.thread.rollout_path,
        &claim.thread.cwd,
        "[rollout content from DB — integration pending]",
    );

    let input = vec![serde_json::json!({
        "type": "message",
        "role": "user",
        "content": [{"type": "input_text", "text": input_message}]
    })];

    // Call the LLM with structured output
    let result = client::complete_structured(
        &ctx.url,
        &ctx.api_key,
        &ctx.headers,
        model,
        Some(prompts::STAGE_ONE_SYSTEM),
        input,
        &output_schema(),
    )
    .await;

    let db = db.lock().await;

    match result {
        Err(e) => {
            warn!(
                "phase-1 extraction failed for {}: {}",
                claim.thread.id, e.message
            );
            let _ = db.mark_stage1_job_failed(
                claim.thread.id,
                &claim.ownership_token,
                &e.message,
                JOB_RETRY_DELAY_SECONDS,
            );
            false
        }
        Ok(text) => {
            let output: StageOneOutput = match serde_json::from_str(&text) {
                Ok(o) => o,
                Err(e) => {
                    warn!("phase-1 parse failed for {}: {e}", claim.thread.id);
                    let _ = db.mark_stage1_job_failed(
                        claim.thread.id,
                        &claim.ownership_token,
                        &format!("json parse: {e}"),
                        JOB_RETRY_DELAY_SECONDS,
                    );
                    return false;
                }
            };

            if output.raw_memory.is_empty() && output.rollout_summary.is_empty() {
                let _ =
                    db.mark_stage1_job_succeeded_no_output(claim.thread.id, &claim.ownership_token);
                return true;
            }

            let source_updated_at = claim.thread.updated_at.timestamp();
            match db.mark_stage1_job_succeeded(
                claim.thread.id,
                &claim.ownership_token,
                source_updated_at,
                &output.raw_memory,
                &output.rollout_summary,
                output.rollout_slug.as_deref(),
            ) {
                Ok(true) => true,
                Ok(false) => {
                    warn!(
                        "phase-1 mark succeeded returned false for {}",
                        claim.thread.id
                    );
                    false
                }
                Err(e) => {
                    warn!("phase-1 mark succeeded failed for {}: {e}", claim.thread.id);
                    false
                }
            }
        }
    }
}

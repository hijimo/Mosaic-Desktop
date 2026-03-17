//! Entry point for the memory startup pipeline.

use crate::config::toml_types::MemoriesConfig;
use crate::core::agent::AgentControl;
use crate::core::features::{Feature, Features};
use crate::core::memories::{phase1, phase2};
use crate::protocol::ThreadId;
use crate::provider::ModelProviderInfo;
use crate::state::StateDb;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

/// Start the asynchronous memory pipeline for an eligible root session.
///
/// Skipped when memories are disabled or the feature flag is off.
pub fn start_memories_startup_task(
    db: Arc<Mutex<StateDb>>,
    config: MemoriesConfig,
    features: &Features,
    codex_home: &Path,
    current_thread_id: ThreadId,
    provider_info: Option<ModelProviderInfo>,
    agent_control: Option<Arc<AgentControl>>,
) {
    if !config.generate_memories || !features.enabled(Feature::MemoryTool) {
        return;
    }

    let codex_home = codex_home.to_path_buf();
    tokio::spawn(async move {
        let provider_ctx = provider_info.as_ref().and_then(|info| {
            phase1::ProviderContext::from_provider_info(info)
                .map_err(|e| warn!("memory pipeline: provider context failed: {e}"))
                .ok()
        });

        phase1::run(&db, &config, current_thread_id, provider_ctx.as_ref()).await;
        phase2::run(
            &db,
            &config,
            &codex_home,
            current_thread_id,
            agent_control.as_deref(),
            provider_ctx.as_ref(),
        )
        .await;
    });
}

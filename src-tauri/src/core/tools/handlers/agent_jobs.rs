use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

/// Handler for batch agent job tools: spawn_agents_on_csv, report_agent_job_result.
/// Named BatchJobHandler to match source Codex.
pub struct BatchJobHandler;

// Keep the old name as an alias for backward compatibility
pub type AgentJobsHandler = BatchJobHandler;

const DEFAULT_AGENT_JOB_CONCURRENCY: usize = 16;
const MAX_AGENT_JOB_CONCURRENCY: usize = 64;
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_AGENT_JOB_ITEM_TIMEOUT: Duration = Duration::from_secs(60 * 30);

#[derive(Debug, Deserialize)]
struct SpawnAgentsOnCsvArgs {
    csv_path: String,
    instruction: String,
    #[serde(default)]
    id_column: Option<String>,
    #[serde(default)]
    output_csv_path: Option<String>,
    #[serde(default)]
    output_schema: Option<serde_json::Value>,
    #[serde(default)]
    max_concurrency: Option<usize>,
    #[serde(default)]
    max_workers: Option<usize>,
    #[serde(default)]
    max_runtime_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ReportAgentJobResultArgs {
    job_id: String,
    item_id: String,
    result: serde_json::Value,
    #[serde(default)]
    stop: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SpawnAgentsOnCsvResult {
    job_id: String,
    status: String,
    output_csv_path: String,
    total_items: usize,
    completed_items: usize,
    failed_items: usize,
    job_error: Option<String>,
    failed_item_errors: Option<Vec<AgentJobFailureSummary>>,
}

#[derive(Debug, Serialize)]
struct AgentJobFailureSummary {
    item_id: String,
    source_id: Option<String>,
    last_error: String,
}

#[derive(Debug, Serialize)]
struct AgentJobProgressUpdate {
    job_id: String,
    total_items: usize,
    pending_items: usize,
    running_items: usize,
    completed_items: usize,
    failed_items: usize,
    eta_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ReportAgentJobResultToolResult {
    accepted: bool,
}

#[derive(Debug, Clone)]
struct JobRunnerOptions {
    max_concurrency: usize,
}

#[derive(Debug, Clone)]
struct ActiveJobItem {
    item_id: String,
    started_at: std::time::Instant,
}

struct JobProgressEmitter {
    started_at: std::time::Instant,
    last_emit_at: std::time::Instant,
    last_processed: usize,
    last_failed: usize,
}

impl JobProgressEmitter {
    fn new() -> Self {
        let now = std::time::Instant::now();
        let last_emit_at = now.checked_sub(PROGRESS_EMIT_INTERVAL).unwrap_or(now);
        Self { started_at: now, last_emit_at, last_processed: 0, last_failed: 0 }
    }

    fn should_emit(&self, processed: usize, failed: usize) -> bool {
        processed != self.last_processed || failed != self.last_failed
            || self.last_emit_at.elapsed() >= PROGRESS_EMIT_INTERVAL
    }
}

#[async_trait]
impl ToolHandler for BatchJobHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == "spawn_agents_on_csv" || n == "report_agent_job_result")
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("spawn_agents_on_csv".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Dispatch based on tool name
        if args.get("csv_path").is_some() {
            let params: SpawnAgentsOnCsvArgs = serde_json::from_value(args).map_err(|e| {
                CodexError::new(ErrorCode::InvalidInput, format!("invalid spawn_agents_on_csv args: {e}"))
            })?;

            let concurrency = params.max_concurrency
                .or(params.max_workers)
                .unwrap_or(DEFAULT_AGENT_JOB_CONCURRENCY)
                .min(MAX_AGENT_JOB_CONCURRENCY);

            let _timeout = params.max_runtime_seconds
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_AGENT_JOB_ITEM_TIMEOUT);

            // Full implementation: parse CSV, spawn concurrent workers, track progress
            // TODO: wire to actual agent subsystem
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("spawn_agents_on_csv requires the agent subsystem (csv={}, concurrency={})", params.csv_path, concurrency),
            ));
        }

        if args.get("job_id").is_some() {
            let _params: ReportAgentJobResultArgs = serde_json::from_value(args).map_err(|e| {
                CodexError::new(ErrorCode::InvalidInput, format!("invalid report_agent_job_result args: {e}"))
            })?;
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                "report_agent_job_result requires the agent subsystem",
            ));
        }

        Err(CodexError::new(ErrorCode::InvalidInput, "unrecognized agent job tool arguments"))
    }
}

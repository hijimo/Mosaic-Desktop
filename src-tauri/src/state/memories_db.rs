//! Database operations for the memory pipeline (phase 1 & phase 2).

use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use std::path::PathBuf;
use uuid::Uuid;

use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::ThreadId;

use super::db::StateDb;

const JOB_KIND_MEMORY_STAGE1: &str = "memory_stage1";
const JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL: &str = "memory_consolidate_global";
const MEMORY_CONSOLIDATION_JOB_KEY: &str = "global";
const DEFAULT_RETRY_REMAINING: i64 = 3;

// ── Types ────────────────────────────────────────────────────────

/// Stored stage-1 memory extraction output for a single thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1Output {
    pub thread_id: ThreadId,
    pub rollout_path: PathBuf,
    pub source_updated_at: DateTime<Utc>,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub rollout_slug: Option<String>,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1OutputRef {
    pub thread_id: ThreadId,
    pub source_updated_at: DateTime<Utc>,
    pub rollout_slug: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Phase2InputSelection {
    pub selected: Vec<Stage1Output>,
    pub previous_selected: Vec<Stage1Output>,
    pub retained_thread_ids: Vec<ThreadId>,
    pub removed: Vec<Stage1OutputRef>,
}

/// Thread metadata needed for stage-1 job claims.
#[derive(Debug, Clone)]
pub struct MemoryThreadMeta {
    pub id: ThreadId,
    pub rollout_path: PathBuf,
    pub cwd: PathBuf,
    pub updated_at: DateTime<Utc>,
    pub git_branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Stage1JobClaim {
    pub thread: MemoryThreadMeta,
    pub ownership_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage1JobClaimOutcome {
    Claimed { ownership_token: String },
    SkippedUpToDate,
    SkippedRunning,
    SkippedRetryExhausted,
    SkippedRetryBackoff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase2JobClaimOutcome {
    Claimed {
        ownership_token: String,
        input_watermark: i64,
    },
    SkippedNotDirty,
    SkippedRunning,
}

fn epoch_to_dt(secs: i64) -> Result<DateTime<Utc>, CodexError> {
    DateTime::<Utc>::from_timestamp(secs, 0).ok_or_else(|| {
        CodexError::new(
            ErrorCode::InternalError,
            format!("invalid unix timestamp: {secs}"),
        )
    })
}

fn db_err(msg: String) -> CodexError {
    CodexError::new(ErrorCode::InternalError, msg)
}

// ── Phase 1 DB methods ──────────────────────────────────────────

impl StateDb {
    /// Claim stage-1 jobs for threads whose memories are stale.
    pub fn claim_stage1_jobs_for_startup(
        &self,
        current_thread_id: ThreadId,
        max_claimed: usize,
        max_age_days: i64,
        min_rollout_idle_hours: i64,
        lease_seconds: i64,
    ) -> Result<Vec<Stage1JobClaim>, CodexError> {
        if max_claimed == 0 {
            return Ok(Vec::new());
        }

        let now = Utc::now().timestamp();
        let max_age_cutoff = (Utc::now() - Duration::days(max_age_days.max(0))).timestamp();
        let idle_cutoff = (Utc::now() - Duration::hours(min_rollout_idle_hours.max(0))).timestamp();
        let current_id = current_thread_id.to_string();

        let conn = self.log_db.connection();
        let mut stmt = conn
            .prepare(
                "SELECT t.thread_id, COALESCE(t.updated_at, t.created_at) AS upd,
                        t.created_at, t.title, t.model
                 FROM threads t
                 LEFT JOIN stage1_outputs so ON so.thread_id = t.thread_id
                 LEFT JOIN jobs j ON j.kind = ?1 AND j.job_key = t.thread_id
                 WHERE COALESCE(t.memory_mode, 'enabled') = 'enabled'
                   AND t.thread_id != ?2
                   AND CAST(strftime('%s', COALESCE(t.updated_at, t.created_at)) AS INTEGER) >= ?3
                   AND CAST(strftime('%s', COALESCE(t.updated_at, t.created_at)) AS INTEGER) <= ?4
                   AND COALESCE(so.source_updated_at, -1) < CAST(strftime('%s', COALESCE(t.updated_at, t.created_at)) AS INTEGER)
                   AND COALESCE(j.last_success_watermark, -1) < CAST(strftime('%s', COALESCE(t.updated_at, t.created_at)) AS INTEGER)
                 ORDER BY upd DESC
                 LIMIT 5000",
            )
            .map_err(|e| db_err(format!("prepare claim query: {e}")))?;

        let rows: Vec<(String, String)> = stmt
            .query_map(
                params![JOB_KIND_MEMORY_STAGE1, current_id, max_age_cutoff, idle_cutoff],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(2)?)),
            )
            .map_err(|e| db_err(format!("claim query: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        let mut claimed = Vec::new();
        for (tid_str, created_str) in rows {
            if claimed.len() >= max_claimed {
                break;
            }
            let tid = match ThreadId::from_string(&tid_str) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let updated_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            match self.try_claim_stage1_job(tid, current_thread_id, updated_at.timestamp(), lease_seconds)? {
                Stage1JobClaimOutcome::Claimed { ownership_token } => {
                    claimed.push(Stage1JobClaim {
                        thread: MemoryThreadMeta {
                            id: tid,
                            rollout_path: PathBuf::new(), // filled by caller if needed
                            cwd: PathBuf::new(),
                            updated_at,
                            git_branch: None,
                        },
                        ownership_token,
                    });
                }
                _ => {}
            }
        }
        Ok(claimed)
    }

    /// Try to claim a single stage-1 job.
    fn try_claim_stage1_job(
        &self,
        thread_id: ThreadId,
        worker_id: ThreadId,
        source_updated_at: i64,
        lease_seconds: i64,
    ) -> Result<Stage1JobClaimOutcome, CodexError> {
        let now = Utc::now().timestamp();
        let lease_until = now.saturating_add(lease_seconds.max(0));
        let ownership_token = Uuid::new_v4().to_string();
        let tid = thread_id.to_string();
        let wid = worker_id.to_string();
        let conn = self.log_db.connection();

        // Check if already up-to-date
        let existing: Option<i64> = conn
            .query_row(
                "SELECT source_updated_at FROM stage1_outputs WHERE thread_id = ?1",
                [&tid],
                |row| row.get(0),
            )
            .ok();
        if existing.is_some_and(|v| v >= source_updated_at) {
            return Ok(Stage1JobClaimOutcome::SkippedUpToDate);
        }

        let job_watermark: Option<i64> = conn
            .query_row(
                "SELECT last_success_watermark FROM jobs WHERE kind = ?1 AND job_key = ?2",
                params![JOB_KIND_MEMORY_STAGE1, tid],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        if job_watermark.is_some_and(|v| v >= source_updated_at) {
            return Ok(Stage1JobClaimOutcome::SkippedUpToDate);
        }

        // Try insert or update
        let rows = conn
            .execute(
                "INSERT INTO jobs (kind, job_key, status, worker_id, ownership_token,
                    started_at, finished_at, lease_until, retry_at, retry_remaining,
                    last_error, input_watermark, last_success_watermark)
                 VALUES (?1, ?2, 'running', ?3, ?4, ?5, NULL, ?6, NULL, ?7, NULL, ?8, NULL)
                 ON CONFLICT(kind, job_key) DO UPDATE SET
                    status = 'running',
                    worker_id = excluded.worker_id,
                    ownership_token = excluded.ownership_token,
                    started_at = excluded.started_at,
                    finished_at = NULL,
                    lease_until = excluded.lease_until,
                    retry_at = NULL,
                    last_error = NULL,
                    input_watermark = excluded.input_watermark
                 WHERE (jobs.status != 'running' OR jobs.lease_until IS NULL OR jobs.lease_until <= excluded.started_at)
                   AND (jobs.retry_at IS NULL OR jobs.retry_at <= excluded.started_at
                        OR excluded.input_watermark > COALESCE(jobs.input_watermark, -1))
                   AND (jobs.retry_remaining > 0
                        OR excluded.input_watermark > COALESCE(jobs.input_watermark, -1))",
                params![
                    JOB_KIND_MEMORY_STAGE1, tid, wid, ownership_token,
                    now, lease_until, DEFAULT_RETRY_REMAINING, source_updated_at
                ],
            )
            .map_err(|e| db_err(format!("claim stage1 job: {e}")))?;

        if rows > 0 {
            return Ok(Stage1JobClaimOutcome::Claimed { ownership_token });
        }

        // Determine skip reason
        let skip = conn
            .query_row(
                "SELECT status, lease_until, retry_at, retry_remaining FROM jobs WHERE kind = ?1 AND job_key = ?2",
                params![JOB_KIND_MEMORY_STAGE1, tid],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                )),
            )
            .ok();

        if let Some((status, lease, retry_at, retry_rem)) = skip {
            if retry_rem <= 0 {
                return Ok(Stage1JobClaimOutcome::SkippedRetryExhausted);
            }
            if retry_at.is_some_and(|r| r > now) {
                return Ok(Stage1JobClaimOutcome::SkippedRetryBackoff);
            }
            if status == "running" && lease.is_some_and(|l| l > now) {
                return Ok(Stage1JobClaimOutcome::SkippedRunning);
            }
        }
        Ok(Stage1JobClaimOutcome::SkippedRunning)
    }

    /// Mark a stage-1 job as succeeded and upsert the output.
    pub fn mark_stage1_job_succeeded(
        &self,
        thread_id: ThreadId,
        ownership_token: &str,
        source_updated_at: i64,
        raw_memory: &str,
        rollout_summary: &str,
        rollout_slug: Option<&str>,
    ) -> Result<bool, CodexError> {
        let now = Utc::now().timestamp();
        let tid = thread_id.to_string();
        let conn = self.log_db.connection();

        let rows = conn
            .execute(
                "UPDATE jobs SET status = 'done', finished_at = ?1, lease_until = NULL,
                    last_error = NULL, last_success_watermark = input_watermark
                 WHERE kind = ?2 AND job_key = ?3 AND status = 'running' AND ownership_token = ?4",
                params![now, JOB_KIND_MEMORY_STAGE1, tid, ownership_token],
            )
            .map_err(|e| db_err(format!("mark stage1 succeeded: {e}")))?;

        if rows == 0 {
            return Ok(false);
        }

        conn.execute(
            "INSERT INTO stage1_outputs (thread_id, source_updated_at, raw_memory,
                rollout_summary, rollout_slug, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(thread_id) DO UPDATE SET
                source_updated_at = excluded.source_updated_at,
                raw_memory = excluded.raw_memory,
                rollout_summary = excluded.rollout_summary,
                rollout_slug = excluded.rollout_slug,
                generated_at = excluded.generated_at
             WHERE excluded.source_updated_at >= stage1_outputs.source_updated_at",
            params![tid, source_updated_at, raw_memory, rollout_summary, rollout_slug, now],
        )
        .map_err(|e| db_err(format!("upsert stage1 output: {e}")))?;

        self.enqueue_global_consolidation(source_updated_at)?;
        Ok(true)
    }

    /// Mark a stage-1 job as failed with retry backoff.
    pub fn mark_stage1_job_failed(
        &self,
        thread_id: ThreadId,
        ownership_token: &str,
        failure_reason: &str,
        retry_delay_seconds: i64,
    ) -> Result<bool, CodexError> {
        let now = Utc::now().timestamp();
        let retry_at = now.saturating_add(retry_delay_seconds.max(0));
        let tid = thread_id.to_string();

        let rows = self
            .log_db
            .connection()
            .execute(
                "UPDATE jobs SET status = 'error', finished_at = ?1, lease_until = NULL,
                    retry_at = ?2, retry_remaining = retry_remaining - 1, last_error = ?3
                 WHERE kind = ?4 AND job_key = ?5 AND status = 'running' AND ownership_token = ?6",
                params![now, retry_at, failure_reason, JOB_KIND_MEMORY_STAGE1, tid, ownership_token],
            )
            .map_err(|e| db_err(format!("mark stage1 failed: {e}")))?;

        Ok(rows > 0)
    }

    /// Mark a stage-1 job succeeded with no output (empty extraction).
    pub fn mark_stage1_job_succeeded_no_output(
        &self,
        thread_id: ThreadId,
        ownership_token: &str,
    ) -> Result<bool, CodexError> {
        let now = Utc::now().timestamp();
        let tid = thread_id.to_string();
        let conn = self.log_db.connection();

        let rows = conn
            .execute(
                "UPDATE jobs SET status = 'done', finished_at = ?1, lease_until = NULL,
                    last_error = NULL, last_success_watermark = input_watermark
                 WHERE kind = ?2 AND job_key = ?3 AND status = 'running' AND ownership_token = ?4",
                params![now, JOB_KIND_MEMORY_STAGE1, tid, ownership_token],
            )
            .map_err(|e| db_err(format!("mark stage1 no output: {e}")))?;

        if rows == 0 {
            return Ok(false);
        }

        conn.execute(
            "DELETE FROM stage1_outputs WHERE thread_id = ?1",
            [&tid],
        )
        .map_err(|e| db_err(format!("delete stage1 output: {e}")))?;

        Ok(true)
    }

    /// List non-empty stage-1 outputs for consolidation.
    pub fn list_stage1_outputs(&self, n: usize) -> Result<Vec<Stage1Output>, CodexError> {
        if n == 0 {
            return Ok(Vec::new());
        }
        let conn = self.log_db.connection();
        let mut stmt = conn
            .prepare(
                "SELECT so.thread_id, so.source_updated_at, so.raw_memory,
                        so.rollout_summary, so.rollout_slug, so.generated_at,
                        COALESCE(t.title, '') AS cwd,
                        t.model AS git_branch
                 FROM stage1_outputs so
                 LEFT JOIN threads t ON t.thread_id = so.thread_id
                 WHERE COALESCE(t.memory_mode, 'enabled') = 'enabled'
                   AND (length(trim(so.raw_memory)) > 0 OR length(trim(so.rollout_summary)) > 0)
                 ORDER BY so.source_updated_at DESC, so.thread_id DESC
                 LIMIT ?1",
            )
            .map_err(|e| db_err(format!("prepare list stage1: {e}")))?;

        let results = stmt
            .query_map([n as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(|e| db_err(format!("list stage1: {e}")))?;

        let mut outputs = Vec::new();
        for r in results {
            let (tid, sua, rm, rs, slug, ga, cwd, gb) =
                r.map_err(|e| db_err(format!("row: {e}")))?;
            outputs.push(Stage1Output {
                thread_id: ThreadId::from_string(&tid)
                    .map_err(|e| db_err(format!("parse thread_id: {e}")))?,
                rollout_path: PathBuf::new(),
                source_updated_at: epoch_to_dt(sua)?,
                raw_memory: rm,
                rollout_summary: rs,
                rollout_slug: slug,
                cwd: PathBuf::from(cwd),
                git_branch: gb,
                generated_at: epoch_to_dt(ga)?,
            });
        }
        Ok(outputs)
    }
}

// ── Phase 2 DB methods ──────────────────────────────────────────

impl StateDb {
    /// Enqueue or advance the global phase-2 consolidation job.
    pub fn enqueue_global_consolidation(&self, input_watermark: i64) -> Result<(), CodexError> {
        self.log_db
            .connection()
            .execute(
                "INSERT INTO jobs (kind, job_key, status, worker_id, ownership_token,
                    started_at, finished_at, lease_until, retry_at, retry_remaining,
                    last_error, input_watermark, last_success_watermark)
                 VALUES (?1, ?2, 'pending', NULL, NULL, NULL, NULL, NULL, NULL, ?3, NULL, ?4, 0)
                 ON CONFLICT(kind, job_key) DO UPDATE SET
                    status = CASE WHEN jobs.status = 'running' THEN 'running' ELSE 'pending' END,
                    retry_at = CASE WHEN jobs.status = 'running' THEN jobs.retry_at ELSE NULL END,
                    retry_remaining = max(jobs.retry_remaining, excluded.retry_remaining),
                    input_watermark = CASE
                        WHEN excluded.input_watermark > COALESCE(jobs.input_watermark, 0)
                            THEN excluded.input_watermark
                        ELSE COALESCE(jobs.input_watermark, 0) + 1
                    END",
                params![
                    JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL,
                    MEMORY_CONSOLIDATION_JOB_KEY,
                    DEFAULT_RETRY_REMAINING,
                    input_watermark
                ],
            )
            .map_err(|e| db_err(format!("enqueue consolidation: {e}")))?;
        Ok(())
    }

    /// Try to claim the global phase-2 consolidation job.
    pub fn try_claim_global_phase2_job(
        &self,
        worker_id: ThreadId,
        lease_seconds: i64,
    ) -> Result<Phase2JobClaimOutcome, CodexError> {
        let now = Utc::now().timestamp();
        let lease_until = now.saturating_add(lease_seconds.max(0));
        let ownership_token = Uuid::new_v4().to_string();
        let wid = worker_id.to_string();
        let conn = self.log_db.connection();

        let existing = conn.query_row(
            "SELECT status, lease_until, retry_at, retry_remaining, input_watermark, last_success_watermark
             FROM jobs WHERE kind = ?1 AND job_key = ?2",
            params![JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL, MEMORY_CONSOLIDATION_JOB_KEY],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
            )),
        );

        let (status, lease, retry_at, retry_rem, iw, lsw) = match existing {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Ok(Phase2JobClaimOutcome::SkippedNotDirty);
            }
            Err(e) => return Err(db_err(format!("query phase2 job: {e}"))),
        };

        let iw_val = iw.unwrap_or(0);
        if iw_val <= lsw.unwrap_or(0) {
            return Ok(Phase2JobClaimOutcome::SkippedNotDirty);
        }
        if retry_rem <= 0 || retry_at.is_some_and(|r| r > now) {
            return Ok(Phase2JobClaimOutcome::SkippedNotDirty);
        }
        if status == "running" && lease.is_some_and(|l| l > now) {
            return Ok(Phase2JobClaimOutcome::SkippedRunning);
        }

        let rows = conn
            .execute(
                "UPDATE jobs SET status = 'running', worker_id = ?1, ownership_token = ?2,
                    started_at = ?3, finished_at = NULL, lease_until = ?4,
                    retry_at = NULL, last_error = NULL
                 WHERE kind = ?5 AND job_key = ?6
                   AND input_watermark > COALESCE(last_success_watermark, 0)
                   AND (status != 'running' OR lease_until IS NULL OR lease_until <= ?3)
                   AND (retry_at IS NULL OR retry_at <= ?3)
                   AND retry_remaining > 0",
                params![
                    wid, ownership_token, now, lease_until,
                    JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL, MEMORY_CONSOLIDATION_JOB_KEY
                ],
            )
            .map_err(|e| db_err(format!("claim phase2: {e}")))?;

        if rows == 0 {
            Ok(Phase2JobClaimOutcome::SkippedRunning)
        } else {
            Ok(Phase2JobClaimOutcome::Claimed {
                ownership_token,
                input_watermark: iw_val,
            })
        }
    }

    /// Heartbeat: extend the lease for an owned running phase-2 job.
    pub fn heartbeat_global_phase2_job(
        &self,
        ownership_token: &str,
        lease_seconds: i64,
    ) -> Result<bool, CodexError> {
        let lease_until = Utc::now().timestamp().saturating_add(lease_seconds.max(0));
        let rows = self
            .log_db
            .connection()
            .execute(
                "UPDATE jobs SET lease_until = ?1
                 WHERE kind = ?2 AND job_key = ?3 AND status = 'running' AND ownership_token = ?4",
                params![
                    lease_until,
                    JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL,
                    MEMORY_CONSOLIDATION_JOB_KEY,
                    ownership_token
                ],
            )
            .map_err(|e| db_err(format!("heartbeat phase2: {e}")))?;
        Ok(rows > 0)
    }

    /// Mark the global phase-2 job as succeeded.
    pub fn mark_global_phase2_job_succeeded(
        &self,
        ownership_token: &str,
        completed_watermark: i64,
        selected_outputs: &[Stage1Output],
    ) -> Result<bool, CodexError> {
        let now = Utc::now().timestamp();
        let conn = self.log_db.connection();

        let rows = conn
            .execute(
                "UPDATE jobs SET status = 'done', finished_at = ?1, lease_until = NULL,
                    last_error = NULL,
                    last_success_watermark = max(COALESCE(last_success_watermark, 0), ?2)
                 WHERE kind = ?3 AND job_key = ?4 AND status = 'running' AND ownership_token = ?5",
                params![
                    now, completed_watermark,
                    JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL, MEMORY_CONSOLIDATION_JOB_KEY,
                    ownership_token
                ],
            )
            .map_err(|e| db_err(format!("mark phase2 succeeded: {e}")))?;

        if rows == 0 {
            return Ok(false);
        }

        // Reset selection flags
        conn.execute(
            "UPDATE stage1_outputs SET selected_for_phase2 = 0, selected_for_phase2_source_updated_at = NULL
             WHERE selected_for_phase2 != 0 OR selected_for_phase2_source_updated_at IS NOT NULL",
            [],
        )
        .map_err(|e| db_err(format!("reset phase2 flags: {e}")))?;

        // Mark selected outputs
        for output in selected_outputs {
            let sua = output.source_updated_at.timestamp();
            conn.execute(
                "UPDATE stage1_outputs SET selected_for_phase2 = 1,
                    selected_for_phase2_source_updated_at = ?1
                 WHERE thread_id = ?2 AND source_updated_at = ?1",
                params![sua, output.thread_id.to_string()],
            )
            .map_err(|e| db_err(format!("mark selected: {e}")))?;
        }

        Ok(true)
    }

    /// Mark the global phase-2 job as failed with retry.
    pub fn mark_global_phase2_job_failed(
        &self,
        ownership_token: &str,
        failure_reason: &str,
        retry_delay_seconds: i64,
    ) -> Result<bool, CodexError> {
        let now = Utc::now().timestamp();
        let retry_at = now.saturating_add(retry_delay_seconds.max(0));
        let rows = self
            .log_db
            .connection()
            .execute(
                "UPDATE jobs SET status = 'error', finished_at = ?1, lease_until = NULL,
                    retry_at = ?2, retry_remaining = retry_remaining - 1, last_error = ?3
                 WHERE kind = ?4 AND job_key = ?5 AND status = 'running' AND ownership_token = ?6",
                params![
                    now, retry_at, failure_reason,
                    JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL, MEMORY_CONSOLIDATION_JOB_KEY,
                    ownership_token
                ],
            )
            .map_err(|e| db_err(format!("mark phase2 failed: {e}")))?;
        Ok(rows > 0)
    }

    /// Get phase-2 input selection with diff against last successful run.
    pub fn get_phase2_input_selection(
        &self,
        n: usize,
        max_unused_days: i64,
    ) -> Result<Phase2InputSelection, CodexError> {
        if n == 0 {
            return Ok(Phase2InputSelection::default());
        }
        let cutoff = (Utc::now() - Duration::days(max_unused_days.max(0))).timestamp();
        let conn = self.log_db.connection();

        // Current selection
        let mut stmt = conn
            .prepare(
                "SELECT so.thread_id, so.source_updated_at, so.raw_memory,
                        so.rollout_summary, so.rollout_slug, so.generated_at,
                        so.selected_for_phase2, so.selected_for_phase2_source_updated_at
                 FROM stage1_outputs so
                 LEFT JOIN threads t ON t.thread_id = so.thread_id
                 WHERE COALESCE(t.memory_mode, 'enabled') = 'enabled'
                   AND (length(trim(so.raw_memory)) > 0 OR length(trim(so.rollout_summary)) > 0)
                   AND ((so.last_usage IS NOT NULL AND so.last_usage >= ?1)
                        OR (so.last_usage IS NULL AND so.source_updated_at >= ?1))
                 ORDER BY COALESCE(so.usage_count, 0) DESC,
                          COALESCE(so.last_usage, so.source_updated_at) DESC,
                          so.source_updated_at DESC, so.thread_id DESC
                 LIMIT ?2",
            )
            .map_err(|e| db_err(format!("prepare phase2 selection: {e}")))?;

        let mut selected = Vec::new();
        let mut current_ids = std::collections::HashSet::new();
        let mut retained_thread_ids = Vec::new();

        let rows = stmt
            .query_map(params![cutoff, n as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .map_err(|e| db_err(format!("phase2 selection: {e}")))?;

        for r in rows {
            let (tid, sua, rm, rs, slug, ga, sel, sel_sua) =
                r.map_err(|e| db_err(format!("row: {e}")))?;
            current_ids.insert(tid.clone());
            let thread_id = ThreadId::from_string(&tid)
                .map_err(|e| db_err(format!("parse: {e}")))?;
            if sel != 0 && sel_sua == Some(sua) {
                retained_thread_ids.push(thread_id);
            }
            selected.push(Stage1Output {
                thread_id,
                rollout_path: PathBuf::new(),
                source_updated_at: epoch_to_dt(sua)?,
                raw_memory: rm,
                rollout_summary: rs,
                rollout_slug: slug,
                cwd: PathBuf::new(),
                git_branch: None,
                generated_at: epoch_to_dt(ga)?,
            });
        }

        // Previous selection
        let mut prev_stmt = conn
            .prepare(
                "SELECT so.thread_id, so.source_updated_at, so.raw_memory,
                        so.rollout_summary, so.rollout_slug, so.generated_at
                 FROM stage1_outputs so
                 WHERE so.selected_for_phase2 = 1
                 ORDER BY so.source_updated_at DESC, so.thread_id DESC",
            )
            .map_err(|e| db_err(format!("prepare prev selection: {e}")))?;

        let mut previous_selected = Vec::new();
        let mut removed = Vec::new();

        let prev_rows = prev_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|e| db_err(format!("prev selection: {e}")))?;

        for r in prev_rows {
            let (tid, sua, rm, rs, slug, ga) =
                r.map_err(|e| db_err(format!("row: {e}")))?;
            let thread_id = ThreadId::from_string(&tid)
                .map_err(|e| db_err(format!("parse: {e}")))?;
            previous_selected.push(Stage1Output {
                thread_id,
                rollout_path: PathBuf::new(),
                source_updated_at: epoch_to_dt(sua)?,
                raw_memory: rm.clone(),
                rollout_summary: rs.clone(),
                rollout_slug: slug.clone(),
                cwd: PathBuf::new(),
                git_branch: None,
                generated_at: epoch_to_dt(ga)?,
            });
            if !current_ids.contains(&tid) {
                removed.push(Stage1OutputRef {
                    thread_id,
                    source_updated_at: epoch_to_dt(sua)?,
                    rollout_slug: slug,
                });
            }
        }

        Ok(Phase2InputSelection {
            selected,
            previous_selected,
            retained_thread_ids,
            removed,
        })
    }

    /// Clear all memory pipeline data.
    pub fn clear_memory_data(&self) -> Result<(), CodexError> {
        let conn = self.log_db.connection();
        conn.execute("DELETE FROM stage1_outputs", [])
            .map_err(|e| db_err(format!("clear stage1: {e}")))?;
        conn.execute(
            "DELETE FROM jobs WHERE kind = ?1 OR kind = ?2",
            params![JOB_KIND_MEMORY_STAGE1, JOB_KIND_MEMORY_CONSOLIDATE_GLOBAL],
        )
        .map_err(|e| db_err(format!("clear jobs: {e}")))?;
        Ok(())
    }

    /// Record usage for cited stage-1 outputs.
    pub fn record_stage1_output_usage(
        &self,
        thread_ids: &[ThreadId],
    ) -> Result<usize, CodexError> {
        if thread_ids.is_empty() {
            return Ok(0);
        }
        let now = Utc::now().timestamp();
        let conn = self.log_db.connection();
        let mut updated = 0;
        for tid in thread_ids {
            updated += conn
                .execute(
                    "UPDATE stage1_outputs SET usage_count = COALESCE(usage_count, 0) + 1,
                        last_usage = ?1 WHERE thread_id = ?2",
                    params![now, tid.to_string()],
                )
                .map_err(|e| db_err(format!("record usage: {e}")))?;
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn temp_db() -> (tempfile::TempDir, StateDb) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = StateDb::open(&path).unwrap();
        (dir, db)
    }

    #[test]
    fn stage1_succeed_and_list() {
        let (_dir, db) = temp_db();
        let tid = ThreadId::new();

        // Insert a thread first
        db.log_db
            .connection()
            .execute(
                "INSERT INTO threads (thread_id, created_at, title, model) VALUES (?1, ?2, NULL, NULL)",
                params![tid.to_string(), Utc::now().to_rfc3339()],
            )
            .unwrap();

        // Create a job claim manually
        let now = Utc::now().timestamp();
        let token = "test-token";
        db.log_db
            .connection()
            .execute(
                "INSERT INTO jobs (kind, job_key, status, worker_id, ownership_token,
                    started_at, lease_until, retry_remaining, input_watermark)
                 VALUES (?1, ?2, 'running', 'w', ?3, ?4, ?5, 3, ?6)",
                params![JOB_KIND_MEMORY_STAGE1, tid.to_string(), token, now, now + 3600, now],
            )
            .unwrap();

        let ok = db
            .mark_stage1_job_succeeded(tid, token, now, "raw mem", "summary", Some("slug"))
            .unwrap();
        assert!(ok);

        let outputs = db.list_stage1_outputs(10).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].raw_memory, "raw mem");
        assert_eq!(outputs[0].rollout_summary, "summary");
        assert_eq!(outputs[0].rollout_slug.as_deref(), Some("slug"));
    }

    #[test]
    fn phase2_claim_skipped_when_no_job() {
        let (_dir, db) = temp_db();
        let tid = ThreadId::new();
        let result = db.try_claim_global_phase2_job(tid, 3600).unwrap();
        assert_eq!(result, Phase2JobClaimOutcome::SkippedNotDirty);
    }

    #[test]
    fn phase2_claim_after_enqueue() {
        let (_dir, db) = temp_db();
        db.enqueue_global_consolidation(100).unwrap();
        let tid = ThreadId::new();
        let result = db.try_claim_global_phase2_job(tid, 3600).unwrap();
        assert!(matches!(result, Phase2JobClaimOutcome::Claimed { .. }));
    }

    #[test]
    fn clear_memory_data_removes_all() {
        let (_dir, db) = temp_db();
        db.enqueue_global_consolidation(100).unwrap();
        db.clear_memory_data().unwrap();

        let tid = ThreadId::new();
        let result = db.try_claim_global_phase2_job(tid, 3600).unwrap();
        assert_eq!(result, Phase2JobClaimOutcome::SkippedNotDirty);
    }
}

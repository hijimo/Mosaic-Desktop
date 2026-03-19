use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::protocol::error::{CodexError, ErrorCode};

/// Runtime configuration for the state database.
#[derive(Debug, Clone)]
pub struct StateConfig {
    pub db_path: std::path::PathBuf,
    pub max_connections: usize,
}

/// Runtime metrics for the state database.
#[derive(Debug, Clone, Default)]
pub struct StateMetrics {
    pub total_reads: u64,
    pub total_writes: u64,
}

/// Runtime state combining config and metrics.
#[derive(Debug)]
pub struct StateRuntime {
    pub config: StateConfig,
    pub metrics: StateMetrics,
}

/// Low-level storage engine wrapping a SQLite connection.
#[derive(Debug)]
pub struct LogDb {
    conn: Connection,
}

impl LogDb {
    pub fn open(path: &Path) -> Result<Self, CodexError> {
        let conn = Connection::open(path).map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to open log database: {e}"),
            )
        })?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// Session metadata stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMeta {
    pub id: String,
    /// Timestamp when the session was created (renamed from `created_at` per reference).
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub config_profile: Option<String>,
    /// ID of the session this was forked from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<String>,
    /// Working directory for this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<std::path::PathBuf>,
    /// Where this session originated (e.g. "cli", "gui").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator: Option<String>,
    /// CLI version that created this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
    /// Source identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Agent nickname for multi-agent support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_nickname: Option<String>,
    /// Agent role for multi-agent support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
    /// Model provider identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    /// Base instructions for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_instructions: Option<String>,
    /// Dynamic tools available in this session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dynamic_tools: Vec<serde_json::Value>,
    /// Memory mode for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mode: Option<String>,
}

/// Thread metadata for multi-agent support.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadMetadata {
    pub thread_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub title: Option<String>,
    pub model: Option<String>,
}

/// Agent job status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Individual item within an agent job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentJobItem {
    pub item_id: String,
    pub input: String,
    pub output: Option<String>,
    pub status: AgentJobStatus,
}

/// Agent job tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentJob {
    pub job_id: String,
    pub thread_id: String,
    pub status: AgentJobStatus,
    pub items: Vec<AgentJobItem>,
}

/// Backfill state for incremental processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackfillState {
    pub status: BackfillStatus,
    pub last_watermark: Option<String>,
    pub last_success_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for BackfillState {
    fn default() -> Self {
        Self {
            status: BackfillStatus::Pending,
            last_watermark: None,
            last_success_at: None,
        }
    }
}

/// Backfill lifecycle status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackfillStatus {
    Pending,
    Running,
    Complete,
}

impl BackfillStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Complete => "complete",
        }
    }

    pub fn parse(value: &str) -> Result<Self, CodexError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "complete" => Ok(Self::Complete),
            _ => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("invalid backfill status: {value}"),
            )),
        }
    }
}

/// Log entry for writing to the database.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub ts: i64,
    pub ts_nanos: i64,
    pub level: String,
    pub target: String,
    pub message: Option<String>,
    pub thread_id: Option<String>,
    pub process_uuid: Option<String>,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<i64>,
}

/// Log row read from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRow {
    pub id: i64,
    pub ts: i64,
    pub ts_nanos: i64,
    pub level: String,
    pub target: String,
    pub message: Option<String>,
    pub thread_id: Option<String>,
    pub process_uuid: Option<String>,
    pub file: Option<String>,
    pub line: Option<i64>,
}

/// Query parameters for log retrieval.
#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub level_upper: Option<String>,
    pub from_ts: Option<i64>,
    pub to_ts: Option<i64>,
    pub thread_ids: Vec<String>,
    pub search: Option<String>,
    pub include_threadless: bool,
    pub after_id: Option<i64>,
    pub limit: Option<usize>,
    pub descending: bool,
}

/// State DB filename constant.
pub const STATE_DB_FILENAME: &str = "state";
/// Current DB version — bump when adding migrations.
pub const STATE_DB_VERSION: u32 = 5;

/// Main state database with runtime config and log storage.
pub struct StateDb {
    pub runtime: StateRuntime,
    pub log_db: LogDb,
}

impl StateDb {
    /// Open (or create) the state database at the given path.
    pub fn open(path: &Path) -> Result<Self, CodexError> {
        let log_db = LogDb::open(path)?;
        // Enable WAL mode for better concurrency
        log_db.connection().execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;"
        ).map_err(|e| CodexError::new(
            ErrorCode::InternalError,
            format!("failed to set pragmas: {e}"),
        ))?;
        let runtime = StateRuntime {
            config: StateConfig {
                db_path: path.to_path_buf(),
                max_connections: 1,
            },
            metrics: StateMetrics::default(),
        };
        let db = Self { runtime, log_db };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open the versioned state database inside the given home directory.
    /// Creates the directory if needed and cleans up legacy DB files.
    pub fn open_versioned(home_dir: &Path) -> Result<Self, CodexError> {
        std::fs::create_dir_all(home_dir).map_err(|e| CodexError::new(
            ErrorCode::InternalError,
            format!("failed to create home dir: {e}"),
        ))?;
        remove_legacy_state_files(home_dir);
        let db_path = state_db_path(home_dir);
        Self::open(&db_path)
    }

    /// Execute all pending migrations.
    pub fn run_migrations(&self) -> Result<(), CodexError> {
        super::migration_runner::run_migrations(self.log_db.connection())
    }

    /// Save session metadata.
    pub fn save_session_meta(&mut self, meta: &SessionMeta) -> Result<(), CodexError> {
        // Ensure sessions table exists (for backward compat with old DBs)
        self.log_db.connection().execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                last_activity TEXT NOT NULL,
                config_profile TEXT
            )"
        ).map_err(|e| CodexError::new(ErrorCode::InternalError, format!("{e}")))?;

        self.log_db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO sessions (id, created_at, last_activity, config_profile)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    meta.id,
                    meta.timestamp.to_rfc3339(),
                    meta.last_activity.to_rfc3339(),
                    meta.config_profile,
                ],
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to save session meta: {e}"),
                )
            })?;
        self.runtime.metrics.total_writes += 1;
        Ok(())
    }

    /// Load session metadata by ID.
    pub fn load_session_meta(&mut self, id: &str) -> Result<Option<SessionMeta>, CodexError> {
        self.runtime.metrics.total_reads += 1;

        // Check if sessions table exists
        let exists: bool = self.log_db.connection().query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='sessions'",
            [],
            |r| r.get(0),
        ).map_err(|e| CodexError::new(ErrorCode::InternalError, format!("{e}")))?;
        if !exists {
            return Ok(None);
        }

        let result = self.log_db.connection().query_row(
            "SELECT id, created_at, last_activity, config_profile FROM sessions WHERE id = ?1",
            [id],
            |row| {
                let created_str: String = row.get(1)?;
                let activity_str: String = row.get(2)?;
                Ok(SessionMeta {
                    id: row.get(0)?,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&created_str)
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    last_activity: chrono::DateTime::parse_from_rfc3339(&activity_str)
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    config_profile: row.get(3)?,
                    forked_from_id: None,
                    cwd: None,
                    originator: None,
                    cli_version: None,
                    source: None,
                    agent_nickname: None,
                    agent_role: None,
                    model_provider: None,
                    base_instructions: None,
                    dynamic_tools: vec![],
                    memory_mode: None,
                })
            },
        );

        match result {
            Ok(meta) => Ok(Some(meta)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("failed to load session meta: {e}"),
            )),
        }
    }

    /// Save thread metadata (using the new Codex-aligned schema).
    pub fn save_thread_metadata(&mut self, meta: &ThreadMetadata) -> Result<(), CodexError> {
        let now = chrono::Utc::now().timestamp();
        let created_at = meta.created_at.timestamp();
        self.log_db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO threads (id, rollout_path, created_at, updated_at, source,
                 model_provider, cwd, title, sandbox_policy, approval_mode)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    meta.thread_id,
                    "",
                    created_at,
                    now,
                    "gui",
                    meta.model.as_deref().unwrap_or(""),
                    "",
                    meta.title.as_deref().unwrap_or(""),
                    "read_only",
                    "on_request",
                ],
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to save thread metadata: {e}"),
                )
            })?;
        self.runtime.metrics.total_writes += 1;
        Ok(())
    }

    /// Save an agent job.
    pub fn save_agent_job(&mut self, job: &AgentJob) -> Result<(), CodexError> {
        let items_json = serde_json::to_string(&job.items).map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to serialize agent job items: {e}"),
            )
        })?;
        let status_str = serde_json::to_string(&job.status).map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to serialize agent job status: {e}"),
            )
        })?;
        // Use the legacy agent_jobs table format for backward compat
        self.log_db.connection().execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_jobs_legacy (
                job_id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                status TEXT NOT NULL,
                items_json TEXT NOT NULL
            )"
        ).map_err(|e| CodexError::new(ErrorCode::InternalError, format!("{e}")))?;

        self.log_db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO agent_jobs_legacy (job_id, thread_id, status, items_json)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![job.job_id, job.thread_id, status_str, items_json],
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to save agent job: {e}"),
                )
            })?;
        self.runtime.metrics.total_writes += 1;
        Ok(())
    }

    /// Load an agent job by ID.
    pub fn load_agent_job(&mut self, job_id: &str) -> Result<Option<AgentJob>, CodexError> {
        self.runtime.metrics.total_reads += 1;

        let exists: bool = self.log_db.connection().query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='agent_jobs_legacy'",
            [],
            |r| r.get(0),
        ).map_err(|e| CodexError::new(ErrorCode::InternalError, format!("{e}")))?;
        if !exists {
            return Ok(None);
        }

        let result = self.log_db.connection().query_row(
            "SELECT job_id, thread_id, status, items_json FROM agent_jobs_legacy WHERE job_id = ?1",
            [job_id],
            |row| {
                let status_str: String = row.get(2)?;
                let items_json: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    status_str,
                    items_json,
                ))
            },
        );

        match result {
            Ok((job_id, thread_id, status_str, items_json)) => {
                let status: AgentJobStatus = serde_json::from_str(&status_str).map_err(|e| {
                    CodexError::new(
                        ErrorCode::InternalError,
                        format!("failed to deserialize job status: {e}"),
                    )
                })?;
                let items: Vec<AgentJobItem> = serde_json::from_str(&items_json).map_err(|e| {
                    CodexError::new(
                        ErrorCode::InternalError,
                        format!("failed to deserialize job items: {e}"),
                    )
                })?;
                Ok(Some(AgentJob {
                    job_id,
                    thread_id,
                    status,
                    items,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("failed to load agent job: {e}"),
            )),
        }
    }

    /// Get the current backfill state.
    pub fn get_backfill_state(&mut self) -> Result<BackfillState, CodexError> {
        self.runtime.metrics.total_reads += 1;
        let result = self.log_db.connection().query_row(
            "SELECT status, last_watermark, last_success_at FROM backfill_state WHERE id = 1",
            [],
            |row| {
                let status_str: String = row.get(0)?;
                let last_watermark: Option<String> = row.get(1)?;
                let last_success_at: Option<i64> = row.get(2)?;
                Ok((status_str, last_watermark, last_success_at))
            },
        );

        match result {
            Ok((status_str, last_watermark, last_success_at)) => {
                let status = BackfillStatus::parse(&status_str)?;
                let last_success_at = last_success_at.and_then(|secs| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
                });
                Ok(BackfillState { status, last_watermark, last_success_at })
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(BackfillState::default()),
            Err(e) => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("failed to get backfill state: {e}"),
            )),
        }
    }

    /// Update the backfill state.
    pub fn update_backfill_state(&mut self, state: &BackfillState) -> Result<(), CodexError> {
        let now = chrono::Utc::now().timestamp();
        let last_success_at = state.last_success_at.map(|dt| dt.timestamp());
        self.log_db
            .connection()
            .execute(
                "UPDATE backfill_state SET status = ?1, last_watermark = ?2,
                 last_success_at = ?3, updated_at = ?4 WHERE id = 1",
                rusqlite::params![state.status.as_str(), state.last_watermark, last_success_at, now],
            )
            .map_err(|e| CodexError::new(
                ErrorCode::InternalError,
                format!("failed to update backfill state: {e}"),
            ))?;
        self.runtime.metrics.total_writes += 1;
        Ok(())
    }

    /// Write a log entry.
    pub fn write_log(&mut self, entry: &LogEntry) -> Result<(), CodexError> {
        let estimated_bytes = entry.message.as_deref().map_or(0, |m| m.len())
            + entry.level.len()
            + entry.target.len()
            + entry.module_path.as_deref().map_or(0, |m| m.len())
            + entry.file.as_deref().map_or(0, |f| f.len());

        self.log_db
            .connection()
            .execute(
                "INSERT INTO logs (ts, ts_nanos, level, target, message, thread_id,
                 process_uuid, module_path, file, line, estimated_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    entry.ts, entry.ts_nanos, entry.level, entry.target,
                    entry.message, entry.thread_id, entry.process_uuid,
                    entry.module_path, entry.file, entry.line, estimated_bytes as i64,
                ],
            )
            .map_err(|e| CodexError::new(
                ErrorCode::InternalError,
                format!("failed to write log: {e}"),
            ))?;
        self.runtime.metrics.total_writes += 1;
        Ok(())
    }

    /// Query logs with filtering.
    pub fn query_logs(&mut self, query: &LogQuery) -> Result<Vec<LogRow>, CodexError> {
        self.runtime.metrics.total_reads += 1;
        let mut sql = String::from(
            "SELECT id, ts, ts_nanos, level, target, message, thread_id, process_uuid, file, line FROM logs WHERE 1=1"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref level) = query.level_upper {
            sql.push_str(" AND level <= ?");
            params.push(Box::new(level.clone()));
        }
        if let Some(from_ts) = query.from_ts {
            sql.push_str(" AND ts >= ?");
            params.push(Box::new(from_ts));
        }
        if let Some(to_ts) = query.to_ts {
            sql.push_str(" AND ts <= ?");
            params.push(Box::new(to_ts));
        }
        if let Some(after_id) = query.after_id {
            sql.push_str(" AND id > ?");
            params.push(Box::new(after_id));
        }
        if !query.thread_ids.is_empty() {
            let placeholders: Vec<String> = query.thread_ids.iter().enumerate()
                .map(|(_, _)| "?".to_string()).collect();
            sql.push_str(&format!(" AND thread_id IN ({})", placeholders.join(",")));
            for tid in &query.thread_ids {
                params.push(Box::new(tid.clone()));
            }
        }
        if let Some(ref search) = query.search {
            sql.push_str(" AND message LIKE ?");
            params.push(Box::new(format!("%{search}%")));
        }

        if query.descending {
            sql.push_str(" ORDER BY ts DESC, ts_nanos DESC, id DESC");
        } else {
            sql.push_str(" ORDER BY ts ASC, ts_nanos ASC, id ASC");
        }

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.log_db.connection().prepare(&sql).map_err(|e| {
            CodexError::new(ErrorCode::InternalError, format!("failed to prepare log query: {e}"))
        })?;

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(LogRow {
                id: row.get(0)?,
                ts: row.get(1)?,
                ts_nanos: row.get(2)?,
                level: row.get(3)?,
                target: row.get(4)?,
                message: row.get(5)?,
                thread_id: row.get(6)?,
                process_uuid: row.get(7)?,
                file: row.get(8)?,
                line: row.get(9)?,
            })
        }).map_err(|e| {
            CodexError::new(ErrorCode::InternalError, format!("failed to query logs: {e}"))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| {
                CodexError::new(ErrorCode::InternalError, format!("failed to read log row: {e}"))
            })?);
        }
        Ok(result)
    }
}

/// Return the versioned state DB filename.
pub fn state_db_filename() -> String {
    format!("{STATE_DB_FILENAME}_{STATE_DB_VERSION}.sqlite")
}

/// Return the full path to the versioned state DB.
pub fn state_db_path(home_dir: &Path) -> std::path::PathBuf {
    home_dir.join(state_db_filename())
}

/// Remove legacy state DB files from the home directory.
fn remove_legacy_state_files(home_dir: &Path) {
    let current_name = state_db_filename();
    let entries = match std::fs::read_dir(home_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if should_remove_state_file(&name, &current_name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn should_remove_state_file(file_name: &str, current_name: &str) -> bool {
    let mut base_name = file_name;
    for suffix in ["-wal", "-shm", "-journal"] {
        if let Some(stripped) = file_name.strip_suffix(suffix) {
            base_name = stripped;
            break;
        }
    }
    if base_name == current_name {
        return false;
    }
    let unversioned = format!("{STATE_DB_FILENAME}.sqlite");
    if base_name == unversioned {
        return true;
    }
    let Some(ver_ext) = base_name.strip_prefix(&format!("{STATE_DB_FILENAME}_")) else {
        return false;
    };
    let Some(ver) = ver_ext.strip_suffix(".sqlite") else {
        return false;
    };
    !ver.is_empty() && ver.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, StateDb) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = StateDb::open(&path).unwrap();
        (dir, db)
    }

    fn make_session_meta(id: &str, config_profile: Option<&str>) -> SessionMeta {
        let now = chrono::Utc::now();
        SessionMeta {
            id: id.to_string(),
            timestamp: now,
            last_activity: now,
            config_profile: config_profile.map(String::from),
            forked_from_id: None,
            cwd: None,
            originator: None,
            cli_version: None,
            source: None,
            agent_nickname: None,
            agent_role: None,
            model_provider: None,
            base_instructions: None,
            dynamic_tools: vec![],
            memory_mode: None,
        }
    }

    #[test]
    fn open_runs_migrations() {
        let (_dir, db) = temp_db();
        let ver = super::super::migration_runner::current_version(db.log_db.connection()).unwrap();
        assert_eq!(ver, super::super::migration_runner::SCHEMA_VERSION as i64);
    }

    #[test]
    fn migrations_are_idempotent() {
        let (_dir, db) = temp_db();
        db.run_migrations().unwrap();
        let ver = super::super::migration_runner::current_version(db.log_db.connection()).unwrap();
        assert_eq!(ver, super::super::migration_runner::SCHEMA_VERSION as i64);
    }

    #[test]
    fn session_meta_roundtrip() {
        let (_dir, mut db) = temp_db();
        let meta = make_session_meta("sess-1", Some("fast"));
        db.save_session_meta(&meta).unwrap();
        let loaded = db.load_session_meta("sess-1").unwrap().unwrap();
        assert_eq!(loaded.id, meta.id);
        assert_eq!(loaded.config_profile, meta.config_profile);
    }

    #[test]
    fn load_nonexistent_session() {
        let (_dir, mut db) = temp_db();
        let result = db.load_session_meta("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn session_meta_upsert() {
        let (_dir, mut db) = temp_db();
        let meta1 = make_session_meta("sess-1", Some("v1"));
        db.save_session_meta(&meta1).unwrap();

        let meta2 = make_session_meta("sess-1", Some("v2"));
        db.save_session_meta(&meta2).unwrap();

        let loaded = db.load_session_meta("sess-1").unwrap().unwrap();
        assert_eq!(loaded.config_profile, Some("v2".to_string()));
    }

    #[test]
    fn thread_metadata_save() {
        let (_dir, mut db) = temp_db();
        let meta = ThreadMetadata {
            thread_id: "t-1".to_string(),
            created_at: chrono::Utc::now(),
            title: Some("Test thread".to_string()),
            model: Some("gpt-4".to_string()),
        };
        db.save_thread_metadata(&meta).unwrap();
        assert_eq!(db.runtime.metrics.total_writes, 1);
    }

    #[test]
    fn agent_job_roundtrip() {
        let (_dir, mut db) = temp_db();
        let job = AgentJob {
            job_id: "job-1".to_string(),
            thread_id: "t-1".to_string(),
            status: AgentJobStatus::Running,
            items: vec![
                AgentJobItem {
                    item_id: "item-1".to_string(),
                    input: "hello".to_string(),
                    output: None,
                    status: AgentJobStatus::Pending,
                },
                AgentJobItem {
                    item_id: "item-2".to_string(),
                    input: "world".to_string(),
                    output: Some("done".to_string()),
                    status: AgentJobStatus::Completed,
                },
            ],
        };
        db.save_agent_job(&job).unwrap();
        let loaded = db.load_agent_job("job-1").unwrap().unwrap();
        assert_eq!(loaded, job);
    }

    #[test]
    fn load_nonexistent_job() {
        let (_dir, mut db) = temp_db();
        assert!(db.load_agent_job("nope").unwrap().is_none());
    }

    #[test]
    fn backfill_state_default() {
        let (_dir, mut db) = temp_db();
        let state = db.get_backfill_state().unwrap();
        assert_eq!(state.status, BackfillStatus::Pending);
        assert!(state.last_watermark.is_none());
    }

    #[test]
    fn backfill_state_update() {
        let (_dir, mut db) = temp_db();
        let now = chrono::Utc::now();
        let state = BackfillState {
            status: BackfillStatus::Complete,
            last_watermark: Some("wm-1".to_string()),
            last_success_at: Some(now),
        };
        db.update_backfill_state(&state).unwrap();
        let loaded = db.get_backfill_state().unwrap();
        assert_eq!(loaded.status, BackfillStatus::Complete);
        assert_eq!(loaded.last_watermark, Some("wm-1".to_string()));
    }

    #[test]
    fn metrics_tracking() {
        let (_dir, mut db) = temp_db();
        let meta = make_session_meta("s1", None);
        db.save_session_meta(&meta).unwrap();
        db.load_session_meta("s1").unwrap();
        assert_eq!(db.runtime.metrics.total_writes, 1);
        assert_eq!(db.runtime.metrics.total_reads, 1);
    }

    #[test]
    fn log_write_and_query() {
        let (_dir, mut db) = temp_db();
        let entry = LogEntry {
            ts: 1000,
            ts_nanos: 500,
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: Some("hello world".to_string()),
            thread_id: Some("t1".to_string()),
            process_uuid: None,
            module_path: None,
            file: None,
            line: None,
        };
        db.write_log(&entry).unwrap();

        let rows = db.query_logs(&LogQuery {
            thread_ids: vec!["t1".to_string()],
            limit: Some(10),
            ..Default::default()
        }).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message, Some("hello world".to_string()));
    }

    #[test]
    fn log_query_search() {
        let (_dir, mut db) = temp_db();
        for i in 0..3 {
            db.write_log(&LogEntry {
                ts: 1000 + i,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "test".to_string(),
                message: Some(format!("msg-{i}")),
                thread_id: None,
                process_uuid: None,
                module_path: None,
                file: None,
                line: None,
            }).unwrap();
        }

        let rows = db.query_logs(&LogQuery {
            search: Some("msg-1".to_string()),
            ..Default::default()
        }).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message, Some("msg-1".to_string()));
    }

    #[test]
    fn versioned_db_filename() {
        let name = state_db_filename();
        assert_eq!(name, format!("state_{STATE_DB_VERSION}.sqlite"));
    }

    #[test]
    fn should_remove_state_file_logic() {
        let current = "state_5.sqlite";
        assert!(!should_remove_state_file("state_5.sqlite", current));
        assert!(!should_remove_state_file("state_5.sqlite-wal", current));
        assert!(should_remove_state_file("state.sqlite", current));
        assert!(should_remove_state_file("state_4.sqlite", current));
        assert!(should_remove_state_file("state_4.sqlite-wal", current));
        assert!(should_remove_state_file("state_3.sqlite-shm", current));
        assert!(!should_remove_state_file("other.sqlite", current));
    }

    #[test]
    fn open_versioned_creates_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open_versioned(dir.path()).unwrap();
        let expected_path = dir.path().join(state_db_filename());
        assert_eq!(db.runtime.config.db_path, expected_path);
    }

    #[test]
    fn backfill_status_parse() {
        assert_eq!(BackfillStatus::parse("pending").unwrap(), BackfillStatus::Pending);
        assert_eq!(BackfillStatus::parse("running").unwrap(), BackfillStatus::Running);
        assert_eq!(BackfillStatus::parse("complete").unwrap(), BackfillStatus::Complete);
        assert!(BackfillStatus::parse("invalid").is_err());
    }
}

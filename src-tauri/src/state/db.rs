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
    pub last_processed_id: Option<String>,
    pub total_processed: u64,
}

/// Migration scripts for the state database.
const MIGRATIONS: &[&str] = &[
    // 1. Sessions table
    "CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY,
        created_at TEXT NOT NULL,
        last_activity TEXT NOT NULL,
        config_profile TEXT
    )",
    // 2. Rollouts table
    "CREATE TABLE IF NOT EXISTS rollouts (
        session_id TEXT PRIMARY KEY,
        events_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )",
    // 3. Threads table
    "CREATE TABLE IF NOT EXISTS threads (
        thread_id TEXT PRIMARY KEY,
        created_at TEXT NOT NULL,
        title TEXT,
        model TEXT
    )",
    // 4. Agent jobs table
    "CREATE TABLE IF NOT EXISTS agent_jobs (
        job_id TEXT PRIMARY KEY,
        thread_id TEXT NOT NULL,
        status TEXT NOT NULL,
        items_json TEXT NOT NULL
    )",
    // 5. Backfill state table
    "CREATE TABLE IF NOT EXISTS backfill_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        last_processed_id TEXT,
        total_processed INTEGER NOT NULL DEFAULT 0
    )",
    // 6. Memories table
    "CREATE TABLE IF NOT EXISTS memories (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        phase TEXT NOT NULL,
        content TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        relevance_score REAL NOT NULL
    )",
    // 7-18: Reserved for future migrations (indexes, etc.)
    "CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity)",
    "CREATE INDEX IF NOT EXISTS idx_agent_jobs_thread ON agent_jobs(thread_id)",
    "CREATE INDEX IF NOT EXISTS idx_memories_phase ON memories(phase)",
    "CREATE INDEX IF NOT EXISTS idx_memories_relevance ON memories(relevance_score)",
    "CREATE INDEX IF NOT EXISTS idx_threads_created ON threads(created_at)",
    // Placeholder migrations to reach 18 total (no-op CREATE TABLE IF NOT EXISTS)
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_12 (id INTEGER PRIMARY KEY)",
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_13 (id INTEGER PRIMARY KEY)",
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_14 (id INTEGER PRIMARY KEY)",
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_15 (id INTEGER PRIMARY KEY)",
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_16 (id INTEGER PRIMARY KEY)",
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_17 (id INTEGER PRIMARY KEY)",
    "CREATE TABLE IF NOT EXISTS _migration_placeholder_18 (id INTEGER PRIMARY KEY)",
];

/// Main state database with runtime config and log storage.
pub struct StateDb {
    pub runtime: StateRuntime,
    pub log_db: LogDb,
}

impl StateDb {
    /// Open (or create) the state database at the given path.
    pub fn open(path: &Path) -> Result<Self, CodexError> {
        let log_db = LogDb::open(path)?;
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

    /// Execute all migration scripts in order.
    pub fn run_migrations(&self) -> Result<(), CodexError> {
        // Create migration tracking table
        self.log_db
            .connection()
            .execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                )",
                [],
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to create migrations table: {e}"),
                )
            })?;

        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let version = (i + 1) as i64;
            let already_applied: bool = self
                .log_db
                .connection()
                .query_row(
                    "SELECT COUNT(*) > 0 FROM schema_migrations WHERE version = ?1",
                    [version],
                    |row| row.get(0),
                )
                .map_err(|e| {
                    CodexError::new(
                        ErrorCode::InternalError,
                        format!("failed to check migration {version}: {e}"),
                    )
                })?;

            if !already_applied {
                self.log_db.connection().execute(sql, []).map_err(|e| {
                    CodexError::new(
                        ErrorCode::InternalError,
                        format!("migration {version} failed: {e}"),
                    )
                })?;
                let now = chrono::Utc::now().to_rfc3339();
                self.log_db
                    .connection()
                    .execute(
                        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                        rusqlite::params![version, now],
                    )
                    .map_err(|e| {
                        CodexError::new(
                            ErrorCode::InternalError,
                            format!("failed to record migration {version}: {e}"),
                        )
                    })?;
            }
        }
        Ok(())
    }

    /// Save session metadata.
    pub fn save_session_meta(&mut self, meta: &SessionMeta) -> Result<(), CodexError> {
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

    /// Save thread metadata.
    pub fn save_thread_metadata(&mut self, meta: &ThreadMetadata) -> Result<(), CodexError> {
        self.log_db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO threads (thread_id, created_at, title, model)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    meta.thread_id,
                    meta.created_at.to_rfc3339(),
                    meta.title,
                    meta.model,
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
        self.log_db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO agent_jobs (job_id, thread_id, status, items_json)
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
        let result = self.log_db.connection().query_row(
            "SELECT job_id, thread_id, status, items_json FROM agent_jobs WHERE job_id = ?1",
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
            "SELECT last_processed_id, total_processed FROM backfill_state WHERE id = 1",
            [],
            |row| {
                Ok(BackfillState {
                    last_processed_id: row.get(0)?,
                    total_processed: row.get::<_, i64>(1)? as u64,
                })
            },
        );

        match result {
            Ok(state) => Ok(state),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(BackfillState {
                last_processed_id: None,
                total_processed: 0,
            }),
            Err(e) => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("failed to get backfill state: {e}"),
            )),
        }
    }
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
    fn open_creates_tables() {
        let (_dir, db) = temp_db();
        let count: i64 = db
            .log_db
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[test]
    fn migrations_are_idempotent() {
        let (_dir, db) = temp_db();
        db.run_migrations().unwrap();
        let count: i64 = db
            .log_db
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
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
        assert_eq!(
            state,
            BackfillState {
                last_processed_id: None,
                total_processed: 0,
            }
        );
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
}

//! File-based SQLite migration runner.
//!
//! Reads `.sql` files from the embedded migrations directory and applies them
//! in order, tracking applied versions in a `_migrations` table.

use rusqlite::Connection;

use crate::protocol::error::{CodexError, ErrorCode};

/// Embedded migration files, sorted by filename.
/// Each entry is (version_number, filename, sql_content).
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        1,
        "0001_threads",
        include_str!("../../migrations/0001_threads.sql"),
    ),
    (
        2,
        "0002_logs",
        include_str!("../../migrations/0002_logs.sql"),
    ),
    (
        3,
        "0003_logs_thread_id",
        include_str!("../../migrations/0003_logs_thread_id.sql"),
    ),
    (
        4,
        "0004_thread_dynamic_tools",
        include_str!("../../migrations/0004_thread_dynamic_tools.sql"),
    ),
    (
        5,
        "0005_threads_cli_version",
        include_str!("../../migrations/0005_threads_cli_version.sql"),
    ),
    (
        6,
        "0006_memories",
        include_str!("../../migrations/0006_memories.sql"),
    ),
    (
        7,
        "0007_threads_first_user_message",
        include_str!("../../migrations/0007_threads_first_user_message.sql"),
    ),
    (
        8,
        "0008_backfill_state",
        include_str!("../../migrations/0008_backfill_state.sql"),
    ),
    (
        9,
        "0009_stage1_outputs_rollout_slug",
        include_str!("../../migrations/0009_stage1_outputs_rollout_slug.sql"),
    ),
    (
        10,
        "0010_logs_process_id",
        include_str!("../../migrations/0010_logs_process_id.sql"),
    ),
    (
        11,
        "0011_logs_partition_prune_indexes",
        include_str!("../../migrations/0011_logs_partition_prune_indexes.sql"),
    ),
    (
        12,
        "0012_logs_estimated_bytes",
        include_str!("../../migrations/0012_logs_estimated_bytes.sql"),
    ),
    (
        13,
        "0013_threads_agent_nickname",
        include_str!("../../migrations/0013_threads_agent_nickname.sql"),
    ),
    (
        14,
        "0014_agent_jobs",
        include_str!("../../migrations/0014_agent_jobs.sql"),
    ),
    (
        15,
        "0015_agent_jobs_max_runtime_seconds",
        include_str!("../../migrations/0015_agent_jobs_max_runtime_seconds.sql"),
    ),
    (
        16,
        "0016_memory_usage",
        include_str!("../../migrations/0016_memory_usage.sql"),
    ),
    (
        17,
        "0017_phase2_selection_flag",
        include_str!("../../migrations/0017_phase2_selection_flag.sql"),
    ),
    (
        18,
        "0018_phase2_selection_snapshot",
        include_str!("../../migrations/0018_phase2_selection_snapshot.sql"),
    ),
    (
        19,
        "0019_mosaic_compat",
        include_str!("../../migrations/0019_mosaic_compat.sql"),
    ),
];

/// The current schema version (number of migrations).
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Run all pending migrations on the given connection.
pub fn run_migrations(conn: &Connection) -> Result<(), CodexError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        )",
    )
    .map_err(|e| {
        CodexError::new(
            ErrorCode::InternalError,
            format!("failed to create _migrations table: {e}"),
        )
    })?;

    let max_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |r| r.get(0),
        )
        .map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to query migration version: {e}"),
            )
        })?;

    for &(version, name, sql) in MIGRATIONS {
        if version <= max_version {
            continue;
        }
        // Execute each statement in the migration file separately.
        // SQLite's execute_batch handles multiple statements separated by `;`.
        conn.execute_batch(sql).map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("migration {version} ({name}) failed: {e}"),
            )
        })?;

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![version, name, now],
        )
        .map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to record migration {version}: {e}"),
            )
        })?;
    }

    Ok(())
}

/// Return the current applied schema version.
pub fn current_version(conn: &Connection) -> Result<i64, CodexError> {
    // Table may not exist yet.
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_migrations'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| CodexError::new(ErrorCode::InternalError, format!("{e}")))?;

    if !exists {
        return Ok(0);
    }

    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _migrations",
        [],
        |r| r.get(0),
    )
    .map_err(|e| CodexError::new(ErrorCode::InternalError, format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn
    }

    #[test]
    fn run_all_migrations_from_scratch() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();
        let ver = current_version(&conn).unwrap();
        assert_eq!(ver, SCHEMA_VERSION);
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();
        let ver = current_version(&conn).unwrap();
        assert_eq!(ver, SCHEMA_VERSION);
    }

    #[test]
    fn tables_created_correctly() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();

        // Verify key tables exist
        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };

        assert!(tables.contains(&"threads".to_string()));
        assert!(tables.contains(&"logs".to_string()));
        assert!(tables.contains(&"stage1_outputs".to_string()));
        assert!(tables.contains(&"jobs".to_string()));
        assert!(tables.contains(&"backfill_state".to_string()));
        assert!(tables.contains(&"agent_jobs".to_string()));
        assert!(tables.contains(&"agent_job_items".to_string()));
        assert!(tables.contains(&"thread_dynamic_tools".to_string()));
        assert!(tables.contains(&"_migrations".to_string()));
    }

    #[test]
    fn threads_table_has_all_columns() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();

        // Insert a row to verify all columns from migrations 1,5,7,13,18
        conn.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source,
             model_provider, cwd, title, sandbox_policy, approval_mode,
             cli_version, first_user_message, agent_nickname, agent_role, memory_mode)
             VALUES ('t1', '/tmp/r', 1000, 1000, 'cli', 'openai', '/tmp', 'test',
                     'read_only', 'on_request', '1.0', 'hello', 'nick', 'coder', 'enabled')",
            [],
        )
        .unwrap();

        let title: String = conn
            .query_row("SELECT title FROM threads WHERE id = 't1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(title, "test");
    }

    #[test]
    fn logs_table_has_all_columns() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();

        // Verify columns from migrations 2,3,10,12
        conn.execute(
            "INSERT INTO logs (ts, ts_nanos, level, target, thread_id, process_uuid, estimated_bytes)
             VALUES (1000, 0, 'INFO', 'test', 'tid', 'pid', 42)",
            [],
        ).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn backfill_state_seeded() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();

        let status: String = conn
            .query_row("SELECT status FROM backfill_state WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(status, "pending");
    }

    #[test]
    fn stage1_outputs_has_all_columns() {
        let conn = open_memory_db();
        run_migrations(&conn).unwrap();

        // First insert a thread (FK constraint)
        conn.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source,
             model_provider, cwd, title, sandbox_policy, approval_mode)
             VALUES ('t1', '/tmp/r', 1000, 1000, 'cli', 'openai', '/tmp', 'test', 'ro', 'or')",
            [],
        )
        .unwrap();

        // Verify columns from migrations 6,9,16,17,18
        conn.execute(
            "INSERT INTO stage1_outputs (thread_id, source_updated_at, raw_memory,
             rollout_summary, rollout_slug, generated_at, usage_count, last_usage,
             selected_for_phase2, selected_for_phase2_source_updated_at)
             VALUES ('t1', 1000, 'mem', 'summary', 'slug', 1000, 5, 900, 1, 1000)",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM stage1_outputs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn current_version_on_empty_db() {
        let conn = open_memory_db();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn incremental_migration() {
        let conn = open_memory_db();

        // Apply only first 5 migrations manually
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
        )
        .unwrap();

        for &(version, name, sql) in &MIGRATIONS[..5] {
            conn.execute_batch(sql).unwrap();
            conn.execute(
                "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![version, name, "2026-01-01T00:00:00Z"],
            )
            .unwrap();
        }

        assert_eq!(current_version(&conn).unwrap(), 5);

        // Now run all — should apply 6..18
        run_migrations(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
    }
}

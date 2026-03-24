//! Lightweight key-value state database backed by SQLite.
//!
//! Provides persistent storage for session metadata, feature flags,
//! and other application state that must survive restarts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{Connection, params};
use tokio::sync::Mutex;
use tracing::warn;

/// Persisted thread metadata row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedThreadMeta {
    pub thread_id: String,
    pub cwd: String,
    pub model: Option<String>,
    pub model_provider_id: Option<String>,
    pub name: Option<String>,
    pub created_at: String,
    pub forked_from: Option<String>,
    pub rollout_path: Option<String>,
}

/// Handle to the state database.
#[derive(Clone)]
pub struct StateDb {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl StateDb {
    /// Open (or create) the state database at the given path.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create db directory: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("failed to open db: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                id         TEXT PRIMARY KEY,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                metadata   TEXT
            );
            CREATE TABLE IF NOT EXISTS threads (
                thread_id         TEXT PRIMARY KEY,
                cwd               TEXT NOT NULL,
                model             TEXT,
                model_provider_id TEXT,
                name              TEXT,
                created_at        TEXT NOT NULL,
                forked_from       TEXT,
                rollout_path      TEXT
            );",
        )
        .map_err(|e| format!("failed to init schema: {e}"))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path.to_path_buf(),
        })
    }

    /// Get a value by key.
    pub async fn get(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().await;
        conn.query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
            row.get(0)
        })
        .ok()
    }

    /// Set a key-value pair (upsert).
    pub async fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| format!("set failed: {e}"))?;
        Ok(())
    }

    /// Delete a key.
    pub async fn delete(&self, key: &str) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM kv WHERE key = ?1", params![key])
            .map_err(|e| format!("delete failed: {e}"))?;
        Ok(())
    }

    /// Record a session.
    pub async fn record_session(&self, id: &str, metadata: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO sessions (id, metadata) VALUES (?1, ?2)",
            params![id, metadata],
        )
        .map_err(|e| format!("record_session failed: {e}"))?;
        Ok(())
    }

    /// List session IDs, most recent first.
    pub async fn list_sessions(&self, limit: usize) -> Vec<String> {
        let conn = self.conn.lock().await;
        let mut stmt = match conn
            .prepare("SELECT id FROM sessions ORDER BY created_at DESC LIMIT ?1")
        {
            Ok(s) => s,
            Err(e) => {
                warn!("list_sessions prepare failed: {e}");
                return Vec::new();
            }
        };
        let rows = match stmt.query_map(params![limit as i64], |row| row.get::<_, String>(0)) {
            Ok(r) => r,
            Err(e) => {
                warn!("list_sessions query failed: {e}");
                return Vec::new();
            }
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    // ── Thread persistence ────────────────────────────────────────

    /// Upsert a thread record.
    pub async fn upsert_thread(&self, t: &PersistedThreadMeta) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO threads (thread_id, cwd, model, model_provider_id, name, created_at, forked_from, rollout_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(thread_id) DO UPDATE SET
               model = excluded.model,
               model_provider_id = excluded.model_provider_id,
               name = excluded.name,
               rollout_path = excluded.rollout_path",
            params![t.thread_id, t.cwd, t.model, t.model_provider_id, t.name, t.created_at, t.forked_from, t.rollout_path],
        )
        .map_err(|e| format!("upsert_thread failed: {e}"))?;
        Ok(())
    }

    /// Update specific fields of a thread.
    pub async fn update_thread_fields(
        &self,
        thread_id: &str,
        model: Option<&str>,
        model_provider_id: Option<&str>,
        name: Option<&str>,
        rollout_path: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE threads SET
               model = COALESCE(?2, model),
               model_provider_id = COALESCE(?3, model_provider_id),
               name = COALESCE(?4, name),
               rollout_path = COALESCE(?5, rollout_path)
             WHERE thread_id = ?1",
            params![thread_id, model, model_provider_id, name, rollout_path],
        )
        .map_err(|e| format!("update_thread_fields failed: {e}"))?;
        Ok(())
    }

    /// List threads, most recent first.
    pub async fn list_threads(&self, limit: usize) -> Vec<PersistedThreadMeta> {
        let conn = self.conn.lock().await;
        let mut stmt = match conn.prepare(
            "SELECT thread_id, cwd, model, model_provider_id, name, created_at, forked_from, rollout_path
             FROM threads ORDER BY created_at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("list_threads prepare failed: {e}");
                return Vec::new();
            }
        };
        let rows = match stmt.query_map(params![limit as i64], |row| {
            Ok(PersistedThreadMeta {
                thread_id: row.get(0)?,
                cwd: row.get(1)?,
                model: row.get(2)?,
                model_provider_id: row.get(3)?,
                name: row.get(4)?,
                created_at: row.get(5)?,
                forked_from: row.get(6)?,
                rollout_path: row.get(7)?,
            })
        }) {
            Ok(r) => r,
            Err(e) => {
                warn!("list_threads query failed: {e}");
                return Vec::new();
            }
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Delete a thread record.
    pub async fn delete_thread(&self, thread_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM threads WHERE thread_id = ?1", params![thread_id])
            .map_err(|e| format!("delete_thread failed: {e}"))?;
        Ok(())
    }

    /// Path to the database file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn kv_round_trip() {
        let tmp = TempDir::new().unwrap();
        let db = StateDb::open(&tmp.path().join("state.db")).unwrap();

        assert!(db.get("key1").await.is_none());
        db.set("key1", "value1").await.unwrap();
        assert_eq!(db.get("key1").await.unwrap(), "value1");

        db.set("key1", "updated").await.unwrap();
        assert_eq!(db.get("key1").await.unwrap(), "updated");

        db.delete("key1").await.unwrap();
        assert!(db.get("key1").await.is_none());
    }

    #[tokio::test]
    async fn sessions() {
        let tmp = TempDir::new().unwrap();
        let db = StateDb::open(&tmp.path().join("state.db")).unwrap();

        db.record_session("s1", Some(r#"{"model":"gpt-4o"}"#))
            .await
            .unwrap();
        db.record_session("s2", None).await.unwrap();

        let sessions = db.list_sessions(10).await;
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn open_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let deep = tmp.path().join("a/b/c/state.db");
        let db = StateDb::open(&deep).unwrap();
        db.set("test", "ok").await.unwrap();
        assert_eq!(db.get("test").await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn threads_crud() {
        let tmp = TempDir::new().unwrap();
        let db = StateDb::open(&tmp.path().join("state.db")).unwrap();

        let t = PersistedThreadMeta {
            thread_id: "t1".into(),
            cwd: "/tmp".into(),
            model: None,
            model_provider_id: None,
            name: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            forked_from: None,
            rollout_path: None,
        };
        db.upsert_thread(&t).await.unwrap();

        let list = db.list_threads(10).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].thread_id, "t1");

        db.update_thread_fields("t1", Some("gpt-4o"), None, Some("my chat"), None)
            .await
            .unwrap();
        let list = db.list_threads(10).await;
        assert_eq!(list[0].model.as_deref(), Some("gpt-4o"));
        assert_eq!(list[0].name.as_deref(), Some("my chat"));

        db.delete_thread("t1").await.unwrap();
        assert!(db.list_threads(10).await.is_empty());
    }
}

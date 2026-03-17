//! Lightweight key-value state database backed by SQLite.
//!
//! Provides persistent storage for session metadata, feature flags,
//! and other application state that must survive restarts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{Connection, params};
use tokio::sync::Mutex;
use tracing::warn;

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
}

use serde::{Deserialize, Serialize};

use crate::protocol::error::{CodexError, ErrorCode};

use super::db::StateDb;

/// Memory phase — short-term vs long-term.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryPhase {
    Phase1,
    Phase2,
}

impl MemoryPhase {
    fn as_str(&self) -> &'static str {
        match self {
            MemoryPhase::Phase1 => "phase1",
            MemoryPhase::Phase2 => "phase2",
        }
    }

    fn from_str(s: &str) -> Result<Self, CodexError> {
        match s {
            "phase1" => Ok(MemoryPhase::Phase1),
            "phase2" => Ok(MemoryPhase::Phase2),
            other => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("unknown memory phase: {other}"),
            )),
        }
    }
}

/// A memory entry with phase, content, timestamp, and relevance score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Memory {
    pub id: Option<i64>,
    pub phase: MemoryPhase,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub relevance_score: f64,
}

impl StateDb {
    /// Save a memory entry. Returns the assigned row ID.
    pub fn save_memory(&mut self, memory: &Memory) -> Result<i64, CodexError> {
        self.log_db
            .connection()
            .execute(
                "INSERT INTO memories (phase, content, timestamp, relevance_score)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    memory.phase.as_str(),
                    memory.content,
                    memory.timestamp.to_rfc3339(),
                    memory.relevance_score,
                ],
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to save memory: {e}"),
                )
            })?;
        self.runtime.metrics.total_writes += 1;
        Ok(self.log_db.connection().last_insert_rowid())
    }

    /// Load a memory entry by ID.
    pub fn load_memory(&mut self, id: i64) -> Result<Option<Memory>, CodexError> {
        self.runtime.metrics.total_reads += 1;
        let result = self.log_db.connection().query_row(
            "SELECT id, phase, content, timestamp, relevance_score FROM memories WHERE id = ?1",
            [id],
            |row| {
                let phase_str: String = row.get(1)?;
                let ts_str: String = row.get(3)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    phase_str,
                    row.get::<_, String>(2)?,
                    ts_str,
                    row.get::<_, f64>(4)?,
                ))
            },
        );

        match result {
            Ok((row_id, phase_str, content, ts_str, relevance_score)) => {
                let phase = MemoryPhase::from_str(&phase_str)?;
                let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                    .unwrap()
                    .with_timezone(&chrono::Utc);
                Ok(Some(Memory {
                    id: Some(row_id),
                    phase,
                    content,
                    timestamp,
                    relevance_score,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CodexError::new(
                ErrorCode::InternalError,
                format!("failed to load memory: {e}"),
            )),
        }
    }

    /// List memories by phase.
    pub fn list_memories_by_phase(
        &mut self,
        phase: &MemoryPhase,
    ) -> Result<Vec<Memory>, CodexError> {
        self.runtime.metrics.total_reads += 1;
        let mut stmt = self
            .log_db
            .connection()
            .prepare(
                "SELECT id, phase, content, timestamp, relevance_score
                 FROM memories WHERE phase = ?1 ORDER BY relevance_score DESC",
            )
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to prepare memory query: {e}"),
                )
            })?;

        let rows = stmt
            .query_map([phase.as_str()], |row| {
                let phase_str: String = row.get(1)?;
                let ts_str: String = row.get(3)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    phase_str,
                    row.get::<_, String>(2)?,
                    ts_str,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to query memories: {e}"),
                )
            })?;

        let mut memories = Vec::new();
        for row in rows {
            let (row_id, phase_str, content, ts_str, relevance_score) = row.map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to read memory row: {e}"),
                )
            })?;
            let phase = MemoryPhase::from_str(&phase_str)?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .unwrap()
                .with_timezone(&chrono::Utc);
            memories.push(Memory {
                id: Some(row_id),
                phase,
                content,
                timestamp,
                relevance_score,
            });
        }
        Ok(memories)
    }

    /// Delete a memory entry by ID.
    pub fn delete_memory(&mut self, id: i64) -> Result<bool, CodexError> {
        let affected = self
            .log_db
            .connection()
            .execute("DELETE FROM memories WHERE id = ?1", [id])
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::InternalError,
                    format!("failed to delete memory: {e}"),
                )
            })?;
        self.runtime.metrics.total_writes += 1;
        Ok(affected > 0)
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

    fn sample_memory(phase: MemoryPhase, content: &str, score: f64) -> Memory {
        Memory {
            id: None,
            phase,
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            relevance_score: score,
        }
    }

    #[test]
    fn memory_save_and_load() {
        let (_dir, mut db) = temp_db();
        let mem = sample_memory(MemoryPhase::Phase1, "hello world", 0.95);
        let id = db.save_memory(&mem).unwrap();
        let loaded = db.load_memory(id).unwrap().unwrap();
        assert_eq!(loaded.phase, MemoryPhase::Phase1);
        assert_eq!(loaded.content, "hello world");
        assert!((loaded.relevance_score - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn load_nonexistent_memory() {
        let (_dir, mut db) = temp_db();
        assert!(db.load_memory(999).unwrap().is_none());
    }

    #[test]
    fn list_by_phase() {
        let (_dir, mut db) = temp_db();
        db.save_memory(&sample_memory(MemoryPhase::Phase1, "a", 0.5))
            .unwrap();
        db.save_memory(&sample_memory(MemoryPhase::Phase1, "b", 0.9))
            .unwrap();
        db.save_memory(&sample_memory(MemoryPhase::Phase2, "c", 0.7))
            .unwrap();

        let phase1 = db.list_memories_by_phase(&MemoryPhase::Phase1).unwrap();
        assert_eq!(phase1.len(), 2);
        // Ordered by relevance_score DESC
        assert_eq!(phase1[0].content, "b");
        assert_eq!(phase1[1].content, "a");

        let phase2 = db.list_memories_by_phase(&MemoryPhase::Phase2).unwrap();
        assert_eq!(phase2.len(), 1);
        assert_eq!(phase2[0].content, "c");
    }

    #[test]
    fn delete_memory() {
        let (_dir, mut db) = temp_db();
        let id = db
            .save_memory(&sample_memory(MemoryPhase::Phase1, "temp", 0.1))
            .unwrap();
        assert!(db.delete_memory(id).unwrap());
        assert!(db.load_memory(id).unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let (_dir, mut db) = temp_db();
        assert!(!db.delete_memory(999).unwrap());
    }
}

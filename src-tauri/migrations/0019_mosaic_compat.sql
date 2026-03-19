-- Mosaic-specific compatibility tables not present in Codex schema.

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    last_activity TEXT NOT NULL,
    config_profile TEXT
);

CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity);

CREATE TABLE IF NOT EXISTS rollouts (
    session_id TEXT PRIMARY KEY,
    events_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    phase TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    relevance_score REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_phase ON memories(phase);
CREATE INDEX IF NOT EXISTS idx_memories_relevance ON memories(relevance_score);

CREATE TABLE IF NOT EXISTS agent_jobs_legacy (
    job_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    status TEXT NOT NULL,
    items_json TEXT NOT NULL
);

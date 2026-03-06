/// Context compaction — reduces conversation history to fit within token limits.
pub struct CompactionEngine {
    token_limit: usize,
}

impl CompactionEngine {
    pub fn new(token_limit: usize) -> Self {
        Self { token_limit }
    }

    pub fn token_limit(&self) -> usize {
        self.token_limit
    }

    /// Estimate whether compaction is needed based on current token count.
    pub fn needs_compaction(&self, current_tokens: usize) -> bool {
        current_tokens > self.token_limit
    }
}

/// Truncation strategy for context window management.
pub struct TruncationStrategy {
    pub max_tokens: usize,
    pub preserve_system: bool,
    pub preserve_last_n_turns: usize,
}

impl Default for TruncationStrategy {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            preserve_system: true,
            preserve_last_n_turns: 3,
        }
    }
}

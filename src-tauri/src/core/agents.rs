/// Multi-agent management — spawning and coordinating sub-agents.
pub struct AgentManager {
    max_concurrent: usize,
    active_count: usize,
}

impl AgentManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            active_count: 0,
        }
    }

    pub fn can_spawn(&self) -> bool {
        self.active_count < self.max_concurrent
    }

    pub fn active_count(&self) -> usize {
        self.active_count
    }

    pub fn mark_spawned(&mut self) {
        self.active_count += 1;
    }

    pub fn mark_completed(&mut self) {
        self.active_count = self.active_count.saturating_sub(1);
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new(4)
    }
}

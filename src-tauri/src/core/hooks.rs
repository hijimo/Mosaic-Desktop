/// Lifecycle hooks for the Codex engine.
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn on_turn_start(&self, _turn_id: &str) {}
    fn on_turn_complete(&self, _turn_id: &str) {}
    fn on_session_start(&self, _session_id: &str) {}
    fn on_session_end(&self, _session_id: &str) {}
}

/// Registry for lifecycle hooks.
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: Box<dyn Hook>) {
        self.hooks.push(hook);
    }

    pub fn notify_turn_start(&self, turn_id: &str) {
        for hook in &self.hooks {
            hook.on_turn_start(turn_id);
        }
    }

    pub fn notify_turn_complete(&self, turn_id: &str) {
        for hook in &self.hooks {
            hook.on_turn_complete(turn_id);
        }
    }

    pub fn notify_session_start(&self, session_id: &str) {
        for hook in &self.hooks {
            hook.on_session_start(session_id);
        }
    }

    pub fn notify_session_end(&self, session_id: &str) {
        for hook in &self.hooks {
            hook.on_session_end(session_id);
        }
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

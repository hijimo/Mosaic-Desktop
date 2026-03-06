/// Realtime voice conversation handler.
/// Manages WebSocket connection to the realtime API.
pub struct RealtimeSession {
    active: bool,
    session_id: Option<String>,
}

impl RealtimeSession {
    pub fn new() -> Self {
        Self {
            active: false,
            session_id: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn start(&mut self, session_id: String) {
        self.active = true;
        self.session_id = Some(session_id);
    }

    pub fn close(&mut self) {
        self.active = false;
        self.session_id = None;
    }
}

impl Default for RealtimeSession {
    fn default() -> Self {
        Self::new()
    }
}

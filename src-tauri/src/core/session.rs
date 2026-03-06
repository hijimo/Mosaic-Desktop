use std::path::PathBuf;

use crate::config::toml_types::ConfigToml;

/// Active session managing turn state and context.
pub struct Session {
    id: String,
    cwd: PathBuf,
    model: String,
    turn_counter: u64,
    interrupted: bool,
}

impl Session {
    pub fn new(cwd: PathBuf, config: ConfigToml) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            model: config.model.unwrap_or_else(|| "default".into()),
            cwd,
            turn_counter: 0,
            interrupted: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(&mut self, model: String) {
        self.model = model;
    }

    pub fn start_turn(&mut self) -> String {
        self.turn_counter += 1;
        self.interrupted = false;
        format!("turn-{}", self.turn_counter)
    }

    pub fn interrupt(&mut self) {
        self.interrupted = true;
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lifecycle() {
        let mut session = Session::new(PathBuf::from("/tmp"), ConfigToml::default());
        assert!(!session.id().is_empty());
        assert_eq!(session.model(), "default");

        let turn1 = session.start_turn();
        assert_eq!(turn1, "turn-1");
        assert!(!session.is_interrupted());

        session.interrupt();
        assert!(session.is_interrupted());

        let turn2 = session.start_turn();
        assert_eq!(turn2, "turn-2");
        assert!(!session.is_interrupted());
    }
}

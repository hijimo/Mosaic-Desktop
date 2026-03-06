/// Tool handler trait — each tool type implements this.
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
}

/// Routes tool calls to the appropriate handler.
pub struct ToolRouter {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRouter {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        self.handlers.push(handler);
    }

    pub fn find_handler(&self, name: &str) -> Option<&dyn ToolHandler> {
        self.handlers.iter().find(|h| h.name() == name).map(|h| h.as_ref())
    }

    pub fn list_tools(&self) -> Vec<&str> {
        self.handlers.iter().map(|h| h.name()).collect()
    }
}

impl Default for ToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;
    impl ToolHandler for DummyTool {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> &str { "a dummy tool" }
    }

    #[test]
    fn register_and_find() {
        let mut router = ToolRouter::new();
        router.register(Box::new(DummyTool));
        assert!(router.find_handler("dummy").is_some());
        assert!(router.find_handler("nonexistent").is_none());
        assert_eq!(router.list_tools(), vec!["dummy"]);
    }
}

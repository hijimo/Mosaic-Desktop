use crate::core::tools::router::ToolHandler;
use crate::protocol::types::DynamicToolSpec;

/// Handler for dynamically registered tools.
pub struct DynamicToolHandler {
    specs: Vec<DynamicToolSpec>,
}

impl DynamicToolHandler {
    pub fn new() -> Self {
        Self { specs: Vec::new() }
    }

    pub fn register_tool(&mut self, spec: DynamicToolSpec) {
        self.specs.push(spec);
    }

    pub fn registered_tools(&self) -> &[DynamicToolSpec] {
        &self.specs
    }
}

impl ToolHandler for DynamicToolHandler {
    fn name(&self) -> &str {
        "dynamic"
    }

    fn description(&self) -> &str {
        "handles dynamically registered tool calls"
    }
}

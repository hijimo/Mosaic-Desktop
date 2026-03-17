//! Lightweight model metadata descriptor.

use serde::{Deserialize, Serialize};

/// Metadata for a single model known to the system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Unique slug, e.g. `"gpt-4o"`.
    pub slug: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Sort priority (lower = higher priority).
    #[serde(default = "default_priority")]
    pub priority: i32,
    /// Context window size in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<i64>,
    /// Whether this model is the default selection.
    #[serde(default)]
    pub is_default: bool,
    /// Whether this model should appear in the picker UI.
    #[serde(default = "default_true")]
    pub show_in_picker: bool,
    /// Whether the model supports parallel tool calls.
    #[serde(default)]
    pub supports_parallel_tool_calls: bool,
    /// Whether the model supports reasoning summaries.
    #[serde(default)]
    pub supports_reasoning_summaries: bool,
}

fn default_priority() -> i32 {
    99
}
fn default_true() -> bool {
    true
}

impl ModelDescriptor {
    /// Build a minimal fallback descriptor for an unknown slug.
    pub fn fallback(slug: &str) -> Self {
        Self {
            slug: slug.to_string(),
            display_name: slug.to_string(),
            description: None,
            priority: 99,
            context_window: Some(128_000),
            is_default: false,
            show_in_picker: false,
            supports_parallel_tool_calls: false,
            supports_reasoning_summaries: false,
        }
    }
}

/// Response shape for a models list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelDescriptor>,
}

//! Tool specification types and JSON schema definitions.
//!
//! Adapted from Codex `tools/spec.rs`. Provides `JsonSchema`, `ToolSpec`,
//! and builder functions for all built-in tool definitions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Generic JSON Schema subset for tool parameter definitions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    #[serde(alias = "integer")]
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(
            rename = "additionalProperties",
            skip_serializing_if = "Option::is_none"
        )]
        additional_properties: Option<AdditionalProperties>,
    },
}

/// Whether additional properties are allowed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<JsonSchema>),
}

impl From<bool> for AdditionalProperties {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

/// A tool specification sent to the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ToolSpec {
    #[serde(rename = "function")]
    Function {
        name: String,
        description: String,
        #[serde(default)]
        strict: bool,
        parameters: JsonSchema,
    },
}

impl ToolSpec {
    pub fn name(&self) -> &str {
        match self {
            Self::Function { name, .. } => name,
        }
    }
}

/// Configuration for which tools are enabled.
#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub shell_enabled: bool,
    pub apply_patch_enabled: bool,
    pub js_repl_enabled: bool,
    pub collab_tools: bool,
    pub search_tool: bool,
    pub presentation_artifact: bool,
    pub web_search_enabled: bool,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            shell_enabled: true,
            apply_patch_enabled: true,
            js_repl_enabled: false,
            collab_tools: false,
            search_tool: false,
            presentation_artifact: false,
            web_search_enabled: false,
        }
    }
}

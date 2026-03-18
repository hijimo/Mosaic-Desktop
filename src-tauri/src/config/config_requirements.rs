use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::constraint::{Constrained, ConstraintError, ConstraintResult};
use crate::protocol::types::{AskForApproval, SandboxMode, WebSearchMode};

// ── RequirementSource ────────────────────────────────────────────

/// Where a configuration requirement originated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementSource {
    Unknown,
    CloudRequirements,
    SystemRequirementsToml { file: std::path::PathBuf },
}

impl fmt::Display for RequirementSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "<unspecified>"),
            Self::CloudRequirements => write!(f, "cloud requirements"),
            Self::SystemRequirementsToml { file } => write!(f, "{}", file.display()),
        }
    }
}

// ── ConstrainedWithSource ────────────────────────────────────────

/// A constrained value paired with its requirement source.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstrainedWithSource<T> {
    pub value: Constrained<T>,
    pub source: Option<RequirementSource>,
}

impl<T> ConstrainedWithSource<T> {
    pub fn new(value: Constrained<T>, source: Option<RequirementSource>) -> Self {
        Self { value, source }
    }
}

impl<T> std::ops::Deref for ConstrainedWithSource<T> {
    type Target = Constrained<T>;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for ConstrainedWithSource<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

// ── Sourced ──────────────────────────────────────────────────────

/// A value paired with its requirement source.
#[derive(Debug, Clone, PartialEq)]
pub struct Sourced<T> {
    pub value: T,
    pub source: RequirementSource,
}

impl<T> Sourced<T> {
    pub fn new(value: T, source: RequirementSource) -> Self {
        Self { value, source }
    }
}

impl<T> std::ops::Deref for Sourced<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

// ── SandboxModeRequirement ───────────────────────────────────────

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxModeRequirement {
    #[serde(rename = "read-only")]
    ReadOnly,
    #[serde(rename = "workspace-write")]
    WorkspaceWrite,
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,
}

impl From<SandboxMode> for SandboxModeRequirement {
    fn from(mode: SandboxMode) -> Self {
        match mode {
            SandboxMode::ReadOnly => Self::ReadOnly,
            SandboxMode::WorkspaceWrite => Self::WorkspaceWrite,
            SandboxMode::DangerFullAccess => Self::DangerFullAccess,
        }
    }
}

// ── WebSearchModeRequirement ─────────────────────────────────────

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum WebSearchModeRequirement {
    Disabled,
    Cached,
    Live,
}

impl From<WebSearchMode> for WebSearchModeRequirement {
    fn from(mode: WebSearchMode) -> Self {
        match mode {
            WebSearchMode::Disabled => Self::Disabled,
            WebSearchMode::Cached => Self::Cached,
            WebSearchMode::Live => Self::Live,
        }
    }
}

impl From<WebSearchModeRequirement> for WebSearchMode {
    fn from(mode: WebSearchModeRequirement) -> Self {
        match mode {
            WebSearchModeRequirement::Disabled => Self::Disabled,
            WebSearchModeRequirement::Cached => Self::Cached,
            WebSearchModeRequirement::Live => Self::Live,
        }
    }
}

// ── ResidencyRequirement ─────────────────────────────────────────

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResidencyRequirement {
    Us,
}

// ── MCP server identity ──────────────────────────────────────────

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum McpServerIdentity {
    Command { command: String },
    Url { url: String },
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct McpServerRequirement {
    pub identity: McpServerIdentity,
}

// ── Network constraints ──────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConstraints {
    pub enabled: Option<bool>,
    pub allowed_domains: Option<Vec<String>>,
    pub denied_domains: Option<Vec<String>>,
    pub allow_local_binding: Option<bool>,
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkRequirementsToml {
    pub enabled: Option<bool>,
    pub allowed_domains: Option<Vec<String>>,
    pub denied_domains: Option<Vec<String>>,
    pub allow_local_binding: Option<bool>,
}

impl From<NetworkRequirementsToml> for NetworkConstraints {
    fn from(v: NetworkRequirementsToml) -> Self {
        Self {
            enabled: v.enabled,
            allowed_domains: v.allowed_domains,
            denied_domains: v.denied_domains,
            allow_local_binding: v.allow_local_binding,
        }
    }
}

// ── ConfigRequirementsToml ───────────────────────────────────────

/// Raw requirements deserialized from `requirements.toml` or cloud.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
pub struct ConfigRequirementsToml {
    pub allowed_approval_policies: Option<Vec<AskForApproval>>,
    pub allowed_sandbox_modes: Option<Vec<SandboxModeRequirement>>,
    pub allowed_web_search_modes: Option<Vec<WebSearchModeRequirement>>,
    pub mcp_servers: Option<BTreeMap<String, McpServerRequirement>>,
    pub enforce_residency: Option<ResidencyRequirement>,
    pub network: Option<NetworkRequirementsToml>,
}

impl ConfigRequirementsToml {
    pub fn is_empty(&self) -> bool {
        self.allowed_approval_policies.is_none()
            && self.allowed_sandbox_modes.is_none()
            && self.allowed_web_search_modes.is_none()
            && self.mcp_servers.is_none()
            && self.enforce_residency.is_none()
            && self.network.is_none()
    }
}

// ── ConfigRequirements (normalized) ──────────────────────────────

/// Normalized requirements with constrained values ready for enforcement.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigRequirements {
    pub approval_policy: ConstrainedWithSource<AskForApproval>,
    pub sandbox_mode: ConstrainedWithSource<SandboxMode>,
    pub web_search_mode: ConstrainedWithSource<WebSearchMode>,
    pub mcp_servers: Option<Sourced<BTreeMap<String, McpServerRequirement>>>,
    pub enforce_residency: ConstrainedWithSource<Option<ResidencyRequirement>>,
    pub network: Option<Sourced<NetworkConstraints>>,
}

impl Default for ConfigRequirements {
    fn default() -> Self {
        Self {
            approval_policy: ConstrainedWithSource::new(
                Constrained::allow_any_from_default(),
                None,
            ),
            sandbox_mode: ConstrainedWithSource::new(
                Constrained::allow_any(SandboxMode::ReadOnly),
                None,
            ),
            web_search_mode: ConstrainedWithSource::new(
                Constrained::allow_any(WebSearchMode::Cached),
                None,
            ),
            mcp_servers: None,
            enforce_residency: ConstrainedWithSource::new(Constrained::allow_any(None), None),
            network: None,
        }
    }
}

impl TryFrom<ConfigRequirementsToml> for ConfigRequirements {
    type Error = ConstraintError;

    fn try_from(toml: ConfigRequirementsToml) -> ConstraintResult<Self> {
        let approval_policy = match toml.allowed_approval_policies {
            Some(policies) => {
                let first = policies
                    .first()
                    .cloned()
                    .ok_or_else(|| ConstraintError::empty_field("allowed_approval_policies"))?;
                let source = RequirementSource::Unknown;
                let src_clone = source.clone();
                let constrained = Constrained::new(first, move |candidate| {
                    if policies.contains(candidate) {
                        Ok(())
                    } else {
                        Err(ConstraintError::InvalidValue {
                            field_name: "approval_policy",
                            candidate: format!("{candidate:?}"),
                            allowed: format!("{policies:?}"),
                            requirement_source: src_clone.clone(),
                        })
                    }
                })?;
                ConstrainedWithSource::new(constrained, Some(source))
            }
            None => ConstrainedWithSource::new(Constrained::allow_any_from_default(), None),
        };

        let sandbox_mode = match toml.allowed_sandbox_modes {
            Some(modes) => {
                let source = RequirementSource::Unknown;
                let src_clone = source.clone();
                let constrained =
                    Constrained::new(SandboxMode::ReadOnly, move |candidate| {
                        let req: SandboxModeRequirement = (*candidate).into();
                        if modes.contains(&req) {
                            Ok(())
                        } else {
                            Err(ConstraintError::InvalidValue {
                                field_name: "sandbox_mode",
                                candidate: format!("{req:?}"),
                                allowed: format!("{modes:?}"),
                                requirement_source: src_clone.clone(),
                            })
                        }
                    })?;
                ConstrainedWithSource::new(constrained, Some(source))
            }
            None => ConstrainedWithSource::new(
                Constrained::allow_any(SandboxMode::ReadOnly),
                None,
            ),
        };

        let web_search_mode = match toml.allowed_web_search_modes {
            Some(modes) => {
                let source = RequirementSource::Unknown;
                let src_clone = source.clone();
                let initial = if modes.contains(&WebSearchModeRequirement::Cached) {
                    WebSearchMode::Cached
                } else {
                    WebSearchMode::Disabled
                };
                let constrained = Constrained::new(initial, move |candidate| {
                    let req: WebSearchModeRequirement = (*candidate).into();
                    if modes.contains(&req) {
                        Ok(())
                    } else {
                        Err(ConstraintError::InvalidValue {
                            field_name: "web_search",
                            candidate: format!("{req:?}"),
                            allowed: format!("{modes:?}"),
                            requirement_source: src_clone.clone(),
                        })
                    }
                })?;
                ConstrainedWithSource::new(constrained, Some(source))
            }
            None => ConstrainedWithSource::new(
                Constrained::allow_any(WebSearchMode::Cached),
                None,
            ),
        };

        let network = toml.network.map(|n| {
            Sourced::new(NetworkConstraints::from(n), RequirementSource::Unknown)
        });

        let mcp_servers = toml.mcp_servers.map(|s| {
            Sourced::new(s, RequirementSource::Unknown)
        });

        let enforce_residency = match toml.enforce_residency {
            Some(r) => ConstrainedWithSource::new(
                Constrained::allow_only(Some(r)),
                Some(RequirementSource::Unknown),
            ),
            None => ConstrainedWithSource::new(Constrained::allow_any(None), None),
        };

        Ok(Self {
            approval_policy,
            sandbox_mode,
            web_search_mode,
            mcp_servers,
            enforce_residency,
            network,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_requirements_allows_all() {
        let reqs = ConfigRequirements::default();
        reqs.approval_policy.can_set(&AskForApproval::Never).unwrap();
        reqs.sandbox_mode.can_set(&SandboxMode::DangerFullAccess).unwrap();
    }

    #[test]
    fn constrained_approval_rejects_unlisted() {
        let toml = ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::OnRequest]),
            ..Default::default()
        };
        let reqs = ConfigRequirements::try_from(toml).unwrap();
        assert!(reqs.approval_policy.can_set(&AskForApproval::Never).is_err());
        reqs.approval_policy.can_set(&AskForApproval::OnRequest).unwrap();
    }

    #[test]
    fn constrained_sandbox_rejects_unlisted() {
        let toml = ConfigRequirementsToml {
            allowed_sandbox_modes: Some(vec![SandboxModeRequirement::ReadOnly]),
            ..Default::default()
        };
        let reqs = ConfigRequirements::try_from(toml).unwrap();
        assert!(reqs.sandbox_mode.can_set(&SandboxMode::DangerFullAccess).is_err());
    }

    #[test]
    fn requirements_toml_is_empty() {
        assert!(ConfigRequirementsToml::default().is_empty());
    }
}

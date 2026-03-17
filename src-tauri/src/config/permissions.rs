use serde::{Deserialize, Serialize};

/// Network permission settings from `[permissions.network]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct NetworkToml {
    pub enabled: Option<bool>,
    pub mode: Option<NetworkMode>,
    pub allowed_domains: Option<Vec<String>>,
    pub denied_domains: Option<Vec<String>>,
    pub allow_local_binding: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    Limited,
    Full,
}

/// Top-level `[permissions]` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct PermissionsToml {
    pub network: Option<NetworkToml>,
}

impl NetworkToml {
    /// Check whether a given domain is allowed under this policy.
    pub fn is_domain_allowed(&self, domain: &str) -> bool {
        if let Some(denied) = &self.denied_domains {
            if denied.iter().any(|d| d == domain) {
                return false;
            }
        }
        if let Some(allowed) = &self.allowed_domains {
            return allowed.iter().any(|d| d == domain || d == "*");
        }
        // No explicit allow-list means everything is allowed (unless denied above).
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_policy_allows_all() {
        let net = NetworkToml::default();
        assert!(net.is_domain_allowed("example.com"));
    }

    #[test]
    fn denied_domain_blocked() {
        let net = NetworkToml {
            denied_domains: Some(vec!["evil.com".into()]),
            ..Default::default()
        };
        assert!(!net.is_domain_allowed("evil.com"));
        assert!(net.is_domain_allowed("good.com"));
    }

    #[test]
    fn allow_list_restricts() {
        let net = NetworkToml {
            allowed_domains: Some(vec!["api.openai.com".into()]),
            ..Default::default()
        };
        assert!(net.is_domain_allowed("api.openai.com"));
        assert!(!net.is_domain_allowed("other.com"));
    }

    #[test]
    fn wildcard_allows_all() {
        let net = NetworkToml {
            allowed_domains: Some(vec!["*".into()]),
            ..Default::default()
        };
        assert!(net.is_domain_allowed("anything.com"));
    }

    #[test]
    fn deny_takes_precedence_over_allow() {
        let net = NetworkToml {
            allowed_domains: Some(vec!["*".into()]),
            denied_domains: Some(vec!["blocked.com".into()]),
            ..Default::default()
        };
        assert!(!net.is_domain_allowed("blocked.com"));
        assert!(net.is_domain_allowed("other.com"));
    }
}

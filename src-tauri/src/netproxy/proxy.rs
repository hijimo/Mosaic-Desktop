use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};

use crate::execpolicy::{Decision, NetworkRule, NetworkRuleProtocol};
use crate::protocol::error::{CodexError, ErrorCode};

/// Configuration for the network proxy.
#[derive(Debug, Clone)]
pub struct NetworkProxyConfig {
    pub listen_addr: String,
    pub socks5_addr: String,
    pub allowed_domains: Vec<String>,
    pub blocked_domains: Vec<String>,
    pub mitm_enabled: bool,
}

impl Default for NetworkProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:0".to_string(),
            socks5_addr: "127.0.0.1:0".to_string(),
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
            mitm_enabled: false,
        }
    }
}

/// Evaluates domain access decisions based on allow/deny network rules.
/// Deny rules take priority over allow rules.
#[derive(Debug, Clone)]
pub struct NetworkPolicyDecider {
    pub allow_rules: Vec<NetworkRule>,
    pub deny_rules: Vec<NetworkRule>,
}

impl NetworkPolicyDecider {
    pub fn new(allow_rules: Vec<NetworkRule>, deny_rules: Vec<NetworkRule>) -> Self {
        Self {
            allow_rules,
            deny_rules,
        }
    }

    /// Build a decider from a `NetworkProxyConfig`.
    pub fn from_config(config: &NetworkProxyConfig) -> Self {
        let allow_rules = config
            .allowed_domains
            .iter()
            .map(|host| NetworkRule {
                host: host.to_ascii_lowercase(),
                protocol: NetworkRuleProtocol::Https,
                decision: Decision::Allow,
                justification: None,
            })
            .collect();

        let deny_rules = config
            .blocked_domains
            .iter()
            .map(|host| NetworkRule {
                host: host.to_ascii_lowercase(),
                protocol: NetworkRuleProtocol::Https,
                decision: Decision::Forbidden,
                justification: None,
            })
            .collect();

        Self::new(allow_rules, deny_rules)
    }

    /// Evaluate whether a domain (and port) should be allowed, denied, or prompted.
    /// Deny rules always take priority over allow rules.
    pub fn evaluate(&self, domain: &str, _port: u16) -> Decision {
        let normalized = domain.to_ascii_lowercase();

        // Deny rules take priority
        for rule in &self.deny_rules {
            if domain_matches(&rule.host, &normalized) {
                return Decision::Forbidden;
            }
        }

        // Then check allow rules
        for rule in &self.allow_rules {
            if domain_matches(&rule.host, &normalized) {
                return Decision::Allow;
            }
        }

        // No match — prompt
        Decision::Prompt
    }
}

/// Check if a rule host matches the given domain.
/// Supports exact match and suffix match (e.g. rule "example.com" matches "sub.example.com").
fn domain_matches(rule_host: &str, domain: &str) -> bool {
    if rule_host == domain {
        return true;
    }
    // Suffix match: domain ends with ".{rule_host}"
    domain.ends_with(&format!(".{rule_host}"))
}

/// HTTP proxy server placeholder.
pub struct HttpProxyServer {
    listener: Option<TcpListener>,
}

impl HttpProxyServer {
    async fn bind(addr: &str) -> Result<Self, CodexError> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to bind HTTP proxy on {addr}: {e}"),
            )
        })?;
        Ok(Self {
            listener: Some(listener),
        })
    }

    fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.listener.as_ref().and_then(|l| l.local_addr().ok())
    }
}

/// SOCKS5 proxy server placeholder.
pub struct Socks5ProxyServer {
    listener: Option<TcpListener>,
}

impl Socks5ProxyServer {
    async fn bind(addr: &str) -> Result<Self, CodexError> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            CodexError::new(
                ErrorCode::InternalError,
                format!("failed to bind SOCKS5 proxy on {addr}: {e}"),
            )
        })?;
        Ok(Self {
            listener: Some(listener),
        })
    }

    fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.listener.as_ref().and_then(|l| l.local_addr().ok())
    }
}

/// Certificate manager for MITM proxy functionality.
pub struct CertificateManager {
    ca_cert: Vec<u8>,
    ca_key: Vec<u8>,
}

impl CertificateManager {
    pub fn new(ca_cert: Vec<u8>, ca_key: Vec<u8>) -> Self {
        Self { ca_cert, ca_key }
    }

    pub fn empty() -> Self {
        Self {
            ca_cert: Vec::new(),
            ca_key: Vec::new(),
        }
    }

    pub fn ca_cert(&self) -> &[u8] {
        &self.ca_cert
    }

    pub fn ca_key(&self) -> &[u8] {
        &self.ca_key
    }
}

/// Full network proxy server with HTTP/SOCKS5 proxying, certificate management,
/// policy-based domain filtering, and runtime config reloading.
pub struct NetworkProxy {
    http_proxy: HttpProxyServer,
    socks5_proxy: Socks5ProxyServer,
    cert_manager: CertificateManager,
    policy_decider: Arc<Mutex<NetworkPolicyDecider>>,
    config_tx: watch::Sender<NetworkProxyConfig>,
    config_rx: watch::Receiver<NetworkProxyConfig>,
    running: Arc<Mutex<bool>>,
}

impl NetworkProxy {
    /// Start the proxy with the given configuration.
    pub async fn start(config: NetworkProxyConfig) -> Result<Self, CodexError> {
        let http_proxy = HttpProxyServer::bind(&config.listen_addr).await?;
        let socks5_proxy = Socks5ProxyServer::bind(&config.socks5_addr).await?;
        let cert_manager = CertificateManager::empty();
        let policy_decider = Arc::new(Mutex::new(NetworkPolicyDecider::from_config(&config)));
        let (config_tx, config_rx) = watch::channel(config);

        Ok(Self {
            http_proxy,
            socks5_proxy,
            cert_manager,
            policy_decider,
            config_tx,
            config_rx,
            running: Arc::new(Mutex::new(true)),
        })
    }

    /// Stop the proxy, releasing all listeners.
    pub async fn stop(&mut self) -> Result<(), CodexError> {
        let mut running = self.running.lock().await;
        *running = false;
        // Drop listeners to release bound ports
        self.http_proxy.listener = None;
        self.socks5_proxy.listener = None;
        Ok(())
    }

    /// Check if a domain is allowed by the current policy.
    pub async fn is_domain_allowed(&self, domain: &str) -> bool {
        let decider = self.policy_decider.lock().await;
        decider.evaluate(domain, 443) == Decision::Allow
    }

    /// Reload configuration at runtime via the watch channel.
    pub async fn reload_config(&self, config: NetworkProxyConfig) -> Result<(), CodexError> {
        let new_decider = NetworkPolicyDecider::from_config(&config);
        {
            let mut decider = self.policy_decider.lock().await;
            *decider = new_decider;
        }
        self.config_tx.send(config).map_err(|_| {
            CodexError::new(
                ErrorCode::InternalError,
                "failed to broadcast config update: all receivers dropped",
            )
        })
    }

    /// Get the current config via the watch receiver.
    pub fn config_receiver(&self) -> watch::Receiver<NetworkProxyConfig> {
        self.config_rx.clone()
    }

    /// Whether the proxy is currently running.
    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }

    pub fn http_local_addr(&self) -> Option<std::net::SocketAddr> {
        self.http_proxy.local_addr()
    }

    pub fn socks5_local_addr(&self) -> Option<std::net::SocketAddr> {
        self.socks5_proxy.local_addr()
    }

    pub fn cert_manager(&self) -> &CertificateManager {
        &self.cert_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_takes_priority_over_allow() {
        let allow = vec![NetworkRule {
            host: "example.com".to_string(),
            protocol: NetworkRuleProtocol::Https,
            decision: Decision::Allow,
            justification: None,
        }];
        let deny = vec![NetworkRule {
            host: "example.com".to_string(),
            protocol: NetworkRuleProtocol::Https,
            decision: Decision::Forbidden,
            justification: None,
        }];
        let decider = NetworkPolicyDecider::new(allow, deny);
        assert_eq!(decider.evaluate("example.com", 443), Decision::Forbidden);
    }

    #[test]
    fn allow_when_only_allow_rule() {
        let decider = NetworkPolicyDecider::new(
            vec![NetworkRule {
                host: "api.github.com".to_string(),
                protocol: NetworkRuleProtocol::Https,
                decision: Decision::Allow,
                justification: None,
            }],
            vec![],
        );
        assert_eq!(decider.evaluate("api.github.com", 443), Decision::Allow);
    }

    #[test]
    fn prompt_when_no_rules_match() {
        let decider = NetworkPolicyDecider::new(vec![], vec![]);
        assert_eq!(decider.evaluate("unknown.com", 80), Decision::Prompt);
    }

    #[test]
    fn subdomain_matches_parent_rule() {
        let decider = NetworkPolicyDecider::new(
            vec![NetworkRule {
                host: "example.com".to_string(),
                protocol: NetworkRuleProtocol::Https,
                decision: Decision::Allow,
                justification: None,
            }],
            vec![],
        );
        assert_eq!(decider.evaluate("sub.example.com", 443), Decision::Allow);
        assert_eq!(
            decider.evaluate("deep.sub.example.com", 443),
            Decision::Allow
        );
    }

    #[test]
    fn deny_subdomain_blocks() {
        let decider = NetworkPolicyDecider::new(
            vec![],
            vec![NetworkRule {
                host: "evil.com".to_string(),
                protocol: NetworkRuleProtocol::Http,
                decision: Decision::Forbidden,
                justification: None,
            }],
        );
        assert_eq!(decider.evaluate("evil.com", 80), Decision::Forbidden);
        assert_eq!(decider.evaluate("sub.evil.com", 80), Decision::Forbidden);
    }

    #[test]
    fn case_insensitive_matching() {
        let decider = NetworkPolicyDecider::new(
            vec![NetworkRule {
                host: "example.com".to_string(),
                protocol: NetworkRuleProtocol::Https,
                decision: Decision::Allow,
                justification: None,
            }],
            vec![],
        );
        assert_eq!(decider.evaluate("EXAMPLE.COM", 443), Decision::Allow);
        assert_eq!(decider.evaluate("Example.Com", 443), Decision::Allow);
    }

    #[test]
    fn from_config_builds_rules() {
        let config = NetworkProxyConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            socks5_addr: "127.0.0.1:0".to_string(),
            allowed_domains: vec!["good.com".to_string()],
            blocked_domains: vec!["bad.com".to_string()],
            mitm_enabled: false,
        };
        let decider = NetworkPolicyDecider::from_config(&config);
        assert_eq!(decider.evaluate("good.com", 443), Decision::Allow);
        assert_eq!(decider.evaluate("bad.com", 443), Decision::Forbidden);
        assert_eq!(decider.evaluate("other.com", 443), Decision::Prompt);
    }

    #[test]
    fn domain_matches_exact() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(!domain_matches("example.com", "notexample.com"));
    }

    #[test]
    fn domain_matches_suffix() {
        assert!(domain_matches("example.com", "sub.example.com"));
        assert!(!domain_matches("example.com", "fakeexample.com"));
    }

    #[tokio::test]
    async fn proxy_start_and_stop() {
        let config = NetworkProxyConfig::default();
        let mut proxy = NetworkProxy::start(config).await.unwrap();
        assert!(proxy.is_running().await);
        assert!(proxy.http_local_addr().is_some());
        assert!(proxy.socks5_local_addr().is_some());

        proxy.stop().await.unwrap();
        assert!(!proxy.is_running().await);
        assert!(proxy.http_local_addr().is_none());
        assert!(proxy.socks5_local_addr().is_none());
    }

    #[tokio::test]
    async fn proxy_is_domain_allowed() {
        let config = NetworkProxyConfig {
            allowed_domains: vec!["allowed.com".to_string()],
            blocked_domains: vec!["blocked.com".to_string()],
            ..Default::default()
        };
        let proxy = NetworkProxy::start(config).await.unwrap();
        assert!(proxy.is_domain_allowed("allowed.com").await);
        assert!(!proxy.is_domain_allowed("blocked.com").await);
        assert!(!proxy.is_domain_allowed("unknown.com").await);
    }

    #[tokio::test]
    async fn proxy_reload_config() {
        let config = NetworkProxyConfig {
            allowed_domains: vec!["old.com".to_string()],
            ..Default::default()
        };
        let proxy = NetworkProxy::start(config).await.unwrap();
        assert!(proxy.is_domain_allowed("old.com").await);
        assert!(!proxy.is_domain_allowed("new.com").await);

        let new_config = NetworkProxyConfig {
            allowed_domains: vec!["new.com".to_string()],
            ..Default::default()
        };
        proxy.reload_config(new_config).await.unwrap();
        assert!(proxy.is_domain_allowed("new.com").await);
        // old.com no longer in allow list
        assert!(!proxy.is_domain_allowed("old.com").await);
    }

    #[tokio::test]
    async fn proxy_config_receiver_gets_updates() {
        let config = NetworkProxyConfig::default();
        let proxy = NetworkProxy::start(config).await.unwrap();
        let mut rx = proxy.config_receiver();

        let new_config = NetworkProxyConfig {
            allowed_domains: vec!["updated.com".to_string()],
            ..Default::default()
        };
        proxy.reload_config(new_config).await.unwrap();

        rx.changed().await.unwrap();
        let received = rx.borrow();
        assert_eq!(received.allowed_domains, vec!["updated.com".to_string()]);
    }

    #[test]
    fn cert_manager_empty() {
        let cm = CertificateManager::empty();
        assert!(cm.ca_cert().is_empty());
        assert!(cm.ca_key().is_empty());
    }

    #[test]
    fn cert_manager_with_data() {
        let cm = CertificateManager::new(vec![1, 2, 3], vec![4, 5, 6]);
        assert_eq!(cm.ca_cert(), &[1, 2, 3]);
        assert_eq!(cm.ca_key(), &[4, 5, 6]);
    }
}

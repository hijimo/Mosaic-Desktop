use super::prefix_rule::{Decision, NetworkRuleProtocol};
use crate::execpolicy::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRule {
    pub host: String,
    pub protocol: NetworkRuleProtocol,
    pub decision: Decision,
    pub justification: Option<String>,
}

pub fn normalize_network_rule_host(raw: &str) -> Result<String> {
    let mut host = raw.trim();
    if host.is_empty() {
        return Err(Error::InvalidRule(
            "network_rule host cannot be empty".to_string(),
        ));
    }
    if host.contains("://") || host.contains('/') || host.contains('?') || host.contains('#') {
        return Err(Error::InvalidRule(
            "network_rule host must be a hostname or IP literal (without scheme or path)"
                .to_string(),
        ));
    }

    if let Some(stripped) = host.strip_prefix('[') {
        if let Some((inside, rest)) = stripped.split_once(']') {
            let port_ok = rest
                .strip_prefix(':')
                .is_some_and(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()));
            if !rest.is_empty() && !port_ok {
                return Err(Error::InvalidRule(format!(
                    "network_rule host contains an unsupported suffix: {raw}"
                )));
            }
            host = inside;
        } else {
            return Err(Error::InvalidRule(
                "network_rule host has an invalid bracketed IPv6 literal".to_string(),
            ));
        }
    } else if host.matches(':').count() == 1 {
        if let Some((candidate, port)) = host.rsplit_once(':') {
            if !candidate.is_empty()
                && !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit())
            {
                host = candidate;
            }
        }
    }

    let normalized = host.trim_end_matches('.').trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(Error::InvalidRule(
            "network_rule host cannot be empty".to_string(),
        ));
    }
    if normalized.contains('*') {
        return Err(Error::InvalidRule(
            "network_rule host must be a specific host; wildcards are not allowed".to_string(),
        ));
    }
    if normalized.chars().any(char::is_whitespace) {
        return Err(Error::InvalidRule(
            "network_rule host cannot contain whitespace".to_string(),
        ));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_port() {
        assert_eq!(
            normalize_network_rule_host("example.com:8080").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn normalize_strips_trailing_dot() {
        assert_eq!(
            normalize_network_rule_host("example.com.").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(
            normalize_network_rule_host("Example.COM").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn normalize_rejects_url() {
        assert!(normalize_network_rule_host("https://example.com").is_err());
    }

    #[test]
    fn normalize_rejects_wildcard() {
        assert!(normalize_network_rule_host("*.example.com").is_err());
    }

    #[test]
    fn normalize_rejects_empty() {
        assert!(normalize_network_rule_host("").is_err());
        assert!(normalize_network_rule_host("  ").is_err());
    }

    #[test]
    fn normalize_ipv6_bracket() {
        assert_eq!(normalize_network_rule_host("[::1]").unwrap(), "::1");
        assert_eq!(normalize_network_rule_host("[::1]:8080").unwrap(), "::1");
    }
}

use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

use crate::execpolicy::error::{Error, Result};

/// Decision outcome for a policy evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Allow,
    Prompt,
    Forbidden,
}

impl Decision {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "allow" => Ok(Self::Allow),
            "prompt" => Ok(Self::Prompt),
            "forbidden" => Ok(Self::Forbidden),
            other => Err(Error::InvalidDecision(other.to_string())),
        }
    }
}

/// Matches a single command token, either a fixed string or one of several allowed alternatives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternToken {
    Single(String),
    Alts(Vec<String>),
}

impl PatternToken {
    fn matches(&self, token: &str) -> bool {
        match self {
            Self::Single(expected) => expected == token,
            Self::Alts(alternatives) => alternatives.iter().any(|alt| alt == token),
        }
    }

    pub fn alternatives(&self) -> &[String] {
        match self {
            Self::Single(expected) => std::slice::from_ref(expected),
            Self::Alts(alternatives) => alternatives,
        }
    }
}

/// Prefix matcher for commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrefixPattern {
    pub first: Arc<str>,
    pub rest: Arc<[PatternToken]>,
}

impl PrefixPattern {
    pub fn new(first: String, rest: Vec<PatternToken>) -> Self {
        Self {
            first: first.into(),
            rest: rest.into(),
        }
    }

    pub fn matches_prefix(&self, cmd: &[String]) -> Option<Vec<String>> {
        let pattern_length = self.rest.len() + 1;
        if cmd.len() < pattern_length || cmd[0] != self.first.as_ref() {
            return None;
        }
        for (pattern_token, cmd_token) in self.rest.iter().zip(&cmd[1..pattern_length]) {
            if !pattern_token.matches(cmd_token) {
                return None;
            }
        }
        Some(cmd[..pattern_length].to_vec())
    }
}

/// A rule match result with full context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleMatch {
    PrefixRuleMatch {
        #[serde(rename = "matchedPrefix")]
        matched_prefix: Vec<String>,
        decision: Decision,
        #[serde(rename = "resolvedProgram", skip_serializing_if = "Option::is_none")]
        resolved_program: Option<std::path::PathBuf>,
        #[serde(skip_serializing_if = "Option::is_none")]
        justification: Option<String>,
    },
    HeuristicsRuleMatch {
        command: Vec<String>,
        decision: Decision,
    },
}

impl RuleMatch {
    pub fn decision(&self) -> Decision {
        match self {
            Self::PrefixRuleMatch { decision, .. } => *decision,
            Self::HeuristicsRuleMatch { decision, .. } => *decision,
        }
    }

    pub fn with_resolved_program(self, resolved: &std::path::Path) -> Self {
        match self {
            Self::PrefixRuleMatch {
                matched_prefix,
                decision,
                justification,
                ..
            } => Self::PrefixRuleMatch {
                matched_prefix,
                decision,
                resolved_program: Some(resolved.to_path_buf()),
                justification,
            },
            other => other,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrefixRule {
    pub pattern: PrefixPattern,
    pub decision: Decision,
    pub justification: Option<String>,
}

/// Network rule protocol types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkRuleProtocol {
    Http,
    Https,
    Socks5Tcp,
    Socks5Udp,
}

impl NetworkRuleProtocol {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "http" => Ok(Self::Http),
            "https" | "https_connect" | "http-connect" => Ok(Self::Https),
            "socks5_tcp" => Ok(Self::Socks5Tcp),
            "socks5_udp" => Ok(Self::Socks5Udp),
            other => Err(Error::InvalidRule(format!(
                "network_rule protocol must be one of http, https, socks5_tcp, socks5_udp (got {other})"
            ))),
        }
    }

    pub fn as_policy_string(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5Tcp => "socks5_tcp",
            Self::Socks5Udp => "socks5_udp",
        }
    }
}

pub trait Rule: Any + Debug + Send + Sync {
    fn program(&self) -> &str;
    fn matches(&self, cmd: &[String]) -> Option<RuleMatch>;
    fn as_any(&self) -> &dyn Any;
}

pub type RuleRef = Arc<dyn Rule>;

impl Rule for PrefixRule {
    fn program(&self) -> &str {
        self.pattern.first.as_ref()
    }

    fn matches(&self, cmd: &[String]) -> Option<RuleMatch> {
        self.pattern
            .matches_prefix(cmd)
            .map(|matched_prefix| RuleMatch::PrefixRuleMatch {
                matched_prefix,
                decision: self.decision,
                resolved_program: None,
                justification: self.justification.clone(),
            })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Count how many rules match each provided example and error if any example is unmatched.
pub(crate) fn validate_match_examples(
    policy: &super::Policy,
    rules: &[RuleRef],
    matches: &[Vec<String>],
) -> Result<()> {
    let options = super::MatchOptions {
        resolve_host_executables: true,
    };
    let mut unmatched = Vec::new();
    for example in matches {
        if policy
            .matches_for_command_with_options(example, None, &options)
            .is_empty()
        {
            unmatched.push(
                shlex::try_join(example.iter().map(String::as_str))
                    .unwrap_or_else(|_| "unable to render example".to_string()),
            );
        }
    }
    if unmatched.is_empty() {
        Ok(())
    } else {
        Err(Error::ExampleDidNotMatch {
            rules: rules.iter().map(|r| format!("{r:?}")).collect(),
            examples: unmatched,
            location: None,
        })
    }
}

/// Ensure that no rule matches any provided negative example.
pub(crate) fn validate_not_match_examples(
    policy: &super::Policy,
    _rules: &[RuleRef],
    not_matches: &[Vec<String>],
) -> Result<()> {
    let options = super::MatchOptions {
        resolve_host_executables: true,
    };
    for example in not_matches {
        if let Some(rule) = policy
            .matches_for_command_with_options(example, None, &options)
            .first()
        {
            return Err(Error::ExampleDidMatch {
                rule: format!("{rule:?}"),
                example: shlex::try_join(example.iter().map(String::as_str))
                    .unwrap_or_else(|_| "unable to render example".to_string()),
                location: None,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(first: &str, rest: &[&str]) -> PrefixPattern {
        PrefixPattern::new(
            first.to_string(),
            rest.iter()
                .map(|s| PatternToken::Single(s.to_string()))
                .collect(),
        )
    }

    fn cmd(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_match() {
        let p = pattern("git", &["status"]);
        assert!(p.matches_prefix(&cmd(&["git", "status"])).is_some());
    }

    #[test]
    fn prefix_match_accepts_extra_tokens() {
        let p = pattern("git", &[]);
        assert!(p.matches_prefix(&cmd(&["git", "status"])).is_some());
    }

    #[test]
    fn no_match_wrong_prefix() {
        let p = pattern("npm", &[]);
        assert!(p.matches_prefix(&cmd(&["git", "status"])).is_none());
    }

    #[test]
    fn too_few_command_tokens() {
        let p = pattern("git", &["push"]);
        assert!(p.matches_prefix(&cmd(&["git"])).is_none());
    }

    #[test]
    fn alts_pattern_matches() {
        let p = PrefixPattern::new(
            "git".to_string(),
            vec![PatternToken::Alts(vec![
                "push".to_string(),
                "pull".to_string(),
            ])],
        );
        assert!(p.matches_prefix(&cmd(&["git", "push"])).is_some());
        assert!(p.matches_prefix(&cmd(&["git", "pull"])).is_some());
        assert!(p.matches_prefix(&cmd(&["git", "status"])).is_none());
    }

    #[test]
    fn decision_parse() {
        assert_eq!(Decision::parse("allow").unwrap(), Decision::Allow);
        assert_eq!(Decision::parse("prompt").unwrap(), Decision::Prompt);
        assert_eq!(Decision::parse("forbidden").unwrap(), Decision::Forbidden);
        assert!(Decision::parse("deny").is_err());
    }

    #[test]
    fn rule_matches_returns_rule_match() {
        let rule = PrefixRule {
            pattern: pattern("git", &["status"]),
            decision: Decision::Allow,
            justification: Some("safe".to_string()),
        };
        let m = rule.matches(&cmd(&["git", "status", "extra"])).unwrap();
        match m {
            RuleMatch::PrefixRuleMatch {
                matched_prefix,
                decision,
                justification,
                ..
            } => {
                assert_eq!(matched_prefix, cmd(&["git", "status"]));
                assert_eq!(decision, Decision::Allow);
                assert_eq!(justification, Some("safe".to_string()));
            }
            _ => panic!("expected PrefixRuleMatch"),
        }
    }

    #[test]
    fn network_rule_protocol_parse() {
        assert_eq!(
            NetworkRuleProtocol::parse("http").unwrap(),
            NetworkRuleProtocol::Http
        );
        assert_eq!(
            NetworkRuleProtocol::parse("https").unwrap(),
            NetworkRuleProtocol::Https
        );
        assert_eq!(
            NetworkRuleProtocol::parse("https_connect").unwrap(),
            NetworkRuleProtocol::Https
        );
        assert!(NetworkRuleProtocol::parse("ftp").is_err());
    }
}

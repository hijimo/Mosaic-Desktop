pub mod amend;
pub mod error;
pub mod network_rule;
pub mod parser;
pub mod prefix_rule;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use multimap::MultiMap;
use serde::{Deserialize, Serialize};

use crate::protocol::error::{CodexError, ErrorCode};

pub use error::{Error as ExecPolicyError, Result as ExecPolicyResult};
pub use network_rule::{normalize_network_rule_host, NetworkRule};
pub use parser::PolicyParser;
pub use prefix_rule::{
    Decision, NetworkRuleProtocol, PatternToken, PrefixPattern, PrefixRule, Rule, RuleMatch,
    RuleRef,
};

pub use amend::AmendError;

type HeuristicsFallback<'a> = Option<&'a dyn Fn(&[String]) -> Decision>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MatchOptions {
    pub resolve_host_executables: bool,
}

#[derive(Clone, Debug)]
pub struct Policy {
    rules_by_program: MultiMap<String, RuleRef>,
    network_rules: Vec<NetworkRule>,
    host_executables_by_name: HashMap<String, Arc<[std::path::PathBuf]>>,
}

impl Policy {
    pub fn new(rules_by_program: MultiMap<String, RuleRef>) -> Self {
        Self::from_parts(rules_by_program, Vec::new(), HashMap::new())
    }

    pub fn from_parts(
        rules_by_program: MultiMap<String, RuleRef>,
        network_rules: Vec<NetworkRule>,
        host_executables_by_name: HashMap<String, Arc<[std::path::PathBuf]>>,
    ) -> Self {
        Self {
            rules_by_program,
            network_rules,
            host_executables_by_name,
        }
    }

    pub fn empty() -> Self {
        Self::new(MultiMap::new())
    }

    pub fn rules(&self) -> &MultiMap<String, RuleRef> {
        &self.rules_by_program
    }

    pub fn network_rules(&self) -> &[NetworkRule] {
        &self.network_rules
    }

    pub fn host_executables(&self) -> &HashMap<String, Arc<[std::path::PathBuf]>> {
        &self.host_executables_by_name
    }

    pub fn get_allowed_prefixes(&self) -> Vec<Vec<String>> {
        let mut prefixes = Vec::new();
        for (_program, rules) in self.rules_by_program.iter_all() {
            for rule in rules {
                let Some(pr) = rule.as_any().downcast_ref::<PrefixRule>() else {
                    continue;
                };
                if pr.decision != Decision::Allow {
                    continue;
                }
                let mut prefix = Vec::with_capacity(pr.pattern.rest.len() + 1);
                prefix.push(pr.pattern.first.as_ref().to_string());
                prefix.extend(pr.pattern.rest.iter().map(render_pattern_token));
                prefixes.push(prefix);
            }
        }
        prefixes.sort();
        prefixes.dedup();
        prefixes
    }

    pub fn add_prefix_rule(
        &mut self,
        prefix: &[String],
        decision: Decision,
    ) -> ExecPolicyResult<()> {
        let (first, rest) = prefix
            .split_first()
            .ok_or_else(|| ExecPolicyError::InvalidPattern("prefix cannot be empty".to_string()))?;
        let rule: RuleRef = Arc::new(PrefixRule {
            pattern: PrefixPattern {
                first: Arc::from(first.as_str()),
                rest: rest
                    .iter()
                    .map(|t| PatternToken::Single(t.clone()))
                    .collect::<Vec<_>>()
                    .into(),
            },
            decision,
            justification: None,
        });
        self.rules_by_program.insert(first.clone(), rule);
        Ok(())
    }

    pub fn add_network_rule(
        &mut self,
        host: &str,
        protocol: NetworkRuleProtocol,
        decision: Decision,
        justification: Option<String>,
    ) -> ExecPolicyResult<()> {
        let host = normalize_network_rule_host(host)?;
        if let Some(raw) = justification.as_deref() {
            if raw.trim().is_empty() {
                return Err(ExecPolicyError::InvalidRule(
                    "justification cannot be empty".to_string(),
                ));
            }
        }
        self.network_rules.push(NetworkRule {
            host,
            protocol,
            decision,
            justification,
        });
        Ok(())
    }

    pub fn set_host_executable_paths(&mut self, name: String, paths: Vec<std::path::PathBuf>) {
        self.host_executables_by_name.insert(name, paths.into());
    }

    /// Merge overlay on top of self, returning a new Policy.
    pub fn merge_overlay(&self, overlay: &Policy) -> Policy {
        let mut combined_rules = self.rules_by_program.clone();
        for (program, rules) in overlay.rules_by_program.iter_all() {
            for rule in rules {
                combined_rules.insert(program.clone(), rule.clone());
            }
        }
        let mut combined_network = self.network_rules.clone();
        combined_network.extend(overlay.network_rules.iter().cloned());
        let mut combined_host = self.host_executables_by_name.clone();
        combined_host.extend(
            overlay
                .host_executables_by_name
                .iter()
                .map(|(n, p)| (n.clone(), p.clone())),
        );
        Policy::from_parts(combined_rules, combined_network, combined_host)
    }

    /// Returns (allowed_domains, denied_domains).
    pub fn compiled_network_domains(&self) -> (Vec<String>, Vec<String>) {
        let mut allowed = Vec::new();
        let mut denied = Vec::new();
        for rule in &self.network_rules {
            match rule.decision {
                Decision::Allow => {
                    denied.retain(|e: &String| e != &rule.host);
                    upsert_domain(&mut allowed, &rule.host);
                }
                Decision::Forbidden => {
                    allowed.retain(|e: &String| e != &rule.host);
                    upsert_domain(&mut denied, &rule.host);
                }
                Decision::Prompt => {}
            }
        }
        (allowed, denied)
    }

    pub fn check<F>(&self, cmd: &[String], heuristics_fallback: &F) -> Evaluation
    where
        F: Fn(&[String]) -> Decision,
    {
        let matched = self.matches_for_command_with_options(
            cmd,
            Some(heuristics_fallback),
            &MatchOptions::default(),
        );
        Evaluation::from_matches(matched)
    }

    pub fn check_with_options<F>(
        &self,
        cmd: &[String],
        heuristics_fallback: &F,
        options: &MatchOptions,
    ) -> Evaluation
    where
        F: Fn(&[String]) -> Decision,
    {
        let matched =
            self.matches_for_command_with_options(cmd, Some(heuristics_fallback), options);
        Evaluation::from_matches(matched)
    }

    /// Checks multiple commands and aggregates the results into a single Evaluation.
    pub fn check_multiple<Commands, F>(
        &self,
        commands: Commands,
        heuristics_fallback: &F,
    ) -> Evaluation
    where
        Commands: IntoIterator,
        Commands::Item: AsRef<[String]>,
        F: Fn(&[String]) -> Decision,
    {
        let matched: Vec<RuleMatch> = commands
            .into_iter()
            .flat_map(|cmd| {
                self.matches_for_command_with_options(
                    cmd.as_ref(),
                    Some(heuristics_fallback),
                    &MatchOptions::default(),
                )
            })
            .collect();
        Evaluation::from_matches(matched)
    }

    pub fn matches_for_command(
        &self,
        cmd: &[String],
        heuristics_fallback: HeuristicsFallback<'_>,
    ) -> Vec<RuleMatch> {
        self.matches_for_command_with_options(cmd, heuristics_fallback, &MatchOptions::default())
    }

    pub fn matches_for_command_with_options(
        &self,
        cmd: &[String],
        heuristics_fallback: HeuristicsFallback<'_>,
        options: &MatchOptions,
    ) -> Vec<RuleMatch> {
        let matched = self
            .match_exact_rules(cmd)
            .filter(|m| !m.is_empty())
            .or_else(|| {
                options
                    .resolve_host_executables
                    .then(|| self.match_host_executable_rules(cmd))
                    .filter(|m| !m.is_empty())
            })
            .unwrap_or_default();

        if matched.is_empty() {
            if let Some(fallback) = heuristics_fallback {
                return vec![RuleMatch::HeuristicsRuleMatch {
                    command: cmd.to_vec(),
                    decision: fallback(cmd),
                }];
            }
        }
        matched
    }

    fn match_exact_rules(&self, cmd: &[String]) -> Option<Vec<RuleMatch>> {
        let first = cmd.first()?;
        Some(
            self.rules_by_program
                .get_vec(first)
                .map(|rules| rules.iter().filter_map(|r| r.matches(cmd)).collect())
                .unwrap_or_default(),
        )
    }

    fn match_host_executable_rules(&self, cmd: &[String]) -> Vec<RuleMatch> {
        let first = match cmd.first() {
            Some(f) => f,
            None => return Vec::new(),
        };
        let program = std::path::Path::new(first);
        if !program.is_absolute() {
            return Vec::new();
        }
        let basename = match program.file_stem().and_then(|s| s.to_str()) {
            Some(b) => b.to_string(),
            None => return Vec::new(),
        };
        let rules = match self.rules_by_program.get_vec(&basename) {
            Some(r) => r,
            None => return Vec::new(),
        };
        if let Some(paths) = self.host_executables_by_name.get(&basename) {
            if !paths.iter().any(|p| p == program) {
                return Vec::new();
            }
        }
        let basename_cmd: Vec<String> = std::iter::once(basename)
            .chain(cmd.iter().skip(1).cloned())
            .collect();
        rules
            .iter()
            .filter_map(|r| r.matches(&basename_cmd))
            .map(|m| m.with_resolved_program(program))
            .collect()
    }

    pub fn load_from_file(path: &Path) -> Result<Self, CodexError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            CodexError::new(
                ErrorCode::ConfigurationError,
                format!("failed to read policy file: {e}"),
            )
        })?;
        let mut parser = PolicyParser::new();
        parser
            .parse(&path.display().to_string(), &content)
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::ConfigurationError,
                    format!("failed to parse policy file: {e}"),
                )
            })?;
        Ok(parser.build())
    }
}

fn upsert_domain(entries: &mut Vec<String>, host: &str) {
    entries.retain(|e| e != host);
    entries.push(host.to_string());
}

fn render_pattern_token(token: &PatternToken) -> String {
    match token {
        PatternToken::Single(v) => v.clone(),
        PatternToken::Alts(alts) => format!("[{}]", alts.join("|")),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub decision: Decision,
    #[serde(rename = "matchedRules")]
    pub matched_rules: Vec<RuleMatch>,
}

impl Evaluation {
    pub fn is_match(&self) -> bool {
        self.matched_rules
            .iter()
            .any(|m| !matches!(m, RuleMatch::HeuristicsRuleMatch { .. }))
    }

    fn from_matches(matched_rules: Vec<RuleMatch>) -> Self {
        let decision = matched_rules
            .iter()
            .map(RuleMatch::decision)
            .max()
            .unwrap_or(Decision::Prompt);
        Self {
            decision,
            matched_rules,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    fn heuristic(cmd: &[String]) -> Decision {
        if cmd.first().map(|s| s.as_str()) == Some("ls") {
            Decision::Allow
        } else {
            Decision::Prompt
        }
    }

    fn policy() -> Policy {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"
prefix_rule(["git", "status"], decision="allow")
prefix_rule(["npm", "install"], decision="prompt")
prefix_rule(["rm", "-rf", "/"], decision="forbidden")
network_rule(host="api.github.com", protocol="https", decision="allow")
network_rule(host="evil.com", protocol="http", decision="deny")
"#,
            )
            .unwrap();
        parser.build()
    }

    #[test]
    fn allow_matching_command() {
        let p = policy();
        let eval = p.check(&cmd(&["git", "status"]), &heuristic);
        assert_eq!(eval.decision, Decision::Allow);
        assert!(eval.is_match());
    }

    #[test]
    fn prompt_matching_command() {
        let p = policy();
        let eval = p.check(&cmd(&["npm", "install", "lodash"]), &heuristic);
        assert_eq!(eval.decision, Decision::Prompt);
    }

    #[test]
    fn forbidden_matching_command() {
        let p = policy();
        let eval = p.check(&cmd(&["rm", "-rf", "/"]), &heuristic);
        assert_eq!(eval.decision, Decision::Forbidden);
    }

    #[test]
    fn heuristics_fallback_for_unknown() {
        let p = policy();
        let eval = p.check(&cmd(&["ls", "-la"]), &heuristic);
        assert_eq!(eval.decision, Decision::Allow);
        assert!(!eval.is_match()); // heuristics match, not rule match
    }

    #[test]
    fn heuristics_fallback_prompt() {
        let p = policy();
        let eval = p.check(&cmd(&["curl", "https://example.com"]), &heuristic);
        assert_eq!(eval.decision, Decision::Prompt);
    }

    #[test]
    fn check_multiple_aggregates() {
        let p = policy();
        let commands = vec![cmd(&["git", "status"]), cmd(&["rm", "-rf", "/"])];
        let eval = p.check_multiple(commands, &heuristic);
        // Forbidden > Allow, so aggregated decision is Forbidden
        assert_eq!(eval.decision, Decision::Forbidden);
    }

    #[test]
    fn get_allowed_prefixes() {
        let p = policy();
        let prefixes = p.get_allowed_prefixes();
        assert!(!prefixes.is_empty());
        assert!(prefixes.iter().any(|p| p[0] == "git"));
    }

    #[test]
    fn merge_overlay() {
        let p1 = Policy::empty();
        let mut p2 = Policy::empty();
        p2.add_prefix_rule(&["test".to_string()], Decision::Allow)
            .unwrap();
        let merged = p1.merge_overlay(&p2);
        let eval = merged.check(&cmd(&["test", "arg"]), &heuristic);
        assert_eq!(eval.decision, Decision::Allow);
    }

    #[test]
    fn compiled_network_domains() {
        let p = policy();
        let (allowed, denied) = p.compiled_network_domains();
        assert!(allowed.contains(&"api.github.com".to_string()));
        assert!(denied.contains(&"evil.com".to_string()));
    }

    #[test]
    fn add_network_rule() {
        let mut p = Policy::empty();
        p.add_network_rule(
            "example.com",
            NetworkRuleProtocol::Https,
            Decision::Allow,
            None,
        )
        .unwrap();
        assert_eq!(p.network_rules().len(), 1);
    }

    #[test]
    fn evaluation_serde_roundtrip() {
        let eval = Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["git".to_string(), "status".to_string()],
                decision: Decision::Allow,
                resolved_program: None,
                justification: Some("safe".to_string()),
            }],
        };
        let json = serde_json::to_string(&eval).unwrap();
        let decoded: Evaluation = serde_json::from_str(&json).unwrap();
        assert_eq!(eval, decoded);
    }
}

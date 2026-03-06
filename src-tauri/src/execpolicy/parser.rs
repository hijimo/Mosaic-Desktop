use multimap::MultiMap;
use starlark::any::ProvidesStaticType;
use starlark::codemap::FileSpan;
use starlark::environment::GlobalsBuilder;
use starlark::environment::Module;
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::syntax::AstModule;
use starlark::syntax::Dialect;
use starlark::values::Value;
use starlark::values::list::ListRef;
use starlark::values::list::UnpackList;
use starlark::values::none::NoneType;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::error::{Error, ErrorLocation, Result, TextPosition, TextRange};
use super::network_rule::NetworkRule;
use super::prefix_rule::{
    Decision, NetworkRuleProtocol, PatternToken, PrefixPattern, PrefixRule, RuleRef,
    validate_match_examples, validate_not_match_examples,
};

pub struct PolicyParser {
    builder: RefCell<PolicyBuilder>,
}

impl Default for PolicyParser {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyParser {
    pub fn new() -> Self {
        Self {
            builder: RefCell::new(PolicyBuilder::new()),
        }
    }

    pub fn parse(&mut self, policy_identifier: &str, policy_file_contents: &str) -> Result<()> {
        let pending_count = self.builder.borrow().pending_example_validations.len();
        let mut dialect = Dialect::Extended.clone();
        dialect.enable_f_strings = true;
        let ast = AstModule::parse(
            policy_identifier,
            policy_file_contents.to_string(),
            &dialect,
        )
        .map_err(Error::Starlark)?;
        let globals = GlobalsBuilder::standard().with(policy_builtins).build();
        let module = Module::new();
        {
            let mut eval = Evaluator::new(&module);
            eval.extra = Some(&self.builder);
            eval.eval_module(ast, &globals).map_err(Error::Starlark)?;
        }
        self.builder
            .borrow()
            .validate_pending_examples_from(pending_count)?;
        Ok(())
    }

    pub fn build(self) -> super::Policy {
        self.builder.into_inner().build()
    }
}

#[derive(Debug, ProvidesStaticType)]
pub struct PolicyBuilder {
    rules_by_program: MultiMap<String, RuleRef>,
    network_rules: Vec<NetworkRule>,
    host_executables_by_name: HashMap<String, Arc<[std::path::PathBuf]>>,
    pending_example_validations: Vec<PendingExampleValidation>,
}

impl PolicyBuilder {
    fn new() -> Self {
        Self {
            rules_by_program: MultiMap::new(),
            network_rules: Vec::new(),
            host_executables_by_name: HashMap::new(),
            pending_example_validations: Vec::new(),
        }
    }

    fn add_rule(&mut self, rule: RuleRef) {
        self.rules_by_program
            .insert(rule.program().to_string(), rule);
    }

    fn add_network_rule(&mut self, rule: NetworkRule) {
        self.network_rules.push(rule);
    }

    fn add_host_executable(&mut self, name: String, paths: Vec<std::path::PathBuf>) {
        self.host_executables_by_name.insert(name, paths.into());
    }

    fn validate_pending_examples_from(&self, start: usize) -> Result<()> {
        for validation in &self.pending_example_validations[start..] {
            let mut rules_by_program = MultiMap::new();
            for rule in &validation.rules {
                rules_by_program.insert(rule.program().to_string(), rule.clone());
            }
            let policy = super::Policy::from_parts(
                rules_by_program,
                Vec::new(),
                self.host_executables_by_name.clone(),
            );
            validate_not_match_examples(&policy, &validation.rules, &validation.not_matches)
                .map_err(|e| attach_location(e, validation.location.clone()))?;
            validate_match_examples(&policy, &validation.rules, &validation.matches)
                .map_err(|e| attach_location(e, validation.location.clone()))?;
        }
        Ok(())
    }

    fn build(self) -> super::Policy {
        super::Policy::from_parts(
            self.rules_by_program,
            self.network_rules,
            self.host_executables_by_name,
        )
    }
}

#[derive(Debug)]
struct PendingExampleValidation {
    rules: Vec<RuleRef>,
    matches: Vec<Vec<String>>,
    not_matches: Vec<Vec<String>>,
    location: Option<ErrorLocation>,
}

fn parse_pattern<'v>(pattern: UnpackList<Value<'v>>) -> Result<Vec<PatternToken>> {
    let tokens: Vec<PatternToken> = pattern
        .items
        .into_iter()
        .map(parse_pattern_token)
        .collect::<Result<_>>()?;
    if tokens.is_empty() {
        Err(Error::InvalidPattern("pattern cannot be empty".to_string()))
    } else {
        Ok(tokens)
    }
}

fn parse_pattern_token<'v>(value: Value<'v>) -> Result<PatternToken> {
    if let Some(s) = value.unpack_str() {
        Ok(PatternToken::Single(s.to_string()))
    } else if let Some(list) = ListRef::from_value(value) {
        let tokens: Vec<String> = list
            .content()
            .iter()
            .map(|v| {
                v.unpack_str()
                    .ok_or_else(|| {
                        Error::InvalidPattern(format!(
                            "pattern alternative must be a string (got {})",
                            v.get_type()
                        ))
                    })
                    .map(str::to_string)
            })
            .collect::<Result<_>>()?;
        match tokens.as_slice() {
            [] => Err(Error::InvalidPattern(
                "pattern alternatives cannot be empty".to_string(),
            )),
            [single] => Ok(PatternToken::Single(single.clone())),
            _ => Ok(PatternToken::Alts(tokens)),
        }
    } else {
        Err(Error::InvalidPattern(format!(
            "pattern element must be a string or list of strings (got {})",
            value.get_type()
        )))
    }
}

fn parse_examples<'v>(examples: UnpackList<Value<'v>>) -> Result<Vec<Vec<String>>> {
    examples.items.into_iter().map(parse_example).collect()
}

fn parse_example<'v>(value: Value<'v>) -> Result<Vec<String>> {
    if let Some(raw) = value.unpack_str() {
        let tokens = shlex::split(raw).ok_or_else(|| {
            Error::InvalidExample("example string has invalid shell syntax".to_string())
        })?;
        if tokens.is_empty() {
            Err(Error::InvalidExample(
                "example cannot be an empty string".to_string(),
            ))
        } else {
            Ok(tokens)
        }
    } else if let Some(list) = ListRef::from_value(value) {
        let tokens: Vec<String> = list
            .content()
            .iter()
            .map(|v| {
                v.unpack_str()
                    .ok_or_else(|| {
                        Error::InvalidExample(format!(
                            "example tokens must be strings (got {})",
                            v.get_type()
                        ))
                    })
                    .map(str::to_string)
            })
            .collect::<Result<_>>()?;
        if tokens.is_empty() {
            Err(Error::InvalidExample(
                "example cannot be an empty list".to_string(),
            ))
        } else {
            Ok(tokens)
        }
    } else {
        Err(Error::InvalidExample(format!(
            "example must be a string or list of strings (got {})",
            value.get_type()
        )))
    }
}

fn parse_network_rule_decision(raw: &str) -> Result<Decision> {
    match raw {
        "deny" => Ok(Decision::Forbidden),
        other => Decision::parse(other),
    }
}

fn error_location_from_file_span(span: FileSpan) -> ErrorLocation {
    let resolved = span.resolve_span();
    ErrorLocation {
        path: span.filename().to_string(),
        range: TextRange {
            start: TextPosition {
                line: resolved.begin.line + 1,
                column: resolved.begin.column + 1,
            },
            end: TextPosition {
                line: resolved.end.line + 1,
                column: resolved.end.column + 1,
            },
        },
    }
}

fn attach_location(error: Error, location: Option<ErrorLocation>) -> Error {
    match location {
        Some(loc) => error.with_location(loc),
        None => error,
    }
}

fn executable_lookup_key(raw: &str) -> String {
    #[cfg(windows)]
    {
        let raw = raw.to_ascii_lowercase();
        for suffix in [".exe", ".cmd", ".bat", ".com"] {
            if raw.ends_with(suffix) {
                return raw[..raw.len() - suffix.len()].to_string();
            }
        }
        raw
    }
    #[cfg(not(windows))]
    {
        raw.to_string()
    }
}

fn policy_builder<'v, 'a>(eval: &Evaluator<'v, 'a>) -> RefMut<'a, PolicyBuilder> {
    #[expect(clippy::expect_used)]
    eval.extra
        .as_ref()
        .expect("policy_builder requires Evaluator.extra")
        .downcast_ref::<RefCell<PolicyBuilder>>()
        .expect("Evaluator.extra must contain a PolicyBuilder")
        .borrow_mut()
}

#[starlark_module]
fn policy_builtins(builder: &mut GlobalsBuilder) {
    fn prefix_rule<'v>(
        pattern: UnpackList<Value<'v>>,
        decision: Option<&'v str>,
        r#match: Option<UnpackList<Value<'v>>>,
        not_match: Option<UnpackList<Value<'v>>>,
        justification: Option<&'v str>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<NoneType> {
        let decision = match decision {
            Some(raw) => Decision::parse(raw)?,
            None => Decision::Allow,
        };
        let justification = match justification {
            Some(raw) if raw.trim().is_empty() => {
                return Err(Error::InvalidRule("justification cannot be empty".to_string()).into());
            }
            Some(raw) => Some(raw.to_string()),
            None => None,
        };
        let pattern_tokens = parse_pattern(pattern)?;
        let matches = r#match.map(parse_examples).transpose()?.unwrap_or_default();
        let not_matches = not_match.map(parse_examples).transpose()?.unwrap_or_default();
        let location = eval
            .call_stack_top_location()
            .map(error_location_from_file_span);

        let mut pb = policy_builder(eval);
        let (first_token, remaining) = pattern_tokens
            .split_first()
            .ok_or_else(|| Error::InvalidPattern("pattern cannot be empty".to_string()))?;
        let rest: Arc<[PatternToken]> = remaining.to_vec().into();

        let rules: Vec<RuleRef> = first_token
            .alternatives()
            .iter()
            .map(|head| {
                Arc::new(PrefixRule {
                    pattern: PrefixPattern {
                        first: Arc::from(head.as_str()),
                        rest: rest.clone(),
                    },
                    decision,
                    justification: justification.clone(),
                }) as RuleRef
            })
            .collect();

        pb.pending_example_validations
            .push(PendingExampleValidation {
                rules: rules.clone(),
                matches,
                not_matches,
                location,
            });
        rules.into_iter().for_each(|rule| pb.add_rule(rule));
        Ok(NoneType)
    }

    fn network_rule<'v>(
        host: &'v str,
        protocol: &'v str,
        decision: &'v str,
        justification: Option<&'v str>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<NoneType> {
        let protocol = NetworkRuleProtocol::parse(protocol)?;
        let decision = parse_network_rule_decision(decision)?;
        let justification = match justification {
            Some(raw) if raw.trim().is_empty() => {
                return Err(Error::InvalidRule("justification cannot be empty".to_string()).into());
            }
            Some(raw) => Some(raw.to_string()),
            None => None,
        };
        let mut pb = policy_builder(eval);
        pb.add_network_rule(NetworkRule {
            host: super::network_rule::normalize_network_rule_host(host)?,
            protocol,
            decision,
            justification,
        });
        Ok(NoneType)
    }

    fn host_executable<'v>(
        name: &'v str,
        paths: UnpackList<Value<'v>>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<NoneType> {
        if name.is_empty() {
            return Err(Error::InvalidRule("host_executable name cannot be empty".to_string()).into());
        }
        let path = Path::new(name);
        if path.components().count() != 1
            || path.file_name().and_then(|v| v.to_str()) != Some(name)
        {
            return Err(Error::InvalidRule(format!(
                "host_executable name must be a bare executable name (got {name})"
            ))
            .into());
        }

        let mut parsed_paths = Vec::new();
        for value in paths.items {
            let raw = value.unpack_str().ok_or_else(|| {
                Error::InvalidRule(format!(
                    "host_executable paths must be strings (got {})",
                    value.get_type()
                ))
            })?;
            if !Path::new(raw).is_absolute() {
                return Err(Error::InvalidRule(format!(
                    "host_executable paths must be absolute (got {raw})"
                ))
                .into());
            }
            let p = std::path::PathBuf::from(raw);
            if !parsed_paths.contains(&p) {
                parsed_paths.push(p);
            }
        }

        policy_builder(eval).add_host_executable(executable_lookup_key(name), parsed_paths);
        Ok(NoneType)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefix_rule_via_starlark() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"prefix_rule(["git", "status"], decision="allow")"#,
            )
            .unwrap();
        let policy = parser.build();
        assert!(!policy.rules().get_vec("git").unwrap().is_empty());
    }

    #[test]
    fn parse_alts_pattern() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"prefix_rule(["git", ["push", "pull"]], decision="allow")"#,
            )
            .unwrap();
        let policy = parser.build();
        let rules = policy.rules().get_vec("git").unwrap();
        assert_eq!(rules.len(), 1);
        let cmd = vec!["git".to_string(), "push".to_string()];
        assert!(rules[0].matches(&cmd).is_some());
        let cmd2 = vec!["git".to_string(), "status".to_string()];
        assert!(rules[0].matches(&cmd2).is_none());
    }

    #[test]
    fn parse_network_rule_via_starlark() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"network_rule(host="api.github.com", protocol="https", decision="allow")"#,
            )
            .unwrap();
        let policy = parser.build();
        assert_eq!(policy.network_rules().len(), 1);
        assert_eq!(policy.network_rules()[0].host, "api.github.com");
    }

    #[test]
    fn parse_multiple_rules() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"
prefix_rule(["git", "status"], decision="allow")
prefix_rule(["npm", "install"], decision="prompt")
network_rule(host="example.com", protocol="https", decision="deny")
"#,
            )
            .unwrap();
        let policy = parser.build();
        assert!(!policy.rules().get_vec("git").unwrap().is_empty());
        assert!(!policy.rules().get_vec("npm").unwrap().is_empty());
        assert_eq!(policy.network_rules().len(), 1);
    }

    #[test]
    fn parse_with_justification() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"prefix_rule(["rm"], decision="forbidden", justification="dangerous")"#,
            )
            .unwrap();
        let policy = parser.build();
        let rules = policy.rules().get_vec("rm").unwrap();
        let cmd = vec!["rm".to_string(), "-rf".to_string()];
        let m = rules[0].matches(&cmd).unwrap();
        match m {
            super::super::prefix_rule::RuleMatch::PrefixRuleMatch {
                justification, ..
            } => assert_eq!(justification, Some("dangerous".to_string())),
            _ => panic!("expected PrefixRuleMatch"),
        }
    }

    #[test]
    fn parse_with_match_examples() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"prefix_rule(["git", "status"], decision="allow", match=["git status"])"#,
            )
            .unwrap();
    }

    #[test]
    fn parse_with_not_match_examples() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"prefix_rule(["git", "status"], decision="allow", not_match=["npm install"])"#,
            )
            .unwrap();
    }

    #[test]
    fn parse_comments_and_blank_lines() {
        let mut parser = PolicyParser::new();
        parser
            .parse(
                "test",
                r#"
# This is a comment
prefix_rule(["ls"], decision="allow")

# Another comment
"#,
            )
            .unwrap();
        let policy = parser.build();
        assert!(!policy.rules().get_vec("ls").unwrap().is_empty());
    }
}

//! ExecPolicyManager — the core execution policy engine.
//!
//! Loads `.rules` files, evaluates commands against policy + heuristics,
//! and produces approval requirements. Supports runtime amendment.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;
use tokio::sync::RwLock;
use thiserror::Error;
use tracing::debug;

use crate::execpolicy::{
    AmendError, Decision, Evaluation, MatchOptions, NetworkRuleProtocol, Policy, PolicyParser,
    RuleMatch,
};
use crate::execpolicy::amend::{blocking_append_allow_prefix_rule, blocking_append_network_rule};
use crate::protocol::types::{AskForApproval, ExecPolicyAmendment, SandboxPolicy};

use super::bash::{parse_shell_lc_plain_commands, parse_shell_lc_single_command_prefix};
use super::heuristics::render_decision_for_unmatched_command;

const RULES_DIR_NAME: &str = "rules";
const DEFAULT_POLICY_FILE: &str = "default.rules";
const RULE_EXTENSION: &str = "rules";

const PROMPT_CONFLICT_REASON: &str =
    "approval required by policy, but AskForApproval is set to Never";
const REJECT_SANDBOX_REASON: &str =
    "approval required by policy, but AskForApproval::Reject.sandbox_approval is set";
const REJECT_RULES_REASON: &str =
    "approval required by policy rule, but AskForApproval::Reject.rules is set";

/// Banned prefix suggestions — too broad to be useful as auto-amendments.
static BANNED_PREFIX_SUGGESTIONS: &[&[&str]] = &[
    &["python3"], &["python3", "-c"], &["python"], &["python", "-c"],
    &["bash"], &["bash", "-lc"], &["sh"], &["sh", "-c"],
    &["zsh"], &["zsh", "-lc"], &["/bin/bash"], &["/bin/bash", "-lc"],
    &["/bin/zsh"], &["/bin/zsh", "-lc"],
    &["sudo"], &["git"], &["node"], &["node", "-e"],
    &["env"], &["ruby"], &["ruby", "-e"], &["perl"], &["perl", "-e"],
];

// ── Error types ──────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ExecPolicyError {
    #[error("failed to read rules dir {dir}: {source}")]
    ReadDir { dir: PathBuf, source: std::io::Error },
    #[error("failed to read rules file {path}: {source}")]
    ReadFile { path: PathBuf, source: std::io::Error },
    #[error("failed to parse rules file {path}: {source}")]
    ParsePolicy { path: String, source: crate::execpolicy::ExecPolicyError },
}

#[derive(Debug, Error)]
pub enum ExecPolicyUpdateError {
    #[error("failed to update rules file {path}: {source}")]
    AppendRule { path: PathBuf, source: AmendError },
    #[error("failed to join blocking task: {source}")]
    JoinBlockingTask { source: tokio::task::JoinError },
    #[error("failed to update in-memory rules: {source}")]
    AddRule { #[from] source: crate::execpolicy::ExecPolicyError },
}

// ── Approval requirement ─────────────────────────────────────────

/// The result of evaluating a command against the execution policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecApprovalRequirement {
    /// Command is allowed; optionally bypass sandbox.
    Skip {
        bypass_sandbox: bool,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    /// Command needs user approval.
    NeedsApproval {
        reason: Option<String>,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    /// Command is forbidden.
    Forbidden { reason: String },
}

// ── Manager ──────────────────────────────────────────────────────

pub struct ExecPolicyManager {
    policy: RwLock<Arc<Policy>>,
}

impl Default for ExecPolicyManager {
    fn default() -> Self {
        Self { policy: RwLock::new(Arc::new(Policy::empty())) }
    }
}

impl ExecPolicyManager {
    pub fn new(policy: Arc<Policy>) -> Self {
        Self { policy: RwLock::new(policy) }
    }

    /// Load policy from `.rules` files under `mosaic_home/rules/`.
    pub async fn load(mosaic_home: &Path) -> Result<Self, ExecPolicyError> {
        let policy = load_exec_policy(mosaic_home).await?;
        Ok(Self::new(Arc::new(policy)))
    }

    pub async fn current(&self) -> Arc<Policy> {
        self.policy.read().await.clone()
    }

    /// Evaluate a command and produce an approval requirement.
    pub async fn evaluate_command(
        &self,
        command: &[String],
        approval_policy: AskForApproval,
        sandbox_policy: &SandboxPolicy,
        prefix_rule: Option<Vec<String>>,
    ) -> ExecApprovalRequirement {
        let exec_policy = self.current().await;
        let (commands, used_complex_parsing) = commands_for_exec_policy(command);
        let auto_amendment_allowed = !used_complex_parsing;

        let fallback = |cmd: &[String]| {
            render_decision_for_unmatched_command(
                approval_policy.clone(),
                sandbox_policy,
                cmd,
                used_complex_parsing,
            )
        };
        let opts = MatchOptions { resolve_host_executables: true };
        let evaluation = exec_policy.check_multiple_with_options(
            commands.iter(), &fallback, &opts,
        );

        let requested_amendment = derive_requested_amendment(
            prefix_rule.as_ref(), &evaluation.matched_rules,
            &exec_policy, &commands, &fallback, &opts,
        );

        match evaluation.decision {
            Decision::Forbidden => ExecApprovalRequirement::Forbidden {
                reason: derive_forbidden_reason(command, &evaluation),
            },
            Decision::Prompt => {
                let prompt_is_rule = evaluation.matched_rules.iter().any(|m| {
                    is_policy_match(m) && m.decision() == Decision::Prompt
                });
                match prompt_is_rejected(approval_policy, prompt_is_rule) {
                    Some(reason) => ExecApprovalRequirement::Forbidden {
                        reason: reason.to_string(),
                    },
                    None => ExecApprovalRequirement::NeedsApproval {
                        reason: derive_prompt_reason(command, &evaluation),
                        proposed_execpolicy_amendment: requested_amendment.or_else(|| {
                            if auto_amendment_allowed {
                                try_derive_amendment_for_prompt(&evaluation.matched_rules)
                            } else {
                                None
                            }
                        }),
                    },
                }
            }
            Decision::Allow => ExecApprovalRequirement::Skip {
                bypass_sandbox: evaluation.matched_rules.iter().any(|m| {
                    is_policy_match(m) && m.decision() == Decision::Allow
                }),
                proposed_execpolicy_amendment: if auto_amendment_allowed {
                    try_derive_amendment_for_allow(&evaluation.matched_rules)
                } else {
                    None
                },
            },
        }
    }

    /// Append an allow prefix rule to disk and update in-memory policy.
    pub async fn append_amendment(
        &self,
        mosaic_home: &Path,
        amendment: &ExecPolicyAmendment,
    ) -> Result<(), ExecPolicyUpdateError> {
        let policy_path = default_policy_path(mosaic_home);
        let prefix = amendment.command.clone();
        tokio::task::spawn_blocking({
            let policy_path = policy_path.clone();
            let prefix = prefix.clone();
            move || blocking_append_allow_prefix_rule(&policy_path, &prefix)
        })
        .await
        .map_err(|source| ExecPolicyUpdateError::JoinBlockingTask { source })?
        .map_err(|source| ExecPolicyUpdateError::AppendRule { path: policy_path, source })?;

        let mut guard = self.policy.write().await;
        let mut updated = guard.as_ref().clone();
        updated.add_prefix_rule(&prefix, Decision::Allow)?;
        *guard = Arc::new(updated);
        Ok(())
    }

    /// Append a network rule to disk and update in-memory policy.
    pub async fn append_network_rule(
        &self,
        mosaic_home: &Path,
        host: &str,
        protocol: NetworkRuleProtocol,
        decision: Decision,
        justification: Option<String>,
    ) -> Result<(), ExecPolicyUpdateError> {
        let policy_path = default_policy_path(mosaic_home);
        let host = host.to_string();
        tokio::task::spawn_blocking({
            let policy_path = policy_path.clone();
            let host = host.clone();
            let justification = justification.clone();
            move || blocking_append_network_rule(
                &policy_path, &host, protocol, decision, justification.as_deref(),
            )
        })
        .await
        .map_err(|source| ExecPolicyUpdateError::JoinBlockingTask { source })?
        .map_err(|source| ExecPolicyUpdateError::AppendRule { path: policy_path, source })?;

        let mut guard = self.policy.write().await;
        let mut updated = guard.as_ref().clone();
        updated.add_network_rule(&host, protocol, decision, justification)?;
        *guard = Arc::new(updated);
        Ok(())
    }
}

// ── Policy loading ───────────────────────────────────────────────

/// Load execution policy from `.rules` files under `mosaic_home/rules/`.
pub async fn load_exec_policy(mosaic_home: &Path) -> Result<Policy, ExecPolicyError> {
    let policy_dir = mosaic_home.join(RULES_DIR_NAME);
    let policy_paths = collect_policy_files(&policy_dir).await?;

    let mut parser = PolicyParser::new();
    for path in &policy_paths {
        let contents = fs::read_to_string(path).await.map_err(|source| {
            ExecPolicyError::ReadFile { path: path.clone(), source }
        })?;
        let id = path.to_string_lossy().to_string();
        parser.parse(&id, &contents).map_err(|source| {
            ExecPolicyError::ParsePolicy { path: id, source }
        })?;
    }

    debug!("loaded rules from {} files", policy_paths.len());
    Ok(parser.build())
}

/// Collect `.rules` files from a directory, sorted by name.
pub async fn collect_policy_files(dir: &Path) -> Result<Vec<PathBuf>, ExecPolicyError> {
    let mut read_dir = match fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ExecPolicyError::ReadDir { dir: dir.to_path_buf(), source }),
    };

    let mut paths = Vec::new();
    while let Some(entry) = read_dir.next_entry().await.map_err(|source| {
        ExecPolicyError::ReadDir { dir: dir.to_path_buf(), source }
    })? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some(RULE_EXTENSION)
            && entry.file_type().await.map(|ft| ft.is_file()).unwrap_or(false)
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn default_policy_path(mosaic_home: &Path) -> PathBuf {
    mosaic_home.join(RULES_DIR_NAME).join(DEFAULT_POLICY_FILE)
}

// ── Command parsing ──────────────────────────────────────────────

fn commands_for_exec_policy(command: &[String]) -> (Vec<Vec<String>>, bool) {
    if let Some(commands) = parse_shell_lc_plain_commands(command) {
        if !commands.is_empty() {
            return (commands, false);
        }
    }
    if let Some(single) = parse_shell_lc_single_command_prefix(command) {
        return (vec![single], true);
    }
    (vec![command.to_vec()], false)
}

// ── Amendment derivation ─────────────────────────────────────────

fn is_policy_match(m: &RuleMatch) -> bool {
    matches!(m, RuleMatch::PrefixRuleMatch { .. })
}

fn try_derive_amendment_for_prompt(matched: &[RuleMatch]) -> Option<ExecPolicyAmendment> {
    if matched.iter().any(|m| is_policy_match(m) && m.decision() == Decision::Prompt) {
        return None;
    }
    matched.iter().find_map(|m| match m {
        RuleMatch::HeuristicsRuleMatch { command, decision: Decision::Prompt } => {
            Some(ExecPolicyAmendment { command: command.clone() })
        }
        _ => None,
    })
}

fn try_derive_amendment_for_allow(matched: &[RuleMatch]) -> Option<ExecPolicyAmendment> {
    if matched.iter().any(is_policy_match) {
        return None;
    }
    matched.iter().find_map(|m| match m {
        RuleMatch::HeuristicsRuleMatch { command, decision: Decision::Allow } => {
            Some(ExecPolicyAmendment { command: command.clone() })
        }
        _ => None,
    })
}

fn derive_requested_amendment(
    prefix_rule: Option<&Vec<String>>,
    matched_rules: &[RuleMatch],
    exec_policy: &Policy,
    commands: &[Vec<String>],
    fallback: &impl Fn(&[String]) -> Decision,
    opts: &MatchOptions,
) -> Option<ExecPolicyAmendment> {
    let prefix = prefix_rule?;
    if prefix.is_empty() {
        return None;
    }
    if BANNED_PREFIX_SUGGESTIONS.iter().any(|banned| {
        prefix.len() == banned.len()
            && prefix.iter().map(String::as_str).eq(banned.iter().copied())
    }) {
        return None;
    }
    if matched_rules.iter().any(is_policy_match) {
        return None;
    }
    let amendment = ExecPolicyAmendment { command: prefix.clone() };
    if prefix_rule_would_approve_all(exec_policy, &amendment.command, commands, fallback, opts) {
        Some(amendment)
    } else {
        None
    }
}

fn prefix_rule_would_approve_all(
    exec_policy: &Policy,
    prefix: &[String],
    commands: &[Vec<String>],
    fallback: &impl Fn(&[String]) -> Decision,
    opts: &MatchOptions,
) -> bool {
    let mut policy = exec_policy.clone();
    if policy.add_prefix_rule(prefix, Decision::Allow).is_err() {
        return false;
    }
    commands.iter().all(|cmd| {
        policy.check_with_options(cmd, fallback, opts).decision == Decision::Allow
    })
}

// ── Reason derivation ────────────────────────────────────────────

fn prompt_is_rejected(approval_policy: AskForApproval, prompt_is_rule: bool) -> Option<&'static str> {
    match approval_policy {
        AskForApproval::Never => Some(PROMPT_CONFLICT_REASON),
        AskForApproval::Reject(rc) => {
            if prompt_is_rule {
                rc.rejects_rules_approval().then_some(REJECT_RULES_REASON)
            } else {
                rc.rejects_sandbox_approval().then_some(REJECT_SANDBOX_REASON)
            }
        }
        _ => None,
    }
}

fn derive_prompt_reason(command: &[String], evaluation: &Evaluation) -> Option<String> {
    let cmd_str = render_shlex(command);
    evaluation.matched_rules.iter()
        .filter_map(|m| match m {
            RuleMatch::PrefixRuleMatch { matched_prefix, decision: Decision::Prompt, justification, .. } => {
                Some((matched_prefix.len(), justification.as_deref()))
            }
            _ => None,
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, just)| match just {
            Some(j) => format!("`{cmd_str}` requires approval: {j}"),
            None => format!("`{cmd_str}` requires approval by policy"),
        })
}

fn derive_forbidden_reason(command: &[String], evaluation: &Evaluation) -> String {
    let cmd_str = render_shlex(command);
    evaluation.matched_rules.iter()
        .filter_map(|m| match m {
            RuleMatch::PrefixRuleMatch { matched_prefix, decision: Decision::Forbidden, justification, .. } => {
                Some((matched_prefix, justification.as_deref()))
            }
            _ => None,
        })
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(prefix, just)| match just {
            Some(j) => format!("`{cmd_str}` rejected: {j}"),
            None => {
                let p = render_shlex(prefix);
                format!("`{cmd_str}` rejected: policy forbids commands starting with `{p}`")
            }
        })
        .unwrap_or_else(|| format!("`{cmd_str}` rejected: blocked by policy"))
}

fn render_shlex(args: &[String]) -> String {
    shlex::try_join(args.iter().map(String::as_str)).unwrap_or_else(|_| args.join(" "))
}

// ── Extension trait for Policy ───────────────────────────────────

trait PolicyExt {
    fn check_multiple_with_options<'a, I, F>(
        &self, commands: I, fallback: &F, opts: &MatchOptions,
    ) -> Evaluation
    where
        I: IntoIterator<Item = &'a Vec<String>>,
        F: Fn(&[String]) -> Decision;
}

impl PolicyExt for Policy {
    fn check_multiple_with_options<'a, I, F>(
        &self, commands: I, fallback: &F, opts: &MatchOptions,
    ) -> Evaluation
    where
        I: IntoIterator<Item = &'a Vec<String>>,
        F: Fn(&[String]) -> Decision,
    {
        let matched: Vec<RuleMatch> = commands
            .into_iter()
            .flat_map(|cmd| self.matches_for_command_with_options(cmd, Some(fallback), opts))
            .collect();
        let decision = matched.iter().map(RuleMatch::decision).max().unwrap_or(Decision::Prompt);
        Evaluation { decision, matched_rules: matched }
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

    #[tokio::test]
    async fn empty_policy_uses_heuristics() {
        let mgr = ExecPolicyManager::default();
        let req = mgr.evaluate_command(
            &cmd(&["ls", "-la"]),
            AskForApproval::OnRequest,
            &SandboxPolicy::new_read_only_policy(),
            None,
        ).await;
        assert!(matches!(req, ExecApprovalRequirement::Skip { .. }));
    }

    #[tokio::test]
    async fn forbidden_command_by_policy() {
        let mut parser = PolicyParser::new();
        parser.parse("test", r#"prefix_rule(pattern=["rm"], decision="forbidden")"#).unwrap();
        let mgr = ExecPolicyManager::new(Arc::new(parser.build()));

        let req = mgr.evaluate_command(
            &cmd(&["rm", "-rf", "/"]),
            AskForApproval::OnRequest,
            &SandboxPolicy::DangerFullAccess,
            None,
        ).await;
        assert!(matches!(req, ExecApprovalRequirement::Forbidden { .. }));
    }

    #[tokio::test]
    async fn prompt_command_by_policy() {
        let mut parser = PolicyParser::new();
        parser.parse("test", r#"prefix_rule(pattern=["npm"], decision="prompt")"#).unwrap();
        let mgr = ExecPolicyManager::new(Arc::new(parser.build()));

        let req = mgr.evaluate_command(
            &cmd(&["npm", "install"]),
            AskForApproval::OnRequest,
            &SandboxPolicy::DangerFullAccess,
            None,
        ).await;
        assert!(matches!(req, ExecApprovalRequirement::NeedsApproval { .. }));
    }

    #[tokio::test]
    async fn prompt_rejected_by_never_policy() {
        let mut parser = PolicyParser::new();
        parser.parse("test", r#"prefix_rule(pattern=["rm"], decision="prompt")"#).unwrap();
        let mgr = ExecPolicyManager::new(Arc::new(parser.build()));

        let req = mgr.evaluate_command(
            &cmd(&["rm"]),
            AskForApproval::Never,
            &SandboxPolicy::DangerFullAccess,
            None,
        ).await;
        match req {
            ExecApprovalRequirement::Forbidden { reason } => {
                assert_eq!(reason, PROMPT_CONFLICT_REASON);
            }
            _ => panic!("expected Forbidden"),
        }
    }

    #[tokio::test]
    async fn bash_lc_evaluates_inner_commands() {
        let mut parser = PolicyParser::new();
        parser.parse("test", r#"prefix_rule(pattern=["rm"], decision="forbidden")"#).unwrap();
        let mgr = ExecPolicyManager::new(Arc::new(parser.build()));

        let req = mgr.evaluate_command(
            &cmd(&["bash", "-lc", "rm -rf /some/folder"]),
            AskForApproval::OnRequest,
            &SandboxPolicy::DangerFullAccess,
            None,
        ).await;
        assert!(matches!(req, ExecApprovalRequirement::Forbidden { .. }));
    }

    #[tokio::test]
    async fn append_amendment_updates_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ExecPolicyManager::default();
        let amendment = ExecPolicyAmendment { command: vec!["echo".into(), "hello".into()] };

        mgr.append_amendment(tmp.path(), &amendment).await.unwrap();

        let policy = mgr.current().await;
        let eval = policy.check(&cmd(&["echo", "hello", "world"]), &heuristic);
        assert_eq!(eval.decision, Decision::Allow);
        assert!(eval.is_match());
    }

    #[tokio::test]
    async fn load_from_rules_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let rules_dir = tmp.path().join(RULES_DIR_NAME);
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::write(
            rules_dir.join("test.rules"),
            r#"prefix_rule(pattern=["rm"], decision="forbidden")"#,
        ).unwrap();

        let policy = load_exec_policy(tmp.path()).await.unwrap();
        let eval = policy.check(&cmd(&["rm"]), &heuristic);
        assert_eq!(eval.decision, Decision::Forbidden);
    }

    #[tokio::test]
    async fn collect_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let files = collect_policy_files(&tmp.path().join("nonexistent")).await.unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn commands_for_exec_policy_parses_bash_lc() {
        let (cmds, complex) = commands_for_exec_policy(&cmd(&["bash", "-lc", "cargo build && echo ok"]));
        assert!(!complex);
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn commands_for_exec_policy_fallback_for_non_shell() {
        let (cmds, complex) = commands_for_exec_policy(&cmd(&["cargo", "build"]));
        assert!(!complex);
        assert_eq!(cmds, vec![cmd(&["cargo", "build"])]);
    }
}

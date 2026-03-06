use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use thiserror::Error;

use super::prefix_rule::{Decision, NetworkRuleProtocol};
use super::network_rule::normalize_network_rule_host;

#[derive(Debug, Error)]
pub enum AmendError {
    #[error("prefix rule requires at least one token")]
    EmptyPrefix,
    #[error("invalid network rule: {0}")]
    InvalidNetworkRule(String),
    #[error("policy path has no parent: {path}")]
    MissingParent { path: PathBuf },
    #[error("failed to create policy directory {dir}: {source}")]
    CreatePolicyDir {
        dir: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to format prefix tokens: {source}")]
    SerializePrefix { source: serde_json::Error },
    #[error("failed to serialize network rule field: {source}")]
    SerializeNetworkRule { source: serde_json::Error },
    #[error("failed to open policy file {path}: {source}")]
    OpenPolicyFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write to policy file {path}: {source}")]
    WritePolicyFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to lock policy file {path}: {source}")]
    LockPolicyFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to seek policy file {path}: {source}")]
    SeekPolicyFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to read policy file {path}: {source}")]
    ReadPolicyFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Append an allow prefix rule. Uses advisory file locking + blocking I/O.
pub fn blocking_append_allow_prefix_rule(
    policy_path: &Path,
    prefix: &[String],
) -> Result<(), AmendError> {
    if prefix.is_empty() {
        return Err(AmendError::EmptyPrefix);
    }
    let tokens = prefix
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| AmendError::SerializePrefix { source })?;
    let pattern = format!("[{}]", tokens.join(", "));
    let rule = format!(r#"prefix_rule(pattern={pattern}, decision="allow")"#);
    append_rule_line(policy_path, &rule)
}

/// Append a network rule. Uses advisory file locking + blocking I/O.
pub fn blocking_append_network_rule(
    policy_path: &Path,
    host: &str,
    protocol: NetworkRuleProtocol,
    decision: Decision,
    justification: Option<&str>,
) -> Result<(), AmendError> {
    let host = normalize_network_rule_host(host)
        .map_err(|e| AmendError::InvalidNetworkRule(e.to_string()))?;
    if let Some(raw) = justification {
        if raw.trim().is_empty() {
            return Err(AmendError::InvalidNetworkRule(
                "justification cannot be empty".to_string(),
            ));
        }
    }

    let host_json =
        serde_json::to_string(&host).map_err(|s| AmendError::SerializeNetworkRule { source: s })?;
    let proto_json = serde_json::to_string(protocol.as_policy_string())
        .map_err(|s| AmendError::SerializeNetworkRule { source: s })?;
    let decision_json = serde_json::to_string(match decision {
        Decision::Allow => "allow",
        Decision::Prompt => "prompt",
        Decision::Forbidden => "deny",
    })
    .map_err(|s| AmendError::SerializeNetworkRule { source: s })?;

    let mut args = vec![
        format!("host={host_json}"),
        format!("protocol={proto_json}"),
        format!("decision={decision_json}"),
    ];
    if let Some(j) = justification {
        let j_json = serde_json::to_string(j)
            .map_err(|s| AmendError::SerializeNetworkRule { source: s })?;
        args.push(format!("justification={j_json}"));
    }
    let rule = format!("network_rule({})", args.join(", "));
    append_rule_line(policy_path, &rule)
}

fn append_rule_line(policy_path: &Path, rule: &str) -> Result<(), AmendError> {
    let dir = policy_path
        .parent()
        .ok_or_else(|| AmendError::MissingParent {
            path: policy_path.to_path_buf(),
        })?;
    match std::fs::create_dir(dir) {
        Ok(()) => {}
        Err(ref e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(source) => {
            return Err(AmendError::CreatePolicyDir {
                dir: dir.to_path_buf(),
                source,
            });
        }
    }
    append_locked_line(policy_path, rule)
}

fn append_locked_line(policy_path: &Path, line: &str) -> Result<(), AmendError> {
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(policy_path)
        .map_err(|source| AmendError::OpenPolicyFile {
            path: policy_path.to_path_buf(),
            source,
        })?;

    file.lock_exclusive()
        .map_err(|source| AmendError::LockPolicyFile {
            path: policy_path.to_path_buf(),
            source,
        })?;

    file.seek(SeekFrom::Start(0))
        .map_err(|source| AmendError::SeekPolicyFile {
            path: policy_path.to_path_buf(),
            source,
        })?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|source| AmendError::ReadPolicyFile {
            path: policy_path.to_path_buf(),
            source,
        })?;

    // Deduplicate: skip if line already exists
    if contents.lines().any(|existing| existing == line) {
        return Ok(());
    }

    if !contents.is_empty() && !contents.ends_with('\n') {
        file.write_all(b"\n")
            .map_err(|source| AmendError::WritePolicyFile {
                path: policy_path.to_path_buf(),
                source,
            })?;
    }

    file.write_all(format!("{line}\n").as_bytes())
        .map_err(|source| AmendError::WritePolicyFile {
            path: policy_path.to_path_buf(),
            source,
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn appends_rule_and_creates_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("rules").join("default.rules");
        blocking_append_allow_prefix_rule(
            &policy_path,
            &["echo".to_string(), "Hello, world!".to_string()],
        )
        .unwrap();
        let contents = std::fs::read_to_string(&policy_path).unwrap();
        assert_eq!(
            contents,
            "prefix_rule(pattern=[\"echo\", \"Hello, world!\"], decision=\"allow\")\n"
        );
    }

    #[test]
    fn deduplicates_existing_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("rules").join("default.rules");
        let prefix = &["ls".to_string()];
        blocking_append_allow_prefix_rule(&policy_path, prefix).unwrap();
        blocking_append_allow_prefix_rule(&policy_path, prefix).unwrap();
        let contents = std::fs::read_to_string(&policy_path).unwrap();
        assert_eq!(
            contents,
            "prefix_rule(pattern=[\"ls\"], decision=\"allow\")\n"
        );
    }

    #[test]
    fn inserts_newline_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("rules").join("default.rules");
        std::fs::create_dir_all(policy_path.parent().unwrap()).unwrap();
        std::fs::write(&policy_path, "prefix_rule(pattern=[\"ls\"], decision=\"allow\")").unwrap();
        blocking_append_allow_prefix_rule(&policy_path, &["echo".to_string()]).unwrap();
        let contents = std::fs::read_to_string(&policy_path).unwrap();
        assert!(contents.contains("\nprefix_rule(pattern=[\"echo\"]"));
    }

    #[test]
    fn appends_network_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("rules").join("default.rules");
        blocking_append_network_rule(
            &policy_path,
            "Api.GitHub.com",
            NetworkRuleProtocol::Https,
            Decision::Allow,
            Some("Allow https access"),
        )
        .unwrap();
        let contents = std::fs::read_to_string(&policy_path).unwrap();
        assert!(contents.contains("api.github.com"));
        assert!(contents.contains("\"allow\""));
        assert!(contents.contains("justification="));
    }

    #[test]
    fn network_rule_deny_decision() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("rules").join("default.rules");
        blocking_append_network_rule(
            &policy_path,
            "evil.com",
            NetworkRuleProtocol::Http,
            Decision::Forbidden,
            None,
        )
        .unwrap();
        let contents = std::fs::read_to_string(&policy_path).unwrap();
        assert!(contents.contains("\"deny\""));
    }

    #[test]
    fn rejects_empty_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("default.rules");
        assert!(blocking_append_allow_prefix_rule(&policy_path, &[]).is_err());
    }

    #[test]
    fn rejects_wildcard_host() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_path = tmp.path().join("default.rules");
        assert!(blocking_append_network_rule(
            &policy_path,
            "*.example.com",
            NetworkRuleProtocol::Https,
            Decision::Allow,
            None,
        )
        .is_err());
    }
}

//! Implicit skill invocation detection.
//!
//! When a command is executed during a turn, we check whether it runs a script
//! or reads a file that belongs to a skill directory. If so, we record an
//! implicit skill invocation for analytics.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::model::{SkillLoadOutcome, SkillMetadata};

/// Build path indexes for implicit skill invocation detection.
///
/// Returns two maps:
/// - `by_scripts_dir`: maps a skill's `scripts/` directory to its metadata
/// - `by_doc_path`: maps a skill's `SKILL.md` path to its metadata
pub fn build_implicit_skill_path_indexes(
    skills: Vec<SkillMetadata>,
) -> (HashMap<PathBuf, SkillMetadata>, HashMap<PathBuf, SkillMetadata>) {
    let mut by_scripts_dir = HashMap::new();
    let mut by_doc_path = HashMap::new();
    for skill in skills {
        let doc_path = normalize_path(&skill.path_to_skills_md);
        by_doc_path.insert(doc_path, skill.clone());
        if let Some(skill_dir) = skill.path_to_skills_md.parent() {
            let scripts_dir = normalize_path(&skill_dir.join("scripts"));
            by_scripts_dir.insert(scripts_dir, skill);
        }
    }
    (by_scripts_dir, by_doc_path)
}

/// Detect whether a shell command implicitly invokes a skill.
///
/// Checks two patterns:
/// 1. Running a script inside a skill's `scripts/` directory
/// 2. Reading (cat/head/tail/…) a skill's `SKILL.md` file
pub fn detect_implicit_skill_invocation(
    outcome: &SkillLoadOutcome,
    command: &str,
    workdir: &Path,
) -> Option<SkillMetadata> {
    let tokens = tokenize_command(command);
    if let Some(candidate) = detect_skill_script_run(outcome, &tokens, workdir) {
        return Some(candidate);
    }
    if let Some(candidate) = detect_skill_doc_read(outcome, &tokens, workdir) {
        return Some(candidate);
    }
    None
}

fn tokenize_command(command: &str) -> Vec<String> {
    shlex::split(command).unwrap_or_else(|| {
        command.split_whitespace().map(String::from).collect()
    })
}

fn script_run_token(tokens: &[String]) -> Option<&str> {
    const RUNNERS: &[&str] = &[
        "python", "python3", "bash", "zsh", "sh", "node", "deno", "ruby", "perl", "pwsh",
    ];
    const SCRIPT_EXTENSIONS: &[&str] = &[".py", ".sh", ".js", ".ts", ".rb", ".pl", ".ps1"];

    let runner_token = tokens.first()?;
    let runner = command_basename(runner_token).to_ascii_lowercase();
    let runner = runner.strip_suffix(".exe").unwrap_or(&runner);
    if !RUNNERS.contains(&runner) {
        return None;
    }

    // Find the first non-flag argument after the runner.
    for token in tokens.iter().skip(1) {
        if token == "--" { continue; }
        if token.starts_with('-') { continue; }
        let lower = token.to_ascii_lowercase();
        if SCRIPT_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
            return Some(token.as_str());
        }
        break;
    }
    None
}

fn detect_skill_script_run(
    outcome: &SkillLoadOutcome,
    tokens: &[String],
    workdir: &Path,
) -> Option<SkillMetadata> {
    let script_token = script_run_token(tokens)?;
    let script_path = Path::new(script_token);
    let script_path = if script_path.is_absolute() {
        script_path.to_path_buf()
    } else {
        workdir.join(script_path)
    };
    let script_path = normalize_path(&script_path);

    for ancestor in script_path.ancestors() {
        if let Some(candidate) = outcome.implicit_skills_by_scripts_dir.get(ancestor) {
            return Some(candidate.clone());
        }
    }
    None
}

fn detect_skill_doc_read(
    outcome: &SkillLoadOutcome,
    tokens: &[String],
    workdir: &Path,
) -> Option<SkillMetadata> {
    if !command_reads_file(tokens) {
        return None;
    }
    for token in tokens.iter().skip(1) {
        if token.starts_with('-') { continue; }
        let path = Path::new(token);
        let candidate_path = if path.is_absolute() {
            normalize_path(path)
        } else {
            normalize_path(&workdir.join(path))
        };
        if let Some(candidate) = outcome.implicit_skills_by_doc_path.get(&candidate_path) {
            return Some(candidate.clone());
        }
    }
    None
}

fn command_reads_file(tokens: &[String]) -> bool {
    const READERS: &[&str] = &["cat", "sed", "head", "tail", "less", "more", "bat", "awk"];
    let Some(program) = tokens.first() else { return false };
    let program = command_basename(program).to_ascii_lowercase();
    READERS.contains(&program.as_str())
}

fn command_basename(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command)
        .to_string()
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skills::model::*;
    use std::collections::HashSet;
    use std::sync::Arc;

    fn test_skill(doc_path: PathBuf) -> SkillMetadata {
        SkillMetadata {
            name: "test-skill".into(),
            description: "test".into(),
            short_description: None,
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: None,
            policy: None,
            permission_profile: None,
            path_to_skills_md: doc_path,
            scope: SkillScope::User,
        }
    }

    #[test]
    fn script_run_detection_matches_runner_plus_extension() {
        let tokens: Vec<String> = vec!["python3".into(), "-u".into(), "scripts/fetch.py".into()];
        assert!(script_run_token(&tokens).is_some());
    }

    #[test]
    fn script_run_detection_excludes_python_c() {
        let tokens: Vec<String> = vec!["python3".into(), "-c".into(), "print(1)".into()];
        assert!(script_run_token(&tokens).is_none());
    }

    #[test]
    fn skill_doc_read_detection_matches_absolute_path() {
        let doc_path = PathBuf::from("/tmp/skill-test/SKILL.md");
        let normalized_doc = normalize_path(&doc_path);
        let skill = test_skill(doc_path);
        let outcome = SkillLoadOutcome {
            implicit_skills_by_scripts_dir: Arc::new(HashMap::new()),
            implicit_skills_by_doc_path: Arc::new(HashMap::from([(normalized_doc, skill)])),
            ..Default::default()
        };
        let tokens: Vec<String> = vec!["cat".into(), "/tmp/skill-test/SKILL.md".into()];
        let found = detect_skill_doc_read(&outcome, &tokens, Path::new("/tmp"));
        assert_eq!(found.map(|s| s.name), Some("test-skill".to_string()));
    }

    #[test]
    fn skill_script_run_detection_matches_relative_path() {
        let doc_path = PathBuf::from("/tmp/skill-test/SKILL.md");
        let scripts_dir = normalize_path(Path::new("/tmp/skill-test/scripts"));
        let skill = test_skill(doc_path);
        let outcome = SkillLoadOutcome {
            implicit_skills_by_scripts_dir: Arc::new(HashMap::from([(scripts_dir, skill)])),
            implicit_skills_by_doc_path: Arc::new(HashMap::new()),
            ..Default::default()
        };
        let tokens: Vec<String> = vec!["python3".into(), "scripts/fetch.py".into()];
        let found = detect_skill_script_run(&outcome, &tokens, Path::new("/tmp/skill-test"));
        assert_eq!(found.map(|s| s.name), Some("test-skill".to_string()));
    }

    #[test]
    fn skill_script_run_detection_matches_absolute_path() {
        let doc_path = PathBuf::from("/tmp/skill-test/SKILL.md");
        let scripts_dir = normalize_path(Path::new("/tmp/skill-test/scripts"));
        let skill = test_skill(doc_path);
        let outcome = SkillLoadOutcome {
            implicit_skills_by_scripts_dir: Arc::new(HashMap::from([(scripts_dir, skill)])),
            implicit_skills_by_doc_path: Arc::new(HashMap::new()),
            ..Default::default()
        };
        let tokens: Vec<String> = vec![
            "python3".into(),
            "/tmp/skill-test/scripts/fetch.py".into(),
        ];
        let found = detect_skill_script_run(&outcome, &tokens, Path::new("/tmp/other"));
        assert_eq!(found.map(|s| s.name), Some("test-skill".to_string()));
    }

    #[test]
    fn build_indexes_creates_both_maps() {
        let skill = test_skill(PathBuf::from("/tmp/a/SKILL.md"));
        let (by_scripts, by_doc) = super::build_implicit_skill_path_indexes(vec![skill]);
        assert!(by_doc.contains_key(&PathBuf::from("/tmp/a/SKILL.md")));
        assert!(by_scripts.contains_key(&PathBuf::from("/tmp/a/scripts")));
    }
}

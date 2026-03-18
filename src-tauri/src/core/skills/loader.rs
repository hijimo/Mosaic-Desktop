use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::protocol::error::{CodexError, ErrorCode};

use super::model::*;

/// Maximum directory traversal depth for skill discovery.
const MAX_SKILL_DEPTH: usize = 6;

/// Skill root directory with associated priority scope.
#[derive(Debug, Clone)]
pub struct SkillRoot {
    pub path: PathBuf,
    pub scope: SkillScope,
}

/// Discover and load skills from multiple root directories using BFS.
/// When the same skill name appears in multiple roots, the highest-priority root wins.
pub fn load_skills_from_roots(roots: Vec<SkillRoot>) -> SkillLoadOutcome {
    let mut sorted_roots = roots;
    sorted_roots.sort_by_key(|r| r.scope.priority());

    let mut seen_names: HashMap<String, u8> = HashMap::new();
    let mut skills = Vec::new();
    let mut errors = Vec::new();

    for root in &sorted_roots {
        if !root.path.is_dir() {
            continue;
        }
        for result in bfs_discover_skills(&root.path, &root.scope) {
            match result {
                Ok(meta) => {
                    let priority = root.scope.priority();
                    if let Some(&existing) = seen_names.get(&meta.name) {
                        if priority >= existing {
                            continue;
                        }
                    }
                    seen_names.insert(meta.name.clone(), priority);
                    skills.push(meta);
                }
                Err(e) => errors.push(e),
            }
        }
    }

    // Deduplicate: keep highest-priority per name.
    let mut deduped: HashMap<String, SkillMetadata> = HashMap::new();
    for skill in skills {
        deduped
            .entry(skill.name.clone())
            .and_modify(|existing| {
                if skill.scope.priority() < existing.scope.priority() {
                    *existing = skill.clone();
                }
            })
            .or_insert(skill);
    }

    SkillLoadOutcome {
        skills: deduped.into_values().collect(),
        errors,
        ..Default::default()
    }
}

fn bfs_discover_skills(
    root: &Path,
    scope: &SkillScope,
) -> Vec<Result<SkillMetadata, SkillError>> {
    let mut results = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        if depth > MAX_SKILL_DEPTH {
            continue;
        }

        let skill_file = dir.join("SKILL.md");
        if skill_file.is_file() {
            match std::fs::read_to_string(&skill_file) {
                Ok(content) => {
                    let dir_name = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    match parse_skill_md(&content, &dir_name, &skill_file, scope) {
                        Ok(meta) => results.push(Ok(meta)),
                        Err(e) => results.push(Err(e)),
                    }
                }
                Err(io_err) => {
                    results.push(Err(SkillError {
                        path: skill_file,
                        message: format!("failed to read SKILL.md: {io_err}"),
                    }));
                }
            }
        }

        if depth < MAX_SKILL_DEPTH {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        queue.push_back((path, depth + 1));
                    }
                }
            }
        }
    }

    results
}

fn parse_skill_md(
    content: &str,
    fallback_name: &str,
    file_path: &Path,
    scope: &SkillScope,
) -> Result<SkillMetadata, SkillError> {
    let trimmed = content.trim_start();
    let (frontmatter, _body) = extract_frontmatter(trimmed);

    let mut name = fallback_name.to_string();
    let mut short_description: Option<String> = None;
    let mut description = String::new();
    let mut version = String::from("0.0.0");
    let mut triggers: Vec<String> = Vec::new();
    let mut interface: Option<SkillInterface> = None;
    let mut dependencies: Option<SkillDependencies> = None;
    let mut policy: Option<SkillPolicy> = None;
    let mut permission_profile: Option<String> = None;

    if let Some(fm) = frontmatter {
        for line in fm.lines() {
            let line = line.trim();
            if let Some(val) = strip_yaml_key(line, "name") {
                name = val;
            } else if let Some(val) = strip_yaml_key(line, "short_description") {
                short_description = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "description") {
                description = val;
            } else if let Some(val) = strip_yaml_key(line, "version") {
                version = val;
            } else if let Some(val) = strip_yaml_key(line, "permission_profile") {
                permission_profile = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "allow_implicit_invocation") {
                policy = Some(SkillPolicy {
                    allow_implicit_invocation: Some(val == "true"),
                });
            } else if let Some(val) = strip_yaml_key(line, "display_name") {
                interface.get_or_insert_with(Default::default).display_name = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "icon") {
                interface.get_or_insert_with(Default::default).icon = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "brand_color") {
                interface.get_or_insert_with(Default::default).brand_color = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "triggers") {
                triggers = parse_inline_list(&val);
            } else if let Some(val) = strip_yaml_key(line, "tools") {
                let tools = parse_inline_list(&val)
                    .into_iter()
                    .map(|v| SkillToolDependency {
                        r#type: "tool".into(),
                        value: v,
                        description: None,
                        transport: None,
                        command: None,
                        url: None,
                    })
                    .collect();
                dependencies = Some(SkillDependencies { tools });
            }
        }
    }

    Ok(SkillMetadata {
        name,
        short_description,
        description,
        version,
        triggers,
        interface,
        dependencies,
        policy,
        permission_profile,
        path_to_skills_md: file_path.to_path_buf(),
        scope: *scope,
    })
}

impl Default for SkillInterface {
    fn default() -> Self {
        Self {
            display_name: None,
            short_description: None,
            icon: None,
            brand_color: None,
        }
    }
}

fn extract_frontmatter(content: &str) -> (Option<&str>, &str) {
    if !content.starts_with("---") {
        return (None, content);
    }
    let after_open = &content[3..];
    if let Some(close_pos) = after_open.find("\n---") {
        let fm = &after_open[..close_pos];
        let body_start = 3 + close_pos + 4;
        let body = if body_start < content.len() {
            content[body_start..].trim_start()
        } else {
            ""
        };
        (Some(fm), body)
    } else {
        (None, content)
    }
}

fn strip_yaml_key(line: &str, key: &str) -> Option<String> {
    let stripped = line.strip_prefix(key)?.trim_start();
    let stripped = stripped.strip_prefix(':')?;
    let val = stripped.trim().trim_matches('"').trim_matches('\'');
    Some(val.to_string())
}

fn parse_inline_list(val: &str) -> Vec<String> {
    let inner = val.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_skill_dir(parent: &Path, name: &str, frontmatter: &str) -> PathBuf {
        let dir = parent.join(name);
        fs::create_dir_all(&dir).unwrap();
        let content = format!("---\n{frontmatter}\n---\n# Instructions\nDo things.");
        fs::write(dir.join("SKILL.md"), content).unwrap();
        dir
    }

    #[test]
    fn parse_frontmatter_extracts_fields() {
        let content = "---\nname: my-skill\ndescription: A test\nversion: 1.2.3\ntriggers: [build, test]\n---\n# Body";
        let meta = parse_skill_md(content, "fallback", Path::new("/tmp/SKILL.md"), &SkillScope::Repo).unwrap();
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.description, "A test");
        assert_eq!(meta.version, "1.2.3");
        assert_eq!(meta.triggers, vec!["build", "test"]);
    }

    #[test]
    fn load_discovers_and_deduplicates() {
        let repo = TempDir::new().unwrap();
        let user = TempDir::new().unwrap();
        make_skill_dir(repo.path(), "shared", "name: shared\ndescription: from repo\nversion: 1.0");
        make_skill_dir(user.path(), "shared", "name: shared\ndescription: from user\nversion: 2.0");
        make_skill_dir(user.path(), "extra", "name: extra\ndescription: user only\nversion: 1.0");

        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: repo.path().to_path_buf(), scope: SkillScope::Repo },
            SkillRoot { path: user.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert_eq!(outcome.skills.len(), 2);
        let shared = outcome.skills.iter().find(|s| s.name == "shared").unwrap();
        assert_eq!(shared.description, "from repo");
    }

    #[test]
    fn nonexistent_root_skipped() {
        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: PathBuf::from("/nonexistent"),
            scope: SkillScope::Admin,
        }]);
        assert!(outcome.skills.is_empty());
    }
}

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::protocol::error::{CodexError, ErrorCode};

/// Maximum directory traversal depth for skill discovery.
const MAX_SKILL_DEPTH: usize = 6;

/// Skill root directory with associated priority scope.
#[derive(Debug, Clone)]
pub struct SkillRoot {
    pub path: PathBuf,
    pub scope: SkillScope,
}

/// Scope / priority of a skill root (Repo > User > System > Admin).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SkillScope {
    Repo,
    User,
    System,
    Admin,
}

impl SkillScope {
    /// Lower number = higher priority.
    fn priority(&self) -> u8 {
        match self {
            SkillScope::Repo => 0,
            SkillScope::User => 1,
            SkillScope::System => 2,
            SkillScope::Admin => 3,
        }
    }
}

/// UI display configuration for a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillInterface {
    pub display_name: Option<String>,
    pub icon: Option<String>,
    pub brand_color: Option<String>,
}

/// Tool dependencies required by a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDependencies {
    pub tools: Vec<String>,
}

/// Invocation policy for a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPolicy {
    pub allow_implicit_invocation: bool,
}

/// Complete metadata for a discovered skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata {
    pub name: String,
    pub short_description: Option<String>,
    pub description: String,
    pub version: String,
    pub triggers: Vec<String>,
    pub interface: Option<SkillInterface>,
    pub dependencies: Option<SkillDependencies>,
    pub policy: Option<SkillPolicy>,
    pub permission_profile: Option<String>,
    pub path_to_skills_md: PathBuf,
    pub scope: SkillScope,
}

/// Result of loading skills from multiple roots.
pub struct SkillLoadOutcome {
    pub skills: Vec<SkillMetadata>,
    pub errors: Vec<(PathBuf, CodexError)>,
    pub disabled_paths: Vec<PathBuf>,
    pub implicit_skills: HashMap<String, SkillMetadata>,
}

/// Discover and load skills from multiple root directories using BFS
/// with a maximum depth of 6. When the same skill name appears in
/// multiple roots, the highest-priority root wins (Repo > User > System > Admin).
pub fn load_skills_from_roots(roots: Vec<SkillRoot>) -> SkillLoadOutcome {
    // Sort roots by priority (highest first = lowest number).
    let mut sorted_roots = roots;
    sorted_roots.sort_by_key(|r| r.scope.priority());

    let mut seen_names: HashMap<String, u8> = HashMap::new();
    let mut skills = Vec::new();
    let mut errors: Vec<(PathBuf, CodexError)> = Vec::new();
    let disabled_paths: Vec<PathBuf> = Vec::new();
    let mut implicit_skills: HashMap<String, SkillMetadata> = HashMap::new();

    for root in &sorted_roots {
        if !root.path.is_dir() {
            continue;
        }
        let discovered = bfs_discover_skills(&root.path, &root.scope);
        for result in discovered {
            match result {
                Ok(meta) => {
                    let priority = root.scope.priority();
                    if let Some(&existing_priority) = seen_names.get(&meta.name) {
                        if priority >= existing_priority {
                            // Lower-priority root — skip.
                            continue;
                        }
                    }
                    seen_names.insert(meta.name.clone(), priority);

                    if meta
                        .policy
                        .as_ref()
                        .is_some_and(|p| p.allow_implicit_invocation)
                    {
                        implicit_skills.insert(meta.name.clone(), meta.clone());
                    }
                    skills.push(meta);
                }
                Err((path, err)) => {
                    errors.push((path, err));
                }
            }
        }
    }

    // Deduplicate: keep only the highest-priority entry per name.
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
        disabled_paths,
        implicit_skills,
    }
}

/// BFS traversal of a single root directory, up to MAX_SKILL_DEPTH levels.
/// Returns a list of results — either parsed SkillMetadata or (path, error).
fn bfs_discover_skills(
    root: &Path,
    scope: &SkillScope,
) -> Vec<Result<SkillMetadata, (PathBuf, CodexError)>> {
    let mut results = Vec::new();
    // Queue entries: (directory_path, current_depth)
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
                        Err(e) => results.push(Err((skill_file, e))),
                    }
                }
                Err(io_err) => {
                    results.push(Err((
                        skill_file,
                        CodexError::new(
                            ErrorCode::InternalError,
                            format!("failed to read SKILL.md: {io_err}"),
                        ),
                    )));
                }
            }
        }

        // Enqueue child directories for BFS.
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

/// Parse a SKILL.md file's YAML frontmatter into SkillMetadata.
fn parse_skill_md(
    content: &str,
    fallback_name: &str,
    file_path: &Path,
    scope: &SkillScope,
) -> Result<SkillMetadata, CodexError> {
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
                let allow = val == "true";
                policy = Some(SkillPolicy {
                    allow_implicit_invocation: allow,
                });
            } else if let Some(val) = strip_yaml_key(line, "display_name") {
                let iface = interface.get_or_insert(SkillInterface {
                    display_name: None,
                    icon: None,
                    brand_color: None,
                });
                iface.display_name = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "icon") {
                let iface = interface.get_or_insert(SkillInterface {
                    display_name: None,
                    icon: None,
                    brand_color: None,
                });
                iface.icon = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "brand_color") {
                let iface = interface.get_or_insert(SkillInterface {
                    display_name: None,
                    icon: None,
                    brand_color: None,
                });
                iface.brand_color = Some(val);
            } else if let Some(val) = strip_yaml_key(line, "triggers") {
                // Simple inline list: triggers: [a, b, c]
                triggers = parse_inline_list(&val);
            } else if let Some(val) = strip_yaml_key(line, "tools") {
                let tools = parse_inline_list(&val);
                if !tools.is_empty() {
                    dependencies = Some(SkillDependencies { tools });
                }
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
        scope: scope.clone(),
    })
}

/// Extract YAML frontmatter (between `---` markers).
/// Returns (Some(frontmatter_str), body) or (None, full_content).
fn extract_frontmatter(content: &str) -> (Option<&str>, &str) {
    if !content.starts_with("---") {
        return (None, content);
    }
    let after_open = &content[3..];
    if let Some(close_pos) = after_open.find("\n---") {
        let fm = &after_open[..close_pos];
        let body_start = 3 + close_pos + 4; // "---" + fm + "\n---"
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

/// Strip a YAML key prefix and return the trimmed, unquoted value.
fn strip_yaml_key(line: &str, key: &str) -> Option<String> {
    let stripped = line.strip_prefix(key)?.trim_start();
    let stripped = stripped.strip_prefix(':')?;
    let val = stripped.trim().trim_matches('"').trim_matches('\'');
    Some(val.to_string())
}

/// Parse a simple inline YAML list like `[a, b, c]` or a bare comma-separated string.
fn parse_inline_list(val: &str) -> Vec<String> {
    let inner = val.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Convenience: list all skill metadata from a load outcome.
pub fn list_skills(outcome: &SkillLoadOutcome) -> &[SkillMetadata] {
    &outcome.skills
}

// ---------------------------------------------------------------------------
// Backward-compatible SkillLoader (used by codex.rs)
// ---------------------------------------------------------------------------

/// Async skill loader that wraps `load_skills_from_roots` for use in the
/// Codex submission loop. Each search directory is treated as a Repo-scope root.
pub struct SkillLoader {
    search_dirs: Vec<PathBuf>,
    loaded: Vec<SkillMetadata>,
}

impl SkillLoader {
    pub fn new(search_dirs: Vec<PathBuf>) -> Self {
        Self {
            search_dirs,
            loaded: Vec::new(),
        }
    }

    /// Scan search directories for SKILL.md files and load them.
    pub async fn load_all(&mut self) -> Result<&[SkillMetadata], CodexError> {
        let roots: Vec<SkillRoot> = self
            .search_dirs
            .iter()
            .map(|p| SkillRoot {
                path: p.clone(),
                scope: SkillScope::Repo,
            })
            .collect();
        let outcome = load_skills_from_roots(roots);
        self.loaded = outcome.skills;
        Ok(&self.loaded)
    }

    pub fn loaded_skills(&self) -> &[SkillMetadata] {
        &self.loaded
    }
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
    fn parse_frontmatter_extracts_all_fields() {
        let content = "---\n\
            name: \"my-skill\"\n\
            short_description: \"Short desc\"\n\
            description: \"A test skill\"\n\
            version: \"1.2.3\"\n\
            triggers: [build, test]\n\
            permission_profile: \"elevated\"\n\
            allow_implicit_invocation: true\n\
            display_name: \"My Skill\"\n\
            icon: \"star\"\n\
            brand_color: \"#ff0000\"\n\
            tools: [cargo, rustc]\n\
            ---\n\
            \n\
            # Instructions here\n\
            Do something.";
        let meta = parse_skill_md(
            content,
            "fallback",
            Path::new("/tmp/SKILL.md"),
            &SkillScope::Repo,
        )
        .unwrap();
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.short_description.as_deref(), Some("Short desc"));
        assert_eq!(meta.description, "A test skill");
        assert_eq!(meta.version, "1.2.3");
        assert_eq!(meta.triggers, vec!["build", "test"]);
        assert_eq!(meta.permission_profile.as_deref(), Some("elevated"));
        assert_eq!(
            meta.policy.as_ref().unwrap().allow_implicit_invocation,
            true
        );
        let iface = meta.interface.as_ref().unwrap();
        assert_eq!(iface.display_name.as_deref(), Some("My Skill"));
        assert_eq!(iface.icon.as_deref(), Some("star"));
        assert_eq!(iface.brand_color.as_deref(), Some("#ff0000"));
        let deps = meta.dependencies.as_ref().unwrap();
        assert_eq!(deps.tools, vec!["cargo", "rustc"]);
        assert_eq!(meta.scope, SkillScope::Repo);
    }

    #[test]
    fn parse_no_frontmatter_uses_fallback() {
        let content = "# Just instructions\nNo frontmatter.";
        let meta = parse_skill_md(
            content,
            "dir-name",
            Path::new("/tmp/SKILL.md"),
            &SkillScope::User,
        )
        .unwrap();
        assert_eq!(meta.name, "dir-name");
        assert_eq!(meta.description, "");
        assert_eq!(meta.version, "0.0.0");
        assert_eq!(meta.scope, SkillScope::User);
    }

    #[test]
    fn load_skills_discovers_from_single_root() {
        let tmp = TempDir::new().unwrap();
        make_skill_dir(
            tmp.path(),
            "skill-a",
            "name: skill-a\ndescription: A\nversion: 1.0",
        );
        make_skill_dir(
            tmp.path(),
            "skill-b",
            "name: skill-b\ndescription: B\nversion: 2.0",
        );

        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: tmp.path().to_path_buf(),
            scope: SkillScope::Repo,
        }]);

        assert_eq!(outcome.errors.len(), 0);
        assert_eq!(outcome.skills.len(), 2);
        let names: Vec<&str> = outcome.skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"skill-a"));
        assert!(names.contains(&"skill-b"));
    }

    #[test]
    fn load_skills_priority_resolution() {
        let repo_dir = TempDir::new().unwrap();
        let user_dir = TempDir::new().unwrap();

        make_skill_dir(
            repo_dir.path(),
            "shared",
            "name: shared\ndescription: from repo\nversion: 1.0",
        );
        make_skill_dir(
            user_dir.path(),
            "shared",
            "name: shared\ndescription: from user\nversion: 2.0",
        );
        make_skill_dir(
            user_dir.path(),
            "user-only",
            "name: user-only\ndescription: only in user\nversion: 1.0",
        );

        let outcome = load_skills_from_roots(vec![
            SkillRoot {
                path: repo_dir.path().to_path_buf(),
                scope: SkillScope::Repo,
            },
            SkillRoot {
                path: user_dir.path().to_path_buf(),
                scope: SkillScope::User,
            },
        ]);

        assert_eq!(outcome.errors.len(), 0);
        // "shared" from Repo wins, plus "user-only"
        assert_eq!(outcome.skills.len(), 2);
        let shared = outcome.skills.iter().find(|s| s.name == "shared").unwrap();
        assert_eq!(shared.description, "from repo");
        assert_eq!(shared.scope, SkillScope::Repo);
    }

    #[test]
    fn load_skills_respects_max_depth() {
        let tmp = TempDir::new().unwrap();
        // Create a skill at depth 7 (beyond MAX_SKILL_DEPTH=6)
        let mut deep = tmp.path().to_path_buf();
        for i in 0..8 {
            deep = deep.join(format!("level{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(
            deep.join("SKILL.md"),
            "---\nname: deep-skill\ndescription: too deep\nversion: 1.0\n---\n",
        )
        .unwrap();

        // Create a skill at depth 2 (within limit)
        make_skill_dir(
            &tmp.path().join("level0"),
            "shallow",
            "name: shallow\ndescription: ok\nversion: 1.0",
        );
        // Ensure level0 exists
        fs::create_dir_all(tmp.path().join("level0")).ok();

        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: tmp.path().to_path_buf(),
            scope: SkillScope::Repo,
        }]);

        let names: Vec<&str> = outcome.skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"shallow"));
        assert!(!names.contains(&"deep-skill"));
    }

    #[test]
    fn load_skills_implicit_skills_tracked() {
        let tmp = TempDir::new().unwrap();
        make_skill_dir(
            tmp.path(),
            "auto-skill",
            "name: auto-skill\ndescription: auto\nversion: 1.0\nallow_implicit_invocation: true",
        );
        make_skill_dir(
            tmp.path(),
            "manual-skill",
            "name: manual-skill\ndescription: manual\nversion: 1.0\nallow_implicit_invocation: false",
        );

        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: tmp.path().to_path_buf(),
            scope: SkillScope::User,
        }]);

        assert!(outcome.implicit_skills.contains_key("auto-skill"));
        assert!(!outcome.implicit_skills.contains_key("manual-skill"));
    }

    #[test]
    fn load_skills_nonexistent_root_is_skipped() {
        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            scope: SkillScope::Admin,
        }]);
        assert_eq!(outcome.skills.len(), 0);
        assert_eq!(outcome.errors.len(), 0);
    }

    #[test]
    fn parse_inline_list_variants() {
        assert_eq!(parse_inline_list("[a, b, c]"), vec!["a", "b", "c"]);
        assert_eq!(parse_inline_list("a, b"), vec!["a", "b"]);
        assert_eq!(parse_inline_list("[\"x\", 'y']"), vec!["x", "y"]);
        assert!(parse_inline_list("[]").is_empty());
    }

    #[test]
    fn list_skills_returns_all() {
        let tmp = TempDir::new().unwrap();
        make_skill_dir(tmp.path(), "s1", "name: s1\ndescription: d1\nversion: 1.0");
        make_skill_dir(tmp.path(), "s2", "name: s2\ndescription: d2\nversion: 1.0");
        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: tmp.path().to_path_buf(),
            scope: SkillScope::Repo,
        }]);
        assert_eq!(list_skills(&outcome).len(), 2);
    }
}

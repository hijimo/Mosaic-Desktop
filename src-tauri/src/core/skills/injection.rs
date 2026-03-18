use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::model::{SkillError, SkillMetadata};

/// Result of injecting skills into a conversation turn.
#[derive(Debug, Default)]
pub struct SkillInjections {
    /// Skill instruction content to inject as context items.
    pub items: Vec<SkillInstructionItem>,
    /// Warnings for skills that failed to load.
    pub warnings: Vec<String>,
}

/// A loaded skill instruction ready for injection.
#[derive(Debug, Clone)]
pub struct SkillInstructionItem {
    pub name: String,
    pub path: String,
    pub contents: String,
}

/// Build skill injections by reading SKILL.md files for mentioned skills.
pub async fn build_skill_injections(mentioned_skills: &[SkillMetadata]) -> SkillInjections {
    if mentioned_skills.is_empty() {
        return SkillInjections::default();
    }

    let mut result = SkillInjections {
        items: Vec::with_capacity(mentioned_skills.len()),
        warnings: Vec::new(),
    };

    for skill in mentioned_skills {
        match tokio::fs::read_to_string(&skill.path_to_skills_md).await {
            Ok(contents) => {
                result.items.push(SkillInstructionItem {
                    name: skill.name.clone(),
                    path: skill.path_to_skills_md.to_string_lossy().into_owned(),
                    contents,
                });
            }
            Err(err) => {
                result.warnings.push(format!(
                    "Failed to load skill {} at {}: {err:#}",
                    skill.name,
                    skill.path_to_skills_md.display()
                ));
            }
        }
    }

    result
}

/// Parsed mentions from user input text.
#[derive(Debug, Default)]
pub struct ToolMentions<'a> {
    /// All mentioned skill names (from both `$name` and `[$name](path)`).
    pub names: HashSet<&'a str>,
    /// Resource paths from linked mentions `[$name](path)`.
    pub paths: HashSet<&'a str>,
}

impl<'a> ToolMentions<'a> {
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.paths.is_empty()
    }
}

/// Extract `$skill-name` and `[$skill-name](path)` mentions from text.
pub fn extract_tool_mentions(text: &str) -> ToolMentions<'_> {
    let bytes = text.as_bytes();
    let mut mentions = ToolMentions::default();
    let mut i = 0;

    while i < bytes.len() {
        // Try linked mention: [$name](path)
        if bytes[i] == b'[' {
            if let Some((name, path, end)) = parse_linked_mention(text, bytes, i) {
                if !is_common_env_var(name) {
                    mentions.names.insert(name);
                    mentions.paths.insert(path);
                }
                i = end;
                continue;
            }
        }

        // Try plain mention: $name
        if bytes[i] == b'$' {
            let start = i + 1;
            if start < bytes.len() && is_name_char(bytes[start]) {
                let mut end = start + 1;
                while end < bytes.len() && is_name_char(bytes[end]) {
                    end += 1;
                }
                let name = &text[start..end];
                if !is_common_env_var(name) {
                    mentions.names.insert(name);
                }
                i = end;
                continue;
            }
        }

        i += 1;
    }

    mentions
}

/// Parse `[$name](path)` starting at `[`.
fn parse_linked_mention<'a>(
    text: &'a str,
    bytes: &[u8],
    start: usize,
) -> Option<(&'a str, &'a str, usize)> {
    // Expect [$
    if bytes.get(start + 1)? != &b'$' {
        return None;
    }
    let name_start = start + 2;
    if !is_name_char(*bytes.get(name_start)?) {
        return None;
    }

    let mut name_end = name_start + 1;
    while name_end < bytes.len() && is_name_char(bytes[name_end]) {
        name_end += 1;
    }

    // Expect ]
    if bytes.get(name_end)? != &b']' {
        return None;
    }

    // Skip whitespace, expect (
    let mut p = name_end + 1;
    while p < bytes.len() && bytes[p].is_ascii_whitespace() {
        p += 1;
    }
    if bytes.get(p)? != &b'(' {
        return None;
    }

    // Find closing )
    let path_start = p + 1;
    let mut path_end = path_start;
    while path_end < bytes.len() && bytes[path_end] != b')' {
        path_end += 1;
    }
    if bytes.get(path_end)? != &b')' {
        return None;
    }

    let path = text[path_start..path_end].trim();
    if path.is_empty() {
        return None;
    }

    Some((&text[name_start..name_end], path, path_end + 1))
}

/// Collect explicitly mentioned skills from user input text.
///
/// Scans for `$skill-name` and `[$skill-name](path)` tokens and resolves them
/// against the available skills. Ambiguous names (multiple roots) are skipped
/// unless a linked path provides an exact match.
pub fn collect_explicit_skill_mentions(
    text: &str,
    skills: &[SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
) -> Vec<SkillMetadata> {
    let mentions = extract_tool_mentions(text);
    if mentions.is_empty() {
        return Vec::new();
    }

    // Normalize linked paths for matching.
    let linked_paths: HashSet<PathBuf> = mentions
        .paths
        .iter()
        .map(|p| PathBuf::from(p))
        .collect();

    // Count how many enabled skills share each name.
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for skill in skills {
        if disabled_paths.contains(&skill.path_to_skills_md) {
            continue;
        }
        *name_counts.entry(&skill.name).or_default() += 1;
    }

    let mut selected = Vec::new();
    let mut seen = HashSet::new();

    for skill in skills {
        if disabled_paths.contains(&skill.path_to_skills_md) {
            continue;
        }

        // Path-based exact match from linked mentions.
        if linked_paths.contains(&skill.path_to_skills_md) {
            if seen.insert(skill.path_to_skills_md.clone()) {
                selected.push(skill.clone());
            }
            continue;
        }

        if !mentions.names.contains(skill.name.as_str()) {
            continue;
        }
        // Skip ambiguous names (multiple skills with same name).
        if name_counts.get(skill.name.as_str()).copied().unwrap_or(0) != 1 {
            continue;
        }
        if seen.insert(skill.path_to_skills_md.clone()) {
            selected.push(skill.clone());
        }
    }

    selected
}

fn is_name_char(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
}

fn is_common_env_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "PATH" | "HOME" | "USER" | "SHELL" | "LANG" | "TERM" | "PWD" | "EDITOR"
            | "TMPDIR" | "TEMP" | "TMP"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skills::SkillScope;

    fn make_skill(name: &str, path: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.into(),
            short_description: None,
            description: format!("{name} skill"),
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: None,
            policy: None,
            permission_profile: None,
            path_to_skills_md: PathBuf::from(path),
            scope: SkillScope::User,
        }
    }

    #[test]
    fn extract_mentions() {
        let mentions = extract_tool_mentions("use $alpha and $beta-skill please");
        assert!(mentions.names.contains("alpha"));
        assert!(mentions.names.contains("beta-skill"));
        assert!(!mentions.names.contains("please"));
    }

    #[test]
    fn extract_linked_mention() {
        let mentions = extract_tool_mentions("use [$alpha](/tmp/a/SKILL.md) and $beta");
        assert!(mentions.names.contains("alpha"));
        assert!(mentions.names.contains("beta"));
        assert!(mentions.paths.contains("/tmp/a/SKILL.md"));
    }

    #[test]
    fn linked_mention_ignores_env_vars() {
        let mentions = extract_tool_mentions("set [$PATH](/usr/bin)");
        assert!(mentions.names.is_empty());
        assert!(mentions.paths.is_empty());
    }

    #[test]
    fn linked_mention_resolves_ambiguous_by_path() {
        let skills = vec![
            make_skill("demo", "/a/SKILL.md"),
            make_skill("demo", "/b/SKILL.md"),
        ];
        // Plain $demo would be ambiguous, but linked mention with path resolves it.
        let result = collect_explicit_skill_mentions(
            "use [$demo](/a/SKILL.md)",
            &skills,
            &HashSet::new(),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path_to_skills_md, PathBuf::from("/a/SKILL.md"));
    }

    #[test]
    fn collect_skips_ambiguous() {
        let skills = vec![
            make_skill("demo", "/a/SKILL.md"),
            make_skill("demo", "/b/SKILL.md"),
        ];
        let result = collect_explicit_skill_mentions("use $demo", &skills, &HashSet::new());
        assert!(result.is_empty());
    }

    #[test]
    fn collect_skips_disabled() {
        let skills = vec![make_skill("alpha", "/a/SKILL.md")];
        let disabled = HashSet::from([PathBuf::from("/a/SKILL.md")]);
        let result = collect_explicit_skill_mentions("use $alpha", &skills, &disabled);
        assert!(result.is_empty());
    }

    #[test]
    fn collect_finds_unique() {
        let skills = vec![
            make_skill("alpha", "/a/SKILL.md"),
            make_skill("beta", "/b/SKILL.md"),
        ];
        let result = collect_explicit_skill_mentions("use $alpha", &skills, &HashSet::new());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "alpha");
    }

    #[tokio::test]
    async fn build_injections_handles_missing_file() {
        let skill = make_skill("missing", "/nonexistent/SKILL.md");
        let injections = build_skill_injections(&[skill]).await;
        assert!(injections.items.is_empty());
        assert_eq!(injections.warnings.len(), 1);
    }
}

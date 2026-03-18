//! Skill injection and mention extraction.
//!
//! Handles reading SKILL.md files for mentioned skills and extracting
//! `$skill-name` / `[$skill-name](path)` mentions from user input text.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::model::SkillMetadata;
use crate::protocol::types::UserInput;

// ── Injection ────────────────────────────────────────────────────────────────

/// Result of injecting skills into a conversation turn.
#[derive(Debug, Default)]
pub struct SkillInjections {
    pub items: Vec<SkillInstructionItem>,
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
                    skill.name, skill.path_to_skills_md.display()
                ));
            }
        }
    }
    result
}

// ── Tool mention kind ────────────────────────────────────────────────────────

const APP_PATH_PREFIX: &str = "app://";
const MCP_PATH_PREFIX: &str = "mcp://";
const SKILL_PATH_PREFIX: &str = "skill://";
const SKILL_FILENAME: &str = "SKILL.md";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMentionKind {
    App,
    Mcp,
    Skill,
    Other,
}

pub fn tool_kind_for_path(path: &str) -> ToolMentionKind {
    if path.starts_with(APP_PATH_PREFIX) {
        ToolMentionKind::App
    } else if path.starts_with(MCP_PATH_PREFIX) {
        ToolMentionKind::Mcp
    } else if path.starts_with(SKILL_PATH_PREFIX) || is_skill_filename(path) {
        ToolMentionKind::Skill
    } else {
        ToolMentionKind::Other
    }
}

fn is_skill_filename(path: &str) -> bool {
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file_name.eq_ignore_ascii_case(SKILL_FILENAME)
}

pub fn app_id_from_path(path: &str) -> Option<&str> {
    path.strip_prefix(APP_PATH_PREFIX).filter(|v| !v.is_empty())
}

pub fn normalize_skill_path(path: &str) -> &str {
    path.strip_prefix(SKILL_PATH_PREFIX).unwrap_or(path)
}

// ── Mention extraction ───────────────────────────────────────────────────────

/// Parsed mentions from user input text.
#[derive(Debug, Default)]
pub struct ToolMentions<'a> {
    pub names: HashSet<&'a str>,
    pub paths: HashSet<&'a str>,
    pub plain_names: HashSet<&'a str>,
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
                    let kind = tool_kind_for_path(path);
                    if !matches!(kind, ToolMentionKind::App | ToolMentionKind::Mcp) {
                        mentions.names.insert(name);
                    }
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
                    mentions.plain_names.insert(name);
                }
                i = end;
                continue;
            }
        }

        i += 1;
    }

    mentions
}

fn parse_linked_mention<'a>(
    text: &'a str,
    bytes: &[u8],
    start: usize,
) -> Option<(&'a str, &'a str, usize)> {
    if bytes.get(start + 1)? != &b'$' { return None; }
    let name_start = start + 2;
    if !is_name_char(*bytes.get(name_start)?) { return None; }

    let mut name_end = name_start + 1;
    while name_end < bytes.len() && is_name_char(bytes[name_end]) { name_end += 1; }
    if bytes.get(name_end)? != &b']' { return None; }

    let mut p = name_end + 1;
    while p < bytes.len() && bytes[p].is_ascii_whitespace() { p += 1; }
    if bytes.get(p)? != &b'(' { return None; }

    let path_start = p + 1;
    let mut path_end = path_start;
    while path_end < bytes.len() && bytes[path_end] != b')' { path_end += 1; }
    if bytes.get(path_end)? != &b')' { return None; }

    let path = text[path_start..path_end].trim();
    if path.is_empty() { return None; }
    Some((&text[name_start..name_end], path, path_end + 1))
}

// ── Skill selection ──────────────────────────────────────────────────────────

/// Build a map of skill name → count (excluding disabled skills).
pub fn build_skill_name_counts(
    skills: &[SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for skill in skills {
        if disabled_paths.contains(&skill.path_to_skills_md) { continue; }
        *counts.entry(skill.name.clone()).or_default() += 1;
    }
    counts
}

/// Collect explicitly mentioned skills from structured and text inputs.
///
/// Structured `UserInput::Skill` selections are resolved first by path.
/// Text inputs are then scanned for `$skill-name` mentions.
/// `connector_slug_counts` prevents plain-name matches that collide with
/// MCP connector slugs (e.g. `$slack` matching both a skill and a connector).
pub fn collect_explicit_skill_mentions(
    inputs: &[UserInput],
    skills: &[SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
    connector_slug_counts: &HashMap<String, usize>,
) -> Vec<SkillMetadata> {
    let skill_name_counts = build_skill_name_counts(skills, disabled_paths);

    let mut selected: Vec<SkillMetadata> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut blocked_plain_names: HashSet<String> = HashSet::new();

    // First: resolve structured UserInput::Skill entries by path.
    for input in inputs {
        if let UserInput::Skill { name, path } = input {
            blocked_plain_names.insert(name.clone());
            if disabled_paths.contains(path) || seen_paths.contains(path) {
                continue;
            }
            if let Some(skill) = skills.iter().find(|s| s.path_to_skills_md.as_path() == path.as_path()) {
                seen_paths.insert(skill.path_to_skills_md.clone());
                seen_names.insert(skill.name.clone());
                selected.push(skill.clone());
            }
        }
    }

    // Second: scan text inputs for $mentions.
    for input in inputs {
        if let UserInput::Text { text, .. } = input {
            let mentions = extract_tool_mentions(text);
            select_skills_from_mentions(
                skills, disabled_paths, &skill_name_counts, connector_slug_counts,
                &blocked_plain_names, &mentions,
                &mut seen_names, &mut seen_paths, &mut selected,
            );
        }
    }

    selected
}

/// Convenience wrapper: collect mentions from a single text string.
pub fn collect_explicit_skill_mentions_from_text(
    text: &str,
    skills: &[SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
) -> Vec<SkillMetadata> {
    let inputs = vec![UserInput::Text { text: text.to_string(), text_elements: vec![] }];
    collect_explicit_skill_mentions(&inputs, skills, disabled_paths, &HashMap::new())
}

fn select_skills_from_mentions(
    skills: &[SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
    skill_name_counts: &HashMap<String, usize>,
    connector_slug_counts: &HashMap<String, usize>,
    blocked_plain_names: &HashSet<String>,
    mentions: &ToolMentions<'_>,
    seen_names: &mut HashSet<String>,
    seen_paths: &mut HashSet<PathBuf>,
    selected: &mut Vec<SkillMetadata>,
) {
    if mentions.is_empty() { return; }

    // Path-based exact matches from linked mentions.
    let linked_paths: HashSet<&str> = mentions.paths.iter()
        .filter(|p| !matches!(tool_kind_for_path(p), ToolMentionKind::App | ToolMentionKind::Mcp))
        .map(|p| normalize_skill_path(p))
        .collect();

    for skill in skills {
        if disabled_paths.contains(&skill.path_to_skills_md) || seen_paths.contains(&skill.path_to_skills_md) {
            continue;
        }
        let path_str = skill.path_to_skills_md.to_string_lossy();
        if linked_paths.contains(path_str.as_ref()) {
            seen_paths.insert(skill.path_to_skills_md.clone());
            seen_names.insert(skill.name.clone());
            selected.push(skill.clone());
        }
    }

    // Plain name matches (unambiguous, not blocked by structured input or connector).
    for skill in skills {
        if disabled_paths.contains(&skill.path_to_skills_md) || seen_paths.contains(&skill.path_to_skills_md) {
            continue;
        }
        if blocked_plain_names.contains(skill.name.as_str()) { continue; }
        if !mentions.plain_names.contains(skill.name.as_str()) { continue; }

        let skill_count = skill_name_counts.get(skill.name.as_str()).copied().unwrap_or(0);
        let connector_count = connector_slug_counts
            .get(&skill.name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0);
        if skill_count != 1 || connector_count != 0 { continue; }

        if seen_names.insert(skill.name.clone()) {
            seen_paths.insert(skill.path_to_skills_md.clone());
            selected.push(skill.clone());
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn is_name_char(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b':')
}

fn is_common_env_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "PATH" | "HOME" | "USER" | "SHELL" | "LANG" | "TERM" | "PWD"
            | "TMPDIR" | "TEMP" | "TMP" | "EDITOR" | "XDG_CONFIG_HOME"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skills::SkillScope;
    use crate::protocol::types::TextElement;

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

    fn text_input(text: &str) -> UserInput {
        UserInput::Text { text: text.to_string(), text_elements: vec![] }
    }

    /// Shorthand: collect mentions from a single text string with no connectors.
    fn collect_from_text(
        text: &str,
        skills: &[SkillMetadata],
        disabled: &HashSet<PathBuf>,
    ) -> Vec<SkillMetadata> {
        collect_explicit_skill_mentions(
            &[text_input(text)], skills, disabled, &HashMap::new(),
        )
    }

    #[test]
    fn extract_plain_mentions() {
        let m = extract_tool_mentions("use $alpha and $beta-skill please");
        assert!(m.names.contains("alpha"));
        assert!(m.names.contains("beta-skill"));
        assert!(!m.names.contains("please"));
    }

    #[test]
    fn extract_linked_mention() {
        let m = extract_tool_mentions("use [$alpha](/tmp/a/SKILL.md) and $beta");
        assert!(m.names.contains("alpha"));
        assert!(m.names.contains("beta"));
        assert!(m.paths.contains("/tmp/a/SKILL.md"));
    }

    #[test]
    fn linked_mention_ignores_env_vars() {
        let m = extract_tool_mentions("set [$PATH](/usr/bin)");
        assert!(m.names.is_empty());
    }

    #[test]
    fn colon_in_name_supported() {
        let m = extract_tool_mentions("use $slack:search and $alpha");
        assert!(m.names.contains("slack:search"));
        assert!(m.names.contains("alpha"));
    }

    #[test]
    fn linked_mention_resolves_ambiguous_by_path() {
        let skills = vec![
            make_skill("demo", "/a/SKILL.md"),
            make_skill("demo", "/b/SKILL.md"),
        ];
        let result = collect_from_text("use [$demo](/a/SKILL.md)", &skills, &HashSet::new());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path_to_skills_md, PathBuf::from("/a/SKILL.md"));
    }

    #[test]
    fn collect_skips_ambiguous_plain_name() {
        let skills = vec![
            make_skill("demo", "/a/SKILL.md"),
            make_skill("demo", "/b/SKILL.md"),
        ];
        let result = collect_from_text("use $demo", &skills, &HashSet::new());
        assert!(result.is_empty());
    }

    #[test]
    fn collect_skips_disabled() {
        let skills = vec![make_skill("alpha", "/a/SKILL.md")];
        let disabled = HashSet::from([PathBuf::from("/a/SKILL.md")]);
        let result = collect_from_text("use $alpha", &skills, &disabled);
        assert!(result.is_empty());
    }

    #[test]
    fn collect_finds_unique() {
        let skills = vec![
            make_skill("alpha", "/a/SKILL.md"),
            make_skill("beta", "/b/SKILL.md"),
        ];
        let result = collect_from_text("use $alpha", &skills, &HashSet::new());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "alpha");
    }

    #[test]
    fn tool_kind_detection() {
        assert_eq!(tool_kind_for_path("app://my-app"), ToolMentionKind::App);
        assert_eq!(tool_kind_for_path("mcp://server"), ToolMentionKind::Mcp);
        assert_eq!(tool_kind_for_path("skill:///tmp/SKILL.md"), ToolMentionKind::Skill);
        assert_eq!(tool_kind_for_path("/tmp/SKILL.md"), ToolMentionKind::Skill);
        assert_eq!(tool_kind_for_path("/tmp/readme.md"), ToolMentionKind::Other);
    }

    #[test]
    fn app_id_extraction() {
        assert_eq!(app_id_from_path("app://my-app"), Some("my-app"));
        assert_eq!(app_id_from_path("app://"), None);
        assert_eq!(app_id_from_path("mcp://x"), None);
    }

    #[test]
    fn normalize_skill_path_strips_prefix() {
        assert_eq!(normalize_skill_path("skill:///tmp/SKILL.md"), "/tmp/SKILL.md");
        assert_eq!(normalize_skill_path("/tmp/SKILL.md"), "/tmp/SKILL.md");
    }

    #[tokio::test]
    async fn build_injections_handles_missing_file() {
        let skill = make_skill("missing", "/nonexistent/SKILL.md");
        let injections = build_skill_injections(&[skill]).await;
        assert!(injections.items.is_empty());
        assert_eq!(injections.warnings.len(), 1);
    }

    /// Test helper: check if `$skill_name` appears as a mention in text.
    fn text_mentions_skill(text: &str, skill_name: &str) -> bool {
        if skill_name.is_empty() { return false; }
        let bytes = text.as_bytes();
        let skill_bytes = skill_name.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b != b'$' { continue; }
            let start = i + 1;
            let Some(rest) = bytes.get(start..) else { continue };
            if !rest.starts_with(skill_bytes) { continue; }
            let after = bytes.get(start + skill_bytes.len()).copied();
            if after.is_none() || !is_name_char(after.unwrap()) {
                return true;
            }
        }
        false
    }

    #[test]
    fn text_mentions_skill_requires_exact_boundary() {
        assert!(text_mentions_skill("use $alpha here", "alpha"));
        assert!(!text_mentions_skill("use $alpha-beta here", "alpha"));
        assert!(text_mentions_skill("use $alpha-beta here", "alpha-beta"));
        assert!(!text_mentions_skill("use alpha here", "alpha"));
    }

    #[test]
    fn text_mentions_skill_handles_end_boundary() {
        assert!(text_mentions_skill("use $alpha", "alpha"));
        assert!(!text_mentions_skill("use $alpha", "alph"));
    }

    #[test]
    fn extract_tool_mentions_skips_common_env_vars() {
        let m = extract_tool_mentions("$PATH and $HOME and $alpha");
        assert!(!m.names.contains("PATH"));
        assert!(!m.names.contains("HOME"));
        assert!(m.names.contains("alpha"));
    }

    #[test]
    fn extract_tool_mentions_stops_at_non_name_chars() {
        let m = extract_tool_mentions("$alpha! and $beta.");
        assert!(m.names.contains("alpha"));
        assert!(m.names.contains("beta"));
    }

    #[test]
    fn collect_dedupes_by_path() {
        let skills = vec![make_skill("alpha", "/a/SKILL.md")];
        let result = collect_from_text(
            "use [$alpha](/a/SKILL.md) and $alpha",
            &skills,
            &HashSet::new(),
        );
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn collect_prefers_linked_path_over_name() {
        let skills = vec![
            make_skill("demo", "/a/SKILL.md"),
            make_skill("demo", "/b/SKILL.md"),
        ];
        let result = collect_from_text(
            "use [$demo](/b/SKILL.md)",
            &skills,
            &HashSet::new(),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path_to_skills_md, PathBuf::from("/b/SKILL.md"));
    }

    #[test]
    fn structured_skill_input_resolves_by_path() {
        let skills = vec![
            make_skill("demo", "/a/SKILL.md"),
            make_skill("demo", "/b/SKILL.md"),
        ];
        let inputs = vec![UserInput::Skill {
            name: "demo".into(),
            path: PathBuf::from("/b/SKILL.md"),
        }];
        let result = collect_explicit_skill_mentions(&inputs, &skills, &HashSet::new(), &HashMap::new());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path_to_skills_md, PathBuf::from("/b/SKILL.md"));
    }

    #[test]
    fn structured_skill_blocks_plain_name_fallback() {
        let skills = vec![make_skill("alpha", "/a/SKILL.md")];
        // Structured input for "alpha" with a non-matching path blocks $alpha plain match.
        let inputs = vec![
            UserInput::Skill { name: "alpha".into(), path: PathBuf::from("/nonexistent/SKILL.md") },
            text_input("use $alpha"),
        ];
        let result = collect_explicit_skill_mentions(&inputs, &skills, &HashSet::new(), &HashMap::new());
        assert!(result.is_empty());
    }

    #[test]
    fn connector_slug_conflict_blocks_plain_name() {
        let skills = vec![make_skill("slack", "/a/SKILL.md")];
        let connectors = HashMap::from([("slack".to_string(), 1usize)]);
        let inputs = vec![text_input("use $slack")];
        let result = collect_explicit_skill_mentions(&inputs, &skills, &HashSet::new(), &connectors);
        assert!(result.is_empty());
    }

    #[test]
    fn connector_slug_does_not_block_linked_path() {
        let skills = vec![make_skill("slack", "/a/SKILL.md")];
        let connectors = HashMap::from([("slack".to_string(), 1usize)]);
        let inputs = vec![text_input("use [$slack](/a/SKILL.md)")];
        let result = collect_explicit_skill_mentions(&inputs, &skills, &HashSet::new(), &connectors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "slack");
    }

    #[test]
    fn mixed_structured_and_text_inputs() {
        let skills = vec![
            make_skill("alpha", "/a/SKILL.md"),
            make_skill("beta", "/b/SKILL.md"),
        ];
        let inputs = vec![
            UserInput::Skill { name: "alpha".into(), path: PathBuf::from("/a/SKILL.md") },
            text_input("also use $beta"),
        ];
        let result = collect_explicit_skill_mentions(&inputs, &skills, &HashSet::new(), &HashMap::new());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[1].name, "beta");
    }
}

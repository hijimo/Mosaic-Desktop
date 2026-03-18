use super::model::SkillMetadata;

/// Render loaded skills into a system prompt section.
pub fn render_skills_section(skills: &[SkillMetadata]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut lines = vec![
        "## Skills".to_string(),
        "A skill is a set of local instructions to follow that is stored in a `SKILL.md` file. \
         Below is the list of skills that can be used. Each entry includes a name, description, \
         and file path so you can open the source for full instructions when using a specific skill."
            .to_string(),
        "### Available skills".to_string(),
    ];

    for skill in skills {
        let path_str = skill.path_to_skills_md.to_string_lossy().replace('\\', "/");
        lines.push(format!(
            "- {}: {} (file: {path_str})",
            skill.name, skill.description
        ));
    }

    lines.push("### How to use skills".to_string());
    lines.push(concat!(
        "- Discovery: The list above is the skills available in this session.\n",
        "- Trigger rules: If the user names a skill (with `$SkillName` or plain text) OR the task clearly matches a skill's description, you must use that skill for that turn.\n",
        "- Missing/blocked: If a named skill isn't in the list or the path can't be read, say so briefly and continue with the best fallback.\n",
        "- How to use a skill: After deciding to use a skill, open its `SKILL.md`. Read only enough to follow the workflow.\n",
        "- When `SKILL.md` references relative paths, resolve them relative to the skill directory.\n",
        "- If `scripts/` exist, prefer running or patching them instead of retyping large code blocks.\n",
        "- Keep context small: summarize long sections instead of pasting them."
    ).to_string());

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::core::skills::SkillScope;

    #[test]
    fn empty_skills_returns_none() {
        assert!(render_skills_section(&[]).is_none());
    }

    #[test]
    fn renders_skill_list() {
        let skills = vec![SkillMetadata {
            name: "test-skill".into(),
            short_description: None,
            description: "A test".into(),
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: None,
            policy: None,
            permission_profile: None,
            path_to_skills_md: PathBuf::from("/tmp/test/SKILL.md"),
            scope: SkillScope::Repo,
        }];
        let section = render_skills_section(&skills).unwrap();
        assert!(section.contains("test-skill"));
        assert!(section.contains("A test"));
        assert!(section.contains("/tmp/test/SKILL.md"));
    }
}

use std::path::PathBuf;

/// A loaded skill definition.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
    pub instructions: String,
}

/// Loads and manages skills from the filesystem.
pub struct SkillLoader {
    search_dirs: Vec<PathBuf>,
    loaded: Vec<Skill>,
}

impl SkillLoader {
    pub fn new(search_dirs: Vec<PathBuf>) -> Self {
        Self {
            search_dirs,
            loaded: Vec::new(),
        }
    }

    /// Scan search directories for SKILL.md files and load them.
    /// Parses YAML frontmatter (between `---` markers) for name/description.
    pub async fn load_all(&mut self) -> Result<&[Skill], crate::protocol::error::CodexError> {
        self.loaded.clear();
        for dir in &self.search_dirs {
            if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let skill_file = entry.path().join("SKILL.md");
                    if skill_file.exists() {
                        if let Ok(content) = tokio::fs::read_to_string(&skill_file).await {
                            let dir_name = entry.file_name().to_string_lossy().into_owned();
                            let (name, description, instructions) =
                                parse_skill_content(&content, &dir_name);
                            self.loaded.push(Skill {
                                name,
                                path: skill_file,
                                description,
                                instructions,
                            });
                        }
                    }
                }
            }
        }
        Ok(&self.loaded)
    }

    pub fn loaded_skills(&self) -> &[Skill] {
        &self.loaded
    }
}

/// Parse YAML frontmatter from SKILL.md content.
/// Expects optional `---` delimited frontmatter with `name:` and `description:` fields.
fn parse_skill_content(content: &str, fallback_name: &str) -> (String, String, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (fallback_name.to_string(), String::new(), content.to_string());
    }

    // Find closing `---`
    let after_open = &trimmed[3..];
    let Some(close_pos) = after_open.find("\n---") else {
        return (fallback_name.to_string(), String::new(), content.to_string());
    };

    let frontmatter = &after_open[..close_pos];
    let body_start = 3 + close_pos + 4; // skip opening "---" + frontmatter + "\n---"
    let instructions = trimmed[body_start..].trim_start().to_string();

    let mut name = fallback_name.to_string();
    let mut description = String::new();

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = val.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(val) = line.strip_prefix("description:") {
            description = val.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }

    (name, description, instructions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_extracts_fields() {
        let content = r#"---
name: "my-skill"
description: "A test skill"
version: "1.0"
---

# Instructions here
Do something."#;
        let (name, desc, body) = parse_skill_content(content, "fallback");
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "A test skill");
        assert!(body.starts_with("# Instructions here"));
    }

    #[test]
    fn parse_no_frontmatter_uses_fallback() {
        let content = "# Just instructions\nNo frontmatter.";
        let (name, desc, body) = parse_skill_content(content, "dir-name");
        assert_eq!(name, "dir-name");
        assert_eq!(desc, "");
        assert_eq!(body, content);
    }

    #[test]
    fn parse_single_quotes() {
        let content = "---\nname: 'quoted'\ndescription: 'desc'\n---\nbody";
        let (name, desc, _) = parse_skill_content(content, "fb");
        assert_eq!(name, "quoted");
        assert_eq!(desc, "desc");
    }
}

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
    pub async fn load_all(&mut self) -> Result<&[Skill], crate::protocol::error::CodexError> {
        self.loaded.clear();
        for dir in &self.search_dirs {
            if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let skill_file = entry.path().join("SKILL.md");
                    if skill_file.exists() {
                        if let Ok(content) = tokio::fs::read_to_string(&skill_file).await {
                            self.loaded.push(Skill {
                                name: entry
                                    .file_name()
                                    .to_string_lossy()
                                    .into_owned(),
                                path: skill_file,
                                description: String::new(),
                                instructions: content,
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

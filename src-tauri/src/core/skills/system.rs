use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use include_dir::Dir;

const SYSTEM_SKILLS_DIR: Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/src/core/skills/assets/system-skills");

const SYSTEM_DIR_NAME: &str = ".system";
const SKILLS_DIR_NAME: &str = "skills";
const MARKER_FILENAME: &str = ".codex-system-skills.marker";
const MARKER_SALT: &str = "v1";

/// Returns the on-disk cache location for embedded system skills.
pub fn system_cache_root_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(SKILLS_DIR_NAME).join(SYSTEM_DIR_NAME)
}

/// Install embedded system skills into `codex_home/skills/.system/`.
///
/// Uses a fingerprint marker to skip reinstallation when unchanged.
pub fn install_system_skills(codex_home: &Path) -> Result<(), SystemSkillsError> {
    let skills_root = codex_home.join(SKILLS_DIR_NAME);
    fs::create_dir_all(&skills_root)
        .map_err(|e| SystemSkillsError::io("create skills root", e))?;

    let dest = system_cache_root_dir(codex_home);
    let marker_path = dest.join(MARKER_FILENAME);
    let expected = embedded_fingerprint();

    if dest.is_dir() {
        if let Ok(existing) = fs::read_to_string(&marker_path) {
            if existing.trim() == expected {
                return Ok(());
            }
        }
    }

    if dest.exists() {
        fs::remove_dir_all(&dest)
            .map_err(|e| SystemSkillsError::io("remove existing system skills", e))?;
    }

    write_embedded_dir(&SYSTEM_SKILLS_DIR, &dest)?;
    fs::write(&marker_path, format!("{expected}\n"))
        .map_err(|e| SystemSkillsError::io("write marker", e))?;

    Ok(())
}

fn embedded_fingerprint() -> String {
    let mut items = Vec::new();
    collect_fingerprint_items(&SYSTEM_SKILLS_DIR, &mut items);
    items.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

    let mut hasher = DefaultHasher::new();
    MARKER_SALT.hash(&mut hasher);
    for (path, hash) in items {
        path.hash(&mut hasher);
        hash.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn collect_fingerprint_items(dir: &Dir<'_>, items: &mut Vec<(String, Option<u64>)>) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => {
                items.push((sub.path().to_string_lossy().into_owned(), None));
                collect_fingerprint_items(sub, items);
            }
            include_dir::DirEntry::File(file) => {
                let mut h = DefaultHasher::new();
                file.contents().hash(&mut h);
                items.push((file.path().to_string_lossy().into_owned(), Some(h.finish())));
            }
        }
    }
}

fn write_embedded_dir(dir: &Dir<'_>, dest: &Path) -> Result<(), SystemSkillsError> {
    fs::create_dir_all(dest).map_err(|e| SystemSkillsError::io("create dir", e))?;

    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => {
                let sub_dest = dest.join(sub.path());
                fs::create_dir_all(&sub_dest)
                    .map_err(|e| SystemSkillsError::io("create subdir", e))?;
                // Recurse with the root dest — file.path() is already relative to include root.
                write_embedded_dir(sub, dest)?;
            }
            include_dir::DirEntry::File(file) => {
                let path = dest.join(file.path());
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| SystemSkillsError::io("create parent", e))?;
                }
                fs::write(&path, file.contents())
                    .map_err(|e| SystemSkillsError::io("write file", e))?;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum SystemSkillsError {
    Io {
        action: &'static str,
        source: std::io::Error,
    },
}

impl SystemSkillsError {
    fn io(action: &'static str, source: std::io::Error) -> Self {
        Self::Io { action, source }
    }
}

impl std::fmt::Display for SystemSkillsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { action, source } => write!(f, "io error while {action}: {source}"),
        }
    }
}

impl std::error::Error for SystemSkillsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable() {
        let a = embedded_fingerprint();
        let b = embedded_fingerprint();
        assert_eq!(a, b);
    }

    #[test]
    fn install_creates_and_skips_on_rerun() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        // First install.
        install_system_skills(home).unwrap();
        let dest = system_cache_root_dir(home);
        assert!(dest.join("skill-creator/SKILL.md").exists());

        // Second install should be a no-op (marker matches).
        install_system_skills(home).unwrap();
        assert!(dest.join("skill-creator/SKILL.md").exists());
    }

    #[test]
    fn embedded_dir_has_entries() {
        let mut items = Vec::new();
        collect_fingerprint_items(&SYSTEM_SKILLS_DIR, &mut items);
        assert!(!items.is_empty());
    }
}

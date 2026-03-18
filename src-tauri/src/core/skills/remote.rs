use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// Summary of a remote skill from the skills API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Result of downloading a remote skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillDownloadResult {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RemoteSkillsResponse {
    skills: Vec<RemoteSkillEntry>,
}

#[derive(Debug, Deserialize)]
struct RemoteSkillEntry {
    id: String,
    name: String,
    description: String,
}

/// List remote skills from a skills API endpoint.
pub async fn list_remote_skills(
    base_url: &str,
    auth_token: &str,
) -> Result<Vec<RemoteSkillSummary>, RemoteSkillError> {
    let url = format!("{}/skills", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .bearer_auth(auth_token)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| RemoteSkillError::Network(e.to_string()))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(RemoteSkillError::Api {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: RemoteSkillsResponse =
        serde_json::from_str(&body).map_err(|e| RemoteSkillError::Parse(e.to_string()))?;

    Ok(parsed
        .skills
        .into_iter()
        .map(|s| RemoteSkillSummary {
            id: s.id,
            name: s.name,
            description: s.description,
        })
        .collect())
}

/// Download and extract a remote skill to a local directory.
pub async fn download_remote_skill(
    base_url: &str,
    auth_token: &str,
    skill_id: &str,
    output_dir: &Path,
) -> Result<RemoteSkillDownloadResult, RemoteSkillError> {
    let url = format!(
        "{}/skills/{}/export",
        base_url.trim_end_matches('/'),
        skill_id
    );
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .bearer_auth(auth_token)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| RemoteSkillError::Network(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(RemoteSkillError::Api {
            status: status.as_u16(),
            body,
        });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| RemoteSkillError::Network(e.to_string()))?;

    let dest = output_dir.join(skill_id);

    if is_zip_payload(&bytes) {
        let dest_clone = dest.clone();
        let zip_bytes = bytes.to_vec();
        tokio::task::spawn_blocking(move || extract_zip_to_dir(&zip_bytes, &dest_clone))
            .await
            .map_err(|e| RemoteSkillError::Io(e.to_string()))?
            .map_err(|e| RemoteSkillError::Io(e.to_string()))?;
    } else {
        // Fallback: write raw payload as SKILL.md.
        tokio::fs::create_dir_all(&dest)
            .await
            .map_err(|e| RemoteSkillError::Io(e.to_string()))?;
        tokio::fs::write(dest.join("SKILL.md"), &bytes)
            .await
            .map_err(|e| RemoteSkillError::Io(e.to_string()))?;
    }

    Ok(RemoteSkillDownloadResult {
        id: skill_id.to_string(),
        path: dest,
    })
}

fn is_zip_payload(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == [0x50, 0x4B, 0x03, 0x04]
}

fn extract_zip_to_dir(zip_bytes: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to open zip: {e}"))?;

    // Detect common prefix to strip (e.g. "skill-name/").
    let prefix = detect_common_prefix(&archive);

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;

        let raw_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue, // skip unsafe paths
        };

        // Strip common prefix.
        let rel_path = if let Some(ref pfx) = prefix {
            match raw_path.strip_prefix(pfx) {
                Ok(stripped) => stripped.to_path_buf(),
                Err(_) => raw_path,
            }
        } else {
            raw_path
        };

        if rel_path.as_os_str().is_empty() {
            continue;
        }

        // Reject path traversal.
        if rel_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            continue;
        }

        let out_path = dest.join(&rel_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("mkdir {}: {e}", out_path.display()))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            let mut outfile = std::fs::File::create(&out_path)
                .map_err(|e| format!("create {}: {e}", out_path.display()))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("write {}: {e}", out_path.display()))?;
        }
    }

    Ok(())
}

fn detect_common_prefix(archive: &zip::ZipArchive<Cursor<&[u8]>>) -> Option<PathBuf> {
    let mut prefix: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let name = archive.name_for_index(i)?;
        let path = PathBuf::from(name);
        let first = path.components().next()?;
        let first_path = PathBuf::from(first.as_os_str());
        match &prefix {
            None => prefix = Some(first_path),
            Some(p) if *p != first_path => return None,
            _ => {}
        }
    }
    prefix
}

/// Errors from remote skill operations.
#[derive(Debug)]
pub enum RemoteSkillError {
    Network(String),
    Api { status: u16, body: String },
    Parse(String),
    Io(String),
}

impl std::fmt::Display for RemoteSkillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Api { status, body } => write!(f, "API error {status}: {body}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for RemoteSkillError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_magic_detected() {
        assert!(is_zip_payload(&[0x50, 0x4B, 0x03, 0x04, 0x00]));
        assert!(!is_zip_payload(&[0x00, 0x00, 0x00, 0x00]));
        assert!(!is_zip_payload(&[0x50, 0x4B]));
    }

    #[test]
    fn extract_zip_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default();
            writer.start_file("skill-x/SKILL.md", opts).unwrap();
            std::io::Write::write_all(&mut writer, b"# Test Skill").unwrap();
            writer.start_file("skill-x/scripts/run.sh", opts).unwrap();
            std::io::Write::write_all(&mut writer, b"#!/bin/bash").unwrap();
            writer.finish().unwrap();
        }
        extract_zip_to_dir(&buf, dir.path()).unwrap();
        // Common prefix "skill-x/" should be stripped.
        assert!(dir.path().join("SKILL.md").exists());
        assert!(dir.path().join("scripts/run.sh").exists());
    }
}

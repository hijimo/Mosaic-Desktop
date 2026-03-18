use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::AuthMode;

// ── AuthDotJson ──────────────────────────────────────────────────

/// Expected structure for `$CODEX_HOME/auth.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthDotJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<AuthMode>,

    #[serde(
        rename = "OPENAI_API_KEY",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub openai_api_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

// ── Storage trait ────────────────────────────────────────────────

pub trait AuthStorageBackend: Debug + Send + Sync {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>>;
    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()>;
    fn delete(&self) -> std::io::Result<bool>;
}

// ── File-based storage ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileAuthStorage {
    codex_home: PathBuf,
}

impl FileAuthStorage {
    pub fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    fn auth_file(&self) -> PathBuf {
        self.codex_home.join("auth.json")
    }
}

impl AuthStorageBackend for FileAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        let path = self.auth_file();
        if !path.exists() {
            return Ok(None);
        }
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let auth: AuthDotJson = serde_json::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(auth))
    }

    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.codex_home)?;
        let path = self.auth_file();
        let json = serde_json::to_string_pretty(auth)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    fn delete(&self) -> std::io::Result<bool> {
        let path = self.auth_file();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ── In-memory storage (for tests) ────────────────────────────────

#[derive(Debug)]
pub struct MemoryAuthStorage {
    data: Mutex<Option<AuthDotJson>>,
}

impl MemoryAuthStorage {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(None),
        }
    }
}

impl AuthStorageBackend for MemoryAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        Ok(self.data.lock().unwrap().clone())
    }

    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        *self.data.lock().unwrap() = Some(auth.clone());
        Ok(())
    }

    fn delete(&self) -> std::io::Result<bool> {
        let mut guard = self.data.lock().unwrap();
        let existed = guard.is_some();
        *guard = None;
        Ok(existed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_dot_json_roundtrip_api_key() {
        let auth = AuthDotJson {
            auth_mode: Some(AuthMode::ApiKey),
            openai_api_key: Some("sk-test".into()),
            access_token: None,
            refresh_token: None,
        };
        let json = serde_json::to_string(&auth).unwrap();
        let parsed: AuthDotJson = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, parsed);
    }

    #[test]
    fn auth_dot_json_roundtrip_chatgpt() {
        let auth = AuthDotJson {
            auth_mode: Some(AuthMode::Chatgpt),
            openai_api_key: None,
            access_token: Some("access-tok".into()),
            refresh_token: Some("refresh-tok".into()),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let parsed: AuthDotJson = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, parsed);
    }

    #[test]
    fn auth_dot_json_skips_none_fields() {
        let auth = AuthDotJson {
            auth_mode: None,
            openai_api_key: Some("sk-x".into()),
            access_token: None,
            refresh_token: None,
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(!json.contains("auth_mode"));
        assert!(!json.contains("access_token"));
        assert!(!json.contains("refresh_token"));
    }

    #[test]
    fn memory_storage_crud() {
        let storage = MemoryAuthStorage::new();
        assert!(storage.load().unwrap().is_none());

        let auth = AuthDotJson {
            auth_mode: Some(AuthMode::ApiKey),
            openai_api_key: Some("sk-mem".into()),
            access_token: None,
            refresh_token: None,
        };
        storage.save(&auth).unwrap();
        assert_eq!(storage.load().unwrap(), Some(auth));

        assert!(storage.delete().unwrap());
        assert!(storage.load().unwrap().is_none());
        assert!(!storage.delete().unwrap());
    }

    #[test]
    fn file_storage_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileAuthStorage::new(dir.path().to_path_buf());

        assert!(storage.load().unwrap().is_none());

        let auth = AuthDotJson {
            auth_mode: Some(AuthMode::ApiKey),
            openai_api_key: Some("sk-file-test".into()),
            access_token: None,
            refresh_token: None,
        };
        storage.save(&auth).unwrap();
        assert_eq!(storage.load().unwrap(), Some(auth));

        assert!(storage.delete().unwrap());
        assert!(storage.load().unwrap().is_none());
    }
}

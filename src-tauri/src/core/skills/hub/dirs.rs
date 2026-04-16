//! Hub directory management, lock file, quarantine, audit log, taps.
//! Mirrors Hermes `tools/skills_hub.py` hub operations.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::guard::content_hash;
use super::models::*;

// ---------------------------------------------------------------------------
// ensure_hub_dirs — mirrors Hermes ensure_hub_dirs()
// ---------------------------------------------------------------------------

pub fn ensure_hub_dirs(paths: &HubPaths) -> std::io::Result<()> {
    fs::create_dir_all(&paths.hub_dir)?;
    fs::create_dir_all(&paths.quarantine_dir)?;
    fs::create_dir_all(&paths.index_cache_dir)?;
    if !paths.lock_file.exists() {
        fs::write(&paths.lock_file, "{\"version\":1,\"installed\":{}}\n")?;
    }
    if !paths.audit_log.exists() {
        fs::File::create(&paths.audit_log)?;
    }
    if !paths.taps_file.exists() {
        fs::write(&paths.taps_file, "{\"taps\":[]}\n")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HubLockFile — mirrors Hermes HubLockFile class
// ---------------------------------------------------------------------------

pub struct HubLockFile {
    path: PathBuf,
}

impl HubLockFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> LockFileData {
        if !self.path.exists() {
            return LockFileData::default();
        }
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data: &LockFileData) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(&self.path, format!("{json}\n"))
    }

    pub fn record_install(
        &self, name: &str, source: &str, identifier: &str,
        trust_level: &str, scan_verdict: &str, skill_hash: &str,
        install_path: &str, files: Vec<String>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> std::io::Result<()> {
        let mut data = self.load();
        let now = Utc::now().to_rfc3339();
        data.installed.insert(name.to_string(), LockEntry {
            source: source.into(), identifier: identifier.into(),
            trust_level: trust_level.into(), scan_verdict: scan_verdict.into(),
            content_hash: skill_hash.into(), install_path: install_path.into(),
            files, metadata, installed_at: now.clone(), updated_at: now,
        });
        self.save(&data)
    }

    pub fn record_uninstall(&self, name: &str) -> std::io::Result<()> {
        let mut data = self.load();
        data.installed.remove(name);
        self.save(&data)
    }

    pub fn get_installed(&self, name: &str) -> Option<LockEntry> {
        self.load().installed.get(name).cloned()
    }

    pub fn list_installed(&self) -> Vec<(String, LockEntry)> {
        self.load().installed.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// TapsManager — mirrors Hermes TapsManager class
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TapEntry {
    pub repo: String,
    #[serde(default = "default_tap_path")]
    pub path: String,
}

fn default_tap_path() -> String { "skills/".into() }

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TapsFile { taps: Vec<TapEntry> }

pub struct TapsManager { path: PathBuf }

impl TapsManager {
    pub fn new(path: PathBuf) -> Self { Self { path } }

    pub fn load(&self) -> Vec<TapEntry> {
        fs::read_to_string(&self.path).ok()
            .and_then(|s| serde_json::from_str::<TapsFile>(&s).ok())
            .map(|f| f.taps).unwrap_or_default()
    }

    fn save(&self, taps: &[TapEntry]) -> std::io::Result<()> {
        let data = TapsFile { taps: taps.to_vec() };
        if let Some(p) = self.path.parent() { fs::create_dir_all(p)?; }
        fs::write(&self.path, serde_json::to_string_pretty(&data).unwrap_or_default() + "\n")
    }

    pub fn add(&self, repo: &str, path: &str) -> std::io::Result<bool> {
        let mut taps = self.load();
        if taps.iter().any(|t| t.repo == repo) { return Ok(false); }
        taps.push(TapEntry { repo: repo.into(), path: path.into() });
        self.save(&taps)?;
        Ok(true)
    }

    pub fn remove(&self, repo: &str) -> std::io::Result<bool> {
        let mut taps = self.load();
        let before = taps.len();
        taps.retain(|t| t.repo != repo);
        if taps.len() == before { return Ok(false); }
        self.save(&taps)?;
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Audit log — mirrors Hermes append_audit_log()
// ---------------------------------------------------------------------------

pub fn append_audit_log(paths: &HubPaths, action: &str, skill_name: &str, source: &str, trust_level: &str, verdict: &str, extra: &str) {
    let _ = (|| -> std::io::Result<()> {
        if let Some(p) = paths.audit_log.parent() { fs::create_dir_all(p)?; }
        let mut f = fs::OpenOptions::new().create(true).append(true).open(&paths.audit_log)?;
        let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let mut line = format!("{ts} {action} {skill_name} {source}:{trust_level} {verdict}");
        if !extra.is_empty() { line.push(' '); line.push_str(extra); }
        writeln!(f, "{line}")?;
        Ok(())
    })();
}

// ---------------------------------------------------------------------------
// Quarantine + Install — mirrors Hermes quarantine_bundle / install_from_quarantine
// ---------------------------------------------------------------------------

pub fn quarantine_bundle(paths: &HubPaths, bundle: &SkillBundle) -> Result<PathBuf, String> {
    ensure_hub_dirs(paths).map_err(|e| e.to_string())?;
    let safe_name = validate_skill_name(&bundle.name)?;
    let dest = paths.quarantine_dir.join(&safe_name);
    if dest.exists() { let _ = fs::remove_dir_all(&dest); }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    for (rel_path, content) in &bundle.files {
        let safe_rel = validate_bundle_rel_path(rel_path)?;
        let parts: Vec<&str> = safe_rel.split('/').collect();
        let file_dest = parts.iter().fold(dest.clone(), |acc, p| acc.join(p));
        if let Some(parent) = file_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&file_dest, content).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

pub fn install_from_quarantine(
    paths: &HubPaths, quarantine_path: &Path, skill_name: &str,
    category: &str, bundle: &SkillBundle, scan_result: &ScanResult,
) -> Result<PathBuf, String> {
    let safe_name = validate_skill_name(skill_name)?;
    let install_dir = if category.is_empty() {
        paths.skills_dir.join(&safe_name)
    } else {
        let safe_cat = validate_category_name(category)?;
        paths.skills_dir.join(&safe_cat).join(&safe_name)
    };

    // Verify quarantine path is within quarantine dir
    let q_resolved = fs::canonicalize(quarantine_path).map_err(|e| e.to_string())?;
    let q_root = fs::canonicalize(&paths.quarantine_dir).map_err(|e| e.to_string())?;
    if !q_resolved.starts_with(&q_root) {
        return Err(format!("Unsafe quarantine path: {}", quarantine_path.display()));
    }

    if install_dir.exists() { let _ = fs::remove_dir_all(&install_dir); }
    if let Some(parent) = install_dir.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(quarantine_path, &install_dir).map_err(|e| e.to_string())?;

    let rel_path = install_dir.strip_prefix(&paths.skills_dir)
        .unwrap_or(&install_dir).to_string_lossy().to_string();
    let hash = content_hash(&install_dir);

    let lock = HubLockFile::new(paths.lock_file.clone());
    lock.record_install(
        &safe_name, &bundle.source, &bundle.identifier,
        &bundle.trust_level.to_string(), &scan_result.verdict.to_string(),
        &hash, &rel_path, bundle.files.keys().cloned().collect(),
        bundle.metadata.clone(),
    ).map_err(|e| e.to_string())?;

    append_audit_log(paths, "INSTALL", &safe_name, &bundle.source,
        &bundle.trust_level.to_string(), &scan_result.verdict.to_string(), &hash);

    Ok(install_dir)
}

pub fn uninstall_skill(paths: &HubPaths, skill_name: &str) -> Result<String, String> {
    let lock = HubLockFile::new(paths.lock_file.clone());
    let entry = lock.get_installed(skill_name)
        .ok_or_else(|| format!("'{skill_name}' is not a hub-installed skill"))?;
    let install_path = paths.skills_dir.join(&entry.install_path);
    if install_path.exists() { let _ = fs::remove_dir_all(&install_path); }
    lock.record_uninstall(skill_name).map_err(|e| e.to_string())?;
    append_audit_log(paths, "UNINSTALL", skill_name, &entry.source, &entry.trust_level, "n/a", "user_request");
    Ok(format!("Uninstalled '{skill_name}' from {}", entry.install_path))
}

// ---------------------------------------------------------------------------
// Index cache helpers — mirrors Hermes _read_index_cache / _write_index_cache
// ---------------------------------------------------------------------------

pub fn read_index_cache(paths: &HubPaths, key: &str) -> Option<serde_json::Value> {
    let file = paths.index_cache_dir.join(format!("{key}.json"));
    if !file.exists() { return None; }
    let meta = fs::metadata(&file).ok()?;
    let age = meta.modified().ok()?.elapsed().ok()?.as_secs();
    if age > INDEX_CACHE_TTL { return None; }
    let s = fs::read_to_string(&file).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn write_index_cache(paths: &HubPaths, key: &str, data: &serde_json::Value) {
    let _ = fs::create_dir_all(&paths.index_cache_dir);
    let file = paths.index_cache_dir.join(format!("{key}.json"));
    let _ = fs::write(&file, serde_json::to_string(data).unwrap_or_default());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::guard::scan_skill;
    use tempfile::TempDir;

    fn make_paths(tmp: &Path) -> HubPaths { HubPaths::new(tmp) }

    #[test]
    fn ensure_dirs_creates_structure() {
        let tmp = TempDir::new().unwrap();
        let paths = make_paths(tmp.path());
        ensure_hub_dirs(&paths).unwrap();
        assert!(paths.hub_dir.exists());
        assert!(paths.quarantine_dir.exists());
        assert!(paths.lock_file.exists());
        assert!(paths.audit_log.exists());
        assert!(paths.taps_file.exists());
    }

    #[test]
    fn lock_file_crud() {
        let tmp = TempDir::new().unwrap();
        let paths = make_paths(tmp.path());
        ensure_hub_dirs(&paths).unwrap();
        let lock = HubLockFile::new(paths.lock_file.clone());
        lock.record_install("test-skill", "github", "owner/repo/skill",
            "community", "safe", "sha256:abc", "test-skill",
            vec!["SKILL.md".into()], HashMap::new()).unwrap();
        assert!(lock.get_installed("test-skill").is_some());
        assert_eq!(lock.list_installed().len(), 1);
        lock.record_uninstall("test-skill").unwrap();
        assert!(lock.get_installed("test-skill").is_none());
    }

    #[test]
    fn taps_add_remove() {
        let tmp = TempDir::new().unwrap();
        let paths = make_paths(tmp.path());
        ensure_hub_dirs(&paths).unwrap();
        let mgr = TapsManager::new(paths.taps_file.clone());
        assert!(mgr.add("owner/repo", "skills/").unwrap());
        assert!(!mgr.add("owner/repo", "skills/").unwrap()); // duplicate
        assert_eq!(mgr.load().len(), 1);
        assert!(mgr.remove("owner/repo").unwrap());
        assert!(mgr.load().is_empty());
    }

    #[test]
    fn quarantine_and_install() {
        let tmp = TempDir::new().unwrap();
        let paths = make_paths(tmp.path());
        ensure_hub_dirs(&paths).unwrap();

        let mut files = HashMap::new();
        files.insert("SKILL.md".into(), "---\nname: demo\n---\n# Demo".into());
        let bundle = SkillBundle {
            name: "demo".into(), files, source: "github".into(),
            identifier: "test/repo/demo".into(), trust_level: TrustLevel::Community,
            metadata: HashMap::new(),
        };

        let q_path = quarantine_bundle(&paths, &bundle).unwrap();
        assert!(q_path.join("SKILL.md").exists());

        let scan = scan_skill(&q_path, "community");
        let install_dir = install_from_quarantine(&paths, &q_path, "demo", "", &bundle, &scan).unwrap();
        assert!(install_dir.join("SKILL.md").exists());
        assert!(!q_path.exists()); // moved out of quarantine

        let lock = HubLockFile::new(paths.lock_file.clone());
        assert!(lock.get_installed("demo").is_some());
    }

    #[test]
    fn uninstall_removes_skill() {
        let tmp = TempDir::new().unwrap();
        let paths = make_paths(tmp.path());
        ensure_hub_dirs(&paths).unwrap();

        let mut files = HashMap::new();
        files.insert("SKILL.md".into(), "# Test".into());
        let bundle = SkillBundle {
            name: "removeme".into(), files, source: "github".into(),
            identifier: "t/r/removeme".into(), trust_level: TrustLevel::Community,
            metadata: HashMap::new(),
        };
        let q = quarantine_bundle(&paths, &bundle).unwrap();
        let scan = scan_skill(&q, "community");
        let dir = install_from_quarantine(&paths, &q, "removeme", "", &bundle, &scan).unwrap();
        assert!(dir.exists());

        let msg = uninstall_skill(&paths, "removeme").unwrap();
        assert!(msg.contains("removeme"));
        assert!(!dir.exists());
    }
}

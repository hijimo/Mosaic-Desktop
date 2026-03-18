use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use dunce::canonicalize as canonicalize_path;
use serde::Deserialize;
use tracing::error;

use super::model::*;

// ── Constants ────────────────────────────────────────────────────────────────

const SKILLS_FILENAME: &str = "SKILL.md";
const AGENTS_DIR_NAME: &str = ".agents";
const SKILLS_METADATA_DIR: &str = "agents";
const SKILLS_METADATA_FILENAME: &str = "openai.yaml";
const SKILLS_DIR_NAME: &str = "skills";
const MAX_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_SHORT_DESCRIPTION_LEN: usize = MAX_DESCRIPTION_LEN;
const MAX_DEPENDENCY_TYPE_LEN: usize = MAX_NAME_LEN;
const MAX_DEPENDENCY_TRANSPORT_LEN: usize = MAX_NAME_LEN;
const MAX_DEPENDENCY_VALUE_LEN: usize = MAX_DESCRIPTION_LEN;
const MAX_DEPENDENCY_DESCRIPTION_LEN: usize = MAX_DESCRIPTION_LEN;
const MAX_DEPENDENCY_COMMAND_LEN: usize = MAX_DESCRIPTION_LEN;
const MAX_DEPENDENCY_URL_LEN: usize = MAX_DESCRIPTION_LEN;
const MAX_DEFAULT_PROMPT_LEN: usize = MAX_DESCRIPTION_LEN;
const MAX_SCAN_DEPTH: usize = 6;
const MAX_SKILLS_DIRS_PER_ROOT: usize = 2000;

// ── Frontmatter serde types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata: SkillFrontmatterMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatterMetadata {
    #[serde(default, rename = "short-description")]
    short_description: Option<String>,
}

// ── openai.yaml serde types ──────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct SkillMetadataFile {
    #[serde(default)]
    interface: Option<InterfaceRaw>,
    #[serde(default)]
    dependencies: Option<DependenciesRaw>,
    #[serde(default)]
    policy: Option<PolicyRaw>,
    #[serde(default)]
    permissions: Option<String>,
}

#[derive(Default)]
struct LoadedSkillExtra {
    interface: Option<SkillInterface>,
    dependencies: Option<SkillDependencies>,
    policy: Option<SkillPolicy>,
    permission_profile: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct InterfaceRaw {
    display_name: Option<String>,
    short_description: Option<String>,
    icon_small: Option<String>,
    icon_large: Option<String>,
    brand_color: Option<String>,
    default_prompt: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DependenciesRaw {
    #[serde(default)]
    tools: Vec<DependencyToolRaw>,
}

#[derive(Debug, Deserialize)]
struct PolicyRaw {
    #[serde(default)]
    allow_implicit_invocation: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct DependencyToolRaw {
    #[serde(rename = "type")]
    kind: Option<String>,
    value: Option<String>,
    description: Option<String>,
    transport: Option<String>,
    command: Option<String>,
    url: Option<String>,
}

// ── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug)]
enum SkillParseError {
    Read(std::io::Error),
    MissingFrontmatter,
    InvalidYaml(serde_yaml::Error),
    MissingField(&'static str),
    InvalidField { field: &'static str, reason: String },
}

impl fmt::Display for SkillParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => write!(f, "failed to read file: {e}"),
            Self::MissingFrontmatter => write!(f, "missing YAML frontmatter delimited by ---"),
            Self::InvalidYaml(e) => write!(f, "invalid YAML: {e}"),
            Self::MissingField(field) => write!(f, "missing field `{field}`"),
            Self::InvalidField { field, reason } => write!(f, "invalid {field}: {reason}"),
        }
    }
}

impl std::error::Error for SkillParseError {}

// ── Public API ───────────────────────────────────────────────────────────────

/// Skill root directory with associated priority scope.
#[derive(Debug, Clone)]
pub struct SkillRoot {
    pub path: PathBuf,
    pub scope: SkillScope,
}

/// Discover and load skills from multiple root directories using BFS.
pub fn load_skills_from_roots(roots: impl IntoIterator<Item = SkillRoot>) -> SkillLoadOutcome {
    let mut outcome = SkillLoadOutcome::default();
    for root in roots {
        discover_skills_under_root(&root.path, root.scope, &mut outcome);
    }

    // Deduplicate by resolved path, keeping first occurrence.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    outcome.skills.retain(|s| seen.insert(s.path_to_skills_md.clone()));

    // Sort: Repo(0) > User(1) > System(2) > Admin(3), then by name, then path.
    outcome.skills.sort_by(|a, b| {
        a.scope.priority()
            .cmp(&b.scope.priority())
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path_to_skills_md.cmp(&b.path_to_skills_md))
    });

    outcome
}

/// Build default skill roots for a given cwd and codex_home.
pub fn skill_roots_for_cwd(codex_home: &Path, cwd: &Path) -> Vec<SkillRoot> {
    let mut roots = Vec::new();

    // Repo: <cwd>/.codex/skills  and  <cwd>/.agents/skills
    let project_skills = cwd.join(".codex").join(SKILLS_DIR_NAME);
    if project_skills.is_dir() {
        roots.push(SkillRoot { path: project_skills, scope: SkillScope::Repo });
    }
    let agents_skills = cwd.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME);
    if agents_skills.is_dir() {
        roots.push(SkillRoot { path: agents_skills, scope: SkillScope::Repo });
    }

    // Walk ancestors up to project root for .agents/skills dirs.
    if let Some(project_root) = find_project_root(cwd) {
        for dir in dirs_between_project_root_and_cwd(cwd, &project_root) {
            if dir == cwd { continue; } // already handled above
            let dir_agents = dir.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME);
            if dir_agents.is_dir() {
                roots.push(SkillRoot { path: dir_agents, scope: SkillScope::Repo });
            }
        }
    }

    // User: $CODEX_HOME/skills
    let user_skills = codex_home.join(SKILLS_DIR_NAME);
    if user_skills.is_dir() {
        roots.push(SkillRoot { path: user_skills, scope: SkillScope::User });
    }

    // User: $HOME/.agents/skills
    if let Some(home) = dirs::home_dir() {
        let home_agents = home.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME);
        if home_agents.is_dir() {
            roots.push(SkillRoot { path: home_agents, scope: SkillScope::User });
        }
    }

    // System: $CODEX_HOME/skills/.system
    let system_skills = super::system::system_cache_root_dir(codex_home);
    if system_skills.is_dir() {
        roots.push(SkillRoot { path: system_skills, scope: SkillScope::System });
    }

    // Deduplicate by path.
    let mut seen = HashSet::new();
    roots.retain(|r| seen.insert(r.path.clone()));
    roots
}

fn find_project_root(cwd: &Path) -> Option<PathBuf> {
    const MARKERS: &[&str] = &[".git", ".hg", ".svn", "Cargo.toml", "package.json"];
    for ancestor in cwd.ancestors() {
        for marker in MARKERS {
            if ancestor.join(marker).exists() {
                return Some(ancestor.to_path_buf());
            }
        }
    }
    None
}

// ── BFS discovery ────────────────────────────────────────────────────────────

fn discover_skills_under_root(root: &Path, scope: SkillScope, outcome: &mut SkillLoadOutcome) {
    let Ok(root) = canonicalize_path(root) else { return };
    if !root.is_dir() { return; }

    let follow_symlinks = matches!(scope, SkillScope::Repo | SkillScope::User | SkillScope::Admin);
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    visited_dirs.insert(root.clone());
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::from([(root.clone(), 0)]);
    let mut truncated = false;

    while let Some((dir, depth)) = queue.pop_front() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => { error!("failed to read skills dir {}: {e:#}", dir.display()); continue; }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name().and_then(|f| f.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if file_name.starts_with('.') { continue; }

            let Ok(file_type) = entry.file_type() else { continue };

            if file_type.is_symlink() {
                if !follow_symlinks { continue; }
                let Ok(meta) = fs::metadata(&path) else { continue };
                if meta.is_dir() {
                    let Ok(resolved) = canonicalize_path(&path) else { continue };
                    enqueue_dir(&mut queue, &mut visited_dirs, &mut truncated, resolved, depth + 1);
                }
                continue;
            }

            if file_type.is_dir() {
                let Ok(resolved) = canonicalize_path(&path) else { continue };
                enqueue_dir(&mut queue, &mut visited_dirs, &mut truncated, resolved, depth + 1);
                continue;
            }

            if file_type.is_file() && file_name == SKILLS_FILENAME {
                match parse_skill_file(&path, scope) {
                    Ok(skill) => outcome.skills.push(skill),
                    Err(err) => {
                        if scope != SkillScope::System {
                            outcome.errors.push(SkillError {
                                path: path.clone(),
                                message: err.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    if truncated {
        tracing::warn!(
            "skills scan truncated after {} directories (root: {})",
            MAX_SKILLS_DIRS_PER_ROOT, root.display()
        );
    }
}

fn enqueue_dir(
    queue: &mut VecDeque<(PathBuf, usize)>,
    visited: &mut HashSet<PathBuf>,
    truncated: &mut bool,
    path: PathBuf,
    depth: usize,
) {
    if depth > MAX_SCAN_DEPTH { return; }
    if visited.len() >= MAX_SKILLS_DIRS_PER_ROOT { *truncated = true; return; }
    if visited.insert(path.clone()) {
        queue.push_back((path, depth));
    }
}

// ── Skill file parsing ───────────────────────────────────────────────────────

fn parse_skill_file(path: &Path, scope: SkillScope) -> Result<SkillMetadata, SkillParseError> {
    let contents = fs::read_to_string(path).map_err(SkillParseError::Read)?;
    let frontmatter_str = extract_frontmatter(&contents).ok_or(SkillParseError::MissingFrontmatter)?;
    let parsed: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str).map_err(SkillParseError::InvalidYaml)?;

    let base_name = parsed.name.as_deref()
        .map(sanitize_single_line)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default_skill_name(path));
    let name = namespaced_skill_name(path, &base_name);
    let description = parsed.description.as_deref()
        .map(sanitize_single_line)
        .unwrap_or_default();
    let short_description = parsed.metadata.short_description.as_deref()
        .map(sanitize_single_line)
        .filter(|v| !v.is_empty());

    validate_len(&name, MAX_NAME_LEN, "name")?;
    validate_len(&description, MAX_DESCRIPTION_LEN, "description")?;
    if let Some(sd) = short_description.as_deref() {
        validate_len(sd, MAX_SHORT_DESCRIPTION_LEN, "metadata.short-description")?;
    }

    let extra = load_skill_metadata_file(path);
    let resolved_path = canonicalize_path(path).unwrap_or_else(|_| path.to_path_buf());

    Ok(SkillMetadata {
        name,
        short_description,
        description,
        version: String::from("1.0"),
        triggers: Vec::new(),
        interface: extra.interface,
        dependencies: extra.dependencies,
        policy: extra.policy,
        permission_profile: extra.permission_profile,
        path_to_skills_md: resolved_path,
        scope,
    })
}

fn default_skill_name(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(sanitize_single_line)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "skill".to_string())
}

/// Prefix the skill name with a plugin namespace if the skill lives under a
/// plugin-managed directory. Returns `"namespace:base_name"` or just `base_name`.
fn namespaced_skill_name(path: &Path, base_name: &str) -> String {
    plugin_namespace_for_skill_path(path)
        .map(|ns| format!("{ns}:{base_name}"))
        .unwrap_or_else(|| base_name.to_string())
}

/// Placeholder: in the full implementation this inspects the path to determine
/// if it belongs to a plugin-managed directory and returns the plugin namespace.
fn plugin_namespace_for_skill_path(_path: &Path) -> Option<String> {
    None
}

/// Return all directories from `project_root` up to (and including) `cwd`.
/// The result is ordered from the project root outward.
fn dirs_between_project_root_and_cwd(cwd: &Path, project_root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = cwd
        .ancestors()
        .scan(false, |done, a| {
            if *done { return None; }
            if a == project_root { *done = true; }
            Some(a.to_path_buf())
        })
        .collect();
    dirs.reverse();
    dirs
}

// ── openai.yaml metadata loading ─────────────────────────────────────────────

fn load_skill_metadata_file(skill_path: &Path) -> LoadedSkillExtra {
    let Some(skill_dir) = skill_path.parent() else { return LoadedSkillExtra::default(); };
    let metadata_path = skill_dir.join(SKILLS_METADATA_DIR).join(SKILLS_METADATA_FILENAME);
    if !metadata_path.exists() { return LoadedSkillExtra::default(); }

    let contents = match fs::read_to_string(&metadata_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("ignoring {}: failed to read: {e}", metadata_path.display());
            return LoadedSkillExtra::default();
        }
    };

    let parsed: SkillMetadataFile = match serde_yaml::from_str(&contents) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("ignoring {}: invalid YAML: {e}", metadata_path.display());
            return LoadedSkillExtra::default();
        }
    };

    LoadedSkillExtra {
        interface: resolve_interface(parsed.interface, skill_dir),
        dependencies: resolve_dependencies(parsed.dependencies),
        policy: resolve_policy(parsed.policy),
        permission_profile: parsed.permissions.filter(|p| !p.is_empty()),
    }
}

fn resolve_interface(raw: Option<InterfaceRaw>, skill_dir: &Path) -> Option<SkillInterface> {
    let raw = raw?;
    let display_name = resolve_str(raw.display_name, MAX_NAME_LEN, "interface.display_name");
    let short_description = resolve_str(raw.short_description, MAX_SHORT_DESCRIPTION_LEN, "interface.short_description");
    let icon = resolve_asset_path(skill_dir, "interface.icon_small", raw.icon_small);
    let icon_large = resolve_asset_path(skill_dir, "interface.icon_large", raw.icon_large);
    let brand_color = resolve_color_str(raw.brand_color, "interface.brand_color");
    let default_prompt = resolve_str(raw.default_prompt, MAX_DEFAULT_PROMPT_LEN, "interface.default_prompt");

    let has_fields = display_name.is_some() || short_description.is_some()
        || icon.is_some() || icon_large.is_some()
        || brand_color.is_some() || default_prompt.is_some();
    if has_fields {
        Some(SkillInterface { display_name, short_description, icon, icon_large, brand_color, default_prompt })
    } else {
        None
    }
}

fn resolve_dependencies(raw: Option<DependenciesRaw>) -> Option<SkillDependencies> {
    let raw = raw?;
    let tools: Vec<SkillToolDependency> = raw.tools.into_iter().filter_map(resolve_dependency_tool).collect();
    if tools.is_empty() { None } else { Some(SkillDependencies { tools }) }
}

fn resolve_policy(raw: Option<PolicyRaw>) -> Option<SkillPolicy> {
    raw.map(|p| SkillPolicy { allow_implicit_invocation: p.allow_implicit_invocation })
}

fn resolve_dependency_tool(tool: DependencyToolRaw) -> Option<SkillToolDependency> {
    let r#type = resolve_required_str(tool.kind, MAX_DEPENDENCY_TYPE_LEN, "dependencies.tools.type")?;
    let value = resolve_required_str(tool.value, MAX_DEPENDENCY_VALUE_LEN, "dependencies.tools.value")?;
    Some(SkillToolDependency {
        r#type,
        value,
        description: resolve_str(tool.description, MAX_DEPENDENCY_DESCRIPTION_LEN, "dependencies.tools.description"),
        transport: resolve_str(tool.transport, MAX_DEPENDENCY_TRANSPORT_LEN, "dependencies.tools.transport"),
        command: resolve_str(tool.command, MAX_DEPENDENCY_COMMAND_LEN, "dependencies.tools.command"),
        url: resolve_str(tool.url, MAX_DEPENDENCY_URL_LEN, "dependencies.tools.url"),
    })
}

fn resolve_asset_path(skill_dir: &Path, field: &'static str, raw: Option<String>) -> Option<String> {
    let raw = raw?;
    if raw.is_empty() { return None; }
    let path = Path::new(&raw);
    if path.is_absolute() {
        tracing::warn!("ignoring {field}: icon must be a relative assets path");
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(c) => normalized.push(c),
            Component::ParentDir => {
                tracing::warn!("ignoring {field}: icon path must not contain '..'");
                return None;
            }
            _ => {
                tracing::warn!("ignoring {field}: icon path must be under assets/");
                return None;
            }
        }
    }
    let mut components = normalized.components();
    match components.next() {
        Some(Component::Normal(c)) if c == "assets" => {}
        _ => {
            tracing::warn!("ignoring {field}: icon path must be under assets/");
            return None;
        }
    }
    Some(skill_dir.join(normalized).to_string_lossy().into_owned())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sanitize_single_line(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_len(value: &str, max_len: usize, field: &'static str) -> Result<(), SkillParseError> {
    if value.is_empty() {
        return Err(SkillParseError::MissingField(field));
    }
    if value.chars().count() > max_len {
        return Err(SkillParseError::InvalidField {
            field,
            reason: format!("exceeds maximum length of {max_len} characters"),
        });
    }
    Ok(())
}

fn resolve_str(value: Option<String>, max_len: usize, field: &'static str) -> Option<String> {
    let value = sanitize_single_line(&value?);
    if value.is_empty() { tracing::warn!("ignoring {field}: value is empty"); return None; }
    if value.chars().count() > max_len { tracing::warn!("ignoring {field}: exceeds max length {max_len}"); return None; }
    Some(value)
}

fn resolve_required_str(value: Option<String>, max_len: usize, field: &'static str) -> Option<String> {
    if value.is_none() { tracing::warn!("ignoring {field}: value is missing"); return None; }
    resolve_str(value, max_len, field)
}

fn resolve_color_str(value: Option<String>, field: &'static str) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() { tracing::warn!("ignoring {field}: value is empty"); return None; }
    let mut chars = value.chars();
    if value.len() == 7 && chars.next() == Some('#') && chars.all(|c| c.is_ascii_hexdigit()) {
        Some(value)
    } else {
        tracing::warn!("ignoring {field}: expected #RRGGBB, got {value}");
        None
    }
}

fn extract_frontmatter(contents: &str) -> Option<String> {
    let mut lines = contents.lines();
    if !matches!(lines.next(), Some(line) if line.trim() == "---") { return None; }
    let mut fm_lines: Vec<&str> = Vec::new();
    let mut found_closing = false;
    for line in lines {
        if line.trim() == "---" { found_closing = true; break; }
        fm_lines.push(line);
    }
    if fm_lines.is_empty() || !found_closing { return None; }
    Some(fm_lines.join("\n"))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn normalized(path: &Path) -> PathBuf {
        canonicalize_path(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn write_skill_at(root: &Path, dir: &str, name: &str, description: &str) -> PathBuf {
        let skill_dir = root.join(dir);
        fs::create_dir_all(&skill_dir).unwrap();
        let desc_indented = description.replace('\n', "\n  ");
        let content = format!("---\nname: {name}\ndescription: |-\n  {desc_indented}\n---\n\n# Body\n");
        let path = skill_dir.join(SKILLS_FILENAME);
        fs::write(&path, content).unwrap();
        path
    }

    fn write_skill_metadata_at(skill_dir: &Path, contents: &str) {
        let path = skill_dir.join(SKILLS_METADATA_DIR).join(SKILLS_METADATA_FILENAME);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).unwrap(); }
        fs::write(&path, contents).unwrap();
    }

    #[test]
    fn loads_valid_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "demo-skill", "does things\ncarefully");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty(), "errors: {:?}", outcome.errors);
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].name, "demo-skill");
        assert_eq!(outcome.skills[0].description, "does things carefully");
        assert_eq!(outcome.skills[0].path_to_skills_md, normalized(&skill_path));
    }

    #[test]
    fn falls_back_to_directory_name() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("directory-derived");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join(SKILLS_FILENAME), "---\ndescription: fallback name\n---\n").unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills[0].name, "directory-derived");
    }

    #[test]
    fn enforces_length_limits() {
        let tmp = TempDir::new().unwrap();
        let too_long = "x".repeat(MAX_DESCRIPTION_LEN + 1);
        write_skill_at(tmp.path(), "too-long", "too-long", &too_long);
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert_eq!(outcome.skills.len(), 0);
        assert_eq!(outcome.errors.len(), 1);
        assert!(outcome.errors[0].message.contains("invalid description"));
    }

    #[test]
    fn skips_hidden_dirs() {
        let tmp = TempDir::new().unwrap();
        let hidden = tmp.path().join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join(SKILLS_FILENAME), "---\nname: hidden\ndescription: hidden\n---\n").unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.skills.is_empty());
    }

    #[test]
    fn invalid_frontmatter_produces_error() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("invalid");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(SKILLS_FILENAME), "---\nname: bad").unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert_eq!(outcome.skills.len(), 0);
        assert_eq!(outcome.errors.len(), 1);
        assert!(outcome.errors[0].message.contains("missing YAML frontmatter"));
    }

    #[test]
    fn deduplicates_by_path() {
        let tmp = TempDir::new().unwrap();
        write_skill_at(tmp.path(), "dupe", "dupe-skill", "from repo");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::Repo },
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].scope, SkillScope::Repo);
    }

    #[test]
    fn respects_max_scan_depth() {
        let tmp = TempDir::new().unwrap();
        write_skill_at(tmp.path(), "d0/d1/d2/d3/d4/d5", "within", "loads");
        write_skill_at(tmp.path(), "d0/d1/d2/d3/d4/d5/d6", "too-deep", "should not load");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].name, "within");
    }

    #[test]
    fn loads_dependencies_from_openai_yaml() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "dep-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, r#"
dependencies:
  tools:
    - type: env_var
      value: GITHUB_TOKEN
      description: "GitHub API token"
    - type: mcp
      value: github
      description: "GitHub MCP server"
      transport: streamable_http
      url: "https://example.com/mcp"
"#);
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        let skill = &outcome.skills[0];
        let deps = skill.dependencies.as_ref().unwrap();
        assert_eq!(deps.tools.len(), 2);
        assert_eq!(deps.tools[0].r#type, "env_var");
        assert_eq!(deps.tools[0].value, "GITHUB_TOKEN");
        assert_eq!(deps.tools[1].transport.as_deref(), Some("streamable_http"));
    }

    #[test]
    fn loads_interface_from_openai_yaml() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "ui-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "interface:\n  display_name: \"UI Skill\"\n  short_description: \"short desc\"\n  icon_small: \"./assets/icon.png\"\n  brand_color: \"#3B82F6\"\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        let iface = outcome.skills[0].interface.as_ref().unwrap();
        assert_eq!(iface.display_name.as_deref(), Some("UI Skill"));
        assert_eq!(iface.brand_color.as_deref(), Some("#3B82F6"));
    }

    #[test]
    fn loads_policy_from_openai_yaml() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "policy-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "policy:\n  allow_implicit_invocation: false\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills[0].policy, Some(SkillPolicy { allow_implicit_invocation: Some(false) }));
        assert!(outcome.allowed_skills_for_implicit_invocation().is_empty());
    }

    #[test]
    fn ignores_invalid_brand_color() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "ui-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "interface:\n  brand_color: blue\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert!(outcome.skills[0].interface.is_none());
    }

    #[test]
    fn nonexistent_root_skipped() {
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: PathBuf::from("/nonexistent"), scope: SkillScope::Admin },
        ]);
        assert!(outcome.skills.is_empty());
    }

    #[test]
    fn short_description_from_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("demo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(SKILLS_FILENAME),
            "---\nname: demo\ndescription: long\nmetadata:\n  short-description: short\n---\n").unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert_eq!(outcome.skills[0].short_description.as_deref(), Some("short"));
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinked_subdir_for_user_scope() {
        let tmp = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        let shared_skill = write_skill_at(shared.path(), "demo", "linked-skill", "from link");
        let skills_root = tmp.path().join("skills");
        fs::create_dir_all(&skills_root).unwrap();
        std::os::unix::fs::symlink(shared.path(), skills_root.join("shared")).unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: skills_root, scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].name, "linked-skill");
    }

    #[cfg(unix)]
    #[test]
    fn system_scope_ignores_symlinks() {
        let tmp = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        write_skill_at(shared.path(), "demo", "sys-linked", "from link");
        let sys_root = tmp.path().join("system");
        fs::create_dir_all(&sys_root).unwrap();
        std::os::unix::fs::symlink(shared.path(), sys_root.join("shared")).unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: sys_root, scope: SkillScope::System },
        ]);
        assert!(outcome.skills.is_empty());
    }

    #[test]
    fn dirs_between_project_root_and_cwd_returns_ordered_list() {
        let dirs = dirs_between_project_root_and_cwd(
            Path::new("/a/b/c/d"),
            Path::new("/a/b"),
        );
        assert_eq!(dirs, vec![
            PathBuf::from("/a/b"),
            PathBuf::from("/a/b/c"),
            PathBuf::from("/a/b/c/d"),
        ]);
    }

    #[test]
    fn namespaced_skill_name_returns_base_when_no_namespace() {
        assert_eq!(namespaced_skill_name(Path::new("/tmp/skills/demo/SKILL.md"), "demo"), "demo");
    }

    #[test]
    fn loads_default_prompt_from_openai_yaml() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "prompt-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "interface:\n  default_prompt: \"Hello world\"\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        let iface = outcome.skills[0].interface.as_ref().unwrap();
        assert_eq!(iface.default_prompt.as_deref(), Some("Hello world"));
    }

    #[test]
    fn loads_permission_profile_from_openai_yaml() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "perm-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "permissions: network\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills[0].permission_profile.as_deref(), Some("network"));
    }

    // ── Additional tests aligned with source project ─────────────────────

    #[test]
    fn keeps_duplicate_names_from_repo_and_user() {
        let repo_root = TempDir::new().unwrap();
        let user_root = TempDir::new().unwrap();
        write_skill_at(repo_root.path(), "demo", "demo", "from repo");
        write_skill_at(user_root.path(), "demo", "demo", "from user");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: repo_root.path().to_path_buf(), scope: SkillScope::Repo },
            SkillRoot { path: user_root.path().to_path_buf(), scope: SkillScope::User },
        ]);
        // Both are kept because they have different paths.
        assert_eq!(outcome.skills.len(), 2);
        assert_eq!(outcome.skills[0].scope, SkillScope::Repo);
        assert_eq!(outcome.skills[1].scope, SkillScope::User);
    }

    #[test]
    fn sorts_by_scope_then_name() {
        let user_root = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();
        write_skill_at(user_root.path(), "beta", "beta", "user beta");
        write_skill_at(repo_root.path(), "alpha", "alpha", "repo alpha");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: user_root.path().to_path_buf(), scope: SkillScope::User },
            SkillRoot { path: repo_root.path().to_path_buf(), scope: SkillScope::Repo },
        ]);
        assert_eq!(outcome.skills.len(), 2);
        // Repo(0) < User(1), so repo comes first.
        assert_eq!(outcome.skills[0].name, "alpha");
        assert_eq!(outcome.skills[0].scope, SkillScope::Repo);
        assert_eq!(outcome.skills[1].name, "beta");
        assert_eq!(outcome.skills[1].scope, SkillScope::User);
    }

    #[test]
    fn empty_permissions_do_not_create_profile() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "empty-perm", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "permissions: \"\"\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert!(outcome.skills[0].permission_profile.is_none());
    }

    #[test]
    fn empty_policy_defaults_to_allow_implicit() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "empty-policy", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "policy: {}\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        let skill = &outcome.skills[0];
        assert!(skill.allow_implicit_invocation());
    }

    #[test]
    fn ignores_default_prompt_over_max_length() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "long-prompt", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        let long_prompt = "x".repeat(MAX_DEFAULT_PROMPT_LEN + 1);
        write_skill_metadata_at(skill_dir, &format!("interface:\n  default_prompt: \"{long_prompt}\"\n"));
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        // Interface should be None because the only field was over-length.
        assert!(outcome.skills[0].interface.is_none());
    }

    #[test]
    fn system_scope_errors_are_suppressed() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(SKILLS_FILENAME), "---\nname: bad").unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::System },
        ]);
        // System scope errors are silently ignored.
        assert!(outcome.errors.is_empty());
        assert!(outcome.skills.is_empty());
    }

    #[test]
    fn loads_skills_from_agents_dir() {
        let tmp = TempDir::new().unwrap();
        let agents_skills = tmp.path().join(".agents").join("skills");
        fs::create_dir_all(&agents_skills).unwrap();
        write_skill_at(&agents_skills, "demo", "agents-skill", "from agents dir");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: agents_skills, scope: SkillScope::Repo },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].name, "agents-skill");
    }

    #[test]
    fn skill_roots_for_cwd_includes_codex_and_agents_dirs() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().join("project");
        fs::create_dir_all(cwd.join(".git")).unwrap();
        fs::create_dir_all(cwd.join(".codex").join("skills").join("a")).unwrap();
        fs::create_dir_all(cwd.join(".agents").join("skills").join("b")).unwrap();

        let codex_home = TempDir::new().unwrap();
        let roots = skill_roots_for_cwd(codex_home.path(), &cwd);
        let root_paths: Vec<_> = roots.iter().map(|r| r.path.clone()).collect();
        assert!(root_paths.contains(&cwd.join(".codex").join("skills")));
        assert!(root_paths.contains(&cwd.join(".agents").join("skills")));
    }

    #[test]
    fn skill_roots_for_cwd_walks_ancestors() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("project");
        let sub = project.join("sub");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join(".agents").join("skills").join("a")).unwrap();
        fs::create_dir_all(&sub).unwrap();

        let codex_home = TempDir::new().unwrap();
        let roots = skill_roots_for_cwd(codex_home.path(), &sub);
        let root_paths: Vec<_> = roots.iter().map(|r| r.path.clone()).collect();
        assert!(root_paths.contains(&project.join(".agents").join("skills")));
    }

    #[cfg(unix)]
    #[test]
    fn does_not_loop_on_symlink_cycle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("skills");
        fs::create_dir_all(&root).unwrap();
        // Create a symlink cycle: skills/loop -> skills
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: root, scope: SkillScope::User },
        ]);
        // Should not hang; visited set prevents infinite loop.
        assert!(outcome.skills.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinked_subdir_for_admin_scope() {
        let tmp = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        write_skill_at(shared.path(), "demo", "admin-linked", "from link");
        let admin_root = tmp.path().join("admin");
        fs::create_dir_all(&admin_root).unwrap();
        std::os::unix::fs::symlink(shared.path(), admin_root.join("shared")).unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: admin_root, scope: SkillScope::Admin },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].name, "admin-linked");
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinked_subdir_for_repo_scope() {
        let tmp = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        write_skill_at(shared.path(), "demo", "repo-linked", "from link");
        let repo_root = tmp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        std::os::unix::fs::symlink(shared.path(), repo_root.join("shared")).unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: repo_root, scope: SkillScope::Repo },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].name, "repo-linked");
    }

    #[test]
    fn enforces_short_description_length_limits() {
        let tmp = TempDir::new().unwrap();
        let too_long = "x".repeat(MAX_SHORT_DESCRIPTION_LEN + 1);
        let dir = tmp.path().join("demo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(SKILLS_FILENAME),
            format!("---\nname: demo\ndescription: ok\nmetadata:\n  short-description: {too_long}\n---\n"),
        ).unwrap();
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert_eq!(outcome.errors.len(), 1);
        assert!(outcome.errors[0].message.contains("metadata.short-description"));
    }

    #[test]
    fn accepts_icon_paths_under_assets_dir() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "icon-skill", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        let assets_dir = skill_dir.join("assets");
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(assets_dir.join("icon.png"), "fake png").unwrap();
        write_skill_metadata_at(skill_dir, "interface:\n  icon_small: \"assets/icon.png\"\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        let iface = outcome.skills[0].interface.as_ref().unwrap();
        assert!(iface.icon.is_some());
    }

    #[test]
    fn rejects_icon_paths_with_parent_traversal() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "bad-icon", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "interface:\n  icon_small: \"../../../etc/passwd\"\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        // Interface should be None because the icon path was rejected.
        assert!(outcome.skills[0].interface.is_none());
    }

    #[test]
    fn rejects_absolute_icon_paths() {
        let tmp = TempDir::new().unwrap();
        let skill_path = write_skill_at(tmp.path(), "demo", "abs-icon", "from yaml");
        let skill_dir = skill_path.parent().unwrap();
        write_skill_metadata_at(skill_dir, "interface:\n  icon_small: \"/tmp/icon.png\"\n");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: tmp.path().to_path_buf(), scope: SkillScope::User },
        ]);
        assert!(outcome.errors.is_empty());
        assert!(outcome.skills[0].interface.is_none());
    }

    #[test]
    fn loads_system_cache_skills() {
        let codex_home = TempDir::new().unwrap();
        let system_dir = super::super::system::system_cache_root_dir(codex_home.path());
        fs::create_dir_all(&system_dir).unwrap();
        write_skill_at(&system_dir, "sys-skill", "sys-skill", "system skill");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: system_dir, scope: SkillScope::System },
        ]);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].scope, SkillScope::System);
    }

    #[test]
    fn admin_scope_has_lowest_priority() {
        let repo_root = TempDir::new().unwrap();
        let admin_root = TempDir::new().unwrap();
        write_skill_at(repo_root.path(), "a", "alpha", "repo");
        write_skill_at(admin_root.path(), "b", "beta", "admin");
        let outcome = load_skills_from_roots(vec![
            SkillRoot { path: admin_root.path().to_path_buf(), scope: SkillScope::Admin },
            SkillRoot { path: repo_root.path().to_path_buf(), scope: SkillScope::Repo },
        ]);
        assert_eq!(outcome.skills.len(), 2);
        assert_eq!(outcome.skills[0].scope, SkillScope::Repo);
        assert_eq!(outcome.skills[1].scope, SkillScope::Admin);
    }
}

//! Project-level documentation discovery.
//!
//! Project docs are stored in `AGENTS.md` files. We walk from the project root
//! (identified by `.git` or configured markers) down to the current working
//! directory, collecting and concatenating all matching files.

use crate::core::features::{Feature, Features};
use std::path::PathBuf;
use tokio::io::AsyncReadExt;

/// Default filename scanned for project-level docs.
pub const DEFAULT_PROJECT_DOC_FILENAME: &str = "AGENTS.md";
/// Preferred local override for project-level docs.
pub const LOCAL_PROJECT_DOC_FILENAME: &str = "AGENTS.override.md";
/// Default max bytes to read from project docs.
const DEFAULT_PROJECT_DOC_MAX_BYTES: usize = 32 * 1024;
/// Separator between existing instructions and project doc.
const PROJECT_DOC_SEPARATOR: &str = "\n\n--- project-doc ---\n\n";

/// Guidance appended when ChildAgentsMd feature is enabled.
const HIERARCHICAL_AGENTS_MESSAGE: &str = r#"Files called AGENTS.md commonly appear in many places inside a container - at "/", in "~", deep within git repositories, or in any other directory; their location is not limited to version-controlled folders.

Their purpose is to pass along human guidance to you, the agent. Such guidance can include coding standards, explanations of the project layout, steps for building or testing, and even wording that must accompany a GitHub pull-request description produced by the agent; all of it is to be followed.

Each AGENTS.md governs the entire directory that contains it and every child directory beneath that point. Whenever you change a file, you have to comply with every AGENTS.md whose scope covers that file. Naming conventions, stylistic rules and similar directives are restricted to the code that falls inside that scope unless the document explicitly states otherwise.

When two AGENTS.md files disagree, the one located deeper in the directory structure overrides the higher-level file, while instructions given directly in the prompt by the system, developer, or user outrank any AGENTS.md content."#;

fn render_js_repl_instructions(features: &Features) -> Option<String> {
    if !features.enabled(Feature::JsRepl) {
        return None;
    }

    let mut s = String::from("## JavaScript REPL (Node)\n");
    s.push_str("- Use `js_repl` for Node-backed JavaScript with top-level await in a persistent kernel.\n");
    s.push_str("- `js_repl` is a freeform/custom tool. Direct `js_repl` calls must send raw JavaScript tool input (optionally with first-line `// codex-js-repl: timeout_ms=15000`). Do not wrap code in JSON (for example `{\"code\":\"...\"}`), quotes, or markdown code fences.\n");
    s.push_str("- Helpers: `codex.tmpDir` and `codex.tool(name, args?)`.\n");
    s.push_str("- `codex.tool` executes a normal tool call and resolves to the raw tool output object. Use it for shell and non-shell tools alike.\n");
    s.push_str("- To share generated images with the model, write a file under `codex.tmpDir`, call `await codex.tool(\"view_image\", { path: \"/absolute/path\" })`, then delete the file.\n");
    s.push_str("- Top-level bindings persist across cells. If you hit `SyntaxError: Identifier 'x' has already been declared`, reuse the binding, pick a new name, wrap in `{ ... }` for block scope, or reset the kernel with `js_repl_reset`.\n");
    s.push_str("- Top-level static import declarations (for example `import x from \"pkg\"`) are currently unsupported in `js_repl`; use dynamic imports with `await import(\"pkg\")` instead.\n");

    if features.enabled(Feature::JsReplToolsOnly) {
        s.push_str("- Do not call tools directly; use `js_repl` + `codex.tool(...)` for all tool calls, including shell commands.\n");
        s.push_str("- MCP tools (if any) can also be called by name via `codex.tool(...)`.\n");
    }

    s.push_str("- Avoid direct access to `process.stdout` / `process.stderr` / `process.stdin`; it can corrupt the JSON line protocol. Use `console.log` and `codex.tool(...)`.");

    Some(s)
}

/// Options for project doc discovery.
pub struct ProjectDocOptions {
    pub cwd: PathBuf,
    pub max_bytes: usize,
    pub fallback_filenames: Vec<String>,
    pub project_root_markers: Vec<String>,
}

impl ProjectDocOptions {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            max_bytes: DEFAULT_PROJECT_DOC_MAX_BYTES,
            fallback_filenames: Vec::new(),
            project_root_markers: vec![".git".to_string()],
        }
    }

    /// Build options from a merged ConfigToml + cwd.
    pub fn from_config(config: &crate::config::ConfigToml, cwd: PathBuf) -> Self {
        Self {
            cwd,
            max_bytes: config
                .project_doc_max_bytes
                .unwrap_or(DEFAULT_PROJECT_DOC_MAX_BYTES),
            fallback_filenames: config
                .project_doc_fallback_filenames
                .clone()
                .unwrap_or_default(),
            project_root_markers: config
                .project_root_markers
                .clone()
                .unwrap_or_else(|| vec![".git".to_string()]),
        }
    }
}

/// Discover project doc file paths from project root to cwd.
pub fn discover_project_doc_paths(opts: &ProjectDocOptions) -> std::io::Result<Vec<PathBuf>> {
    if opts.max_bytes == 0 {
        return Ok(Vec::new());
    }

    let dir = dunce::canonicalize(&opts.cwd).unwrap_or_else(|_| opts.cwd.clone());

    // Find project root by walking up looking for markers.
    let project_root = if !opts.project_root_markers.is_empty() {
        let mut found = None;
        for ancestor in dir.ancestors() {
            for marker in &opts.project_root_markers {
                let marker_path = ancestor.join(marker);
                match std::fs::metadata(&marker_path) {
                    Ok(_) => {
                        found = Some(ancestor.to_path_buf());
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e),
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    } else {
        None
    };

    // Build search dirs from root to cwd.
    let search_dirs: Vec<PathBuf> = if let Some(root) = project_root {
        let mut dirs = Vec::new();
        let mut cursor = dir.as_path();
        loop {
            dirs.push(cursor.to_path_buf());
            if cursor == root {
                break;
            }
            match cursor.parent() {
                Some(parent) => cursor = parent,
                None => break,
            }
        }
        dirs.reverse();
        dirs
    } else {
        vec![dir]
    };

    // Build candidate filenames list.
    let mut candidate_names: Vec<&str> =
        Vec::with_capacity(2 + opts.fallback_filenames.len());
    candidate_names.push(LOCAL_PROJECT_DOC_FILENAME);
    candidate_names.push(DEFAULT_PROJECT_DOC_FILENAME);
    for name in &opts.fallback_filenames {
        let name = name.as_str();
        if !name.is_empty() && !candidate_names.contains(&name) {
            candidate_names.push(name);
        }
    }

    let mut found: Vec<PathBuf> = Vec::new();
    for d in search_dirs {
        for name in &candidate_names {
            let candidate = d.join(name);
            match std::fs::symlink_metadata(&candidate) {
                Ok(md) => {
                    let ft = md.file_type();
                    if ft.is_file() || ft.is_symlink() {
                        found.push(candidate);
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
        }
    }

    Ok(found)
}

/// Read and concatenate project docs, respecting the byte budget.
pub async fn read_project_docs(opts: &ProjectDocOptions) -> std::io::Result<Option<String>> {
    if opts.max_bytes == 0 {
        return Ok(None);
    }

    let paths = discover_project_doc_paths(opts)?;
    if paths.is_empty() {
        return Ok(None);
    }

    let mut remaining = opts.max_bytes as u64;
    let mut parts: Vec<String> = Vec::new();

    for p in paths {
        if remaining == 0 {
            break;
        }

        let file = match tokio::fs::File::open(&p).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };

        let mut reader = tokio::io::BufReader::new(file).take(remaining);
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await?;

        let text = String::from_utf8_lossy(&data).to_string();
        if !text.trim().is_empty() {
            remaining = remaining.saturating_sub(data.len() as u64);
            parts.push(text);
        }
    }

    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts.join("\n\n")))
    }
}

/// Combine existing instructions with project docs using the standard separator.
pub async fn get_user_instructions(
    opts: &ProjectDocOptions,
    features: &Features,
    base_instructions: Option<&str>,
    skills_section: Option<&str>,
) -> Option<String> {
    let project_docs = read_project_docs(opts).await;

    let mut output = String::new();

    if let Some(instructions) = base_instructions {
        output.push_str(instructions);
    }

    match project_docs {
        Ok(Some(docs)) => {
            if !output.is_empty() {
                output.push_str(PROJECT_DOC_SEPARATOR);
            }
            output.push_str(&docs);
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("error trying to find project doc: {e:#}");
        }
    }

    if let Some(js_repl_section) = render_js_repl_instructions(features) {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&js_repl_section);
    }

    if let Some(section) = skills_section {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(section);
    }

    if features.enabled(Feature::ChildAgentsMd) {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(HIERARCHICAL_AGENTS_MESSAGE);
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::features::Features;
    use std::fs;
    use tempfile::TempDir;

    fn make_opts(root: &TempDir, limit: usize) -> ProjectDocOptions {
        ProjectDocOptions {
            cwd: root.path().to_path_buf(),
            max_bytes: limit,
            fallback_filenames: Vec::new(),
            project_root_markers: vec![".git".to_string()],
        }
    }

    fn default_features() -> Features {
        Features::with_defaults()
    }

    #[tokio::test]
    async fn no_doc_file_returns_none() {
        let tmp = TempDir::new().unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 4096), &default_features(), None, None).await;
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn doc_smaller_than_limit_is_returned() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "hello world").unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 4096), &default_features(), None, None)
            .await
            .unwrap();
        assert_eq!(res, "hello world");
    }

    #[tokio::test]
    async fn doc_larger_than_limit_is_truncated() {
        let tmp = TempDir::new().unwrap();
        let huge = "A".repeat(2048);
        fs::write(tmp.path().join("AGENTS.md"), &huge).unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 1024), &default_features(), None, None)
            .await
            .unwrap();
        assert_eq!(res.len(), 1024);
    }

    #[tokio::test]
    async fn finds_doc_in_repo_root() {
        let repo = TempDir::new().unwrap();
        fs::write(repo.path().join(".git"), "gitdir: /fake\n").unwrap();
        fs::write(repo.path().join("AGENTS.md"), "root level doc").unwrap();
        let nested = repo.path().join("workspace/crate_a");
        fs::create_dir_all(&nested).unwrap();

        let mut opts = make_opts(&repo, 4096);
        opts.cwd = nested;
        let res = get_user_instructions(&opts, &default_features(), None, None).await.unwrap();
        assert_eq!(res, "root level doc");
    }

    #[tokio::test]
    async fn zero_byte_limit_disables_docs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "something").unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 0), &default_features(), None, None).await;
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn merges_existing_instructions_with_project_doc() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "proj doc").unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 4096), &default_features(), Some("base instructions"), None)
            .await
            .unwrap();
        assert_eq!(
            res,
            format!("base instructions{PROJECT_DOC_SEPARATOR}proj doc")
        );
    }

    #[tokio::test]
    async fn keeps_existing_instructions_when_doc_missing() {
        let tmp = TempDir::new().unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 4096), &default_features(), Some("some instructions"), None)
            .await;
        assert_eq!(res, Some("some instructions".to_string()));
    }

    #[tokio::test]
    async fn concatenates_root_and_cwd_docs() {
        let repo = TempDir::new().unwrap();
        fs::write(repo.path().join(".git"), "gitdir: /fake\n").unwrap();
        fs::write(repo.path().join("AGENTS.md"), "root doc").unwrap();
        let nested = repo.path().join("workspace/crate_a");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("AGENTS.md"), "crate doc").unwrap();

        let mut opts = make_opts(&repo, 4096);
        opts.cwd = nested;
        let res = get_user_instructions(&opts, &default_features(), None, None).await.unwrap();
        assert_eq!(res, "root doc\n\ncrate doc");
    }

    #[tokio::test]
    async fn agents_local_md_preferred() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(DEFAULT_PROJECT_DOC_FILENAME), "versioned").unwrap();
        fs::write(tmp.path().join(LOCAL_PROJECT_DOC_FILENAME), "local").unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 4096), &default_features(), None, None)
            .await
            .unwrap();
        assert_eq!(res, "local");
    }

    #[tokio::test]
    async fn uses_configured_fallback() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("EXAMPLE.md"), "example instructions").unwrap();
        let mut opts = make_opts(&tmp, 4096);
        opts.fallback_filenames = vec!["EXAMPLE.md".to_string()];
        let res = get_user_instructions(&opts, &default_features(), None, None).await.unwrap();
        assert_eq!(res, "example instructions");
    }

    #[tokio::test]
    async fn agents_md_preferred_over_fallbacks() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "primary").unwrap();
        fs::write(tmp.path().join("EXAMPLE.md"), "secondary").unwrap();
        let mut opts = make_opts(&tmp, 4096);
        opts.fallback_filenames = vec!["EXAMPLE.md".to_string()];
        let res = get_user_instructions(&opts, &default_features(), None, None).await.unwrap();
        assert_eq!(res, "primary");
    }

    #[tokio::test]
    async fn custom_project_root_markers() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join(".codex-root"), "").unwrap();
        fs::write(root.path().join("AGENTS.md"), "parent doc").unwrap();
        let nested = root.path().join("dir1");
        fs::create_dir_all(nested.join(".git")).unwrap();
        fs::write(nested.join("AGENTS.md"), "child doc").unwrap();

        let mut opts = make_opts(&root, 4096);
        opts.cwd = nested;
        opts.project_root_markers = vec![".codex-root".to_string()];
        let res = get_user_instructions(&opts, &default_features(), None, None).await.unwrap();
        assert_eq!(res, "parent doc\n\nchild doc");
    }

    #[tokio::test]
    async fn skills_section_appended() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "base doc").unwrap();
        let res =
            get_user_instructions(&make_opts(&tmp, 4096), &default_features(), None, Some("## Skills\n- my-skill"))
                .await
                .unwrap();
        assert_eq!(res, "base doc\n\n## Skills\n- my-skill");
    }

    #[tokio::test]
    async fn js_repl_instructions_appended_when_enabled() {
        let tmp = TempDir::new().unwrap();
        let mut f = Features::with_defaults();
        f.enable(Feature::JsRepl);
        let res = get_user_instructions(&make_opts(&tmp, 4096), &f, None, None)
            .await
            .unwrap();
        assert!(res.starts_with("## JavaScript REPL (Node)"));
        assert!(res.contains("js_repl"));
        // JsReplToolsOnly not enabled, so no "Do not call tools directly"
        assert!(!res.contains("Do not call tools directly"));
    }

    #[tokio::test]
    async fn js_repl_tools_only_adds_extra_instructions() {
        let tmp = TempDir::new().unwrap();
        let mut f = Features::with_defaults();
        f.enable(Feature::JsRepl);
        f.enable(Feature::JsReplToolsOnly);
        let res = get_user_instructions(&make_opts(&tmp, 4096), &f, None, None)
            .await
            .unwrap();
        assert!(res.contains("Do not call tools directly"));
        assert!(res.contains("MCP tools"));
    }

    #[tokio::test]
    async fn child_agents_md_appended_when_enabled() {
        let tmp = TempDir::new().unwrap();
        let mut f = Features::with_defaults();
        f.enable(Feature::ChildAgentsMd);
        let res = get_user_instructions(&make_opts(&tmp, 4096), &f, None, None)
            .await
            .unwrap();
        assert!(res.contains("AGENTS.md commonly appear"));
    }

    #[tokio::test]
    async fn child_agents_md_not_appended_by_default() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "base doc").unwrap();
        let res = get_user_instructions(&make_opts(&tmp, 4096), &default_features(), None, None)
            .await
            .unwrap();
        assert!(!res.contains("AGENTS.md commonly appear"));
    }

    #[tokio::test]
    async fn all_sections_combine_in_order() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "proj doc").unwrap();
        let mut f = Features::with_defaults();
        f.enable(Feature::JsRepl);
        f.enable(Feature::ChildAgentsMd);
        let res = get_user_instructions(
            &make_opts(&tmp, 4096),
            &f,
            Some("base"),
            Some("## Skills"),
        )
        .await
        .unwrap();
        // Order: base -> project doc -> js_repl -> skills -> child_agents_md
        let base_pos = res.find("base").unwrap();
        let proj_pos = res.find("proj doc").unwrap();
        let js_pos = res.find("## JavaScript REPL").unwrap();
        let skills_pos = res.find("## Skills").unwrap();
        let agents_pos = res.find("AGENTS.md commonly appear").unwrap();
        assert!(base_pos < proj_pos);
        assert!(proj_pos < js_pos);
        assert!(js_pos < skills_pos);
        assert!(skills_pos < agents_pos);
    }
}

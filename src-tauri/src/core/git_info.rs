use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::{timeout, Duration as TokioDuration};

/// Timeout for git commands to prevent freezing on large repositories.
const GIT_COMMAND_TIMEOUT: TokioDuration = TokioDuration::from_secs(5);

/// Git repository information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// Diff from HEAD to the closest remote SHA.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitDiffToRemote {
    pub sha: String,
    pub diff: String,
}

/// A minimal commit summary entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitLogEntry {
    pub sha: String,
    pub timestamp: i64,
    pub subject: String,
}

// ── Repository root detection ────────────────────────────────────

/// Walk up from `base_dir` looking for a `.git` file or directory.
/// Returns the repository root path, or `None` if not inside a git repo.
pub fn get_git_repo_root(base_dir: &Path) -> Option<PathBuf> {
    let mut dir = base_dir.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Resolve the root of the *main* repository (handles worktrees).
pub fn resolve_root_git_project_for_trust(cwd: &Path) -> Option<PathBuf> {
    let base = if cwd.is_dir() { cwd } else { cwd.parent()? };

    let git_dir_out = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(base)
        .output()
        .ok()?;
    if !git_dir_out.status.success() {
        return None;
    }
    let git_dir_s = String::from_utf8(git_dir_out.stdout)
        .ok()?
        .trim()
        .to_string();

    let git_dir_path_raw = resolve_path(base, &PathBuf::from(&git_dir_s));
    let git_dir_path = std::fs::canonicalize(&git_dir_path_raw).unwrap_or(git_dir_path_raw);
    git_dir_path.parent().map(Path::to_path_buf)
}

fn resolve_path(base: &Path, relative: &Path) -> PathBuf {
    if relative.is_absolute() {
        relative.to_path_buf()
    } else {
        base.join(relative)
    }
}

// ── Core info collection ─────────────────────────────────────────

/// Collect git repository information from the given working directory.
/// All git commands run in parallel for better performance.
pub async fn collect_git_info(cwd: &Path) -> Option<GitInfo> {
    let is_git_repo = run_git_command(&["rev-parse", "--git-dir"], cwd)
        .await?
        .status
        .success();
    if !is_git_repo {
        return None;
    }

    let (commit_result, branch_result, url_result) = tokio::join!(
        run_git_command(&["rev-parse", "HEAD"], cwd),
        run_git_command(&["rev-parse", "--abbrev-ref", "HEAD"], cwd),
        run_git_command(&["remote", "get-url", "origin"], cwd)
    );

    let mut info = GitInfo {
        commit_hash: None,
        branch: None,
        repository_url: None,
    };

    if let Some(output) = commit_result {
        if output.status.success() {
            if let Ok(hash) = String::from_utf8(output.stdout) {
                info.commit_hash = Some(hash.trim().to_string());
            }
        }
    }

    if let Some(output) = branch_result {
        if output.status.success() {
            if let Ok(branch) = String::from_utf8(output.stdout) {
                let branch = branch.trim();
                if branch != "HEAD" {
                    info.branch = Some(branch.to_string());
                }
            }
        }
    }

    if let Some(output) = url_result {
        if output.status.success() {
            if let Ok(url) = String::from_utf8(output.stdout) {
                info.repository_url = Some(url.trim().to_string());
            }
        }
    }

    Some(info)
}

// ── Remote URLs ──────────────────────────────────────────────────

/// Collect fetch remotes: `{"origin": "https://..."}`.
pub async fn get_git_remote_urls(cwd: &Path) -> Option<BTreeMap<String, String>> {
    let is_git_repo = run_git_command(&["rev-parse", "--git-dir"], cwd)
        .await?
        .status
        .success();
    if !is_git_repo {
        return None;
    }
    get_git_remote_urls_assume_git_repo(cwd).await
}

/// Collect fetch remotes without checking whether `cwd` is in a git repo.
pub async fn get_git_remote_urls_assume_git_repo(cwd: &Path) -> Option<BTreeMap<String, String>> {
    let output = run_git_command(&["remote", "-v"], cwd).await?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_git_remote_urls(&stdout)
}

fn parse_git_remote_urls(stdout: &str) -> Option<BTreeMap<String, String>> {
    let mut remotes = BTreeMap::new();
    for line in stdout.lines() {
        let Some(fetch_line) = line.strip_suffix(" (fetch)") else {
            continue;
        };
        let Some((name, url_part)) = fetch_line
            .split_once('\t')
            .or_else(|| fetch_line.split_once(' '))
        else {
            continue;
        };
        let url = url_part.trim_start();
        if !url.is_empty() {
            remotes.insert(name.to_string(), url.to_string());
        }
    }
    if remotes.is_empty() {
        None
    } else {
        Some(remotes)
    }
}

// ── HEAD / status helpers ────────────────────────────────────────

/// Return the current HEAD commit hash.
pub async fn get_head_commit_hash(cwd: &Path) -> Option<String> {
    let output = run_git_command(&["rev-parse", "HEAD"], cwd).await?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Return whether the working tree has uncommitted changes.
pub async fn get_has_changes(cwd: &Path) -> Option<bool> {
    let output = run_git_command(&["status", "--porcelain"], cwd).await?;
    if !output.status.success() {
        return None;
    }
    Some(!output.stdout.is_empty())
}

// ── Branch helpers ───────────────────────────────────────────────

/// Returns the current checked-out branch name.
pub async fn current_branch_name(cwd: &Path) -> Option<String> {
    let out = run_git_command(&["branch", "--show-current"], cwd).await?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Returns a list of local git branches (default branch first).
pub async fn local_git_branches(cwd: &Path) -> Vec<String> {
    let mut branches: Vec<String> =
        if let Some(out) = run_git_command(&["branch", "--format=%(refname:short)"], cwd).await {
            if out.status.success() {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

    branches.sort_unstable();

    if let Some(base) = get_default_branch_local(cwd).await {
        if let Some(pos) = branches.iter().position(|name| name == &base) {
            let base_branch = branches.remove(pos);
            branches.insert(0, base_branch);
        }
    }

    branches
}

/// Determine the repository's default branch name.
pub async fn default_branch_name(cwd: &Path) -> Option<String> {
    get_default_branch(cwd).await
}

// ── Recent commits ───────────────────────────────────────────────

/// Return the last `limit` commits reachable from HEAD.
pub async fn recent_commits(cwd: &Path, limit: usize) -> Vec<CommitLogEntry> {
    let Some(out) = run_git_command(&["rev-parse", "--git-dir"], cwd).await else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    let fmt = "%H%x1f%ct%x1f%s";
    let limit_arg = (limit > 0).then(|| limit.to_string());
    let mut args: Vec<String> = vec!["log".to_string()];
    if let Some(n) = &limit_arg {
        args.push("-n".to_string());
        args.push(n.clone());
    }
    args.push(format!("--pretty=format:{fmt}"));
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some(log_out) = run_git_command(&arg_refs, cwd).await else {
        return Vec::new();
    };
    if !log_out.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&log_out.stdout);
    let mut entries = Vec::new();
    for line in text.lines() {
        let mut parts = line.split('\u{001f}');
        let sha = parts.next().unwrap_or("").trim();
        let ts_s = parts.next().unwrap_or("").trim();
        let subject = parts.next().unwrap_or("").trim();
        if sha.is_empty() || ts_s.is_empty() {
            continue;
        }
        let timestamp = ts_s.parse::<i64>().unwrap_or(0);
        entries.push(CommitLogEntry {
            sha: sha.to_string(),
            timestamp,
            subject: subject.to_string(),
        });
    }
    entries
}

// ── Diff to remote ───────────────────────────────────────────────

/// Returns the closest git SHA on a remote and the diff to that SHA.
pub async fn git_diff_to_remote(cwd: &Path) -> Option<GitDiffToRemote> {
    get_git_repo_root(cwd)?;

    let remotes = get_git_remotes(cwd).await?;
    let branches = branch_ancestry(cwd).await?;
    let base_sha = find_closest_sha(cwd, &branches, &remotes).await?;
    let diff = diff_against_sha(cwd, &base_sha).await?;

    Some(GitDiffToRemote {
        sha: base_sha,
        diff,
    })
}

// ── Internal helpers ─────────────────────────────────────────────

async fn run_git_command(args: &[&str], cwd: &Path) -> Option<std::process::Output> {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .current_dir(cwd)
        .kill_on_drop(true);
    match timeout(GIT_COMMAND_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => Some(output),
        _ => None,
    }
}

async fn get_git_remotes(cwd: &Path) -> Option<Vec<String>> {
    let output = run_git_command(&["remote"], cwd).await?;
    if !output.status.success() {
        return None;
    }
    let mut remotes: Vec<String> = String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .map(str::to_string)
        .collect();
    if let Some(pos) = remotes.iter().position(|r| r == "origin") {
        let origin = remotes.remove(pos);
        remotes.insert(0, origin);
    }
    Some(remotes)
}

async fn get_default_branch(cwd: &Path) -> Option<String> {
    let remotes = get_git_remotes(cwd).await.unwrap_or_default();
    for remote in &remotes {
        // Try symbolic-ref
        if let Some(symref_output) = run_git_command(
            &[
                "symbolic-ref",
                "--quiet",
                &format!("refs/remotes/{remote}/HEAD"),
            ],
            cwd,
        )
        .await
        {
            if symref_output.status.success() {
                if let Ok(sym) = String::from_utf8(symref_output.stdout) {
                    if let Some((_, name)) = sym.trim().rsplit_once('/') {
                        return Some(name.to_string());
                    }
                }
            }
        }

        // Fall back to `git remote show`
        if let Some(show_output) = run_git_command(&["remote", "show", remote.as_str()], cwd).await
        {
            if show_output.status.success() {
                if let Ok(text) = String::from_utf8(show_output.stdout) {
                    for line in text.lines() {
                        let line = line.trim();
                        if let Some(rest) = line.strip_prefix("HEAD branch:") {
                            let name = rest.trim();
                            if !name.is_empty() {
                                return Some(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    get_default_branch_local(cwd).await
}

async fn get_default_branch_local(cwd: &Path) -> Option<String> {
    for candidate in ["main", "master"] {
        if let Some(verify) = run_git_command(
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{candidate}"),
            ],
            cwd,
        )
        .await
        {
            if verify.status.success() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

async fn branch_ancestry(cwd: &Path) -> Option<Vec<String>> {
    let current_branch = run_git_command(&["rev-parse", "--abbrev-ref", "HEAD"], cwd)
        .await
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| s != "HEAD");

    let default_branch = get_default_branch(cwd).await;

    let mut ancestry: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(cb) = current_branch {
        seen.insert(cb.clone());
        ancestry.push(cb);
    }
    if let Some(db) = default_branch {
        if !seen.contains(&db) {
            seen.insert(db.clone());
            ancestry.push(db);
        }
    }

    let remotes = get_git_remotes(cwd).await.unwrap_or_default();
    for remote in &remotes {
        if let Some(output) = run_git_command(
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "--contains=HEAD",
                &format!("refs/remotes/{remote}"),
            ],
            cwd,
        )
        .await
        {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    for line in text.lines() {
                        let short = line.trim();
                        if let Some(stripped) = short.strip_prefix(&format!("{remote}/")) {
                            if !stripped.is_empty() && !seen.contains(stripped) {
                                seen.insert(stripped.to_string());
                                ancestry.push(stripped.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Some(ancestry)
}

async fn branch_remote_and_distance(
    cwd: &Path,
    branch: &str,
    remotes: &[String],
) -> Option<(Option<String>, usize)> {
    let mut found_remote_sha: Option<String> = None;
    let mut found_remote_ref: Option<String> = None;
    for remote in remotes {
        let remote_ref = format!("refs/remotes/{remote}/{branch}");
        let Some(verify_output) =
            run_git_command(&["rev-parse", "--verify", "--quiet", &remote_ref], cwd).await
        else {
            return None;
        };
        if !verify_output.status.success() {
            continue;
        }
        let Ok(sha) = String::from_utf8(verify_output.stdout) else {
            return None;
        };
        found_remote_sha = Some(sha.trim().to_string());
        found_remote_ref = Some(remote_ref);
        break;
    }

    let count_output = if let Some(local_count) =
        run_git_command(&["rev-list", "--count", &format!("{branch}..HEAD")], cwd).await
    {
        if local_count.status.success() {
            local_count
        } else if let Some(ref remote_ref) = found_remote_ref {
            run_git_command(
                &["rev-list", "--count", &format!("{remote_ref}..HEAD")],
                cwd,
            )
            .await?
        } else {
            return None;
        }
    } else if let Some(ref remote_ref) = found_remote_ref {
        run_git_command(
            &["rev-list", "--count", &format!("{remote_ref}..HEAD")],
            cwd,
        )
        .await?
    } else {
        return None;
    };

    if !count_output.status.success() {
        return None;
    }
    let distance_str = String::from_utf8(count_output.stdout).ok()?;
    let distance = distance_str.trim().parse::<usize>().ok()?;

    Some((found_remote_sha, distance))
}

async fn find_closest_sha(cwd: &Path, branches: &[String], remotes: &[String]) -> Option<String> {
    let mut closest: Option<(String, usize)> = None;
    for branch in branches {
        let Some((maybe_sha, distance)) = branch_remote_and_distance(cwd, branch, remotes).await
        else {
            continue;
        };
        let Some(remote_sha) = maybe_sha else {
            continue;
        };
        match &closest {
            None => closest = Some((remote_sha, distance)),
            Some((_, best)) if distance < *best => {
                closest = Some((remote_sha, distance));
            }
            _ => {}
        }
    }
    closest.map(|(sha, _)| sha)
}

async fn diff_against_sha(cwd: &Path, sha: &str) -> Option<String> {
    let output = run_git_command(&["diff", "--no-textconv", "--no-ext-diff", sha], cwd).await?;
    let exit_ok = output.status.code().is_some_and(|c| c == 0 || c == 1);
    if !exit_ok {
        return None;
    }
    let mut diff = String::from_utf8(output.stdout).ok()?;

    // Include untracked files in the diff
    if let Some(untracked_output) =
        run_git_command(&["ls-files", "--others", "--exclude-standard"], cwd).await
    {
        if untracked_output.status.success() {
            let untracked: Vec<String> = String::from_utf8(untracked_output.stdout)
                .ok()?
                .lines()
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .collect();

            let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
            let futures_iter = untracked.into_iter().map(|file| async move {
                run_git_command(
                    &[
                        "diff",
                        "--no-textconv",
                        "--no-ext-diff",
                        "--binary",
                        "--no-index",
                        "--",
                        null_device,
                        &file,
                    ],
                    cwd,
                )
                .await
            });
            let results = futures::future::join_all(futures_iter).await;
            for extra in results.into_iter().flatten() {
                if extra.status.code().is_some_and(|c| c == 0 || c == 1) {
                    if let Ok(s) = String::from_utf8(extra.stdout) {
                        diff.push_str(&s);
                    }
                }
            }
        }
    }

    Some(diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    async fn create_test_git_repo(temp_dir: &TempDir) -> PathBuf {
        let repo_path = temp_dir.path().join("repo");
        fs::create_dir(&repo_path).expect("create repo dir");
        let envs = vec![
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ];

        Command::new("git")
            .envs(envs.clone())
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .await
            .expect("git init");

        Command::new("git")
            .envs(envs.clone())
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .await
            .expect("set user.name");

        Command::new("git")
            .envs(envs.clone())
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .await
            .expect("set user.email");

        fs::write(repo_path.join("test.txt"), "test content").expect("write test file");

        Command::new("git")
            .envs(envs.clone())
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .await
            .expect("git add");

        Command::new("git")
            .envs(envs)
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&repo_path)
            .output()
            .await
            .expect("git commit");

        repo_path
    }

    #[test]
    fn test_get_git_repo_root_finds_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo = temp_dir.path().join("myrepo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let nested = repo.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(get_git_repo_root(&nested), Some(repo.clone()));
        assert_eq!(get_git_repo_root(&repo), Some(repo));
    }

    #[test]
    fn test_get_git_repo_root_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        assert!(get_git_repo_root(temp_dir.path()).is_none());
    }

    #[tokio::test]
    async fn test_collect_git_info_non_git_directory() {
        let temp_dir = TempDir::new().unwrap();
        assert!(collect_git_info(temp_dir.path()).await.is_none());
    }

    #[tokio::test]
    async fn test_collect_git_info_git_repository() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir).await;

        let info = collect_git_info(&repo_path).await.expect("should collect");
        assert!(info.commit_hash.is_some());
        let hash = info.commit_hash.unwrap();
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(info.branch.is_some());
    }

    #[tokio::test]
    async fn test_get_has_changes_clean() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir).await;
        assert_eq!(get_has_changes(&repo_path).await, Some(false));
    }

    #[tokio::test]
    async fn test_get_has_changes_dirty() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir).await;
        fs::write(repo_path.join("test.txt"), "changed").unwrap();
        assert_eq!(get_has_changes(&repo_path).await, Some(true));
    }

    #[tokio::test]
    async fn test_recent_commits_non_git() {
        let temp_dir = TempDir::new().unwrap();
        assert!(recent_commits(temp_dir.path(), 10).await.is_empty());
    }

    #[test]
    fn test_parse_git_remote_urls() {
        let input = "origin\thttps://github.com/user/repo.git (fetch)\norigin\thttps://github.com/user/repo.git (push)\n";
        let result = parse_git_remote_urls(input).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("origin").unwrap(),
            "https://github.com/user/repo.git"
        );
    }

    #[test]
    fn test_git_info_serialization() {
        let info = GitInfo {
            commit_hash: Some("abc123".to_string()),
            branch: Some("main".to_string()),
            repository_url: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["commit_hash"], "abc123");
        assert_eq!(parsed["branch"], "main");
        assert!(!parsed.as_object().unwrap().contains_key("repository_url"));
    }
}

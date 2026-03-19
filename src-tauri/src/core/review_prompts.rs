use crate::protocol::types::{ReviewRequest, ReviewTarget};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedReviewRequest {
    pub target: ReviewTarget,
    pub prompt: String,
    pub user_facing_hint: String,
}

const UNCOMMITTED_PROMPT: &str = "Review the current code changes (staged, unstaged, and untracked files) and provide prioritized findings.";

const BASE_BRANCH_PROMPT_BACKUP: &str = "Review the code changes against the base branch '{branch}'. Start by finding the merge diff between the current branch and {branch}'s upstream e.g. (`git merge-base HEAD \"$(git rev-parse --abbrev-ref \"{branch}@{upstream}\")\"`), then run `git diff` against that SHA to see what changes we would merge into the {branch} branch. Provide prioritized, actionable findings.";
const BASE_BRANCH_PROMPT: &str = "Review the code changes against the base branch '{baseBranch}'. The merge base commit for this comparison is {mergeBaseSha}. Run `git diff {mergeBaseSha}` to inspect the changes relative to {baseBranch}. Provide prioritized, actionable findings.";

const COMMIT_PROMPT_WITH_TITLE: &str = "Review the code changes introduced by commit {sha} (\"{title}\"). Provide prioritized, actionable findings.";
const COMMIT_PROMPT: &str =
    "Review the code changes introduced by commit {sha}. Provide prioritized, actionable findings.";

/// Resolve a review request into a prompt and user-facing hint.
///
/// This is an async function because `BaseBranch` resolution requires
/// running `git merge-base` via a subprocess.
pub async fn resolve_review_request(
    request: ReviewRequest,
    cwd: &Path,
) -> anyhow::Result<ResolvedReviewRequest> {
    let target = request.target;
    let prompt = review_prompt(&target, cwd).await?;
    let user_facing_hint = request
        .user_facing_hint
        .unwrap_or_else(|| user_facing_hint(&target));

    Ok(ResolvedReviewRequest {
        target,
        prompt,
        user_facing_hint,
    })
}

/// Generate the review prompt text for a given target.
pub async fn review_prompt(target: &ReviewTarget, cwd: &Path) -> anyhow::Result<String> {
    match target {
        ReviewTarget::UncommittedChanges => Ok(UNCOMMITTED_PROMPT.to_string()),
        ReviewTarget::BaseBranch { branch } => {
            if let Some(commit) = merge_base_with_head(cwd, branch).await? {
                Ok(BASE_BRANCH_PROMPT
                    .replace("{baseBranch}", branch)
                    .replace("{mergeBaseSha}", &commit))
            } else {
                Ok(BASE_BRANCH_PROMPT_BACKUP.replace("{branch}", branch))
            }
        }
        ReviewTarget::Commit { sha, title } => {
            if let Some(title) = title {
                Ok(COMMIT_PROMPT_WITH_TITLE
                    .replace("{sha}", sha)
                    .replace("{title}", title))
            } else {
                Ok(COMMIT_PROMPT.replace("{sha}", sha))
            }
        }
        ReviewTarget::Custom { instructions } => {
            let prompt = instructions.trim();
            if prompt.is_empty() {
                anyhow::bail!("Review prompt cannot be empty");
            }
            Ok(prompt.to_string())
        }
    }
}

/// Generate a short user-facing hint describing the review target.
pub fn user_facing_hint(target: &ReviewTarget) -> String {
    match target {
        ReviewTarget::UncommittedChanges => "current changes".to_string(),
        ReviewTarget::BaseBranch { branch } => format!("changes against '{branch}'"),
        ReviewTarget::Commit { sha, title } => {
            let short_sha: String = sha.chars().take(7).collect();
            if let Some(title) = title {
                format!("commit {short_sha}: {title}")
            } else {
                format!("commit {short_sha}")
            }
        }
        ReviewTarget::Custom { instructions } => instructions.trim().to_string(),
    }
}

impl From<ResolvedReviewRequest> for ReviewRequest {
    fn from(resolved: ResolvedReviewRequest) -> Self {
        ReviewRequest {
            target: resolved.target,
            user_facing_hint: Some(resolved.user_facing_hint),
        }
    }
}

/// Run `git merge-base HEAD <branch>` and return the merge-base SHA.
/// Returns `Ok(None)` if the branch or HEAD cannot be resolved.
async fn merge_base_with_head(cwd: &Path, branch: &str) -> anyhow::Result<Option<String>> {
    use tokio::process::Command;

    // Verify HEAD exists
    let head_out = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .await?;
    if !head_out.status.success() {
        return Ok(None);
    }

    // Try merge-base with local branch first, then remote
    let result = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["merge-base", "HEAD", branch])
        .current_dir(cwd)
        .output()
        .await?;

    if result.status.success() {
        let sha = String::from_utf8(result.stdout)?.trim().to_string();
        if !sha.is_empty() {
            return Ok(Some(sha));
        }
    }

    // Try with upstream tracking ref
    let upstream_out = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "rev-parse",
            "--abbrev-ref",
            &format!("{branch}@{{upstream}}"),
        ])
        .current_dir(cwd)
        .output()
        .await?;

    if upstream_out.status.success() {
        let upstream = String::from_utf8(upstream_out.stdout)?.trim().to_string();
        if !upstream.is_empty() {
            let result = Command::new("git")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .args(["merge-base", "HEAD", &upstream])
                .current_dir(cwd)
                .output()
                .await?;
            if result.status.success() {
                let sha = String::from_utf8(result.stdout)?.trim().to_string();
                if !sha.is_empty() {
                    return Ok(Some(sha));
                }
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::process::Command;

    async fn init_git_repo(dir: &Path) {
        let envs = [
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ];
        Command::new("git")
            .envs(envs)
            .args(["init", "-b", "main"])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .envs(envs)
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .envs(envs)
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        std::fs::write(dir.join("f.txt"), "init").unwrap();
        Command::new("git")
            .envs(envs)
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .envs(envs)
            .args(["commit", "-m", "init"])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
    }

    #[test]
    fn uncommitted_prompt() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let p = review_prompt(&ReviewTarget::UncommittedChanges, tmp.path()).await.unwrap();
            assert_eq!(p, UNCOMMITTED_PROMPT);
        });
    }

    #[test]
    fn commit_prompt_with_title() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let target = ReviewTarget::Commit {
                sha: "abc1234567".to_string(),
                title: Some("Fix bug".to_string()),
            };
            let p = review_prompt(&target, tmp.path()).await.unwrap();
            assert!(p.contains("abc1234567"));
            assert!(p.contains("Fix bug"));
        });
    }

    #[test]
    fn commit_prompt_without_title() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let target = ReviewTarget::Commit {
                sha: "abc1234567".to_string(),
                title: None,
            };
            let p = review_prompt(&target, tmp.path()).await.unwrap();
            assert!(p.contains("abc1234567"));
            assert!(!p.contains("\"\""));
        });
    }

    #[test]
    fn custom_prompt() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let target = ReviewTarget::Custom {
                instructions: "Check security".to_string(),
            };
            let p = review_prompt(&target, tmp.path()).await.unwrap();
            assert_eq!(p, "Check security");
        });
    }

    #[test]
    fn custom_prompt_empty_fails() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let target = ReviewTarget::Custom {
                instructions: "   ".to_string(),
            };
            assert!(review_prompt(&target, tmp.path()).await.is_err());
        });
    }

    #[test]
    fn user_facing_hint_uncommitted() {
        assert_eq!(
            user_facing_hint(&ReviewTarget::UncommittedChanges),
            "current changes"
        );
    }

    #[test]
    fn user_facing_hint_base_branch() {
        assert_eq!(
            user_facing_hint(&ReviewTarget::BaseBranch {
                branch: "main".to_string()
            }),
            "changes against 'main'"
        );
    }

    #[test]
    fn user_facing_hint_commit_with_title() {
        let hint = user_facing_hint(&ReviewTarget::Commit {
            sha: "abcdef1234567890".to_string(),
            title: Some("Fix".to_string()),
        });
        assert_eq!(hint, "commit abcdef1: Fix");
    }

    #[test]
    fn user_facing_hint_commit_without_title() {
        let hint = user_facing_hint(&ReviewTarget::Commit {
            sha: "abcdef1234567890".to_string(),
            title: None,
        });
        assert_eq!(hint, "commit abcdef1");
    }

    #[test]
    fn user_facing_hint_custom() {
        assert_eq!(
            user_facing_hint(&ReviewTarget::Custom {
                instructions: "  My review  ".to_string()
            }),
            "My review"
        );
    }

    #[tokio::test]
    async fn base_branch_prompt_with_real_repo() {
        let tmp = TempDir::new().unwrap();
        init_git_repo(tmp.path()).await;

        let target = ReviewTarget::BaseBranch {
            branch: "main".to_string(),
        };
        let p = review_prompt(&target, tmp.path()).await.unwrap();
        // Should resolve merge-base since we're on main
        assert!(
            p.contains("merge base commit") || p.contains("merge diff"),
            "prompt should contain merge-base info or fallback: {p}"
        );
    }

    #[tokio::test]
    async fn base_branch_prompt_missing_branch_uses_fallback() {
        let tmp = TempDir::new().unwrap();
        init_git_repo(tmp.path()).await;

        let target = ReviewTarget::BaseBranch {
            branch: "nonexistent-branch".to_string(),
        };
        let p = review_prompt(&target, tmp.path()).await.unwrap();
        assert!(p.contains("nonexistent-branch"));
        // Should use fallback template
        assert!(p.contains("merge diff"));
    }

    #[tokio::test]
    async fn resolve_review_request_custom() {
        let tmp = TempDir::new().unwrap();
        let req = ReviewRequest {
            target: ReviewTarget::Custom {
                instructions: "Check it".to_string(),
            },
            user_facing_hint: None,
        };
        let resolved = resolve_review_request(req, tmp.path()).await.unwrap();
        assert_eq!(resolved.prompt, "Check it");
        assert_eq!(resolved.user_facing_hint, "Check it");
    }

    #[tokio::test]
    async fn resolve_review_request_with_custom_hint() {
        let tmp = TempDir::new().unwrap();
        let req = ReviewRequest {
            target: ReviewTarget::UncommittedChanges,
            user_facing_hint: Some("my hint".to_string()),
        };
        let resolved = resolve_review_request(req, tmp.path()).await.unwrap();
        assert_eq!(resolved.user_facing_hint, "my hint");
    }

    #[test]
    fn resolved_into_review_request() {
        let resolved = ResolvedReviewRequest {
            target: ReviewTarget::UncommittedChanges,
            prompt: "p".to_string(),
            user_facing_hint: "h".to_string(),
        };
        let req: ReviewRequest = resolved.into();
        assert_eq!(req.target, ReviewTarget::UncommittedChanges);
        assert_eq!(req.user_facing_hint, Some("h".to_string()));
    }
}

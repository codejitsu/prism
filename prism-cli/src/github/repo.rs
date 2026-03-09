use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};

/// Identifies a GitHub repository by owner and name.
#[derive(Debug, Clone, PartialEq)]
pub struct RepoInfo {
    pub owner: String,
    pub repo: String,
}

/// Detect the GitHub owner/repo from the current directory's git remote origin.
///
/// Supports both HTTPS and SSH remote URL formats:
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo`
/// - `git@github.com:owner/repo.git`
/// - `git@github.com:owner/repo`
pub fn detect_repo() -> Result<RepoInfo> {
    let output = ProcessCommand::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .context("Failed to run 'git remote get-url origin'. Is git installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to get git remote origin URL. Are you in a git repository?\n{}",
            stderr.trim()
        );
    }

    let url = String::from_utf8(output.stdout)
        .context("Git remote URL is not valid UTF-8")?
        .trim()
        .to_string();

    parse_github_remote(&url)
}

/// Parse a GitHub remote URL into owner and repo components.
fn parse_github_remote(url: &str) -> Result<RepoInfo> {
    // SSH format: git@github.com:owner/repo.git
    if let Some(path) = url.strip_prefix("git@github.com:") {
        return parse_owner_repo_path(path);
    }

    // HTTPS format: https://github.com/owner/repo.git
    if let Some(rest) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    {
        return parse_owner_repo_path(rest);
    }

    bail!(
        "Remote origin URL is not a GitHub URL: '{}'. \
         Expected https://github.com/owner/repo or git@github.com:owner/repo",
        url
    );
}

/// Extract owner and repo from a path like `owner/repo.git` or `owner/repo`.
///
/// Rejects paths with extra segments beyond `owner/repo`.
fn parse_owner_repo_path(path: &str) -> Result<RepoInfo> {
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("Could not parse owner/repo from path: '{}'", path);
    }

    Ok(RepoInfo {
        owner: parts[0].to_string(),
        repo: parts[1].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_https_url() {
        let result = parse_github_remote("https://github.com/octocat/hello-world.git").unwrap();
        assert_eq!(
            result,
            RepoInfo {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_https_url_without_dot_git() {
        let result = parse_github_remote("https://github.com/octocat/hello-world").unwrap();
        assert_eq!(
            result,
            RepoInfo {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_ssh_url() {
        let result = parse_github_remote("git@github.com:octocat/hello-world.git").unwrap();
        assert_eq!(
            result,
            RepoInfo {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_ssh_url_without_dot_git() {
        let result = parse_github_remote("git@github.com:octocat/hello-world").unwrap();
        assert_eq!(
            result,
            RepoInfo {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_non_github_url_fails() {
        let result = parse_github_remote("https://gitlab.com/octocat/hello-world.git");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_path_fails() {
        let result = parse_github_remote("https://github.com/");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_http_url() {
        let result = parse_github_remote("http://github.com/octocat/hello-world.git").unwrap();
        assert_eq!(
            result,
            RepoInfo {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_url_with_extra_path_segments_fails() {
        let result =
            parse_github_remote("https://github.com/octocat/hello-world/extra/segments.git");
        assert!(result.is_err());
    }
}

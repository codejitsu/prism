use anyhow::{Result, bail};

use crate::ai::{AiReviewResult, ReviewContext, ReviewFileContext, analyze_review_context};
use crate::config::Config;
use crate::github::client::GitHubClient;
use crate::github::repo;
use crate::github::types::{CommitFile, PullRequestFile};

/// Options controlling review output and AI analysis.
pub struct ReviewOptions<'a> {
    /// Whether to run AI-powered analysis.
    pub enable_ai: bool,
    /// Override model for AI analysis (CLI `--model` flag).
    pub model_override: Option<&'a str>,
    /// Whether to print detailed PR/commit metadata and diffs.
    pub verbose: bool,
    /// Application configuration (tokens, default model, etc.).
    pub config: &'a Config,
}

/// The parsed target of a `prism review` invocation.
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewTarget {
    /// A PR number in the current repo (detected from git remote).
    PullRequest { pr_number: u64 },
    /// A full GitHub PR URL -- owner/repo extracted from the URL.
    PullRequestUrl {
        owner: String,
        repo: String,
        pr_number: u64,
    },
    /// A commit hash -- reviewed as a standalone commit diff.
    Commit { hash: String },
}

impl ReviewTarget {
    /// Parse a user-supplied string into a `ReviewTarget`.
    ///
    /// When `force_commit` is true, the input is interpreted as a commit SHA
    /// regardless of whether it looks like a number. When `force_pr` is true,
    /// the input is interpreted as a PR number. When neither flag is set, the
    /// auto-detection order applies:
    ///
    /// 1. URL (starts with `http://` or `https://`) -- must be a GitHub PR URL.
    /// 2. PR number -- parses as `u64` and is >= 1.
    /// 3. Commit hash -- 7-40 hex characters.
    /// 4. Otherwise -- error.
    pub fn parse(input: &str, force_commit: bool, force_pr: bool) -> Result<Self> {
        let input = input.trim();

        // Forced commit interpretation
        if force_commit {
            return Self::parse_as_commit(input);
        }

        // Forced PR interpretation
        if force_pr {
            return Self::parse_as_pr_number(input);
        }

        // 1. URL
        if input.starts_with("http://") || input.starts_with("https://") {
            return Self::parse_url(input);
        }

        // 2. PR number
        if let Ok(n) = input.parse::<u64>() {
            if n == 0 {
                bail!("Invalid PR number: 0. PR numbers start at 1.");
            }
            return Ok(ReviewTarget::PullRequest { pr_number: n });
        }

        // 3. Commit hash (7-40 hex characters)
        if input.len() >= 7 && input.len() <= 40 && input.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(ReviewTarget::Commit {
                hash: input.to_string(),
            });
        }

        bail!(
            "Could not interpret '{}' as a PR number, GitHub PR URL, or commit hash.\n\
             Expected one of:\n  \
               - A PR number (e.g. 42)\n  \
               - A GitHub PR URL (e.g. https://github.com/owner/repo/pull/42)\n  \
               - A commit SHA (e.g. a1b2c3d)",
            input
        );
    }

    /// Parse the input as a commit SHA, validating it is 7-40 hex characters.
    fn parse_as_commit(input: &str) -> Result<Self> {
        if input.len() >= 7 && input.len() <= 40 && input.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(ReviewTarget::Commit {
                hash: input.to_string(),
            })
        } else {
            bail!(
                "Invalid commit SHA: '{}'. Expected 7-40 hexadecimal characters.",
                input
            )
        }
    }

    /// Parse the input as a PR number, validating it is a positive integer.
    fn parse_as_pr_number(input: &str) -> Result<Self> {
        let n: u64 = input.parse().map_err(|_| {
            anyhow::anyhow!(
                "Invalid PR number: '{}'. Expected a positive integer.",
                input
            )
        })?;
        if n == 0 {
            bail!("Invalid PR number: 0. PR numbers start at 1.");
        }
        Ok(ReviewTarget::PullRequest { pr_number: n })
    }

    /// Parse a GitHub PR URL like `https://github.com/owner/repo/pull/42`.
    fn parse_url(url: &str) -> Result<Self> {
        // Strip the scheme and host prefix
        let path = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"));

        let path = match path {
            Some(p) => p,
            None => bail!(
                "URL is not a GitHub URL: '{}'. Expected https://github.com/owner/repo/pull/NUMBER",
                url
            ),
        };

        // Expected path: owner/repo/pull/NUMBER
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 4 || parts[2] != "pull" {
            bail!(
                "URL is not a GitHub pull request URL: '{}'. Expected https://github.com/owner/repo/pull/NUMBER",
                url
            );
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();

        if owner.is_empty() || repo.is_empty() {
            bail!(
                "URL is missing owner or repo: '{}'. Expected https://github.com/owner/repo/pull/NUMBER",
                url
            );
        }

        // Strip query string or fragment from the PR number segment
        // (e.g. "42?foo=bar" -> "42", "42#discussion" -> "42").
        let pr_segment = parts[3].split(&['?', '#'][..]).next().unwrap_or(parts[3]);

        let pr_number: u64 = pr_segment.parse().map_err(|_| {
            anyhow::anyhow!(
                "Invalid PR number in URL: '{}'. Expected a positive integer.",
                pr_segment
            )
        })?;

        if pr_number == 0 {
            bail!("Invalid PR number: 0. PR numbers start at 1.");
        }

        Ok(ReviewTarget::PullRequestUrl {
            owner,
            repo,
            pr_number,
        })
    }
}

/// Run the review for the given target string.
///
/// `force_commit` and `force_pr` correspond to the `--commit` / `--pr` CLI
/// flags and are used to disambiguate inputs that could be either a PR number
/// or a commit SHA (e.g. all-digit hex strings like "1234567").
pub async fn review(
    target: &str,
    force_commit: bool,
    force_pr: bool,
    options: ReviewOptions<'_>,
) -> Result<()> {
    let review_target = ReviewTarget::parse(target, force_commit, force_pr)?;

    let github_token = options.config.github_token().ok_or_else(|| {
        anyhow::anyhow!(
            "GitHub token is required. Set GITHUB_TOKEN environment variable or add it to ~/.config/prism/config.toml"
        )
    })?;
    let client = GitHubClient::new(github_token)?;

    match review_target {
        ReviewTarget::PullRequest { pr_number } => {
            let repo_info = repo::detect_repo()?;
            review_pull_request(
                &client,
                &repo_info.owner,
                &repo_info.repo,
                pr_number,
                &options,
            )
            .await
        }
        ReviewTarget::PullRequestUrl {
            owner,
            repo,
            pr_number,
        } => review_pull_request(&client, &owner, &repo, pr_number, &options).await,
        ReviewTarget::Commit { hash } => {
            let repo_info = repo::detect_repo()?;
            review_commit(&client, &repo_info.owner, &repo_info.repo, &hash, &options).await
        }
    }
}

/// Fetch and display a pull request review.
async fn review_pull_request(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    pr_number: u64,
    options: &ReviewOptions<'_>,
) -> Result<()> {
    log::info!("Fetching PR #{} from {}/{}...", pr_number, owner, repo);

    let pr = client.fetch_pull_request(owner, repo, pr_number).await?;
    let files = client
        .fetch_pull_request_files(owner, repo, pr_number)
        .await?;

    if options.verbose {
        println!();
        println!("PR #{}: {}", pr.number, pr.title);
        println!("Author: {}", pr.user.login);
        println!("State:  {}", pr.state);
        println!("Base:   {} <- {}", pr.base.ref_name, pr.head.ref_name);

        if let Some(body) = &pr.body {
            let body = body.trim();
            if !body.is_empty() {
                println!();
                println!("Description:");
                for line in body.lines() {
                    println!("  {}", line);
                }
            }
        }

        println!();
        println!(
            "Files changed ({}): +{} -{}",
            pr.changed_files, pr.additions, pr.deletions
        );

        print_file_list_pr(&files);

        if files.iter().any(|f| f.patch.is_some()) {
            println!();
            println!("--- Diff ---");
            for file in &files {
                if let Some(patch) = &file.patch {
                    println!();
                    println!("diff --git a/{} b/{}", file.filename, file.filename);
                    println!("{}", patch);
                }
            }
        }
    }

    if options.enable_ai {
        let context = build_pr_ai_context(
            owner,
            repo,
            &pr.title,
            pr.body.as_deref(),
            &files,
            pr_number,
        );
        match analyze_review_context(
            &context,
            options.model_override,
            options.config.default_model(),
            options.config.openai_api_key().as_deref(),
        )
        .await
        {
            Ok(result) => print_ai_sections(&result),
            Err(err) => {
                println!();
                println!("AI analysis unavailable: {}", err);
            }
        }
    }

    Ok(())
}

/// Fetch and display a commit review.
async fn review_commit(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    sha: &str,
    options: &ReviewOptions<'_>,
) -> Result<()> {
    log::info!("Fetching commit {} from {}/{}...", sha, owner, repo);

    let commit = client.fetch_commit(owner, repo, sha).await?;

    if options.verbose {
        let total_additions: u64 = commit.files.iter().map(|f| f.additions).sum();
        let total_deletions: u64 = commit.files.iter().map(|f| f.deletions).sum();

        println!();
        println!("Commit: {}", commit.sha);
        println!("Author: {}", commit.commit.author.name);
        if let Some(date) = &commit.commit.author.date {
            println!("Date:   {}", date);
        }

        let message = commit.commit.message.trim();
        if !message.is_empty() {
            println!();
            println!("Message:");
            for line in message.lines() {
                println!("  {}", line);
            }
        }

        println!();
        println!(
            "Files changed ({}): +{} -{}",
            commit.files.len(),
            total_additions,
            total_deletions
        );

        print_file_list_commit(&commit.files);

        if commit.files.iter().any(|f| f.patch.is_some()) {
            println!();
            println!("--- Diff ---");
            for file in &commit.files {
                if let Some(patch) = &file.patch {
                    println!();
                    println!("diff --git a/{} b/{}", file.filename, file.filename);
                    println!("{}", patch);
                }
            }
        }
    }

    if options.enable_ai {
        let context = build_commit_ai_context(
            owner,
            repo,
            &commit.commit.message,
            &commit.files,
            &commit.sha,
        );
        match analyze_review_context(
            &context,
            options.model_override,
            options.config.default_model(),
            options.config.openai_api_key().as_deref(),
        )
        .await
        {
            Ok(result) => print_ai_sections(&result),
            Err(err) => {
                println!();
                println!("AI analysis unavailable: {}", err);
            }
        }
    }

    Ok(())
}

fn build_pr_ai_context(
    owner: &str,
    repo: &str,
    title: &str,
    body: Option<&str>,
    files: &[PullRequestFile],
    pr_number: u64,
) -> ReviewContext {
    ReviewContext {
        target_label: format!("pull_request#{}", pr_number),
        owner: owner.to_string(),
        repo: repo.to_string(),
        title_or_message: title.to_string(),
        body: body.map(ToString::to_string),
        files: files
            .iter()
            .map(|file| ReviewFileContext {
                filename: file.filename.clone(),
                status: file.status.clone(),
                additions: file.additions,
                deletions: file.deletions,
                patch: file.patch.clone(),
            })
            .collect(),
    }
}

fn build_commit_ai_context(
    owner: &str,
    repo: &str,
    message: &str,
    files: &[CommitFile],
    sha: &str,
) -> ReviewContext {
    ReviewContext {
        target_label: format!("commit:{}", sha),
        owner: owner.to_string(),
        repo: repo.to_string(),
        title_or_message: message.to_string(),
        body: None,
        files: files
            .iter()
            .map(|file| ReviewFileContext {
                filename: file.filename.clone(),
                status: file.status.clone(),
                additions: file.additions,
                deletions: file.deletions,
                patch: file.patch.clone(),
            })
            .collect(),
    }
}

fn print_ai_sections(result: &AiReviewResult) {
    println!();
    println!("=== AI Summary ===");
    println!("{}", result.summary.overview);
    for item in &result.summary.key_changes {
        println!("- {}", item);
    }

    println!();
    println!("=== Top 5 Potential Regressions ===");
    for (index, finding) in result.regressions.findings.iter().enumerate() {
        println!("{}. {} [{}]", index + 1, finding.title, finding.severity);
        println!("   Why: {}", finding.rationale);
        if !finding.affected_files.is_empty() {
            println!("   Files: {}", finding.affected_files.join(", "));
        }
        println!("   Check: {}", finding.suggested_check);
    }

    println!();
    println!("=== Production Readiness ===");
    println!(
        "Verdict: {} (score: {})",
        result.prod_readiness.verdict, result.prod_readiness.readiness_score
    );
    print_labeled_list(
        "Logging/Observability",
        &result.prod_readiness.logging_and_observability,
    );
    print_labeled_list("Scalability", &result.prod_readiness.scalability);
    print_labeled_list("Edge Cases", &result.prod_readiness.edge_cases);
    print_labeled_list("Blocking Issues", &result.prod_readiness.blocking_issues);
}

fn print_labeled_list(label: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }

    println!("{}:", label);
    for value in values {
        println!("- {}", value);
    }
}

/// Print the file summary list for PR files.
fn print_file_list_pr(files: &[PullRequestFile]) {
    for file in files {
        let status_char = status_to_char(&file.status);
        println!(
            "  {} {:<40} (+{} -{})",
            status_char, file.filename, file.additions, file.deletions
        );
    }
}

/// Print the file summary list for commit files.
fn print_file_list_commit(files: &[CommitFile]) {
    for file in files {
        let status_char = status_to_char(&file.status);
        println!(
            "  {} {:<40} (+{} -{})",
            status_char, file.filename, file.additions, file.deletions
        );
    }
}

/// Map GitHub file status strings to single-character indicators.
fn status_to_char(status: &str) -> char {
    match status {
        "added" => 'A',
        "removed" => 'D',
        "modified" => 'M',
        "renamed" => 'R',
        "copied" => 'C',
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ReviewTarget::parse tests (auto-detect, no flags) ---

    #[test]
    fn test_parse_pr_number() {
        let target = ReviewTarget::parse("42", false, false).unwrap();
        assert_eq!(target, ReviewTarget::PullRequest { pr_number: 42 });
    }

    #[test]
    fn test_parse_pr_number_one() {
        let target = ReviewTarget::parse("1", false, false).unwrap();
        assert_eq!(target, ReviewTarget::PullRequest { pr_number: 1 });
    }

    #[test]
    fn test_parse_pr_number_zero_fails() {
        let result = ReviewTarget::parse("0", false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("PR numbers start at 1"),
            "Expected error about PR numbers, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_github_pr_url() {
        let target = ReviewTarget::parse(
            "https://github.com/octocat/hello-world/pull/99",
            false,
            false,
        )
        .unwrap();
        assert_eq!(
            target,
            ReviewTarget::PullRequestUrl {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
                pr_number: 99,
            }
        );
    }

    #[test]
    fn test_parse_github_pr_url_with_zero_fails() {
        let result = ReviewTarget::parse(
            "https://github.com/octocat/hello-world/pull/0",
            false,
            false,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("PR numbers start at 1"),
            "Expected error about PR numbers, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_github_issue_url_fails() {
        let result = ReviewTarget::parse(
            "https://github.com/octocat/hello-world/issues/10",
            false,
            false,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not a GitHub pull request URL"),
            "Expected error about PR URL, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_non_github_url_fails() {
        let result = ReviewTarget::parse("https://gitlab.com/foo/bar/pull/1", false, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_url_empty_owner_fails() {
        let result = ReviewTarget::parse("https://github.com//repo/pull/1", false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing owner or repo"),
            "Expected error about missing owner/repo, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_url_empty_repo_fails() {
        let result = ReviewTarget::parse("https://github.com/owner//pull/1", false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing owner or repo"),
            "Expected error about missing owner/repo, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_url_with_query_string() {
        let target = ReviewTarget::parse(
            "https://github.com/octocat/hello-world/pull/42?diff=split",
            false,
            false,
        )
        .unwrap();
        assert_eq!(
            target,
            ReviewTarget::PullRequestUrl {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
                pr_number: 42,
            }
        );
    }

    #[test]
    fn test_parse_url_with_fragment() {
        let target = ReviewTarget::parse(
            "https://github.com/octocat/hello-world/pull/42#discussion_r123",
            false,
            false,
        )
        .unwrap();
        assert_eq!(
            target,
            ReviewTarget::PullRequestUrl {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
                pr_number: 42,
            }
        );
    }

    #[test]
    fn test_parse_url_with_query_and_fragment() {
        let target = ReviewTarget::parse(
            "https://github.com/octocat/hello-world/pull/42?diff=split#discussion_r123",
            false,
            false,
        )
        .unwrap();
        assert_eq!(
            target,
            ReviewTarget::PullRequestUrl {
                owner: "octocat".to_string(),
                repo: "hello-world".to_string(),
                pr_number: 42,
            }
        );
    }

    #[test]
    fn test_parse_short_commit_hash() {
        let target = ReviewTarget::parse("a1b2c3d", false, false).unwrap();
        assert_eq!(
            target,
            ReviewTarget::Commit {
                hash: "a1b2c3d".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_full_commit_hash() {
        let hash = "abc123ef01234567890abcdef01234567890abcd";
        let target = ReviewTarget::parse(hash, false, false).unwrap();
        assert_eq!(
            target,
            ReviewTarget::Commit {
                hash: hash.to_string(),
            }
        );
    }

    #[test]
    fn test_parse_too_short_hex_fails() {
        // 6 hex chars -- too short for a commit hash, and not a valid u64 PR number
        let result = ReviewTarget::parse("abcdef", false, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_non_hex_string_fails() {
        let result = ReviewTarget::parse("not-valid", false, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_string_fails() {
        let result = ReviewTarget::parse("", false, false);
        assert!(result.is_err());
    }

    // --- Forced --commit flag tests ---

    #[test]
    fn test_parse_forced_commit_all_digits() {
        // All-digit string that would normally be a PR number
        let target = ReviewTarget::parse("1234567", true, false).unwrap();
        assert_eq!(
            target,
            ReviewTarget::Commit {
                hash: "1234567".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_forced_commit_mixed_hex() {
        let target = ReviewTarget::parse("a1b2c3d", true, false).unwrap();
        assert_eq!(
            target,
            ReviewTarget::Commit {
                hash: "a1b2c3d".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_forced_commit_invalid_non_hex() {
        let result = ReviewTarget::parse("not-hex", true, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid commit SHA"),
            "Expected error about invalid commit SHA, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_forced_commit_too_short() {
        let result = ReviewTarget::parse("abc", true, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid commit SHA"),
            "Expected error about invalid commit SHA, got: {}",
            err
        );
    }

    // --- Forced --pr flag tests ---

    #[test]
    fn test_parse_forced_pr_all_digits() {
        let target = ReviewTarget::parse("1234567", false, true).unwrap();
        assert_eq!(target, ReviewTarget::PullRequest { pr_number: 1234567 });
    }

    #[test]
    fn test_parse_forced_pr_invalid_non_numeric() {
        let result = ReviewTarget::parse("abc", false, true);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid PR number"),
            "Expected error about invalid PR number, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_forced_pr_zero() {
        let result = ReviewTarget::parse("0", false, true);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("PR numbers start at 1"),
            "Expected error about PR numbers, got: {}",
            err
        );
    }

    // --- status_to_char tests ---

    #[test]
    fn test_status_to_char_known() {
        assert_eq!(status_to_char("added"), 'A');
        assert_eq!(status_to_char("removed"), 'D');
        assert_eq!(status_to_char("modified"), 'M');
        assert_eq!(status_to_char("renamed"), 'R');
        assert_eq!(status_to_char("copied"), 'C');
    }

    #[test]
    fn test_status_to_char_unknown() {
        assert_eq!(status_to_char("something_else"), '?');
    }
}

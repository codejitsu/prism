use std::env;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};

use super::types::{CommitResponse, PullRequest, PullRequestFile};

/// HTTP client for the GitHub REST API.
///
/// Authenticates via a `GITHUB_TOKEN` environment variable and sends all
/// requests with the required `Accept`, `Authorization`, and `User-Agent`
/// headers.
pub struct GitHubClient {
    client: Client,
    token: String,
}

impl GitHubClient {
    /// Create a new client, reading the token from `GITHUB_TOKEN`.
    ///
    /// Returns an error if the environment variable is not set.
    pub fn new() -> Result<Self> {
        let token = env::var("GITHUB_TOKEN")
            .context("GITHUB_TOKEN environment variable is not set. Set it to a GitHub personal access token.")?;

        Ok(Self {
            client: Client::new(),
            token,
        })
    }

    /// Fetch pull request metadata.
    pub async fn fetch_pull_request(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<PullRequest> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            owner, repo, pr_number
        );

        let response = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(ACCEPT, "application/vnd.github.v3+json")
            .header(USER_AGENT, "prism-cli")
            .send()
            .await
            .context("Failed to send request to GitHub API")?;

        if response.status() == 404 {
            bail!(
                "Pull request #{} not found in {}/{}. Check that the PR exists and your token has access.",
                pr_number,
                owner,
                repo
            );
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "GitHub API returned {} for PR #{}: {}",
                status,
                pr_number,
                body
            );
        }

        response
            .json::<PullRequest>()
            .await
            .context("Failed to parse pull request response")
    }

    /// Fetch the list of files changed in a pull request.
    pub async fn fetch_pull_request_files(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<PullRequestFile>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}/files",
            owner, repo, pr_number
        );

        let response = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(ACCEPT, "application/vnd.github.v3+json")
            .header(USER_AGENT, "prism-cli")
            .send()
            .await
            .context("Failed to send request to GitHub API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "GitHub API returned {} when fetching files for PR #{}: {}",
                status,
                pr_number,
                body
            );
        }

        response
            .json::<Vec<PullRequestFile>>()
            .await
            .context("Failed to parse pull request files response")
    }

    /// Fetch a single commit by SHA.
    pub async fn fetch_commit(&self, owner: &str, repo: &str, sha: &str) -> Result<CommitResponse> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}",
            owner, repo, sha
        );

        let response = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(ACCEPT, "application/vnd.github.v3+json")
            .header(USER_AGENT, "prism-cli")
            .send()
            .await
            .context("Failed to send request to GitHub API")?;

        if response.status() == 404 {
            bail!(
                "Commit {} not found in {}/{}. Check that the commit exists and your token has access.",
                sha,
                owner,
                repo
            );
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "GitHub API returned {} for commit {}: {}",
                status,
                sha,
                body
            );
        }

        response
            .json::<CommitResponse>()
            .await
            .context("Failed to parse commit response")
    }
}

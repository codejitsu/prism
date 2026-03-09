use serde::Deserialize;

/// A GitHub user (author of a PR or commit).
#[derive(Debug, Deserialize)]
pub struct User {
    pub login: String,
}

/// Branch reference within a pull request (head or base).
#[derive(Debug, Deserialize)]
pub struct PullRequestRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

/// GitHub Pull Request metadata returned by `GET /repos/{owner}/{repo}/pulls/{number}`.
#[derive(Debug, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub user: User,
    pub head: PullRequestRef,
    pub base: PullRequestRef,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
}

/// A single file changed in a pull request, returned by
/// `GET /repos/{owner}/{repo}/pulls/{number}/files`.
#[derive(Debug, Deserialize)]
pub struct PullRequestFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
}

/// The author of a git commit (from the commit object, not the GitHub user).
#[derive(Debug, Deserialize)]
pub struct CommitAuthor {
    pub name: String,
    pub date: Option<String>,
}

/// The inner commit object containing the message and author info.
#[derive(Debug, Deserialize)]
pub struct CommitDetail {
    pub message: String,
    pub author: CommitAuthor,
}

/// A single file changed in a commit.
#[derive(Debug, Deserialize)]
pub struct CommitFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
}

/// GitHub commit response returned by `GET /repos/{owner}/{repo}/commits/{ref}`.
#[derive(Debug, Deserialize)]
pub struct CommitResponse {
    pub sha: String,
    pub commit: CommitDetail,
    #[serde(default)]
    pub files: Vec<CommitFile>,
}

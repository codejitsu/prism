use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Summary {
    pub overview: String,
    pub key_changes: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegressionFinding {
    pub title: String,
    pub severity: String,
    pub rationale: String,
    pub affected_files: Vec<String>,
    pub suggested_check: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegressionReport {
    pub findings: Vec<RegressionFinding>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProdReadinessReport {
    pub verdict: String,
    pub readiness_score: u8,
    pub logging_and_observability: Vec<String>,
    pub scalability: Vec<String>,
    pub edge_cases: Vec<String>,
    pub blocking_issues: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReviewFileContext {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReviewContext {
    pub target_label: String,
    pub owner: String,
    pub repo: String,
    pub title_or_message: String,
    pub body: Option<String>,
    pub files: Vec<ReviewFileContext>,
}

#[derive(Debug)]
pub struct AiReviewResult {
    pub summary: Summary,
    pub regressions: RegressionReport,
    pub prod_readiness: ProdReadinessReport,
}

use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Summary {
    pub overview: String,
    pub key_changes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[serde(alias = "Low", alias = "LOW")]
    Low,
    #[serde(alias = "Medium", alias = "MEDIUM")]
    Medium,
    #[serde(alias = "High", alias = "HIGH")]
    High,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Low => write!(f, "low"),
            Severity::Medium => write!(f, "medium"),
            Severity::High => write!(f, "high"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RegressionFinding {
    pub title: String,
    pub severity: Severity,
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

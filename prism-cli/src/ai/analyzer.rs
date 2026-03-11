use std::env;
use std::fmt::Write;
use std::time::Duration;

use anyhow::{Context, Result};
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::TypedPrompt;
use rig::providers::openai;

use super::prompts::{SYSTEM_PROMPT, prod_readiness_prompt, regressions_prompt, summary_prompt};
use super::types::{ProdReadinessReport, RegressionReport, ReviewContext, Summary};

const MAX_CONTEXT_CHARS: usize = 20_000;
const MAX_FILE_PATCH_CHARS: usize = 2_500;
const SECTION_TIMEOUT_SECS: u64 = 30;

/// Configuration for AI analysis.
pub struct AnalyzerConfig {
    /// OpenAI API key.
    api_key: String,
    /// Model to use (e.g., "gpt-4o").
    model: String,
}

impl AnalyzerConfig {
    /// Create a new analyzer configuration.
    ///
    /// - `model_override`: CLI `--model` flag value (highest priority)
    /// - `config_model`: Model from config file (fallback)
    /// - `api_key`: OpenAI API key resolved from env or config
    pub fn new(
        model_override: Option<&str>,
        config_model: &str,
        api_key: Option<&str>,
    ) -> Result<Self> {
        let api_key = api_key
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenAI API key is required. \
                     Set OPENAI_API_KEY environment variable or add it to ~/.config/prism/config.toml"
                )
            })?;

        let model = model_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(config_model)
            .to_string();

        Ok(Self {
            api_key: api_key.to_string(),
            model,
        })
    }

    /// Initialize the OpenAI environment for an analysis call.
    ///
    /// Sets the `OPENAI_API_KEY` environment variable so that
    /// `rig::providers::openai::Client::from_env()` can pick it up. Called
    /// by each `analyze_*` method before creating a client.
    fn init_env(&self) {
        // SAFETY: In Rust 2024 edition, env::set_var is unsafe because it can
        // cause data races. We always set the same key to the same value from
        // the configured API key, and this runs on the main async runtime
        // before spawning the OpenAI client.
        unsafe {
            env::set_var("OPENAI_API_KEY", &self.api_key);
        }
    }

    /// Analyze the summary section.
    pub async fn analyze_summary(&self, context: &ReviewContext) -> Result<Summary> {
        self.init_env();
        let openai_client = openai::Client::from_env();
        let agent = openai_client
            .agent(&self.model)
            .preamble(SYSTEM_PROMPT)
            .max_tokens(2_000)
            .build();

        let flattened_context = render_context(context);
        tokio::time::timeout(Duration::from_secs(SECTION_TIMEOUT_SECS), async {
            agent
                .prompt_typed::<Summary>(summary_prompt(&flattened_context))
                .await
                .map_err(|err| anyhow::anyhow!("failed to generate AI summary section: {err}"))
        })
        .await
        .context("Summary generation timed out")?
    }

    /// Analyze for potential regressions.
    pub async fn analyze_regressions(&self, context: &ReviewContext) -> Result<RegressionReport> {
        self.init_env();
        let openai_client = openai::Client::from_env();
        let agent = openai_client
            .agent(&self.model)
            .preamble(SYSTEM_PROMPT)
            .max_tokens(2_000)
            .build();

        let flattened_context = render_context(context);
        tokio::time::timeout(Duration::from_secs(SECTION_TIMEOUT_SECS), async {
            agent
                .prompt_typed::<RegressionReport>(regressions_prompt(&flattened_context))
                .await
                .map_err(|err| anyhow::anyhow!("failed to generate AI regressions section: {err}"))
        })
        .await
        .context("Regressions analysis timed out")?
    }

    /// Analyze production readiness.
    pub async fn analyze_prod_readiness(
        &self,
        context: &ReviewContext,
    ) -> Result<ProdReadinessReport> {
        self.init_env();
        let openai_client = openai::Client::from_env();
        let agent = openai_client
            .agent(&self.model)
            .preamble(SYSTEM_PROMPT)
            .max_tokens(2_000)
            .build();

        let flattened_context = render_context(context);
        tokio::time::timeout(Duration::from_secs(SECTION_TIMEOUT_SECS), async {
            agent
                .prompt_typed::<ProdReadinessReport>(prod_readiness_prompt(&flattened_context))
                .await
                .map_err(|err| {
                    anyhow::anyhow!("failed to generate AI production readiness section: {err}")
                })
        })
        .await
        .context("Production readiness analysis timed out")?
    }
}

fn render_context(context: &ReviewContext) -> String {
    let mut out = String::new();
    let _ = writeln!(&mut out, "Target: {}", context.target_label);
    let _ = writeln!(&mut out, "Repository: {}/{}", context.owner, context.repo);
    let _ = writeln!(&mut out, "Title/Message: {}", context.title_or_message);
    if let Some(body) = &context.body {
        let body = body.trim();
        if !body.is_empty() {
            let _ = writeln!(&mut out, "Body:");
            let _ = writeln!(&mut out, "{}", truncate(body, 2_000));
        }
    }
    let _ = writeln!(&mut out, "Files Changed: {}", context.files.len());

    for file in &context.files {
        let _ = writeln!(
            &mut out,
            "- {} [{}] (+{} -{})",
            file.filename, file.status, file.additions, file.deletions
        );
        if let Some(patch) = &file.patch {
            let _ = writeln!(&mut out, "  Patch:");
            let trimmed_patch = truncate(patch, MAX_FILE_PATCH_CHARS);
            for line in trimmed_patch.lines() {
                let _ = writeln!(&mut out, "  {}", line);
            }
        }
    }

    truncate(&out, MAX_CONTEXT_CHARS)
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let mut collected = String::new();
    for ch in input.chars().take(max_chars) {
        collected.push(ch);
    }
    collected.push_str("\n...[truncated]...");
    collected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{ReviewContext, ReviewFileContext};

    fn sample_context_with_patch(patch: Option<String>) -> ReviewContext {
        ReviewContext {
            target_label: "pull_request#42".to_string(),
            owner: "octocat".to_string(),
            repo: "hello-world".to_string(),
            title_or_message: "Add improved parser".to_string(),
            body: Some("Implements parser improvements and edge-case handling".to_string()),
            files: vec![ReviewFileContext {
                filename: "src/parser.rs".to_string(),
                status: "modified".to_string(),
                additions: 10,
                deletions: 4,
                patch,
            }],
        }
    }

    #[test]
    fn test_truncate_without_cutoff_returns_original() {
        let input = "short";
        let output = truncate(input, 20);
        assert_eq!(output, input);
    }

    #[test]
    fn test_truncate_with_cutoff_appends_marker() {
        let input = "abcdefghijklmnopqrstuvwxyz";
        let output = truncate(input, 10);
        assert!(
            output.starts_with("abcdefghij"),
            "Expected output to start with truncated prefix, got: {output}"
        );
        assert!(
            output.contains("...[truncated]..."),
            "Expected truncation marker, got: {output}"
        );
    }

    #[test]
    fn test_render_context_includes_patch_block() {
        let context = sample_context_with_patch(Some("+added\n-removed".to_string()));
        let rendered = render_context(&context);

        assert!(rendered.contains("Target: pull_request#42"));
        assert!(rendered.contains("Repository: octocat/hello-world"));
        assert!(rendered.contains("- src/parser.rs [modified] (+10 -4)"));
        assert!(rendered.contains("  Patch:"));
        assert!(rendered.contains("  +added"));
        assert!(rendered.contains("  -removed"));
    }

    #[test]
    fn test_render_context_omits_empty_body() {
        let mut context = sample_context_with_patch(None);
        context.body = Some("   \n\t".to_string());

        let rendered = render_context(&context);
        assert!(
            !rendered.contains("Body:"),
            "Expected empty body to be omitted, got: {rendered}"
        );
    }

    #[test]
    fn test_render_context_truncates_large_patch() {
        let long_patch = "x".repeat(MAX_FILE_PATCH_CHARS + 500);
        let context = sample_context_with_patch(Some(long_patch));
        let rendered = render_context(&context);

        assert!(
            rendered.contains("...[truncated]..."),
            "Expected long patch to be truncated"
        );
    }

    #[test]
    fn test_render_context_global_limit_applies() {
        let many_files = (0..120)
            .map(|i| ReviewFileContext {
                filename: format!("src/file_{i}.rs"),
                status: "modified".to_string(),
                additions: 50,
                deletions: 10,
                patch: Some("line\n".repeat(300)),
            })
            .collect::<Vec<_>>();

        let context = ReviewContext {
            target_label: "commit:deadbeef".to_string(),
            owner: "octocat".to_string(),
            repo: "hello-world".to_string(),
            title_or_message: "Large change".to_string(),
            body: None,
            files: many_files,
        };

        let rendered = render_context(&context);

        assert!(
            rendered.chars().count() <= MAX_CONTEXT_CHARS + "\n...[truncated]...".chars().count(),
            "Expected rendered context to respect global size limit"
        );
        assert!(
            rendered.contains("...[truncated]..."),
            "Expected global truncation marker for oversized context"
        );
    }
}

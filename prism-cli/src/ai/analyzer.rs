use std::env;
use std::fmt::Write;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::TypedPrompt;
use rig::providers::openai;

use super::prompts::{SYSTEM_PROMPT, prod_readiness_prompt, regressions_prompt, summary_prompt};
use super::types::{AiReviewResult, ProdReadinessReport, RegressionReport, ReviewContext, Summary};

const DEFAULT_MODEL: &str = "gpt-4o";
const MAX_CONTEXT_CHARS: usize = 20_000;
const MAX_FILE_PATCH_CHARS: usize = 2_500;
const ANALYSIS_TIMEOUT_SECS: u64 = 45;

pub async fn analyze_review_context(
    context: &ReviewContext,
    model_override: Option<&str>,
) -> Result<AiReviewResult> {
    ensure_openai_api_key()?;

    let model = model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MODEL);

    let openai_client = openai::Client::from_env();
    let agent = openai_client
        .agent(model)
        .preamble(SYSTEM_PROMPT)
        .max_tokens(2_000)
        .build();

    let flattened_context = render_context(context);
    let (summary, regressions, prod_readiness) =
        tokio::time::timeout(Duration::from_secs(ANALYSIS_TIMEOUT_SECS), async {
            let summary: Summary = agent
                .prompt_typed::<Summary>(summary_prompt(&flattened_context))
                .await
                .map_err(|err| anyhow::anyhow!("failed to generate AI summary section: {err}"))?;

            let regressions: RegressionReport = agent
                .prompt_typed::<RegressionReport>(regressions_prompt(&flattened_context))
                .await
                .map_err(|err| {
                    anyhow::anyhow!("failed to generate AI regressions section: {err}")
                })?;

            let prod_readiness: ProdReadinessReport = agent
                .prompt_typed::<ProdReadinessReport>(prod_readiness_prompt(&flattened_context))
                .await
                .map_err(|err| {
                    anyhow::anyhow!("failed to generate AI production readiness section: {err}")
                })?;

            Ok::<(Summary, RegressionReport, ProdReadinessReport), anyhow::Error>((
                summary,
                regressions,
                prod_readiness,
            ))
        })
        .await
        .context("AI analysis timed out")??;

    Ok(AiReviewResult {
        summary,
        regressions,
        prod_readiness,
    })
}

fn ensure_openai_api_key() -> Result<()> {
    match env::var("OPENAI_API_KEY") {
        Ok(value) if !value.trim().is_empty() => Ok(()),
        _ => bail!("OPENAI_API_KEY is required when using --ai. Export it before running prism."),
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

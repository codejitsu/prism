pub const SYSTEM_PROMPT: &str = "You are a senior software reviewer. Analyze code changes strictly from provided context. Be precise, conservative, and actionable. Do not invent files or behavior not present in the diff.";

pub fn summary_prompt(context: &str) -> String {
    format!(
        "You are reviewing a code change. Produce a concise summary.\n\
         Return JSON that matches the required schema.\n\
         - overview: 2-4 sentences\n\
         - key_changes: 3-8 bullet-like short items\n\n\
         Review context:\n{}",
        context
    )
}

pub fn regressions_prompt(context: &str) -> String {
    format!(
        "Identify the top 5 potential regressions/bugs introduced by this change.\n\
         Return JSON that matches the required schema.\n\
         - Provide exactly 5 findings when possible, otherwise fewer if evidence is limited\n\
         - severity must be one of: high, medium, low\n\
         - tie each finding to concrete code evidence\n\
         - suggested_check should be a specific validation step\n\n\
         Review context:\n{}",
        context
    )
}

pub fn prod_readiness_prompt(context: &str) -> String {
    format!(
        "Assess production readiness for this change.\n\
         Return JSON that matches the required schema.\n\
         - verdict should be one of: ready, caution, not_ready\n\
         - readiness_score should be 0-100\n\
         - evaluate logging_and_observability, scalability, and edge_cases\n\
         - list blocking_issues that must be fixed before production\n\n\
         Review context:\n{}",
        context
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_prompt_contains_context_and_instructions() {
        let context = "Target: pull_request#12";
        let prompt = summary_prompt(context);

        assert!(prompt.contains("Return JSON that matches the required schema"));
        assert!(prompt.contains("overview: 2-4 sentences"));
        assert!(prompt.contains(context));
    }

    #[test]
    fn test_regressions_prompt_mentions_top_5_and_severity() {
        let context = "Files Changed: 3";
        let prompt = regressions_prompt(context);

        assert!(prompt.contains("top 5 potential regressions/bugs"));
        assert!(prompt.contains("severity must be one of: high, medium, low"));
        assert!(prompt.contains(context));
    }

    #[test]
    fn test_prod_readiness_prompt_mentions_verdict_and_score() {
        let context = "Repository: org/repo";
        let prompt = prod_readiness_prompt(context);

        assert!(prompt.contains("verdict should be one of: ready, caution, not_ready"));
        assert!(prompt.contains("readiness_score should be 0-100"));
        assert!(prompt.contains(context));
    }
}

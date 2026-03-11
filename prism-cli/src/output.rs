//! Rich terminal output for Prism CLI.
//!
//! This module provides beautiful terminal output using the `richrs` crate,
//! including syntax-highlighted diffs, markdown rendering, and spinners.

use std::future::Future;
use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use richrs::color::{Color, StandardColor};
use richrs::style::Style;
use richrs::syntax::Syntax;
use richrs::text::Text;

use crate::ai::{ProdReadinessReport, RegressionReport, Severity, Summary};
use crate::github::types::{CommitFile, CommitResponse, PullRequest, PullRequestFile};

/// Maximum terminal width (capped for readability).
const MAX_WIDTH: usize = 120;

/// Maximum number of diff lines to show before truncating.
const MAX_DIFF_LINES: usize = 500;

/// File statistics for display.
pub struct FileStats {
    pub total_files: usize,
    pub additions: u64,
    pub deletions: u64,
}

/// Rich terminal printer for Prism output.
pub struct RichPrinter {
    width: usize,
}

impl RichPrinter {
    /// Create a new RichPrinter with auto-detected terminal width.
    pub fn new() -> Self {
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80);
        let width = term_width.min(MAX_WIDTH);

        Self { width }
    }

    /// Print an empty line.
    pub fn newline(&self) {
        println!();
    }

    /// Print a section separator (double line).
    pub fn print_separator(&self) {
        println!("{}", "═".repeat(self.width));
    }

    /// Print a pull request header.
    pub fn print_pr_header(&self, pr: &PullRequest) -> Result<()> {
        // Title with emoji
        let title = format!("PR #{}: {}", pr.number, pr.title);
        println!(
            "{}",
            Text::styled(format!("🚀 {}", title), Style::new().bold())
                .to_segments()
                .to_ansi()
        );
        println!();

        // Author in cyan
        println!(
            "{}",
            Text::styled(
                format!("Author: {}", pr.user.login),
                Style::new().with_color(Color::Standard(StandardColor::Cyan))
            )
            .to_segments()
            .to_ansi()
        );

        // State with color
        let state_style = match pr.state.as_str() {
            "open" => Style::new().with_color(Color::Standard(StandardColor::Green)),
            "closed" => Style::new().with_color(Color::Standard(StandardColor::Red)),
            "merged" => Style::new().with_color(Color::Standard(StandardColor::Magenta)),
            _ => Style::new(),
        };
        println!(
            "State:  {}",
            Text::styled(&pr.state, state_style).to_segments().to_ansi()
        );

        // Base/head branches
        println!("Base:   {} <- {}", pr.base.ref_name, pr.head.ref_name);

        self.print_separator();
        Ok(())
    }

    /// Print a commit header.
    pub fn print_commit_header(&self, commit: &CommitResponse) -> Result<()> {
        // Extract first line of commit message as title
        let message_first_line = commit
            .commit
            .message
            .lines()
            .next()
            .unwrap_or("(no message)");
        let short_sha = &commit.sha[..7.min(commit.sha.len())];

        // Title with emoji
        println!(
            "{}",
            Text::styled(format!("🚀 Commit {}", short_sha), Style::new().bold())
                .to_segments()
                .to_ansi()
        );
        println!();

        // Message first line in bold
        println!(
            "{}",
            Text::styled(message_first_line, Style::new().bold())
                .to_segments()
                .to_ansi()
        );

        // Author in cyan
        println!(
            "{}",
            Text::styled(
                format!("Author: {}", commit.commit.author.name),
                Style::new().with_color(Color::Standard(StandardColor::Cyan))
            )
            .to_segments()
            .to_ansi()
        );

        // Date if present
        if let Some(date) = &commit.commit.author.date {
            println!("Date:   {}", date);
        }

        self.print_separator();
        Ok(())
    }

    /// Print a description with markdown rendering.
    pub fn print_description(&self, body: &str) -> Result<()> {
        let body = body.trim();
        if body.is_empty() {
            return Ok(());
        }

        // Bold header with emoji
        println!(
            "{}",
            Text::styled("Description", Style::new().bold())
                .to_segments()
                .to_ansi()
        );

        // Render markdown
        let md = richrs::markdown::Markdown::new(body);
        let segments = md.render(self.width);
        print!("{}", segments.to_ansi());

        self.print_separator();
        Ok(())
    }

    /// Print a files changed list for PR files.
    pub fn print_files_table_pr(&self, files: &[PullRequestFile], stats: FileStats) -> Result<()> {
        // Bold header with emoji
        println!(
            "{}",
            Text::styled(
                format!(
                    "Files Changed ({}) +{} -{}",
                    stats.total_files, stats.additions, stats.deletions
                ),
                Style::new().bold()
            )
            .to_segments()
            .to_ansi()
        );

        for file in files {
            let (status_char, status_style) = status_to_styled_char(&file.status);
            let changes = format!("+{} -{}", file.additions, file.deletions);

            println!(
                "  {} {} ({})",
                Text::styled(status_char.to_string(), status_style)
                    .to_segments()
                    .to_ansi(),
                file.filename,
                Text::styled(
                    changes,
                    Style::new().with_color(Color::Standard(StandardColor::Cyan))
                )
                .to_segments()
                .to_ansi()
            );
        }

        Ok(())
    }

    /// Print a files changed list for commit files.
    pub fn print_files_table_commit(&self, files: &[CommitFile], stats: FileStats) -> Result<()> {
        // Bold header with emoji
        println!(
            "{}",
            Text::styled(
                format!(
                    "Files Changed ({}) +{} -{}",
                    stats.total_files, stats.additions, stats.deletions
                ),
                Style::new().bold()
            )
            .to_segments()
            .to_ansi()
        );

        for file in files {
            let (status_char, status_style) = status_to_styled_char(&file.status);
            let changes = format!("+{} -{}", file.additions, file.deletions);

            println!(
                "  {} {} ({})",
                Text::styled(status_char.to_string(), status_style)
                    .to_segments()
                    .to_ansi(),
                file.filename,
                Text::styled(
                    changes,
                    Style::new().with_color(Color::Standard(StandardColor::Cyan))
                )
                .to_segments()
                .to_ansi()
            );
        }

        Ok(())
    }

    /// Print diffs for PR files with syntax highlighting.
    pub fn print_diff_pr(&self, files: &[PullRequestFile]) -> Result<()> {
        let patches: Vec<(&str, &str)> = files
            .iter()
            .filter_map(|f| f.patch.as_ref().map(|p| (f.filename.as_str(), p.as_str())))
            .collect();

        self.print_diffs(&patches)
    }

    /// Print diffs for commit files with syntax highlighting.
    pub fn print_diff_commit(&self, files: &[CommitFile]) -> Result<()> {
        let patches: Vec<(&str, &str)> = files
            .iter()
            .filter_map(|f| f.patch.as_ref().map(|p| (f.filename.as_str(), p.as_str())))
            .collect();

        self.print_diffs(&patches)
    }

    /// Internal: print diffs with syntax highlighting and truncation.
    fn print_diffs(&self, patches: &[(&str, &str)]) -> Result<()> {
        if patches.is_empty() {
            return Ok(());
        }

        let mut total_lines = 0;
        let mut truncated = false;

        for (filename, patch) in patches {
            let patch_lines: Vec<&str> = patch.lines().collect();
            let remaining = MAX_DIFF_LINES.saturating_sub(total_lines);

            if remaining == 0 {
                truncated = true;
                break;
            }

            let lines_to_show = patch_lines.len().min(remaining);
            let display_patch: String = patch_lines[..lines_to_show].join("\n");

            if lines_to_show < patch_lines.len() {
                truncated = true;
            }

            total_lines += lines_to_show;

            // Print diff header (bold, dim)
            println!(
                "{}",
                Text::styled(
                    format!("diff --git a/{} b/{}", filename, filename),
                    Style::new().bold().dim()
                )
                .to_segments()
                .to_ansi()
            );

            // Syntax-highlighted diff
            let syntax = Syntax::new(&display_patch, "diff");
            let segments = syntax.render(self.width);
            print!("{}", segments.to_ansi());

            println!();

            if truncated {
                break;
            }
        }

        if truncated {
            println!(
                "{}",
                Text::styled(
                    format!("... showing first {} lines of diff", MAX_DIFF_LINES),
                    Style::new()
                        .italic()
                        .with_color(Color::Standard(StandardColor::Yellow))
                )
                .to_segments()
                .to_ansi()
            );
        }

        self.print_separator();
        Ok(())
    }

    /// Print AI summary.
    pub fn print_ai_summary(&self, summary: &Summary) -> Result<()> {
        // Bold header with emoji
        println!(
            "{}",
            Text::styled("AI Summary", Style::new().bold())
                .to_segments()
                .to_ansi()
        );

        // Overview
        println!("{}", summary.overview);

        // Key changes as bullet points
        if !summary.key_changes.is_empty() {
            println!();
            println!(
                "{}",
                Text::styled("Key changes:", Style::new().bold())
                    .to_segments()
                    .to_ansi()
            );
            for item in &summary.key_changes {
                println!(
                    "  {}",
                    Text::styled(format!("• {}", item), Style::new().dim())
                        .to_segments()
                        .to_ansi()
                );
            }
        }

        self.print_separator();
        Ok(())
    }

    /// Print regressions as a list.
    pub fn print_regressions(&self, regressions: &RegressionReport) -> Result<()> {
        // Bold header with emoji
        println!(
            "{}",
            Text::styled("Potential Regressions", Style::new().bold())
                .to_segments()
                .to_ansi()
        );

        if regressions.findings.is_empty() {
            println!(
                "{}",
                Text::styled(
                    "No potential regressions identified.",
                    Style::new().with_color(Color::Standard(StandardColor::Green))
                )
                .to_segments()
                .to_ansi()
            );
            self.print_separator();
            return Ok(());
        }

        let mut sorted_findings = regressions.findings.clone();
        sorted_findings.sort_by(|a, b| b.severity.cmp(&a.severity));
        for (index, finding) in sorted_findings.iter().enumerate() {
            let severity_style = match finding.severity {
                Severity::High => Style::new()
                    .bold()
                    .with_color(Color::Standard(StandardColor::Red)),
                Severity::Medium => Style::new().with_color(Color::Standard(StandardColor::Yellow)),
                Severity::Low => Style::new().with_color(Color::Standard(StandardColor::Green)),
            };

            // Numbered item with severity
            println!(
                "{}. [{}] {}",
                index + 1,
                Text::styled(finding.severity.as_str(), severity_style)
                    .to_segments()
                    .to_ansi(),
                Text::styled(&finding.title, Style::new().bold())
                    .to_segments()
                    .to_ansi()
            );

            // Why (rationale)
            println!(
                "   {}",
                Text::styled(format!("Why: {}", finding.rationale), Style::new().dim())
                    .to_segments()
                    .to_ansi()
            );

            // Files if present
            if !finding.affected_files.is_empty() {
                println!(
                    "   {}",
                    Text::styled(
                        format!("Files: {}", finding.affected_files.join(", ")),
                        Style::new().dim()
                    )
                    .to_segments()
                    .to_ansi()
                );
            }

            // Check (suggested verification)
            println!(
                "   {}",
                Text::styled(
                    format!("Check: {}", finding.suggested_check),
                    Style::new().italic()
                )
                .to_segments()
                .to_ansi()
            );

            // Add spacing between findings
            if index < regressions.findings.len() - 1 {
                println!();
            }
        }

        self.print_separator();
        Ok(())
    }

    /// Print production readiness.
    pub fn print_prod_readiness(&self, readiness: &ProdReadinessReport) -> Result<()> {
        // Bold header with emoji
        println!(
            "{}",
            Text::styled("Production Readiness", Style::new().bold())
                .to_segments()
                .to_ansi()
        );

        // Verdict with color based on score
        let score_style = if readiness.readiness_score >= 80 {
            Style::new()
                .bold()
                .with_color(Color::Standard(StandardColor::Green))
        } else if readiness.readiness_score >= 50 {
            Style::new()
                .bold()
                .with_color(Color::Standard(StandardColor::Yellow))
        } else {
            Style::new()
                .bold()
                .with_color(Color::Standard(StandardColor::Red))
        };

        println!(
            "Verdict: {} (score: {})",
            Text::styled(&readiness.verdict, score_style.clone())
                .to_segments()
                .to_ansi(),
            Text::styled(readiness.readiness_score.to_string(), score_style)
                .to_segments()
                .to_ansi()
        );

        // Subsections
        print_section(
            "Logging/Observability",
            &readiness.logging_and_observability,
        );
        print_section("Scalability", &readiness.scalability);
        print_section("Edge Cases", &readiness.edge_cases);
        print_section("Blocking Issues", &readiness.blocking_issues);

        self.print_separator();
        Ok(())
    }

    /// Print an error message.
    pub fn print_error(&self, message: &str) {
        println!(
            "{}",
            Text::styled(format!("Error: {}", message), Style::new().bold())
                .to_segments()
                .to_ansi()
        );
    }
}

impl Default for RichPrinter {
    fn default() -> Self {
        Self::new()
    }
}

/// Print a labeled section with bullet points.
fn print_section(label: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    println!();
    println!(
        "{}",
        Text::styled(format!("{}:", label), Style::new().bold())
            .to_segments()
            .to_ansi()
    );
    for item in items {
        println!(
            "  {}",
            Text::styled(format!("• {}", item), Style::new().dim())
                .to_segments()
                .to_ansi()
        );
    }
}

/// Map status to styled character.
fn status_to_styled_char(status: &str) -> (char, Style) {
    match status {
        "added" => (
            'A',
            Style::new().with_color(Color::Standard(StandardColor::Green)),
        ),
        "removed" => (
            'D',
            Style::new().with_color(Color::Standard(StandardColor::Red)),
        ),
        "modified" => (
            'M',
            Style::new().with_color(Color::Standard(StandardColor::Yellow)),
        ),
        "renamed" => (
            'R',
            Style::new().with_color(Color::Standard(StandardColor::Cyan)),
        ),
        "copied" => (
            'C',
            Style::new().with_color(Color::Standard(StandardColor::Blue)),
        ),
        _ => ('?', Style::new()),
    }
}

/// Execute an async operation with a spinner.
///
/// Shows a spinner animation while the operation runs, then displays
/// a success or error indicator when complete.
pub async fn with_spinner<T, F, Fut>(message: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    // Spinner frames (dots style)
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    const FRAME_DURATION: Duration = Duration::from_millis(80);

    // Flag to control spinner thread
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();
    let message_owned = message.to_string();

    // Spawn spinner thread
    let spinner_handle = std::thread::spawn(move || {
        let mut frame_idx = 0;
        let stdout = io::stdout();

        while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
            {
                let mut handle = stdout.lock();
                let _ = write!(
                    handle,
                    "\r\x1b[36m{}\x1b[0m {}",
                    FRAMES[frame_idx], message_owned
                );
                let _ = handle.flush();
            }
            frame_idx = (frame_idx + 1) % FRAMES.len();
            std::thread::sleep(FRAME_DURATION);
        }
    });

    // Execute the async operation
    let result = f().await;

    // Stop the spinner
    running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = spinner_handle.join();

    // Clear the line and print result
    print!("\r\x1b[K"); // Clear line

    match &result {
        Ok(_) => {
            println!("\x1b[32m✓\x1b[0m {}", message);
        }
        Err(_) => {
            println!("\x1b[31m✗\x1b[0m {}", message);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_to_styled_char() {
        assert_eq!(status_to_styled_char("added").0, 'A');
        assert_eq!(status_to_styled_char("removed").0, 'D');
        assert_eq!(status_to_styled_char("modified").0, 'M');
        assert_eq!(status_to_styled_char("renamed").0, 'R');
        assert_eq!(status_to_styled_char("copied").0, 'C');
        assert_eq!(status_to_styled_char("unknown").0, '?');
    }

    #[test]
    fn test_rich_printer_new() {
        let printer = RichPrinter::new();
        assert!(printer.width <= MAX_WIDTH);
        assert!(printer.width > 0);
    }

    #[test]
    fn test_file_stats() {
        let stats = FileStats {
            total_files: 5,
            additions: 100,
            deletions: 50,
        };
        assert_eq!(stats.total_files, 5);
        assert_eq!(stats.additions, 100);
        assert_eq!(stats.deletions, 50);
    }
}

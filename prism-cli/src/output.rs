//! Rich terminal output for Prism CLI.
//!
//! This module provides beautiful terminal output using the `richrs` crate,
//! including panels, tables, syntax-highlighted diffs, and spinners.

use std::future::Future;
use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use richrs::color::{Color, StandardColor};
use richrs::panel::Panel;
use richrs::style::Style;
use richrs::syntax::Syntax;
use richrs::table::{Column, Row, Table};
use richrs::text::Text;

use crate::ai::{ProdReadinessReport, RegressionReport, Summary};
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

    /// Print a pull request header panel.
    pub fn print_pr_header(&self, pr: &PullRequest) -> Result<()> {
        // Build title with PR number
        let title = format!("PR #{}: {}", pr.number, pr.title);

        // Color the state
        let state_color = match pr.state.as_str() {
            "open" => Color::Standard(StandardColor::Green),
            "closed" => Color::Standard(StandardColor::Red),
            "merged" => Color::Standard(StandardColor::Magenta),
            _ => Color::Default,
        };

        // Build content with styled text
        let content = Text::assemble([
            (
                format!("Author: {}", pr.user.login),
                Some(Style::new().with_color(Color::Standard(StandardColor::Cyan))),
            ),
            ("\n".to_string(), None),
            (
                format!("State:  {}", pr.state),
                Some(Style::new().with_color(state_color)),
            ),
            ("\n".to_string(), None),
            (
                format!("Base:   {} <- {}", pr.base.ref_name, pr.head.ref_name),
                None,
            ),
        ]);

        let panel = Panel::new(content)
            .title(Text::styled(title, Style::new().bold()))
            .border_style(Style::new().with_color(Color::Standard(StandardColor::Blue)))
            .width(self.width);

        let segments = panel.render(self.width);
        print!("{}", segments.to_ansi());

        Ok(())
    }

    /// Print a commit header panel.
    pub fn print_commit_header(&self, commit: &CommitResponse) -> Result<()> {
        // Extract first line of commit message as title
        let message_first_line = commit
            .commit
            .message
            .lines()
            .next()
            .unwrap_or("(no message)");
        let title = format!("Commit: {}", &commit.sha[..7.min(commit.sha.len())]);

        // Build content
        let mut parts: Vec<(String, Option<Style>)> = vec![
            (message_first_line.to_string(), Some(Style::new().bold())),
            ("\n".to_string(), None),
            (
                format!("Author: {}", commit.commit.author.name),
                Some(Style::new().with_color(Color::Standard(StandardColor::Cyan))),
            ),
        ];

        if let Some(date) = &commit.commit.author.date {
            parts.push(("\n".to_string(), None));
            parts.push((format!("Date:   {}", date), None));
        }

        let content = Text::assemble(parts);

        let panel = Panel::new(content)
            .title(Text::styled(title, Style::new().bold()))
            .border_style(Style::new().with_color(Color::Standard(StandardColor::Blue)))
            .width(self.width);

        let segments = panel.render(self.width);
        print!("{}", segments.to_ansi());

        Ok(())
    }

    /// Print a description panel with markdown rendering.
    pub fn print_description(&self, body: &str) -> Result<()> {
        let body = body.trim();
        if body.is_empty() {
            return Ok(());
        }

        // Print title header
        let title_style = Style::new().bold();
        let border_style = Style::new().dim();
        println!(
            "{}── {} {}",
            border_style.to_ansi(),
            Text::styled("Description", title_style)
                .to_segments()
                .to_ansi(),
            "─".repeat(self.width.saturating_sub(16))
        );

        // Use richrs markdown rendering directly (Markdown doesn't impl Into<Text>)
        let md = richrs::markdown::Markdown::new(body);
        let segments = md.render(self.width);
        print!("{}", segments.to_ansi());

        println!();
        Ok(())
    }

    /// Print a files changed table for PR files.
    pub fn print_files_table_pr(&self, files: &[PullRequestFile], stats: FileStats) -> Result<()> {
        let title = format!(
            "Files Changed ({}) +{} -{}",
            stats.total_files, stats.additions, stats.deletions
        );

        let mut table = Table::new().title(Text::styled(title, Style::new().bold()));

        table.add_column(Column::new("St"));
        table.add_column(Column::new("File"));
        table.add_column(Column::new("Changes"));

        for file in files {
            let (status_char, status_style) = status_to_styled_char(&file.status);
            let changes = format!("+{} -{}", file.additions, file.deletions);

            table.add_row(Row::new([
                Text::styled(status_char.to_string(), status_style),
                Text::from_str(&file.filename),
                Text::styled(
                    changes,
                    Style::new().with_color(Color::Standard(StandardColor::Green)),
                ),
            ]));
        }

        let segments = table.render(self.width);
        print!("{}", segments.to_ansi());

        Ok(())
    }

    /// Print a files changed table for commit files.
    pub fn print_files_table_commit(&self, files: &[CommitFile], stats: FileStats) -> Result<()> {
        let title = format!(
            "Files Changed ({}) +{} -{}",
            stats.total_files, stats.additions, stats.deletions
        );

        let mut table = Table::new().title(Text::styled(title, Style::new().bold()));

        table.add_column(Column::new("St"));
        table.add_column(Column::new("File"));
        table.add_column(Column::new("Changes"));

        for file in files {
            let (status_char, status_style) = status_to_styled_char(&file.status);
            let changes = format!("+{} -{}", file.additions, file.deletions);

            table.add_row(Row::new([
                Text::styled(status_char.to_string(), status_style),
                Text::from_str(&file.filename),
                Text::styled(
                    changes,
                    Style::new().with_color(Color::Standard(StandardColor::Green)),
                ),
            ]));
        }

        let segments = table.render(self.width);
        print!("{}", segments.to_ansi());

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

            // Print diff header
            let header_style = Style::new().bold().dim();
            println!(
                "{}",
                Text::styled(
                    format!("diff --git a/{} b/{}", filename, filename),
                    header_style
                )
                .to_segments()
                .to_ansi()
            );

            // Use diff syntax highlighting (Syntax doesn't impl Into<Text>)
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
                "\x1b[33m\x1b[3m... showing first {} lines of diff\x1b[0m",
                MAX_DIFF_LINES
            );
        }

        Ok(())
    }

    /// Print AI summary panel.
    pub fn print_ai_summary(&self, summary: &Summary) -> Result<()> {
        let mut parts: Vec<(String, Option<Style>)> =
            vec![(summary.overview.clone(), None), ("\n".to_string(), None)];

        for item in &summary.key_changes {
            parts.push((format!("\n  • {}", item), Some(Style::new().dim())));
        }

        let content = Text::assemble(parts);

        let panel = Panel::new(content)
            .title(Text::styled(
                "AI Summary",
                Style::new()
                    .bold()
                    .with_color(Color::Standard(StandardColor::Cyan)),
            ))
            .border_style(Style::new().with_color(Color::Standard(StandardColor::Cyan)))
            .width(self.width);

        let segments = panel.render(self.width);
        print!("{}", segments.to_ansi());

        Ok(())
    }

    /// Print regressions table.
    pub fn print_regressions(&self, regressions: &RegressionReport) -> Result<()> {
        if regressions.findings.is_empty() {
            let panel = Panel::new(Text::styled(
                "No potential regressions identified.",
                Style::new().with_color(Color::Standard(StandardColor::Green)),
            ))
            .title(Text::styled(
                "Potential Regressions",
                Style::new()
                    .bold()
                    .with_color(Color::Standard(StandardColor::Yellow)),
            ))
            .border_style(Style::new().with_color(Color::Standard(StandardColor::Yellow)))
            .width(self.width);

            let segments = panel.render(self.width);
            print!("{}", segments.to_ansi());
            return Ok(());
        }

        let mut table = Table::new().title(Text::styled(
            "Top 5 Potential Regressions",
            Style::new()
                .bold()
                .with_color(Color::Standard(StandardColor::Yellow)),
        ));

        table.add_column(Column::new("#"));
        table.add_column(Column::new("Issue"));
        table.add_column(Column::new("Severity"));
        table.add_column(Column::new("Files"));

        for (index, finding) in regressions.findings.iter().enumerate() {
            let severity_style = match finding.severity.to_uppercase().as_str() {
                "HIGH" => Style::new()
                    .bold()
                    .with_color(Color::Standard(StandardColor::Red)),
                "MEDIUM" => Style::new().with_color(Color::Standard(StandardColor::Yellow)),
                "LOW" => Style::new().with_color(Color::Standard(StandardColor::Green)),
                _ => Style::new(),
            };

            let files_display = if finding.affected_files.is_empty() {
                "-".to_string()
            } else {
                finding.affected_files.join(", ")
            };

            table.add_row(Row::new([
                Text::from_str(format!("{}", index + 1)),
                Text::from_str(&finding.title),
                Text::styled(&finding.severity, severity_style),
                Text::from_str(&files_display),
            ]));
        }

        let segments = table.render(self.width);
        print!("{}", segments.to_ansi());

        // Print details for each finding
        for (index, finding) in regressions.findings.iter().enumerate() {
            let content = Text::assemble([
                (
                    format!("Why: {}", finding.rationale),
                    Some(Style::new().dim()),
                ),
                ("\n".to_string(), None),
                (
                    format!("Check: {}", finding.suggested_check),
                    Some(Style::new().italic()),
                ),
            ]);

            let panel = Panel::new(content)
                .title(Text::from_str(format!("{}. {}", index + 1, finding.title)))
                .border_style(Style::new().dim())
                .width(self.width);

            let segments = panel.render(self.width);
            print!("{}", segments.to_ansi());
        }

        Ok(())
    }

    /// Print production readiness panel.
    pub fn print_prod_readiness(&self, readiness: &ProdReadinessReport) -> Result<()> {
        // Color based on score
        let score_color = if readiness.readiness_score >= 80 {
            Color::Standard(StandardColor::Green)
        } else if readiness.readiness_score >= 50 {
            Color::Standard(StandardColor::Yellow)
        } else {
            Color::Standard(StandardColor::Red)
        };

        let mut parts: Vec<(String, Option<Style>)> = vec![(
            format!(
                "Verdict: {} (score: {})",
                readiness.verdict, readiness.readiness_score
            ),
            Some(Style::new().bold().with_color(score_color)),
        )];

        // Add subsections
        append_section_parts(
            &mut parts,
            "Logging/Observability",
            &readiness.logging_and_observability,
        );
        append_section_parts(&mut parts, "Scalability", &readiness.scalability);
        append_section_parts(&mut parts, "Edge Cases", &readiness.edge_cases);
        append_section_parts(&mut parts, "Blocking Issues", &readiness.blocking_issues);

        let content = Text::assemble(parts);

        let panel = Panel::new(content)
            .title(Text::styled(
                "Production Readiness",
                Style::new()
                    .bold()
                    .with_color(Color::Standard(StandardColor::Magenta)),
            ))
            .border_style(Style::new().with_color(Color::Standard(StandardColor::Magenta)))
            .width(self.width);

        let segments = panel.render(self.width);
        print!("{}", segments.to_ansi());

        Ok(())
    }

    /// Print an error message.
    pub fn print_error(&self, message: &str) {
        println!("\x1b[1;31mError: {}\x1b[0m", message);
    }
}

impl Default for RichPrinter {
    fn default() -> Self {
        Self::new()
    }
}

/// Append a labeled section to parts.
fn append_section_parts(parts: &mut Vec<(String, Option<Style>)>, label: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    parts.push(("\n\n".to_string(), None));
    parts.push((format!("{}:", label), Some(Style::new().bold())));
    for item in items {
        parts.push((format!("\n  • {}", item), Some(Style::new().dim())));
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
/// a success (✓) or error (✗) indicator when complete.
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

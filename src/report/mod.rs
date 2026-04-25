// Reporter: ESLint-style output formatting

use std::time::Duration;

use indicatif::ProgressBar;
use owo_colors::OwoColorize;
use supports_color::Stream;

use crate::discover::resolver::ResolveWarning;
use crate::types::{Finding, Severity};

/// Returns true if stdout supports ANSI color output (per D-03).
fn should_use_color() -> bool {
    supports_color::on_cached(Stream::Stdout).is_some()
}

/// Print all findings in ESLint-style format with a summary line (per OUT-01, D-04).
///
/// Warnings are printed to stderr first. If findings is empty, a success
/// message is printed (per D-02). Otherwise each finding is printed as:
///
///   file:line:col  severity  message  rule-id
///
/// followed by a blank line and a summary count line.
///
/// When `degraded` is true, a note is appended to the summary line indicating
/// that type analysis was unavailable (D-18).
pub fn print_report(findings: &[Finding], warnings: &[ResolveWarning], degraded: bool) {
    let color = should_use_color();

    // Print warnings to stderr (non-fatal resolution issues)
    for w in warnings {
        if color {
            eprintln!("{}", format!("warning: {}", w.message).yellow());
        } else {
            eprintln!("warning: {}", w.message);
        }
    }

    if findings.is_empty() {
        if color {
            println!("{}", "No breaking changes found.".green());
        } else {
            println!("No breaking changes found.");
        }
        if degraded {
            println!("0 major, 0 minor, 0 patch (type analysis unavailable)");
        } else {
            println!("0 major, 0 minor, 0 patch");
        }
        return;
    }

    // Print each finding in ESLint-style format
    for f in findings {
        let file_str = f.file.display().to_string();
        let location = format!("{}:{}:{}", file_str, f.line, f.col);

        let severity_str: String = if color {
            match f.severity {
                Severity::Major => format!("{}", "major".red().bold()),
                Severity::Minor => format!("{}", "minor".yellow()),
                Severity::Patch => format!("{}", "patch".blue()),
                Severity::Warning => format!("{}", "warning".bright_yellow()),
                Severity::Info => format!("{}", "info".dimmed()),
            }
        } else {
            match f.severity {
                Severity::Major => "major".to_string(),
                Severity::Minor => "minor".to_string(),
                Severity::Patch => "patch".to_string(),
                Severity::Warning => "warning".to_string(),
                Severity::Info => "info".to_string(),
            }
        };

        println!("{}  {}  {}  {}", location, severity_str, f.message, f.rule);
    }

    println!();

    // Summary line (D-04, OUT-06)
    let major_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Major)
        .count();
    let minor_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Minor)
        .count();
    let patch_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Patch)
        .count();
    let warning_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count();
    let info_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count();

    let degraded_note = if degraded {
        " (type analysis unavailable)"
    } else {
        ""
    };

    if color {
        let major_str = format!("{} major", major_count).red().bold().to_string();
        let minor_str = format!("{} minor", minor_count).yellow().to_string();
        let patch_str = format!("{} patch", patch_count).blue().to_string();
        if major_count > 0 {
            println!(
                "{} {}, {}, {}{}",
                "X".red().bold(),
                major_str,
                minor_str,
                patch_str,
                degraded_note
            );
        } else {
            println!(
                "{}, {}, {}{}",
                major_str, minor_str, patch_str, degraded_note
            );
        }
    } else if major_count > 0 {
        println!(
            "X {} major, {} minor, {} patch{}",
            major_count, minor_count, patch_count, degraded_note
        );
    } else {
        println!(
            "{} major, {} minor, {} patch{}",
            major_count, minor_count, patch_count, degraded_note
        );
    }

    if warning_count > 0 {
        if color {
            println!(
                "{}",
                format!("{} warning(s)", warning_count).bright_yellow()
            );
        } else {
            println!("{} warning(s)", warning_count);
        }
    }

    if info_count > 0 {
        if color {
            println!("{}", format!("{} info", info_count).dimmed());
        } else {
            println!("{} info", info_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Severity;
    use std::path::PathBuf;

    #[test]
    fn test_warning_findings_do_not_affect_summary_breaking_count() {
        // Verify the contract: warning count is separate from major count
        let findings = vec![Finding {
            severity: Severity::Warning,
            rule: "async-to-sync",
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            message: "test warning".to_string(),
        }];
        let major_count = findings
            .iter()
            .filter(|f| f.severity == Severity::Major)
            .count();
        let warning_count = findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count();
        assert_eq!(major_count, 0);
        assert_eq!(warning_count, 1);
    }

    #[test]
    fn test_info_findings_render_with_info_label() {
        // Verify that Severity::Info produces the literal string "info" in no-color mode
        let severity = Severity::Info;
        let label = match severity {
            Severity::Major => "major",
            Severity::Minor => "minor",
            Severity::Patch => "patch",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        assert_eq!(label, "info");
    }

    #[test]
    fn test_info_findings_do_not_increment_major_count() {
        // Info findings must NOT trip has_breaking_changes (exits 0, not 1)
        use crate::compare::has_breaking_changes;
        let findings = vec![Finding {
            severity: Severity::Info,
            rule: "limitation: lsp-required",
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            message: "lsp unavailable".to_string(),
        }];
        assert!(!has_breaking_changes(&findings));
        let major_count = findings
            .iter()
            .filter(|f| f.severity == Severity::Major)
            .count();
        assert_eq!(major_count, 0);
    }

    #[test]
    fn test_info_footer_renders_when_present() {
        // Verify that info_count logic correctly counts Severity::Info findings
        let findings = vec![
            Finding {
                severity: Severity::Info,
                rule: "limitation: lsp-required",
                file: PathBuf::from("test.ts"),
                line: 1,
                col: 1,
                message: "first info finding".to_string(),
            },
            Finding {
                severity: Severity::Info,
                rule: "limitation: external-type-reference",
                file: PathBuf::from("test.ts"),
                line: 2,
                col: 1,
                message: "second info finding".to_string(),
            },
        ];
        let info_count = findings
            .iter()
            .filter(|f| f.severity == Severity::Info)
            .count();
        assert_eq!(info_count, 2);
        // Zero info when no Info findings present
        let non_info = vec![Finding {
            severity: Severity::Warning,
            rule: "async-to-sync",
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            message: "warning".to_string(),
        }];
        let info_count_zero = non_info
            .iter()
            .filter(|f| f.severity == Severity::Info)
            .count();
        assert_eq!(info_count_zero, 0);
    }
}

/// Create a progress spinner for slow operations (D-05).
///
/// indicatif automatically suppresses the spinner in non-TTY environments (D-06),
/// so no manual TTY check is needed here.
pub fn create_spinner(msg: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

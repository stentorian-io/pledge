// Reporter: ESLint-style output formatting

use std::time::Duration;

use indicatif::ProgressBar;
use owo_colors::OwoColorize;
use supports_color::Stream;

use crate::discover::resolver::ResolveWarning;
use crate::types::{ExportMap, Finding, Severity};

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
pub fn print_report(findings: &[Finding], warnings: &[ResolveWarning]) {
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
        println!("0 major, 0 minor, 0 patch");
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
            }
        } else {
            match f.severity {
                Severity::Major => "major".to_string(),
                Severity::Minor => "minor".to_string(),
                Severity::Patch => "patch".to_string(),
            }
        };

        println!(
            "{}  {}  {}  {}",
            location, severity_str, f.message, f.rule
        );
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

    if color {
        let major_str = format!("{} major", major_count).red().bold().to_string();
        let minor_str = format!("{} minor", minor_count).yellow().to_string();
        let patch_str = format!("{} patch", patch_count).blue().to_string();
        if major_count > 0 {
            println!("{} {}, {}, {}", "X".red().bold(), major_str, minor_str, patch_str);
        } else {
            println!("{}, {}, {}", major_str, minor_str, patch_str);
        }
    } else if major_count > 0 {
        println!("X {} major, {} minor, {} patch", major_count, minor_count, patch_count);
    } else {
        println!("{} major, {} minor, {} patch", major_count, minor_count, patch_count);
    }
}

/// Print a first-publish report (PKG-03): all current exports listed as added,
/// with a success message since there are no breaking changes.
pub fn print_first_publish_report(exports: &ExportMap) {
    let color = should_use_color();

    for (name, sym) in exports {
        let file_str = sym.file.display().to_string();
        let location = format!("{}:{}:{}", file_str, sym.line, sym.col);
        let severity_str = if color {
            format!("{}", "minor".yellow())
        } else {
            "minor".to_string()
        };
        println!(
            "{}  {}  exported symbol `{}` was added (first publish)  export-added",
            location, severity_str, name
        );
    }

    println!();

    let count = exports.len();
    if color {
        println!("{}", "No breaking changes found.".green());
        println!(
            "{}, {} minor, 0 patch",
            "0 major".red().bold(),
            count
        );
    } else {
        println!("No breaking changes found.");
        println!("0 major, {} minor, 0 patch", count);
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

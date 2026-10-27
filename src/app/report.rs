//! Human-readable and versioned machine-readable reports.

use crate::app::cli::{ColorMode, OutputFormat};
use crate::app::RunOutcome;
use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::acquire::PackageComparison;
use crate::ecosystems::node::discovery::resolver::ResolveWarning;
use indicatif::ProgressBar;
use owo_colors::OwoColorize;
use serde_json::{json, Value};
use std::io::{self, IsTerminal, Write};
use std::time::Duration;
use supports_color::Stream;

fn use_color(mode: ColorMode, stream: Stream) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            let terminal = match stream {
                Stream::Stdout => io::stdout().is_terminal(),
                Stream::Stderr => io::stderr().is_terminal(),
            };
            terminal && supports_color::on_cached(stream).is_some()
        }
    }
}

fn severity_label(severity: &Severity) -> &'static str {
    match severity {
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Patch => "patch",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn write_json(report: &Value) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, report)?;
    writeln!(stdout)?;
    Ok(())
}

pub fn print_error(format: OutputFormat, error: &anyhow::Error) -> anyhow::Result<()> {
    let message = format!("{error:#}");
    if format == OutputFormat::Json {
        write_json(
            &json!({"schema_version": 1, "status": "error", "exit_code": 2, "error": message}),
        )
    } else {
        writeln!(io::stderr().lock(), "error: {message}")?;
        Ok(())
    }
}

pub(crate) fn print_cache_cleared(format: OutputFormat) -> anyhow::Result<()> {
    if format == OutputFormat::Json {
        write_json(&json!({"schema_version": 1, "status": "cache-cleared", "exit_code": 0}))
    } else {
        writeln!(io::stdout().lock(), "Cache cleared.")?;
        Ok(())
    }
}

pub(crate) fn print_report(
    format: OutputFormat,
    color: ColorMode,
    package: &PackageComparison,
    findings: &[Finding],
    warnings: &[ResolveWarning],
    outcome: RunOutcome,
) -> anyhow::Result<()> {
    let count = |severity| {
        findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    };
    let summary = json!({
        "major": count(Severity::Major), "minor": count(Severity::Minor),
        "patch": count(Severity::Patch), "warning": count(Severity::Warning), "info": count(Severity::Info),
    });
    if format == OutputFormat::Json {
        let findings: Vec<_> = findings.iter().map(|finding| json!({
            "severity": severity_label(&finding.severity), "rule": finding.rule,
            "file": finding.file, "line": finding.line, "col": finding.col, "message": finding.message,
        })).collect();
        let warnings: Vec<_> = warnings
            .iter()
            .map(|warning| {
                json!({
                    "message": warning.message, "incomplete": warning.incomplete,
                })
            })
            .collect();
        return write_json(&json!({
            "schema_version": 1, "status": outcome.label(), "exit_code": outcome.exit_code(),
            "package": package, "summary": summary, "findings": findings, "warnings": warnings,
        }));
    }

    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    for warning in warnings {
        let message = format!("warning: {}", warning.message);
        if use_color(color, Stream::Stderr) {
            writeln!(stderr, "{}", message.yellow())?;
        } else {
            writeln!(stderr, "{message}")?;
        }
    }
    if let Some(baseline) = &package.baseline_version {
        writeln!(
            stdout,
            "{}: {} → {}",
            package.name, baseline, package.current_version
        )?;
    } else {
        writeln!(
            stdout,
            "{}@{}: first publish (no published baseline found)",
            package.name, package.current_version
        )?;
    }
    if outcome == RunOutcome::Degraded {
        writeln!(
            stdout,
            "Analysis is incomplete; compatibility could not be established."
        )?;
    } else if findings.is_empty() {
        writeln!(stdout, "No breaking changes found.")?;
    }
    let color = use_color(color, Stream::Stdout);
    for finding in findings {
        let label = severity_label(&finding.severity);
        let label = if color {
            match finding.severity {
                Severity::Major => label.red().bold().to_string(),
                Severity::Warning => label.yellow().to_string(),
                Severity::Minor => label.cyan().to_string(),
                Severity::Patch => label.blue().to_string(),
                Severity::Info => label.dimmed().to_string(),
            }
        } else {
            label.to_owned()
        };
        writeln!(
            stdout,
            "{}:{}:{}  {}  {}  {}",
            finding.file.display(),
            finding.line,
            finding.col,
            label,
            finding.message,
            finding.rule
        )?;
    }
    writeln!(
        stdout,
        "{} major, {} minor, {} patch, {} warning, {} info{}",
        summary["major"],
        summary["minor"],
        summary["patch"],
        summary["warning"],
        summary["info"],
        if outcome == RunOutcome::Degraded {
            " (incomplete analysis)"
        } else {
            ""
        }
    )?;
    Ok(())
}

pub(crate) fn create_spinner(format: OutputFormat, message: &str) -> ProgressBar {
    let spinner = if format == OutputFormat::Json || !io::stderr().is_terminal() {
        ProgressBar::hidden()
    } else {
        ProgressBar::new_spinner()
    };
    spinner.set_message(message.to_owned());
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

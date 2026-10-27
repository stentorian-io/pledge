//! Bounded npm subprocesses shared by acquisition and dependency preparation.

use anyhow::Context;
use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;

pub(crate) fn command(directory: &Path) -> Command {
    let mut command = Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" });
    command
        .current_dir(directory)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command
}

pub(crate) async fn output(command: &mut Command, operation: &str) -> anyhow::Result<Output> {
    tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .with_context(|| format!("npm {operation} timed out after 120 seconds"))?
        .with_context(|| {
            format!("Could not run npm {operation}; install Node.js and npm and put them on PATH")
        })
}

pub(crate) fn failure(output: &Output, operation: &str) -> anyhow::Error {
    let detail = String::from_utf8_lossy(&output.stderr);
    anyhow::anyhow!(
        "npm {operation} failed ({}): {}",
        output.status,
        detail.trim()
    )
}

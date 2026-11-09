//! Batch independent consumer checks through the installed TypeScript CLI.
#![cfg(feature = "lsp-tests")]

use serde::{Deserialize, Serialize};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Default)]
pub(crate) struct TscBatch {
    cases: Vec<Consumer>,
    verified: bool,
}

struct Consumer {
    directory: tempfile::TempDir,
    args: Vec<String>,
    succeeds: bool,
    label: String,
}

#[derive(Serialize)]
struct Request<'a> {
    id: usize,
    root: PathBuf,
    args: &'a [String],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Outcome {
    id: usize,
    code: i32,
    stdout: String,
    stderr: String,
}

impl TscBatch {
    pub(crate) fn push(
        &mut self,
        directory: tempfile::TempDir,
        args: &[&str],
        succeeds: bool,
        label: String,
    ) {
        self.cases.push(Consumer {
            directory,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            succeeds,
            label,
        });
    }

    pub(crate) fn assert_outcomes(self) {
        if std::env::var_os("PLEDGE_VERIFY_TSC_BATCH").is_some() {
            self.assert_matches_cli();
        } else {
            self.verify(false);
        }
    }

    pub(crate) fn assert_matches_cli(self) {
        assert!(!self.cases.is_empty(), "CLI parity needs consumer cases");
        self.verify(true);
    }

    fn verify(mut self, compare_cli: bool) {
        // Node-only consumer topics have no TypeScript work to batch.
        if self.cases.is_empty() {
            return;
        }
        let cli = selected_cli();
        let library = cli.parent().unwrap().parent().unwrap().join("lib");
        assert!(
            library.join("tsc.js").is_file(),
            "consumer checks require the JavaScript TypeScript CLI selected on PATH: {}",
            cli.display()
        );
        let requests: Vec<_> = self
            .cases
            .iter()
            .enumerate()
            .map(|(id, case)| Request {
                id,
                root: case.directory.path().canonicalize().unwrap(),
                args: &case.args,
            })
            .collect();
        let mut input = tempfile::tempfile().unwrap();
        serde_json::to_writer(&mut input, &requests).unwrap();
        input.flush().unwrap();
        input.rewind().unwrap();
        let output = Command::new("node")
            .args(["--max-old-space-size=512", "-e", include_str!("tsc.cjs")])
            .arg(&library)
            .stdin(Stdio::from(input))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "consumer CLI batch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let outcomes: Vec<Outcome> =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "invalid consumer CLI results: {error}: {}",
                    String::from_utf8_lossy(&output.stdout)
                )
            });
        assert_eq!(
            outcomes.len(),
            self.cases.len(),
            "consumer CLI skipped cases"
        );
        for (id, (case, outcome)) in self.cases.iter().zip(outcomes).enumerate() {
            assert_eq!(outcome.id, id, "consumer CLI returned a mismatched case");
            if compare_cli {
                assert_cli_equivalent(&cli, case, &outcome);
            }
            assert_eq!(
                outcome.code == 0,
                case.succeeds,
                "{}: {}{}",
                case.label,
                outcome.stdout,
                outcome.stderr
            );
        }
        self.verified = true;
    }
}

impl Drop for TscBatch {
    fn drop(&mut self) {
        assert!(
            self.verified || self.cases.is_empty() || std::thread::panicking(),
            "queued TypeScript consumers were not checked"
        );
    }
}

fn selected_cli() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("PATH is required"))
        .map(|directory| directory.join("tsc"))
        .find(|candidate| candidate.is_file())
        .expect("install the documented JavaScript TypeScript compiler on PATH")
        .canonicalize()
        .unwrap()
}

fn assert_cli_equivalent(cli: &Path, case: &Consumer, outcome: &Outcome) {
    let expected = Command::new(cli)
        .args(&case.args)
        .current_dir(case.directory.path())
        .output()
        .unwrap();
    assert_eq!(
        Some(outcome.code),
        expected.status.code(),
        "{}: CLI exit code differs",
        case.label
    );
    assert_eq!(
        outcome.stdout.as_bytes(),
        expected.stdout,
        "{}: CLI stdout differs",
        case.label
    );
    assert_eq!(
        outcome.stderr.as_bytes(),
        expected.stderr,
        "{}: CLI stderr differs",
        case.label
    );
}

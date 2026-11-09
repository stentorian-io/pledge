//! A caller-owned compiler process; programs and source files are never cached.

use super::toolchain::CompilerBinary;
use serde::{de::DeserializeOwned, Deserialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;

const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub(super) struct WorkerSession {
    worker: Option<Worker>,
    sequence: u64,
}

#[derive(Deserialize)]
struct Reply<T> {
    sequence: u64,
    response: T,
}

#[derive(PartialEq)]
struct ExecutableIdentity {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
}

impl ExecutableIdentity {
    fn read(path: &Path) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let metadata = path.metadata().map_err(|error| error.to_string())?;
        Ok(Self {
            path,
            modified: metadata.modified().ok(),
            length: metadata.len(),
        })
    }
}

struct Worker {
    node: ExecutableIdentity,
    compiler: ExecutableIdentity,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    errors: Arc<Mutex<Vec<u8>>>,
    drain_errors: JoinHandle<()>,
}

impl WorkerSession {
    pub(super) async fn request<T: DeserializeOwned>(
        &mut self,
        compiler: &CompilerBinary,
        input: &serde_json::Value,
    ) -> Result<T, String> {
        self.request_with_timeout(compiler, input, Duration::from_secs(30))
            .await
    }

    async fn request_with_timeout<T: DeserializeOwned>(
        &mut self,
        compiler: &CompilerBinary,
        input: &serde_json::Value,
        budget: Duration,
    ) -> Result<T, String> {
        // Taking ownership before awaiting also kills a worker if the caller
        // cancels this future. Its unread reply cannot satisfy a later request.
        let previous = self.worker.take();
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("compiler request sequence exhausted")?;
        let sequence = self.sequence;
        let result = tokio::time::timeout(budget, async move {
            let node = ExecutableIdentity::read(&compiler.node)?;
            let library = ExecutableIdentity::read(&compiler.library)?;
            let mut worker = match previous {
                Some(worker) if worker.node == node && worker.compiler == library => worker,
                _ => Worker::start(node, library)?,
            };
            let request = serde_json::json!({"sequence": sequence, "input": input});
            let response = worker.exchange(&request).await?;
            let reply: Reply<T> = serde_json::from_slice(&response)
                .map_err(|error| format!("invalid compiler response: {error}"))?;
            if reply.sequence != sequence {
                return Err("compiler replied to a different request".into());
            }
            Ok::<_, String>((reply.response, worker))
        })
        .await
        .map_err(|_| "compiler public type evaluation timed out".to_owned())??;
        self.worker = Some(result.1);
        Ok(result.0)
    }
}

impl Worker {
    fn start(node: ExecutableIdentity, compiler: ExecutableIdentity) -> Result<Self, String> {
        let mut child = Command::new(&node.path)
            .args([
                "--max-old-space-size=512",
                "-e",
                concat!(
                    include_str!("compiler.cjs"),
                    "\n",
                    include_str!("worker.cjs")
                ),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("compiler startup failed: {error}"))?;
        let input = child.stdin.take().ok_or("compiler input unavailable")?;
        let output = child.stdout.take().ok_or("compiler output unavailable")?;
        let mut stderr = child.stderr.take().ok_or("compiler errors unavailable")?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&errors);
        let drain_errors = tokio::spawn(async move {
            let mut chunk = [0; 1024];
            while let Ok(count) = stderr.read(&mut chunk).await {
                if count == 0 {
                    break;
                }
                if let Ok(mut errors) = captured.lock() {
                    let remaining = 1000_usize.saturating_sub(errors.len());
                    errors.extend_from_slice(&chunk[..count.min(remaining)]);
                }
            }
        });
        Ok(Self {
            node,
            compiler,
            child,
            input,
            output: BufReader::new(output),
            errors,
            drain_errors,
        })
    }

    async fn exchange(&mut self, request: &serde_json::Value) -> Result<Vec<u8>, String> {
        let mut input = request.to_string().into_bytes();
        input.push(b'\n');
        self.input
            .write_all(&input)
            .await
            .map_err(|error| format!("compiler input failed: {error}"))?;
        let mut response = Vec::new();
        (&mut self.output)
            .take((OUTPUT_LIMIT + 1) as u64)
            .read_until(b'\n', &mut response)
            .await
            .map_err(|error| format!("compiler output failed: {error}"))?;
        if response.len() > OUTPUT_LIMIT {
            return Err("compiler output budget exhausted".into());
        }
        if response.last() != Some(&b'\n') {
            let detail = self
                .errors
                .lock()
                .map(|errors| String::from_utf8_lossy(&errors).into_owned())
                .unwrap_or_default();
            return Err(format!("compiler closed output before replying: {detail}"));
        }
        Ok(response)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.drain_errors.abort();
    }
}

#[cfg(all(test, unix))]
mod tests;

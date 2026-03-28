//! LSP client lifecycle: spawn typescript-language-server, initialize, and shutdown.
//!
//! Per decisions D-04, D-06, D-07.

use std::ops::ControlFlow;
use std::path::Path;
use std::process::Stdio;

use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::lsp_types::{
    notification::{LogMessage, Progress, PublishDiagnostics, ShowMessage},
    ClientCapabilities, InitializeParams, InitializedParams, WindowClientCapabilities,
    WorkspaceFolder,
};
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::tracing::TracingLayer;
use async_lsp::{LanguageServer, ServerSocket};
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::ServiceBuilder;

use super::managed::LspBinary;
use super::LspError;

/// State for the async-lsp client router.
struct ClientState;

/// An active LSP session with a typescript-language-server process.
pub struct LspSession {
    /// The async-lsp server socket used to send LSP requests.
    pub server: ServerSocket,
    /// The background task running the async-lsp main loop.
    _mainloop_task: JoinHandle<()>,
    /// The spawned child process (kept for kill_on_drop).
    _child: tokio::process::Child,
}

impl LspSession {
    /// Gracefully shut down the LSP server.
    pub async fn shutdown(mut self) -> Result<(), LspError> {
        self.server
            .shutdown(())
            .await
            .map_err(|e| LspError::RequestFailed(e.to_string()))?;
        // Send exit notification (best-effort)
        let _ = self.server.exit(());
        // The mainloop task will complete when the server exits.
        // We don't wait to avoid hanging if the server is unresponsive.
        Ok(())
    }
}

/// Spawn and initialize a typescript-language-server for the given project root.
///
/// Writes a synthetic tsconfig.json (per D-14), runs npm install (per D-15),
/// then starts the LSP session and completes the initialize handshake.
pub async fn start_session(binary: &LspBinary, root_dir: &Path) -> Result<LspSession, LspError> {
    // Synthesize tsconfig.json if not already present (D-14)
    maybe_write_tsconfig(root_dir).await?;

    // Run npm install --production (D-15, non-fatal)
    run_npm_install(root_dir).await;

    // Build the command
    let mut cmd = build_command(binary);
    cmd.current_dir(root_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| LspError::InitFailed(format!("failed to spawn LSP process: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LspError::InitFailed("failed to capture LSP stdout".to_string()))?
        .compat(); // Pitfall 3: tokio::io → futures::io bridge

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| LspError::InitFailed("failed to capture LSP stdin".to_string()))?
        .compat_write(); // Pitfall 3: tokio::io → futures::io bridge

    // Build the async-lsp main loop with middleware layers
    // Register handlers for all common server-to-client notifications to avoid
    // the default router error behavior for unhandled non-$/ notifications.
    let (mainloop, mut server) = async_lsp::MainLoop::new_client(|_server| {
        let mut router = Router::new(ClientState);
        router
            .notification::<Progress>(|_, _| ControlFlow::Continue(()))
            .notification::<PublishDiagnostics>(|_, _| ControlFlow::Continue(()))
            .notification::<ShowMessage>(|_, params| {
                tracing::debug!("LSP showMessage: {:?}", params.message);
                ControlFlow::Continue(())
            })
            .notification::<LogMessage>(|_, params| {
                tracing::debug!("LSP logMessage: {}", params.message);
                ControlFlow::Continue(())
            });
        ServiceBuilder::new()
            .layer(TracingLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(ConcurrencyLayer::default())
            .service(router)
    });

    // Spawn the main loop as a background task
    let mainloop_task = tokio::spawn(async move {
        if let Err(e) = mainloop.run_buffered(stdout, stdin).await {
            tracing::debug!("LSP mainloop ended: {e}");
        }
    });

    // Complete the LSP initialize handshake (Pitfall 2: must follow strict sequence)
    let root_uri = async_lsp::lsp_types::Url::from_file_path(root_dir)
        .map_err(|_| LspError::InitFailed(format!("invalid root path: {}", root_dir.display())))?;

    server
        .initialize(InitializeParams {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: "root".into(),
            }]),
            capabilities: ClientCapabilities {
                window: Some(WindowClientCapabilities {
                    work_done_progress: Some(true),
                    ..WindowClientCapabilities::default()
                }),
                ..ClientCapabilities::default()
            },
            ..InitializeParams::default()
        })
        .await
        .map_err(|e| LspError::InitFailed(format!("initialize request failed: {e}")))?;

    // Must send `initialized` notification after receiving `initialize` response (Pitfall 2)
    server
        .initialized(InitializedParams {})
        .map_err(|e| LspError::InitFailed(format!("initialized notification failed: {e}")))?;

    Ok(LspSession {
        server,
        _mainloop_task: mainloop_task,
        _child: child,
    })
}

/// Build a `tokio::process::Command` from the resolved binary spec.
fn build_command(binary: &LspBinary) -> tokio::process::Command {
    match binary {
        LspBinary::Direct(path) => {
            let mut cmd = tokio::process::Command::new(path);
            cmd.arg("--stdio");
            cmd
        }
        LspBinary::Managed { node, script } => {
            let mut cmd = tokio::process::Command::new(node);
            cmd.arg(script).arg("--stdio");
            cmd
        }
    }
}

/// Write a minimal `tsconfig.json` to `root_dir` if one does not already exist.
/// Per D-14: permissive settings to enable JS/TS analysis without strict checks.
async fn maybe_write_tsconfig(root_dir: &Path) -> Result<(), LspError> {
    let tsconfig_path = root_dir.join("tsconfig.json");
    if tsconfig_path.exists() {
        return Ok(());
    }

    let tsconfig = serde_json::json!({
        "compilerOptions": {
            "strict": false,
            "allowJs": true,
            "checkJs": true,
            "moduleResolution": "node",
            "target": "ES2020",
            "module": "commonjs",
            "esModuleInterop": true,
            "skipLibCheck": true,
            "resolveJsonModule": true
        },
        "include": ["**/*"]
    });

    let content = serde_json::to_string_pretty(&tsconfig)
        .map_err(|e| LspError::InitFailed(format!("failed to serialize tsconfig: {e}")))?;

    tokio::fs::write(&tsconfig_path, content)
        .await
        .map_err(|e| LspError::InitFailed(format!("failed to write tsconfig.json: {e}")))?;

    Ok(())
}

/// Run `npm install --production --ignore-scripts` in the package root.
/// Per D-15: non-fatal — failure just means node_modules won't resolve.
async fn run_npm_install(root_dir: &Path) {
    // Use async Command to avoid blocking the tokio runtime (Pitfall 4)
    match tokio::process::Command::new("npm")
        .args(["install", "--production", "--ignore-scripts"])
        .current_dir(root_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
    {
        Ok(status) => {
            if !status.success() {
                tracing::debug!("npm install exited with {status}; node_modules may be incomplete");
            }
        }
        Err(e) => {
            tracing::debug!("npm install failed (non-fatal): {e}");
        }
    }
}

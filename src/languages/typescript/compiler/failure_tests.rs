use crate::ecosystems::node::discovery;
use crate::languages::typescript::compiler::{toolchain, *};

async fn run_mock(
    hover_response: &str,
    budget: std::time::Duration,
    remove_source: bool,
) -> crate::languages::typescript::compiler::EnrichResult {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"mock","main":"index.ts"}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("index.ts"),
        "export function api(x: string): void {}",
    )
    .unwrap();
    let (mut exports, warnings) = discovery::discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(!exports.is_empty());
    if remove_source {
        std::fs::remove_file(dir.path().join("index.ts")).unwrap();
    }

    let script = dir.path().join("mock.mjs");
    std::fs::write(
        &script,
        format!(
            r#"
let buffer = Buffer.alloc(0);
function send(body) {{
    const bytes = Buffer.from(JSON.stringify(body));
    process.stdout.write(`Content-Length: ${{bytes.length}}\r\n\r\n`);
    process.stdout.write(bytes);
}}
process.stdin.on('data', chunk => {{
    buffer = Buffer.concat([buffer, chunk]);
    while (true) {{
        const end = buffer.indexOf('\r\n\r\n');
        if (end < 0) return;
        const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, end).toString())[1]);
        if (buffer.length < end + 4 + length) return;
        const msg = JSON.parse(buffer.subarray(end + 4, end + 4 + length));
        buffer = buffer.subarray(end + 4 + length);
        const id = msg.id;
        if (msg.method === 'initialize') {{
            if (!msg.params.rootUri) process.exit(2);
            send({{jsonrpc:'2.0', id, result:{{capabilities:{{hoverProvider:true}}}}}});
        }} else if (msg.method === 'textDocument/hover') {{
            {hover_response}
        }} else if (msg.method === 'shutdown') {{
            send({{jsonrpc:'2.0', id, result:null}});
        }} else if (msg.method === 'exit') {{ process.exit(0); }}
    }}
}});
"#
        ),
    )
    .unwrap();
    crate::languages::typescript::compiler::enrich_with_binary(
        &mut exports,
        dir.path(),
        &toolchain::LspBinary::Managed {
            node: PathBuf::from("node"),
            script,
        },
        budget,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_single_rejected_hover_degrades_analysis() {
    let result = run_mock(
        "send({jsonrpc:'2.0', id, error:{code:-32603,message:'hover rejected'}});",
        std::time::Duration::from_secs(10),
        false,
    )
    .await;
    assert!(
        matches!(result, crate::languages::typescript::compiler::EnrichResult::Degraded(ref cause) if cause.contains("hover rejected")),
        "{result:?}"
    );
}

#[tokio::test]
async fn server_exit_during_hover_degrades_analysis() {
    let result = run_mock(
        "process.exit(1);",
        std::time::Duration::from_secs(10),
        false,
    )
    .await;
    assert!(
        matches!(
            result,
            crate::languages::typescript::compiler::EnrichResult::Degraded(_)
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn empty_hover_degrades_analysis() {
    let result = run_mock(
        "send({jsonrpc:'2.0', id, result:null});",
        std::time::Duration::from_secs(10),
        false,
    )
    .await;
    assert!(
        matches!(result, crate::languages::typescript::compiler::EnrichResult::Degraded(ref cause) if cause.contains("no type information")),
        "{result:?}"
    );
}

#[tokio::test]
async fn missing_source_degrades_analysis() {
    let result = run_mock("", std::time::Duration::from_secs(10), true).await;
    assert!(
        matches!(result, crate::languages::typescript::compiler::EnrichResult::Degraded(ref cause) if cause.contains("cannot read")),
        "{result:?}"
    );
}

#[tokio::test]
async fn exhausted_budget_degrades_analysis() {
    let result = run_mock("", std::time::Duration::ZERO, false).await;
    assert!(
        matches!(result, crate::languages::typescript::compiler::EnrichResult::Degraded(ref cause) if cause.contains("budget exhausted")),
        "{result:?}"
    );
}

#[tokio::test]
async fn stalled_request_cannot_overrun_total_budget() {
    let result = run_mock("", std::time::Duration::from_millis(100), false).await;
    assert!(
        matches!(result, crate::languages::typescript::compiler::EnrichResult::Degraded(ref cause) if cause.contains("budget exhausted")),
        "{result:?}"
    );
}

#[tokio::test]
async fn valid_hover_completes_analysis() {
    let result = run_mock("send({jsonrpc:'2.0', id, result:{contents:{language:'typescript',value:'function api(x: string): void'}}});", std::time::Duration::from_secs(10), false).await;
    assert!(
        matches!(
            result,
            crate::languages::typescript::compiler::EnrichResult::Enriched
        ),
        "{result:?}"
    );
}

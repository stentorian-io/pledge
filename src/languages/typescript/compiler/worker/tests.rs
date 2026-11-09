use super::*;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

fn toolchain(directory: &std::path::Path, name: &str, script: &str) -> CompilerBinary {
    let node = directory.join(name);
    std::fs::write(&node, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
    let library = directory.join(format!("{name}.js"));
    std::fs::write(&library, "compiler fixture").unwrap();
    CompilerBinary { node, library }
}

const REPLIES: &str = r#"
while IFS= read -r request; do
  sequence=${request#*:}; sequence=${sequence%%,*}
  case "$request" in
    *stall*) ;;
    *) printf '{"sequence":%s,"response":{"pid":%s}}\n' "$sequence" "$$";;
  esac
done
"#;

#[tokio::test]
async fn repeated_requests_reuse_only_the_selected_unchanged_toolchain() {
    let directory = tempfile::tempdir().unwrap();
    let first = toolchain(directory.path(), "first", REPLIES);
    let second = toolchain(directory.path(), "second", REPLIES);
    let mut session = WorkerSession::default();

    let before: Value = session.request(&first, &json!({})).await.unwrap();
    let repeated: Value = session.request(&first, &json!({})).await.unwrap();
    let switched: Value = session.request(&second, &json!({})).await.unwrap();
    std::fs::write(
        &second.library,
        "replacement compiler with different length",
    )
    .unwrap();
    let replaced: Value = session.request(&second, &json!({})).await.unwrap();

    assert_eq!(before, repeated);
    assert_ne!(before, switched);
    assert_ne!(switched, replaced);
}

#[tokio::test]
async fn timeout_and_cancellation_discard_pending_replies_before_restarting() {
    let directory = tempfile::tempdir().unwrap();
    let compiler = toolchain(directory.path(), "node", REPLIES);
    let mut session = WorkerSession::default();
    let first: Value = session.request(&compiler, &json!({})).await.unwrap();

    let error = session
        .request_with_timeout::<Value>(
            &compiler,
            &json!({"stall": true}),
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
    let restarted: Value = session.request(&compiler, &json!({})).await.unwrap();
    assert!(error.contains("timed out"), "{error}");
    assert_ne!(first, restarted);

    let cancelled = tokio::time::timeout(
        Duration::from_millis(20),
        session.request::<Value>(&compiler, &json!({"stall": true})),
    )
    .await;
    let recovered: Value = session.request(&compiler, &json!({})).await.unwrap();
    assert!(cancelled.is_err());
    assert_ne!(restarted, recovered);
}

#[tokio::test]
async fn invalid_truncated_and_oversized_replies_fail_and_allow_the_next_request() {
    let directory = tempfile::tempdir().unwrap();
    let good = toolchain(directory.path(), "good", REPLIES);
    let mut session = WorkerSession::default();
    for (name, script, expected) in [
        (
            "invalid",
            "read request; printf 'invalid\\n'",
            "invalid compiler response",
        ),
        ("truncated", "read request; printf '{}'", "closed output"),
        ("crashed", "read request; exit 7", "closed output"),
        (
            "wrong-request",
            "read request; printf '{\"sequence\":999,\"response\":{}}\\n'",
            "different request",
        ),
        (
            "oversized",
            "read request; head -c 16777217 /dev/zero",
            "output budget exhausted",
        ),
    ] {
        let compiler = toolchain(directory.path(), name, script);
        let error = session
            .request::<Value>(&compiler, &json!({}))
            .await
            .unwrap_err();
        assert!(error.contains(expected), "{name}: {error}");
        let recovered: Value = session.request(&good, &json!({})).await.unwrap();
        assert!(recovered["pid"].as_u64().is_some());
    }
}

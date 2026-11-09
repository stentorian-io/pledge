//! Reused processes must observe the same evidence as isolated evaluations.
#![cfg(feature = "lsp-tests")]

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::typescript::compiler::{CompilerSession, EnrichResult};
use pledge::languages::typescript::types::shape::{PrimitiveKind, TypeShape};
use std::path::Path;

fn package() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"session-contract","types":"index.d.ts"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("index.d.ts"),
        "import type {Kind} from 'contract'; export declare const api: Kind;",
    )
    .unwrap();
    configure(root, "one.d.ts");
    std::fs::write(root.join("one.d.ts"), "export type Kind = string;").unwrap();
    std::fs::write(root.join("two.d.ts"), "export type Kind = number;").unwrap();
    directory
}

fn configure(root: &Path, target: &str) {
    std::fs::write(
        root.join("tsconfig.json"),
        serde_json::json!({
            "compilerOptions": { "baseUrl": ".", "paths": { "contract": [target] } }
        })
        .to_string(),
    )
    .unwrap();
}

async fn assert_type(session: &mut CompilerSession, root: &Path, kind: PrimitiveKind) {
    let (mut exports, warnings) = discover_exports(root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let result = session.enrich_exports(&mut exports, root).await.unwrap();
    assert!(matches!(result, EnrichResult::Enriched), "{result:?}");
    let api = exports
        .values()
        .find(|symbol| symbol.name == "api")
        .unwrap();
    assert_eq!(api.overloads, vec![TypeShape::Primitive(kind)]);
}

#[tokio::test]
async fn reused_processes_refresh_roots_configs_sources_and_missing_evidence() {
    let first = package();
    let second = package();
    configure(second.path(), "two.d.ts");
    let mut session = CompilerSession::default();

    assert_type(&mut session, first.path(), PrimitiveKind::String).await;
    assert_type(&mut session, second.path(), PrimitiveKind::Number).await;
    configure(first.path(), "two.d.ts");
    assert_type(&mut session, first.path(), PrimitiveKind::Number).await;
    std::fs::write(first.path().join("two.d.ts"), "export type Kind = boolean;").unwrap();
    assert_type(&mut session, first.path(), PrimitiveKind::Boolean).await;
    std::fs::remove_file(first.path().join("two.d.ts")).unwrap();
    let (mut exports, _) = discover_exports(first.path()).unwrap();
    let missing = session
        .enrich_exports(&mut exports, first.path())
        .await
        .unwrap();
    assert!(matches!(missing, EnrichResult::Degraded(_)), "{missing:?}");
    assert_type(&mut session, second.path(), PrimitiveKind::Number).await;

    drop(first);
    assert_type(&mut session, second.path(), PrimitiveKind::Number).await;
}

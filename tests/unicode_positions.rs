#![cfg(feature = "lsp-tests")]

use interlint::{discover::discover_exports, lsp};

#[tokio::test]
async fn unicode_before_typeof_import_preserves_definition_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"unicode-references","type":"module","exports":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(root.join("index.ts"), "export {api} from './dependency.js';\n/* 🧭🧭🧭🧭 café */ export type Api = typeof import('./dependency.js').api;").unwrap();
    let declaration = "export declare function api(): string;";
    std::fs::write(root.join("dependency.ts"), declaration).unwrap();
    let (mut exports, warnings) = discover_exports(root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let result = lsp::enrich_exports(&mut exports, root).await.unwrap();
    assert!(matches!(result, lsp::EnrichResult::Enriched), "{result:?}");
    let locations: Vec<_> = exports
        .typeof_targets
        .values()
        .flat_map(|targets| targets.values())
        .filter_map(Option::as_ref)
        .collect();
    assert!(!locations.is_empty(), "{exports:?}");
    for location in locations {
        assert!(
            location.uri.path().ends_with("dependency.ts"),
            "{location:?}"
        );
        assert_eq!(location.range.start.line, 0);
        assert_eq!(
            location.range.start.character,
            declaration.find("api").unwrap() as u32
        );
    }
}

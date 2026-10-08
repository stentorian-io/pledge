//! Reflective wrappers have statically known subsets and explicit runtime gaps.
use interlint::{compare, discover::discover_exports};

fn package(source: &str, extension: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("package.json"), serde_json::json!({"name":"runtime-contracts","type":"module","exports":format!("./index.{extension}")}).to_string()).unwrap();
    std::fs::write(directory.path().join(format!("index.{extension}")), source).unwrap();
    directory
}

#[test]
fn opaque_dispatch_on_either_side_is_incomplete() {
    for (old, new) in [
        (
            "export const api=new Proxy({}, { get:()=>process.env.VALUE });",
            "export const api={};",
        ),
        (
            "export const api={};",
            "export const api=new Proxy({}, { get:()=>process.env.VALUE });",
        ),
        (
            "export default {};",
            "const wrapped=new Proxy({}, {get:()=>1}); export default wrapped;",
        ),
    ] {
        let old = package(old, "js");
        let new = package(new, "js");
        let (before, _) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings = compare::compare_exports(&before, &after, new.path(), false);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "limitation: proxy-dispatched-exports"),
            "{findings:?}"
        );
    }
}

#[test]
fn shared_handlers_and_custom_constructor_prototypes_remain_explicit() {
    for source in [
        "const handler={}; export const api=new Proxy({}, handler);",
        "class Target {} class Prototype {} export const api=Reflect.construct(Target,[],Prototype);",
    ] {
        let root = package(source, "js");
        let (exports, _) = discover_exports(root.path()).unwrap();
        assert!(exports.values().any(|symbol| symbol.is_proxy_dispatched), "{source}: {exports:?}");
    }
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn transparent_wrappers_retain_their_node_observable_contracts() {
    let cases = [
        (
            "cjs",
            "module.exports=new Proxy({value:'x'},{});",
            "module.exports=new Proxy({other:'x'},{});",
            "require('./index.cjs').value.toUpperCase();",
        ),
        (
            "cjs",
            "module.exports=new Proxy({value:'x'},{get:Reflect.get});",
            "module.exports=new Proxy({other:'x'},{get:Reflect.get});",
            "require('./index.cjs').value.toUpperCase();",
        ),
        (
            "cjs",
            "module.exports=Reflect.construct(class {value='x'},[]);",
            "module.exports=Reflect.construct(class {other='x'},[]);",
            "require('./index.cjs').value.toUpperCase();",
        ),
        (
            "js",
            "export const api=new Proxy({value:'x'},{});",
            "export const api=new Proxy({other:'x'},{});",
            "const {api}=await import('./index.js'); api.value.toUpperCase();",
        ),
    ];
    for (extension, before_source, after_source, consumer) in cases {
        let old = package(before_source, extension);
        let new = package(after_source, extension);
        let mut maps = Vec::new();
        for (root, succeeds) in [(&old, true), (&new, false)] {
            let (mut exports, warnings) = discover_exports(root.path()).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            assert!(
                exports.values().all(|symbol| !symbol.is_proxy_dispatched),
                "{exports:?}"
            );
            let result = interlint::lsp::enrich_exports(&mut exports, root.path())
                .await
                .unwrap();
            assert!(
                matches!(result, interlint::lsp::EnrichResult::Enriched),
                "{before_source}: {result:?}"
            );
            let output = std::process::Command::new("node").current_dir(root.path())
                .args(["--input-type=module", "--eval", &format!("import {{createRequire}} from 'node:module'; const require=createRequire(import.meta.url); {consumer}")]).output().unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{consumer}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            maps.push(exports);
        }
        let findings = compare::compare_exports(&maps[0], &maps[1], new.path(), true);
        assert!(
            compare::has_breaking_changes(&findings),
            "{before_source}: {findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn registered_symbol_identity_agrees_with_node_consumers() {
    let cases = [
        (
            "js",
            "export const api=Symbol.for('public');",
            "export const api=Symbol.for('private');",
            "if(api !== Symbol.for('public')) throw Error('identity');",
            true,
        ),
        (
            "cjs",
            "exports.api=Symbol.for('public');",
            "exports.api=Symbol.for('private');",
            "if(api !== Symbol.for('public')) throw Error('identity');",
            true,
        ),
        (
            "js",
            "export const api=Symbol.iterator;",
            "export const api=Symbol.asyncIterator;",
            "if(api !== Symbol.iterator) throw Error('identity');",
            true,
        ),
        (
            "js",
            "const key=Symbol.for('public'); export const api={[key]:'hello'};",
            "const renamed=Symbol.for('public'); export const api={[renamed]:'hello'};",
            "api[Symbol.for('public')].toUpperCase();",
            false,
        ),
        (
            "js",
            "const key=Symbol.for('public'); export const api={[key]:'hello'};",
            "const key=Symbol.for('private'); export const api={[key]:'hello'};",
            "api[Symbol.for('public')].toUpperCase();",
            true,
        ),
        (
            "js",
            "export const api=Symbol.for('public');",
            "const prefix='pub'; export const api=Symbol.for(`${prefix}lic`);",
            "if(api !== Symbol.for('public')) throw Error('identity');",
            false,
        ),
    ];
    for (extension, before, after, consumer, breaking) in cases {
        let old = package(before, extension);
        let new = package(after, extension);
        let mut maps = Vec::new();
        for (root, succeeds) in [(&old, true), (&new, !breaking)] {
            let output = std::process::Command::new("node")
                .current_dir(root.path())
                .args([
                    "--input-type=module",
                    "--eval",
                    &format!("const {{api}}=await import('./index.{extension}'); {consumer}"),
                ])
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{before}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let (mut exports, warnings) = discover_exports(root.path()).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            let result = interlint::lsp::enrich_exports(&mut exports, root.path())
                .await
                .unwrap();
            assert!(
                matches!(result, interlint::lsp::EnrichResult::Enriched),
                "{result:?}"
            );
            maps.push(exports);
        }
        let findings = compare::compare_exports(&maps[0], &maps[1], new.path(), false);
        assert_eq!(
            compare::has_breaking_changes(&findings),
            breaking,
            "{before}: {findings:?}"
        );
    }
}

//! Reflective wrappers have statically known subsets and explicit runtime gaps.
use interlint::ecosystems::node::discovery::discover_exports;
use interlint::languages::javascript::compare;

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
            let result = interlint::languages::typescript::compiler::enrich_exports(
                &mut exports,
                root.path(),
            )
            .await
            .unwrap();
            assert!(
                matches!(
                    result,
                    interlint::languages::typescript::compiler::EnrichResult::Enriched
                ),
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
            interlint::core::findings::has_breaking_changes(&findings),
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
            let result = interlint::languages::typescript::compiler::enrich_exports(
                &mut exports,
                root.path(),
            )
            .await
            .unwrap();
            assert!(
                matches!(
                    result,
                    interlint::languages::typescript::compiler::EnrichResult::Enriched
                ),
                "{result:?}"
            );
            maps.push(exports);
        }
        let findings = compare::compare_exports(&maps[0], &maps[1], new.path(), false);
        assert_eq!(
            interlint::core::findings::has_breaking_changes(&findings),
            breaking,
            "{before}: {findings:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn class_runtime_contracts_agree_with_node_consumers() {
    let cases = [
        (
            "get value(){return 'x';} set value(input){}",
            "set value(input){}",
            "new api().value.toUpperCase();",
            true,
        ),
        (
            "static { { this.value='x'; } }",
            "",
            "api.value.toUpperCase();",
            true,
        ),
        (
            "static { this.value='x'; }",
            "static { this.value=1; }",
            "api.value.toUpperCase();",
            true,
        ),
        (
            "static { const text='x'; this.value=text; }",
            "static { const text=1; this.value=text; }",
            "api.value.toUpperCase();",
            true,
        ),
        (
            "static { this.value=1; { this.value='x'; } }",
            "static { this.value='x'; { this.value=1; } }",
            "api.value.toUpperCase();",
            true,
        ),
        (
            "static { if (true) { this.value='x'; } }",
            "static { if (false) { this.value='x'; } }",
            "api.value.toUpperCase();",
            true,
        ),
        (
            "static value='x'; static { this.value='y'; }",
            "static value='x'; static { this.value=1; }",
            "api.value.toUpperCase();",
            true,
        ),
        (
            "static value={key:'x'}; static { this.value.key='y'; }",
            "static value={key:'x'}; static { this.value.key=1; }",
            "api.value.key.toUpperCase();",
            true,
        ),
        (
            "static { this.value='x'; }",
            "static { { this.value='y'; } }",
            "api.value.toUpperCase();",
            false,
        ),
        (
            "static { this.value=1; } static value='x';",
            "static { this.value=false; } static value='y';",
            "api.value.toUpperCase();",
            false,
        ),
        (
            "static { const api={}; api.value='x'; }",
            "",
            "if ('value' in api) throw Error('local leaked');",
            false,
        ),
        (
            "static { function later() { api.value='x'; } }",
            "",
            "if ('value' in api) throw Error('deferred leaked');",
            false,
        ),
    ];
    for (before, after, consumer, breaking) in cases {
        let old = package(&format!("export class api {{ {before} }}"), "js");
        let new = package(&format!("export class api {{ {after} }}"), "js");
        let mut maps = Vec::new();
        for (root, succeeds) in [(&old, true), (&new, !breaking)] {
            let output = std::process::Command::new("node")
                .current_dir(root.path())
                .args([
                    "--input-type=module",
                    "--eval",
                    &format!("const {{api}}=await import('./index.js'); {consumer}"),
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
            let result = interlint::languages::typescript::compiler::enrich_exports(
                &mut exports,
                root.path(),
            )
            .await
            .unwrap();
            assert!(
                matches!(
                    result,
                    interlint::languages::typescript::compiler::EnrichResult::Enriched
                ),
                "{before}: {result:?}"
            );
            maps.push(exports);
        }
        let mut findings = compare::compare_exports(&maps[0], &maps[1], new.path(), false);
        findings.extend(
            interlint::languages::javascript::compare::structure::compare_structural_diff(
                old.path(),
                new.path(),
                &maps[0],
                &maps[1],
            ),
        );
        assert_eq!(
            interlint::core::findings::has_breaking_changes(&findings),
            breaking,
            "{before}: {findings:?}"
        );
    }
}

#[test]
fn conditional_static_members_are_explicit_without_compiler_evidence() {
    let old = package(
        "export class api { static { if (globalThis.enabled) { this.value='x'; } } }",
        "js",
    );
    let new = package("export class api {}", "js");
    let (before, _) = discover_exports(old.path()).unwrap();
    let (after, _) = discover_exports(new.path()).unwrap();
    let findings = interlint::languages::javascript::compare::structure::compare_structural_diff(
        old.path(),
        new.path(),
        &before,
        &after,
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"),
        "{findings:?}"
    );
}

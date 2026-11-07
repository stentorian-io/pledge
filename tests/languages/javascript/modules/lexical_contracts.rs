//! Lexical forms are checked at public bindings and computed property names.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare;
use std::process::Command;

fn package(source: &str, esm: bool) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let manifest = if esm {
        r#"{"name":"lexical-contract","type":"module","exports":"./index.js"}"#
    } else {
        r#"{"name":"lexical-contract","exports":"./index.js"}"#
    };
    std::fs::write(root.path().join("package.json"), manifest).unwrap();
    std::fs::write(root.path().join("index.js"), source).unwrap();
    root
}

#[test]
fn computed_expression_keys_agree_with_node_or_retain_incomplete_evidence() {
    let mut mismatches = Vec::new();
    for (expression, key) in [
        ("true", "true"),
        ("false", "false"),
        ("null", "null"),
        ("undefined", "undefined"),
        ("this", "undefined"),
        ("(function(){return new.target})()", "undefined"),
        ("import.meta.url.slice(0,4)", "file"),
        ("new (class extends Array {constructor(){super('api')}})()[0]", "api"),
        ("new (class extends class {get name(){return 'api'}} {read(){return super.name}})().read()", "api"),
        (r"/a[\/]b/gi", r"/a[\/]b/gi"),
        (r"/\u0061/u", r"/\u0061/u"),
        (r"'\n'", "\n"),
        (r"'\t'", "\t"),
        (r"`\u0061pi`", "api"),
        ("`a${'p'}${'i'}`", "api"),
        (r"String.raw`\u{110000}`", r"\u{110000}"),
        (r"String.raw`\xg`", r"\xg"),
        ("1 + 2", "3"),
        ("3 - 1", "2"),
        ("2 * 3", "6"),
        ("6 / 2", "3"),
        ("5 % 2", "1"),
        ("2 ** 3", "8"),
        ("1 << 2", "4"),
        ("8 >> 1", "4"),
        ("8 >>> 1", "4"),
        ("3 & 1", "1"),
        ("1 | 2", "3"),
        ("3 ^ 1", "2"),
        ("~1", "-2"),
        ("!false", "true"),
        ("1 < 2", "true"),
        ("1 === 1", "true"),
        ("true && 'api'", "api"),
        ("false || 'api'", "api"),
        ("(({name:null}).name ||= 'api')", "api"),
        ("(({name:'before'}).name &&= 'api')", "api"),
        ("(function(){let name;({name='api'}={});return name})()", "api"),
        ("void 0", "undefined"),
        ("typeof 1", "number"),
        ("+'1'", "1"),
        ("-'1'", "-1"),
        ("delete ({api:1}).api", "true"),
        ("(++({count:0}).count)", "1"),
        ("(--({count:2}).count)", "1"),
        ("(({count:1}).count++)", "1"),
        ("(({count:1}).count--)", "1"),
        ("(0, 'api')", "api"),
        ("new String('api')", "api"),
        ("((...names)=>names[0])('api')", "api"),
        ("((parts)=>parts[0])`api`", "api"),
    ] {
        let root = package(&format!("export class Api {{[{expression}](){{return 1}}}}"), true);
        let consumer = format!("import {{Api}} from './index.js';new Api()[{key:?}]().toFixed();");
        let output = Command::new("node")
            .current_dir(root.path())
            .args(["--input-type=module", "--eval", &consumer])
            .output()
            .unwrap();
        assert!(output.status.success(), "{expression}: {output:?}");

        let (before, old_warnings) = discover_exports(root.path()).unwrap();
        for (replacement, breaking) in [
            ("export class Api {}".to_string(), true),
            (format!("export class Api {{[{key:?}](){{return 1}}}}"), false),
        ] {
            let after_root = package(&replacement, true);
            let output = Command::new("node").current_dir(after_root.path())
                .args(["--input-type=module", "--eval", &consumer]).output().unwrap();
            assert_eq!(output.status.success(), !breaking, "{expression}: {output:?}");
            let (after, new_warnings) = discover_exports(after_root.path()).unwrap();
            let mut findings = compare::compare_exports(&before, &after, after_root.path(), false);
            findings.extend(compare::structure::compare_structural_diff(
                root.path(), after_root.path(), &before, &after,
            ));
            let incomplete = old_warnings.iter().chain(&new_warnings).any(|warning| warning.incomplete)
                || findings.iter().any(|finding| finding.rule == "analysis-incomplete");
            let supported = matches!(expression, "true" | "false" | "null" | r"'\n'" | r"'\t'" | r"`\u0061pi`" | "`a${'p'}${'i'}`");
            if incomplete == supported {
                mismatches.push(format!("{expression}: expected incomplete={}, got {incomplete}", !supported));
            }
            let reported_break = pledge::core::findings::has_breaking_changes(&findings);
            if (!breaking && reported_break) || (breaking && !reported_break && !incomplete) {
                mismatches.push(format!("{expression}/{breaking}: {findings:?}"));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn legacy_comment_trivia_preserves_public_bindings() {
    let manifest = serde_json::json!({"name":"legacy-comments","exports":"./index.js"});
    let mut cases = Vec::new();
    for (name, source) in [
        (
            "html open comment",
            "<!-- exports.decoy=1;\nexports.api=function(){};",
        ),
        (
            "html close comment",
            "\n--> exports.decoy=1;\nexports.api=function(){};",
        ),
        (
            "delimited close comment",
            "/* ** x*/ --> exports.decoy=1;\nexports.api=function(){};",
        ),
        (
            "multiline close comment",
            "/* first\n second */ --> exports.decoy=1;\nexports.api=function(){};",
        ),
    ] {
        for (after, breaking) in [
            ("exports.api=()=>{};", true),
            ("exports.api=function(){};", false),
        ] {
            cases.push(serde_json::json!({"name":format!("{name}/{breaking}"),
                "old":{"manifest":manifest,"files":{"index.js":source}},
                "new":{"manifest":manifest,"files":{"index.js":after}},
                "consumer":"import {createRequire} from 'node:module';const {api}=createRequire(import.meta.url)('./index.js');new api();",
                "breaking":breaking,"runtime":"node"}));
        }
    }
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn unresolved_keys_preserve_independent_inheritance_checks() {
    for (before, after, consumer, rule) in [
        (
            "let key='api';export class Base{};export class Api extends Base {[key](){}}",
            "let key='api';export class Base{};export class Api {[key](){}}",
            "import {Api,Base} from './index.js';if(!(new Api() instanceof Base))throw Error('base lost');",
            "class-inheritance-changed",
        ),
    ] {
        let old = package(before, true);
        let new = package(after, true);
        for (root, succeeds) in [(old.path(), true), (new.path(), false)] {
            let output = Command::new("node")
                .current_dir(root)
                .args(["--input-type=module", "--eval", consumer])
                .output()
                .unwrap();
            assert_eq!(output.status.success(), succeeds, "{output:?}");
        }
        let (before, _) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings = compare::structure::compare_structural_diff(old.path(), new.path(), &before, &after);
        assert!(findings.iter().any(|finding| finding.rule == "analysis-incomplete"), "{findings:?}");
        assert!(findings.iter().any(|finding| finding.rule == rule), "{rule}: {findings:?}");
    }
}

#[test]
fn legacy_bindings_preserve_public_function_contracts() {
    for source in [
        "if(true)function api(){};exports.api=api;",
        "{function api(){}}exports.api=api;",
        "label: function api(){};exports.api=api;",
        "for(var api=function(){} in {}){}exports.api=api;",
        "var api=function(){};try{throw 1}catch(api){var api=2}exports.api=api;",
    ] {
        let old = package(source, false);
        let (before, warnings) = discover_exports(old.path()).unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.incomplete && warning.message.contains("callable alias")),
            "{source}: {warnings:?}"
        );
        let consumer = "new (require('./index.js').api)();";
        let output = Command::new("node")
            .current_dir(old.path())
            .args(["--eval", consumer])
            .output()
            .unwrap();
        assert!(output.status.success(), "{source}: {output:?}");
        for (replacement, breaking) in [
            ("exports.api=()=>{};", true),
            ("exports.api=function(){};", false),
        ] {
            let new = package(replacement, false);
            let output = Command::new("node")
                .current_dir(new.path())
                .args(["--eval", consumer])
                .output()
                .unwrap();
            assert_eq!(output.status.success(), !breaking, "{source}: {output:?}");
            let (after, _) = discover_exports(new.path()).unwrap();
            let mut findings = compare::compare_exports(&before, &after, new.path(), false);
            findings.extend(compare::structure::compare_structural_diff(
                old.path(),
                new.path(),
                &before,
                &after,
            ));
            assert!(
                !pledge::core::findings::has_breaking_changes(&findings),
                "{source}: {findings:?}"
            );
        }
    }

    let manifest = serde_json::json!({"name":"callable-aliases","exports":"./index.js"});
    let mut cases = Vec::new();
    for source in [
        "function local(){};exports.api=local;",
        "exports.api=local;function local(){};",
        "const local=function(){};exports.api=local;",
        "let local=function(){};exports.api=local;",
        "var local=function(){};exports.api=local;",
    ] {
        for (replacement, breaking) in [
            ("exports.api=()=>{};", true),
            ("exports.api=function(){};", false),
        ] {
            cases.push(serde_json::json!({"name":format!("{source}/{breaking}"),
                "old":{"manifest":manifest,"files":{"index.js":source}},
                "new":{"manifest":manifest,"files":{"index.js":replacement}},
                "consumer":"import {createRequire} from 'node:module';const {api}=createRequire(import.meta.url)('./index.js');new api();",
                "breaking":breaking,"runtime":"node"}));
        }
    }
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

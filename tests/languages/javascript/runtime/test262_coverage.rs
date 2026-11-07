//! Pinned feature flags exercise public contracts and explicit discovery limits.

use pledge::ecosystems::node::discovery::discover_exports;

#[test]
fn proposal_module_sources_keep_discovery_incomplete() {
    for (feature, source, asset) in [
        (
            "source-phase-imports-module-source",
            "import source api from '<module source>';export {api};",
            "side.js",
        ),
        (
            "decorators",
            "export class Api { accessor value=1; }",
            "side.js",
        ),
        (
            "source-phase-imports",
            "import source api from './asset.wasm';export {api};",
            "asset.wasm",
        ),
        (
            "import-defer",
            "import defer * as api from './side.js';export {api};",
            "side.js",
        ),
        (
            "export-defer",
            "export defer * as api from './side.js';",
            "side.js",
        ),
        (
            "import-text",
            "import api from './asset.txt' with {type:'text'};export {api};",
            "asset.txt",
        ),
        (
            "import-bytes",
            "import api from './asset.bin' with {type:'bytes'};export {api};",
            "asset.bin",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"proposal-module","type":"module","exports":"./index.js"}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("index.js"), source).unwrap();
        std::fs::write(directory.path().join(asset), "export const member=1;").unwrap();
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(
            exports.is_empty(),
            "{feature}: unsupported module invented exports"
        );
        assert!(
            warnings.iter().any(|warning| warning.incomplete),
            "{feature}: {warnings:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[test]
fn public_class_contracts_agree_with_node_consumers() {
    let manifest =
        serde_json::json!({"name":"feature-contracts","type":"module","exports":"./index.js"});
    let cases = [
        (
            "numeric separator key removal",
            "export class Api {[1_000](){return 1}}",
            "export class Api {}",
            "new Api()[1000]().toFixed();",
            true,
        ),
        (
            "numeric separator key spelling",
            "export class Api {[1_000](){return 1}}",
            "export class Api {[1000](){return 1}}",
            "new Api()[1000]().toFixed();",
            false,
        ),
        (
            "function loses construction to arrow",
            "export const Api=function(){};",
            "export const Api=()=>{};",
            "new Api();",
            true,
        ),
        (
            "arrow parameter rename",
            "export const Api=(value)=>value;",
            "export const Api=(renamed)=>renamed;",
            "Api(1).toFixed();",
            false,
        ),
        (
            "const binding removal",
            "export const Api=1;",
            "export {};",
            "Api.toFixed();",
            true,
        ),
        (
            "const binding alias",
            "export const Api=1;",
            "const local=1;export{local as Api};",
            "Api.toFixed();",
            false,
        ),
        (
            "let binding removal",
            "export let Api=1;",
            "export {};",
            "Api.toFixed();",
            true,
        ),
        (
            "let binding alias",
            "export let Api=1;",
            "let local=1;export{local as Api};",
            "Api.toFixed();",
            false,
        ),
        (
            "public instance field becomes private",
            "export class Api {value=1;}",
            "export class Api {#value=1;}",
            "new Api().value.toFixed();",
            true,
        ),
        (
            "private instance field rename",
            "export class Api {#value=1;get(){return this.#value}}",
            "export class Api {#renamed=1;get(){return this.#renamed}}",
            "new Api().get().toFixed();",
            false,
        ),
        (
            "public static field becomes private",
            "export class Api {static value=1;}",
            "export class Api {static #value=1;}",
            "Api.value.toFixed();",
            true,
        ),
        (
            "private static field rename",
            "export class Api {static #value=1;static get(){return this.#value}}",
            "export class Api {static #renamed=1;static get(){return this.#renamed}}",
            "Api.get().toFixed();",
            false,
        ),
        (
            "public instance method becomes private",
            "export class Api {value(){return 1}}",
            "export class Api {#value(){return 1}}",
            "new Api().value().toFixed();",
            true,
        ),
        (
            "private instance method rename",
            "export class Api {#value(){return 1}get(){return this.#value()}}",
            "export class Api {#renamed(){return 1}get(){return this.#renamed()}}",
            "new Api().get().toFixed();",
            false,
        ),
        (
            "public static method becomes private",
            "export class Api {static value(){return 1}}",
            "export class Api {static #value(){return 1}}",
            "Api.value().toFixed();",
            true,
        ),
        (
            "private static method rename",
            "export class Api {static #value(){return 1}static get(){return this.#value()}}",
            "export class Api {static #renamed(){return 1}static get(){return this.#renamed()}}",
            "Api.get().toFixed();",
            false,
        ),
    ];
    let cases: Vec<_> = cases.into_iter().map(|(name, old, new, operation, breaking)| {
        serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.js":old}},"new":{"manifest":manifest,"files":{"index.js":new}},"consumer":format!("import {{Api}} from './index.js';{operation}"),"breaking":breaking,"runtime":"node"})
    }).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn javascript_fields_retain_private_and_static_identity() {
    let source = b"class Api {value=1; #hidden=2; static shared=3; static #secret=4;}";
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let class = tree.root_node().named_child(0).unwrap();
    let (members, dynamic) =
        pledge::languages::javascript::syntax::classes::parse_class_members(source, class);
    assert!(!dynamic);
    let identities: Vec<_> = members
        .iter()
        .map(|member| (member.name.as_str(), member.is_private, member.is_static))
        .collect();
    assert_eq!(
        identities,
        [
            ("value", false, false),
            ("#hidden", true, false),
            ("shared", false, true),
            ("#secret", true, true)
        ]
    );
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn optional_and_rest_parameter_contracts_agree_with_node_consumers() {
    use pledge::languages::javascript::compare::{
        compare_exports, structure::compare_structural_diff,
    };
    use pledge::languages::typescript::compiler::{enrich_exports, EnrichResult};

    for (name, old_source, new_source, consumer, breaking) in [
        ("default parameter becomes required", "export function api(value=1){return value}", "export function api(value){if(value===undefined)throw Error('required');return value}", "api().toFixed();", true),
        ("default parameter remains optional", "export function api(value=1){return value}", "export function api(value=2){return value}", "api().toFixed();", false),
        ("rest parameter gains required prefix", "export function api(...values){return values}", "export function api(first,...values){if(first===undefined)throw Error('required');return [first,...values]}", "api().length.toFixed();", true),
        ("rest parameter rename", "export function api(...values){return values}", "export function api(...renamed){return renamed}", "api().length.toFixed();", false),
    ] {
        let mut directories = Vec::new();
        let mut maps = Vec::new();
        for (source, succeeds) in [(old_source,true),(new_source,!breaking)] {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(directory.path().join("package.json"), r#"{"name":"parameter-contract","type":"module","exports":"./index.js"}"#).unwrap();
            std::fs::write(directory.path().join("index.js"),source).unwrap();
            let output = std::process::Command::new("node").current_dir(directory.path()).args(["--input-type=module","--eval", &format!("import {{api}} from './index.js';{consumer}")]).output().unwrap();
            assert_eq!(output.status.success(),succeeds,"{name}: {}",String::from_utf8_lossy(&output.stderr));
            let (mut exports,warnings) = discover_exports(directory.path()).unwrap();
            assert!(warnings.is_empty(),"{name}: {warnings:?}");
            assert!(matches!(enrich_exports(&mut exports,directory.path()).await.unwrap(),EnrichResult::Enriched),"{name}");
            maps.push(exports);
            directories.push(directory);
        }
        let mut findings=compare_exports(&maps[0],&maps[1],directories[1].path(),false);
        findings.extend(compare_structural_diff(directories[0].path(),directories[1].path(),&maps[0],&maps[1]));
        assert_eq!(pledge::core::findings::has_breaking_changes(&findings),breaking,"{name}: {findings:?}");
        assert!(!findings.iter().any(|finding| finding.rule=="analysis-incomplete"),"{name}: {findings:?}");
    }
}

#[cfg(feature = "lsp-tests")]
#[test]
fn symbol_protocol_identities_agree_with_node_consumers() {
    let manifest = serde_json::json!({"name":"symbol-contracts","exports":"./index.cjs"});
    let protocols = [
        ("asyncIterator", "async function*(){yield 1}", "let found=false;for await(const value of api){found=true;value.toFixed()}if(!found)throw Error('empty');"),
        ("hasInstance", "()=>true", "if(!({} instanceof api))throw Error('protocol lost');"),
        ("toPrimitive", "()=>1", "if(Number(api)!==1)throw Error('protocol lost');"),
        ("toStringTag", "'Api'", "if(Object.prototype.toString.call(api)!=='[object Api]')throw Error('protocol lost');"),
    ];
    let mut cases = Vec::new();
    for (protocol, implementation, operation) in protocols {
        let source = format!("exports[Symbol.{protocol}]={implementation};");
        let alias = format!("const key=Symbol.{protocol};exports[key]={implementation};");
        for (change, new, breaking) in [
            ("removal", String::new(), true),
            ("constant alias", alias, false),
        ] {
            cases.push(serde_json::json!({"name":format!("{protocol} {change}"),"old":{"manifest":manifest,"files":{"index.cjs":source}},"new":{"manifest":manifest,"files":{"index.cjs":new}},"consumer":format!("import {{createRequire}} from 'node:module';const api=createRequire(import.meta.url)('./index.cjs');{operation}"),"breaking":breaking,"runtime":"node"}));
        }
    }
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[cfg(feature = "lsp-tests")]
#[test]
fn symbol_protocol_aliases_preserve_node_consumers() {
    let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/javascript/properties/symbol-consumers.json"
    )))
    .unwrap();
    let mut cases = Vec::new();
    for case in fixtures {
        let name = case["name"].as_str().unwrap();
        let Some(protocol) = name.strip_prefix("symbol-protocol-") else {
            continue;
        };
        let source = case["old"]["files"]["index.cjs"].as_str().unwrap();
        let key = format!("[Symbol.{protocol}]");
        assert!(source.contains(&key), "{name}: exact protocol key absent");
        let alias = format!(
            "const protocol=Symbol.{protocol};{}",
            source.replace(&key, "[protocol]")
        );
        let mut control = case.clone();
        control["name"] = serde_json::json!(format!("{protocol} constant alias"));
        control["new"] = control["old"].clone();
        control["new"]["files"]["index.cjs"] = serde_json::json!(alias);
        control["breaking"] = serde_json::json!(false);
        cases.push(control);
    }
    assert_eq!(cases.len(), 10);
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[cfg(feature = "lsp-tests")]
#[test]
fn unicode_string_export_names_agree_with_node_consumers() {
    let manifest =
        serde_json::json!({"name":"json-superset-exports","type":"module","exports":"./index.js"});
    let mut cases = Vec::new();
    for (name, separator, escaped) in [
        ("line separator", '\u{2028}', "\\u2028"),
        ("paragraph separator", '\u{2029}', "\\u2029"),
        ("Mongolian vowel separator", '\u{180e}', "\\u180e"),
    ] {
        let public_name = format!("line{separator}break");
        let source = format!("const local=1;export{{local as 'line{separator}break'}};");
        let equivalent = format!("const local=1;export{{local as 'line{escaped}break'}};");
        for (change, new, breaking) in [
            ("removal", "export {};".to_owned(), true),
            ("escaped spelling", equivalent, false),
        ] {
            cases.push(serde_json::json!({"name":format!("{name} {change}"),"old":{"manifest":manifest,"files":{"index.js":source}},"new":{"manifest":manifest,"files":{"index.js":new}},"consumer":format!("const api=await import('./index.js');api[{}].toFixed();",serde_json::to_string(&public_name).unwrap()),"breaking":breaking,"runtime":"node"}));
        }
    }
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[cfg(feature = "lsp-tests")]
#[test]
fn computed_expression_keys_retain_incomplete_interface_evidence() {
    use pledge::languages::javascript::compare::structure::compare_structural_diff;

    for (feature, source, public_name) in [
        ("BigInt", "export class Api {[1n](){return 1}}", "1"),
        (
            "coalesce-expression",
            "export class Api {[null ?? 'api'](){return 1}}",
            "api",
        ),
        (
            "logical-assignment-operators",
            "let name=null;export class Api {[name ??= 'api'](){return 1}}",
            "api",
        ),
        (
            "optional-chaining",
            "export class Api {[({name:'api'})?.name](){return 1}}",
            "api",
        ),
        (
            "destructuring-assignment",
            "let name;export class Api {[({name}={name:'api'},name)](){return 1}}",
            "api",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"expression-key","type":"module","exports":"./index.js"}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("index.js"), source).unwrap();
        let output = std::process::Command::new("node")
            .current_dir(directory.path())
            .args([
                "--input-type=module",
                "--eval",
                &format!(
                    "import {{Api}} from './index.js';new Api()[{public_name:?}]().toFixed();"
                ),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{feature}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(warnings.is_empty(), "{feature}: {warnings:?}");
        let findings =
            compare_structural_diff(directory.path(), directory.path(), &exports, &exports);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{feature}: {findings:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[test]
fn prototype_setters_do_not_conceal_own_property_removal() {
    let manifest = serde_json::json!({"name":"prototype-properties","exports":"./index.cjs"});
    let own = "module.exports={['__proto__'](){return 1}};";
    let descriptor = "Object.defineProperty(exports,'__proto__',{value:()=>1});";
    let cases: Vec<_> = [
        ("bare prototype setter",own,"module.exports={__proto__:null};",true),
        ("quoted prototype setter",own,"module.exports={'__proto__':null};",true),
        ("escaped quoted prototype setter",own,r"module.exports={'__pro\u0074o__':null};",true),
        ("own method spelling",own,"module.exports={__proto__(){return 1}};",false),
        ("own getter",own,"module.exports={get __proto__(){return ()=>1}};",false),
        ("own shorthand",own,"const __proto__=()=>1;module.exports={__proto__};",false),
        ("own computed pair",own,"module.exports={['__proto__']:()=>1};",false),
        ("descriptor map prototype setter",descriptor,"Object.defineProperties(exports,{'__proto__':{value:()=>1}});",true),
        ("descriptor map computed property",descriptor,"Object.defineProperties(exports,{['__proto__']:{value:()=>1}});",false),
        ("assigned source prototype setter",descriptor,"Object.assign(exports,{'__proto__':null});",true),
    ].into_iter().map(|(name,old,new,breaking)|serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.cjs":old}},"new":{"manifest":manifest,"files":{"index.cjs":new}},"consumer":"import {createRequire} from 'node:module';const api=createRequire(import.meta.url)('./index.cjs');api.__proto__().toFixed();","breaking":breaking,"runtime":"node"})).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[cfg(feature = "lsp-tests")]
#[test]
fn inherited_commonjs_prototype_surfaces_retain_incomplete_evidence() {
    for (source,incomplete) in [
        ("module.exports={__proto__:{api:1}};",true),
        ("module.exports={'__proto__':{api:1}};",true),
        ("(function(root,factory){module.exports=factory();})(this,function(){return {__proto__:{api:1}};});",true),
        ("const unrelated={__proto__:{api:1}};module.exports={api:1};",false),
        ("module.exports={api:1};0&&(module.exports={__proto__:{api:2}});",false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"inherited-prototype","exports":"./index.cjs"}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("index.cjs"), source).unwrap();
        let output = std::process::Command::new("node")
            .current_dir(directory.path())
            .args(["--eval", "require('./index.cjs').api.toFixed();"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(exports.values().all(|symbol| symbol.name != "__proto__"));
        assert!(
            warnings.iter().any(|warning| warning.incomplete) == incomplete,
            "{source}: {warnings:?}"
        );
    }
}

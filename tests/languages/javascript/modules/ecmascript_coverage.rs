//! Package consumers exercise selected lexical productions, not engine conformance.

#[test]
fn lexical_export_spellings_agree_with_node_consumers() {
    let manifest =
        serde_json::json!({"name":"lexical-exports","type":"module","exports":"./index.js"});
    let mut cases = Vec::new();
    for (name, source) in [
        (
            "space separators",
            "\u{feff}export\u{00a0}const\u{2003}api=1;",
        ),
        (
            "single line comment",
            "// hidden export const decoy=1;\nexport const api=1;",
        ),
        (
            "multiline comment",
            "/* export const decoy=1; */export/* declaration */const api=1;",
        ),
        ("carriage return", "export\rconst\rapi=1;"),
        ("hashbang", "#!/usr/bin/env node\nexport const api=1;"),
        (
            "automatic semicolon insertion",
            "const local=1\nexport {local as api}\n",
        ),
        ("escaped identifier", r"export const \u0061pi=1;"),
        ("code point escape", r"export const \u{61}pi=1;"),
        (
            "hex string export name",
            r"const local=1;export{local as '\x61pi'};",
        ),
        (
            "nonescape string export name",
            r"const local=1;export{local as '\api'};",
        ),
        (
            "continued string export name",
            "const local=1;export{local as 'a\\\npi'};",
        ),
        ("unicode identifier start", "const ℘=1;export{℘ as api};"),
        ("astral identifier start", "const 𝒜=1;export{𝒜 as api};"),
        (
            "identifier join control",
            "const a\u{200c}pi=1;export{a\u{200c}pi as api};",
        ),
        (
            "identifier combining mark",
            "const a\u{0301}=1;export{a\u{0301} as api};",
        ),
    ] {
        for (suffix, after, breaking) in [
            ("removal", "export {};", true),
            ("equivalent spelling", "export const api=1;", false),
        ] {
            cases.push(serde_json::json!({
                "name":format!("{name} {suffix}"),
                "old":{"manifest":manifest,"files":{"index.js":source}},
                "new":{"manifest":manifest,"files":{"index.js":after}},
                "consumer":"const {api}=await import('./index.js');api.toFixed();",
                "breaking":breaking,
                "runtime":"node"
            }));
        }
    }
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn line_terminators_after_export_retain_incomplete_evidence() {
    for separator in ["\n", "\r\n", "\u{2028}", "\u{2029}"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("package.json"),
            r#"{"name":"unicode-lines","type":"module","exports":"./index.js"}"#,
        )
        .unwrap();
        let source = format!("export{separator}const api=1;");
        std::fs::write(root.path().join("index.js"), source).unwrap();
        let output = std::process::Command::new("node")
            .current_dir(root.path())
            .args([
                "--input-type=module",
                "--eval",
                "const {api}=await import('./index.js');api.toFixed();",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let (_, warnings) =
            pledge::ecosystems::node::discovery::discover_exports(root.path()).unwrap();
        assert!(
            warnings.iter().any(|warning| warning.incomplete
                && warning
                    .message
                    .contains("export keyword parsed as an identifier")),
            "{separator:?}: {warnings:?}"
        );
    }
}

#[test]
fn object_helper_exports_agree_with_node_consumers() {
    let manifest = serde_json::json!({"name":"helper-exports","exports":"./index.cjs"});
    let cases: Vec<_> = [
        ("assign member removal", "Object.assign(exports,{api:1});", "Object.assign(exports,{renamed:1});", "api.api.toFixed();", true),
        ("assign module target", "Object.assign(exports,{api:1});", "Object.assign(module.exports,{api:1});", "api.api.toFixed();", false),
        ("defineProperties write removal", "Object.defineProperties(exports,{api:{value:1,writable:true}});", "Object.defineProperties(exports,{api:{value:1,writable:false}});", "api.api=2;if(api.api!==2)throw Error('write lost');", true),
        ("defineProperties enumeration removal", "Object.defineProperties(exports,{api:{value:1,enumerable:true}});", "Object.defineProperties(exports,{api:{value:1,enumerable:false}});", "if(!Object.keys(api).includes('api'))throw Error('enumeration lost');", true),
        ("defineProperties descriptor spelling", "Object.defineProperties(exports,{api:{value:1,writable:true,enumerable:true}});", "const key='api';Object.defineProperties(exports,{[key]:{enumerable:true,writable:true,value:1}});", "api.api=2;if(!Object.keys(api).includes('api'))throw Error('enumeration lost');", false),
    ].into_iter().map(|(name, before, after, operation, breaking)| {
        serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.cjs":before}},"new":{"manifest":manifest,"files":{"index.cjs":after}},"consumer":format!("const api=(await import('./index.cjs')).default;{operation}"),"breaking":breaking,"runtime":"node"})
    }).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn aliased_object_helper_targets_retain_incomplete_evidence() {
    for call in [
        "Object.assign(target,{api:1});",
        "Object.defineProperty(target,'api',{value:1});",
        "Object.defineProperties(target,{api:{value:1}});",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("package.json"),
            r#"{"name":"aliased-helper","exports":"./index.cjs"}"#,
        )
        .unwrap();
        for (replacement, succeeds) in [("", true), ("module.exports={};", false)] {
            std::fs::write(
                root.path().join("index.cjs"),
                format!("const target=exports;{replacement}{call}"),
            )
            .unwrap();
            let output = std::process::Command::new("node")
                .current_dir(root.path())
                .args(["--eval", "require('./index.cjs').api.toFixed();"])
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{replacement}{call}: {output:?}"
            );
            let (_, warnings) =
                pledge::ecosystems::node::discovery::discover_exports(root.path()).unwrap();
            assert!(
                warnings.iter().any(|warning| warning.incomplete
                    && warning
                        .message
                        .contains("aliased CommonJS Object helper target")),
                "{replacement}{call}: {warnings:?}"
            );
        }
    }
}

#[test]
fn numeric_public_keys_agree_with_node_or_retain_incomplete_evidence() {
    let manifest = serde_json::json!({"name":"numeric-keys","exports":"./index.cjs"});
    let cases: Vec<_> = [
        ("legacy decimal key removal", "Object.defineProperty(exports,08,{value:1});", "Object.defineProperty(exports,9,{value:1});", true),
        ("legacy decimal key spelling", "Object.defineProperty(exports,08,{value:1});", "Object.defineProperty(exports,8,{value:1});", false),
        ("legacy octal-like decimal removal", "Object.defineProperty(exports,078,{value:1});", "Object.defineProperty(exports,79,{value:1});", true),
        ("legacy octal-like decimal spelling", "Object.defineProperty(exports,078,{value:1});", "Object.defineProperty(exports,78,{value:1});", false),
        ("numeric separator key removal", "Object.defineProperty(exports,1_000,{value:1});", "Object.defineProperty(exports,1001,{value:1});", true),
        ("numeric separator key spelling", "Object.defineProperty(exports,1_000,{value:1});", "Object.defineProperty(exports,1000,{value:1});", false),
    ].into_iter().map(|(name, before, after, breaking)| {
        serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.cjs":before}},"new":{"manifest":manifest,"files":{"index.cjs":after}},"consumer":format!("(await import('./index.cjs')).default[{}].toFixed();", if name.starts_with("legacy octal-like") {78} else if name.starts_with("legacy decimal") {8} else {1000}),"breaking":breaking,"runtime":"node"})
    }).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());

    for (spelling, key) in [
        ("0x10", "16"),
        ("0b10000", "16"),
        ("0o20", "16"),
        ("020", "16"),
        ("1.6e1", "16"),
        ("16n", "16"),
        ("-1", "-1"),
        (r"'\141pi'", "api"),
        (r"'\41pi'", "!pi"),
        (r"'\8'", "8"),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), manifest.to_string()).unwrap();
        std::fs::write(
            root.path().join("index.cjs"),
            format!("Object.defineProperty(exports,{spelling},{{value:1}});"),
        )
        .unwrap();
        let output = std::process::Command::new("node")
            .current_dir(root.path())
            .args([
                "--eval",
                &format!("require('./index.cjs')[{key:?}].toFixed();"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{spelling}: {output:?}");
        let (_, warnings) =
            pledge::ecosystems::node::discovery::discover_exports(root.path()).unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.incomplete && warning.message.contains("property key")),
            "{spelling}: {warnings:?}"
        );
    }
}

#[tokio::test]
async fn reflective_public_effects_match_consumers_or_retain_incomplete_evidence() {
    for (name, before, after, consumer) in [
        ("fresh symbol key", "Object.defineProperty(exports,Symbol('api'),{value:1});", "Object.defineProperty(exports,Symbol('renamed'),{value:1});", "const keys=Object.getOwnPropertySymbols(api);if(keys.length!==1||keys[0].description!=='api')throw Error('symbol contract');"),
        (
            "freeze compatible",
            "module.exports=Object.freeze({api:Number(1)});",
            "module.exports=Object.freeze({api:Number(2)});",
            "api.api.toFixed();",
        ),
        (
            "revocable compatible",
            "module.exports=Proxy.revocable({api:1},{}).proxy;",
            "const target={api:1};module.exports=Proxy.revocable(target,{}).proxy;",
            "api.api.toFixed();",
        ),
        (
            "freeze",
            "module.exports={api:1};",
            "module.exports=Object.freeze({api:1});",
            "api.api=2;",
        ),
        (
            "seal",
            "exports.api=1;",
            "exports.api=1;Object.seal(exports);",
            "delete api.api;",
        ),
        (
            "prevent extensions",
            "exports.api=1;",
            "exports.api=1;Object.preventExtensions(exports);",
            "api.extra=1;",
        ),
        (
            "set prototype",
            "Object.setPrototypeOf(exports,{api:1});",
            "Object.setPrototypeOf(exports,{renamed:1});",
            "api.api.toFixed();",
        ),
        (
            "Object.create",
            "module.exports=Object.create({api:1});",
            "module.exports=Object.create({renamed:1});",
            "api.api.toFixed();",
        ),
        (
            "from entries",
            "module.exports=Object.fromEntries([['api',1]]);",
            "module.exports=Object.fromEntries([['renamed',1]]);",
            "api.api.toFixed();",
        ),
        (
            "legacy getter",
            "exports.__defineGetter__('api',()=>1);",
            "exports.__defineGetter__('renamed',()=>1);",
            "api.api.toFixed();",
        ),
        (
            "legacy setter",
            "exports.__defineSetter__('api',function(v){this.observed=v});",
            "exports.__defineSetter__('renamed',function(v){this.observed=v});",
            "api.api=1;if(api.observed!==1)throw Error('setter lost');",
        ),
        (
            "legacy prototype",
            "exports.__proto__={api:1};",
            "exports.__proto__={renamed:1};",
            "api.api.toFixed();",
        ),
        (
            "Reflect define",
            "Reflect.defineProperty(exports,'api',{value:1});",
            "Reflect.defineProperty(exports,'renamed',{value:1});",
            "api.api.toFixed();",
        ),
        (
            "Reflect set",
            "Reflect.set(exports,'api',1);",
            "Reflect.set(exports,'renamed',1);",
            "api.api.toFixed();",
        ),
        (
            "Reflect delete",
            "exports.api=1;",
            "exports.api=1;Reflect.deleteProperty(exports,'api');",
            "api.api.toFixed();",
        ),
        (
            "Reflect prevent extensions",
            "exports.api=1;",
            "exports.api=1;Reflect.preventExtensions(exports);",
            "api.extra=1;",
        ),
        (
            "Reflect prototype",
            "Reflect.setPrototypeOf(exports,{api:1});",
            "Reflect.setPrototypeOf(exports,{renamed:1});",
            "api.api.toFixed();",
        ),
        (
            "revocable proxy",
            "module.exports=Proxy.revocable({api:1},{}).proxy;",
            "module.exports=Proxy.revocable({renamed:1},{}).proxy;",
            "api.api.toFixed();",
        ),
        (
            "bound function",
            "module.exports=(function(){return 1}).bind(null);",
            "module.exports=(function(){return 'x'}).bind(null);",
            "api().toFixed();",
        ),
        (
            "dynamic function",
            "module.exports=new Function('return 1');",
            "module.exports=new Function('return \\\"x\\\"');",
            "api().toFixed();",
        ),
        (
            "eval",
            "eval('exports.api=1');",
            "eval('exports.renamed=1');",
            "api.api.toFixed();",
        ),
    ] {
        let mut packages = Vec::new();
        let mut maps = Vec::new();
        let mut incomplete = false;
        for (source, succeeds) in [(before, true), (after, name.ends_with("compatible"))] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            std::fs::write(
                root.join("package.json"),
                r#"{"name":"reflection-effects","exports":{"require":"./index.cjs"}}"#,
            )
            .unwrap();
            std::fs::write(root.join("index.cjs"), source).unwrap();
            let output = std::process::Command::new("node")
                .current_dir(root)
                .args([
                    "--eval",
                    &format!("'use strict';const api=require('reflection-effects');{consumer}"),
                ])
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{name}: {source}: {output:?}"
            );
            let (mut exports, mut warnings) =
                pledge::ecosystems::node::discovery::discover_exports(root).unwrap();
            let enrichment =
                pledge::languages::typescript::compiler::enrich_exports(&mut exports, root)
                    .await
                    .unwrap();
            enrichment.reconcile_discovery_warnings(&mut warnings);
            incomplete |= warnings.iter().any(|warning| warning.incomplete);
            maps.push(exports);
            packages.push(directory);
        }
        let findings = pledge::languages::javascript::compare::compare_exports(
            &maps[0],
            &maps[1],
            packages[1].path(),
            true,
        );
        incomplete |= findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete");
        if matches!(
            name,
            "freeze" | "revocable proxy" | "freeze compatible" | "revocable compatible"
        ) {
            assert!(!incomplete, "{name}: {findings:?}");
            assert_eq!(
                pledge::core::findings::has_breaking_changes(&findings),
                !name.ends_with("compatible"),
                "{name}: {findings:?}"
            );
        } else {
            assert!(incomplete, "{name}: {findings:?}");
        }
    }
}

#[test]
fn unrelated_reflection_does_not_degrade_public_evidence() {
    for source in [
        "for(let index=0;index<1;index++){const local={};local.api=1}exports.api=1;",
        "while(false){const exports={};exports.private=1}exports.api=1;",
        "label:{const local={};local.api=1}exports.api=1;",
        "with({local:{}}){local.api=1}exports.api=1;",
        "if(process.env.NODE_ENV==='production'){exports.api=1}else{exports.api=2}",
        "const local={};Object.seal(local);exports.api=1;",
        "const local={};Reflect.set(local,'private',1);exports.api=1;",
        "function later(){Object.preventExtensions(exports)}exports.api=1;",
        "const Object={seal(){}};Object.seal(exports);exports.api=1;",
        "const Reflect={set(){}};Reflect.set(exports,'private',1);exports.api=1;",
        "const Function=function(){return ()=>1};module.exports=new Function();",
        "const eval=()=>{};eval('exports.private=1');exports.api=1;",
        "eval(\"const text='exports.private=1'\");exports.api=1;",
        "eval('// exports.private=1');exports.api=1;",
        "eval('function later(){exports.private=1}');exports.api=1;",
        "eval('const exports={};exports.private=1');exports.api=1;",
        "{const exports={};eval('exports.private=1');}module.exports={api:1};",
        "const local={bind(){return()=>1}};module.exports=local.bind();",
        "(function(){return 1}).bind(null);exports.api=1;",
        "Object.create({private:1});Object.fromEntries([['private',1]]);exports.api=1;",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("package.json"),
            r#"{"name":"reflection-controls","exports":"./index.cjs"}"#,
        )
        .unwrap();
        std::fs::write(root.path().join("index.cjs"), source).unwrap();
        let output = std::process::Command::new("node")
            .current_dir(root.path())
            .args(["--eval", "require('./index.cjs');"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{source}: {output:?}");
        let (_, warnings) =
            pledge::ecosystems::node::discovery::discover_exports(root.path()).unwrap();
        assert!(warnings.is_empty(), "{source}: {warnings:?}");
    }
}

#[tokio::test]
async fn callable_expression_and_method_protocols_agree_with_node_consumers() {
    for (name, before, after, consumer) in [
        ("class constructor", "module.exports=class {constructor(input=1){if(input===undefined)throw Error('required')}};", "module.exports=class {constructor(input){if(input===undefined)throw Error('required')}};", "new api();"),
        (
            "async expression",
            "module.exports=async function(){return 1};",
            "module.exports=function(){return 1};",
            "api().then(value=>value.toFixed());",
        ),
        (
            "generator expression",
            "module.exports=function*(){yield 1};",
            "module.exports=function(){return 1};",
            "api().next().value.toFixed();",
        ),
        (
            "async generator expression",
            "module.exports=async function*(){yield 1};",
            "module.exports=async function(){return 1};",
            "api().next().then(result=>result.value.toFixed());",
        ),
        (
            "async method",
            "module.exports={async api(){return 1}};",
            "module.exports={api(){return 1}};",
            "api.api().then(value=>value.toFixed());",
        ),
        (
            "generator method",
            "module.exports={*api(){yield 1}};",
            "module.exports={api(){return 1}};",
            "api.api().next().value.toFixed();",
        ),
        (
            "async generator method",
            "module.exports={async *api(){yield 1}};",
            "module.exports={async api(){return 1}};",
            "api.api().next().then(result=>result.value.toFixed());",
        ),
    ] {
        let mut packages = Vec::new();
        let mut maps = Vec::new();
        let mut incomplete = false;
        let forwarded = format!(
            "{}module.exports=forwarded;",
            before.replacen("module.exports=", "const forwarded=", 1)
        );
        for (source, succeeds) in [(before, true), (after, false), (forwarded.as_str(), true)] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            std::fs::write(
                root.join("package.json"),
                r#"{"name":"callable-protocols","exports":{"require":"./index.cjs"}}"#,
            )
            .unwrap();
            std::fs::write(root.join("index.cjs"), source).unwrap();
            let output = std::process::Command::new("node")
                .current_dir(root)
                .args([
                    "--eval",
                    &format!("const api=require('callable-protocols');{consumer}"),
                ])
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{name}: {source}: {output:?}"
            );
            let (mut exports, mut warnings) =
                pledge::ecosystems::node::discovery::discover_exports(root).unwrap();
            let enrichment =
                pledge::languages::typescript::compiler::enrich_exports(&mut exports, root)
                    .await
                    .unwrap();
            enrichment.reconcile_discovery_warnings(&mut warnings);
            incomplete |= warnings.iter().any(|warning| warning.incomplete);
            maps.push(exports);
            packages.push(directory);
        }
        let findings = pledge::languages::javascript::compare::compare_exports(
            &maps[0],
            &maps[1],
            packages[1].path(),
            true,
        );
        incomplete |= findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete");
        assert!(!incomplete,"{name}: {findings:?}");
        assert!(pledge::core::findings::has_breaking_changes(&findings),"{name}: {findings:?}");
        let compatible = pledge::languages::javascript::compare::compare_exports(
            &maps[0],
            &maps[2],
            packages[2].path(),
            true,
        );
        assert!(
            !pledge::core::findings::has_breaking_changes(&compatible),
            "{name} forwarding: {compatible:?}"
        );
    }
}

#[test]
fn opaque_control_flow_keeps_public_initialization_incomplete() {
    for source in [
        "for(let index=0;index<0;index++){exports.api=1;}",
        "if(false){exports.api=1;}",
        "if(false){Object.assign(exports,{api:1});}",
        "for(const key in {}){exports.api=1;}",
        "for(const entry of []){exports.api=1;}",
        "while(false){exports.api=1;}",
        "do {break;exports.api=1;}while(false);",
        "for(let index=0;index<1;index++){continue;exports.api=1;}",
        "switch(0){case 1:exports.api=1;break;default:break;}",
        "exit:{break exit;exports.api=1;}",
        "with({exports:{}}){exports.api=1;}",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("package.json"),
            r#"{"name":"control-flow-effects","exports":"./index.cjs"}"#,
        )
        .unwrap();
        for (version, succeeds) in [("exports.api=1;", true), (source, false)] {
            std::fs::write(root.path().join("index.cjs"), version).unwrap();
            let output = std::process::Command::new("node")
                .current_dir(root.path())
                .args(["--eval", "require('./index.cjs').api.toFixed();"])
                .output()
                .unwrap();
            assert_eq!(output.status.success(), succeeds, "{version}: {output:?}");
        }
        let (_, warnings) =
            pledge::ecosystems::node::discovery::discover_exports(root.path()).unwrap();
        assert!(
            warnings.iter().any(|warning| warning.incomplete
                && !warning.type_expansion_only
                && warning
                    .message
                    .contains("export mutation requires runtime information")),
            "{source}: {warnings:?}"
        );
    }
}

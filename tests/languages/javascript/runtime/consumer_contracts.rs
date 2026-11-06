//! Real Node consumers independently verify static runtime-contract findings.

#[test]
fn exported_syntax_and_runtime_contracts_agree_with_node_consumers() {
    let cases = [
        ("direct var removal", "export var value=1;", "export {};", "(await import('./index.js')).value.toFixed();", true, false),
        ("generator removal", "export function* value(){yield 1;}", "export {};", "(await import('./index.js')).value().next();", true, false),
        ("reserved alias spelling", "const n=1;export{n as 'while'};", "const n=1;export{n as while};", "(await import('./index.js')).while.toFixed();", false, false),
        ("string export rename", "const local=1;export{local as 'public-name'};", "const local=1;export{local as 'renamed'};", "(await import('./index.js'))['public-name'].toFixed();", true, false),
        ("function construction", "export const Api=function(){};", "export const Api=()=>{};", "new (await import('./index.js')).Api();", true, false),
        ("JavaScript inheritance", "export class BaseA{} export class BaseB{} export class Api extends BaseA{}", "export class BaseA{} export class BaseB{} export class Api extends BaseB{}", "const api=await import('./index.js');if(!(new api.Api() instanceof api.BaseA))throw Error('changed base');", true, false),
        ("constant base alias", "class Base{}export class Api extends Base{}", "class Base{}const Alias=Base;export class Api extends Alias{}", "const api=await import('./index.js');if(Object.getPrototypeOf(api.Api).name!=='Base')throw Error('changed base');", false, false),
        ("escaped export spelling", "export const value=1;", r"export const \u0076alue=1;", "(await import('./index.js')).value.toFixed();", false, false),
        ("discarded export copy", "exports.real=1;", "Object.assign({},exports,{real:1});", "(await import('./index.js')).default.real.toFixed();", true, true),
        ("descriptor key alias", "const key='value';Object.defineProperty(exports,key,{value:1});", "const renamed='value';Object.defineProperty(exports,renamed,{value:1});", "(await import('./index.js')).default.value.toFixed();", false, true),
        ("descriptor method setter removal", "Object.defineProperty(exports,'value',{get(){return 1;},set(v){},enumerable:true});", "Object.defineProperty(exports,'value',{get(){return 1;},enumerable:true});", "const api=(await import('./index.js')).default;api.value=2;", true, true),
        ("descriptor method equivalence", "Object.defineProperty(exports,'value',{get:function(){return 1;},set:function(v){},enumerable:true});", "Object.defineProperty(exports,'value',{get(){return 1;},set(v){},enumerable:true});", "const api=(await import('./index.js')).default;api.value=2;", false, true),
        ("descriptor write removal", "Object.defineProperty(exports,'value',{value:1,writable:true});", "Object.defineProperty(exports,'value',{value:1,writable:false});", "const api=(await import('./index.js')).default;api.value=2;if(api.value!==2)throw Error('write lost');", true, true),
        ("descriptor enumeration removal", "Object.defineProperty(exports,'value',{value:1,enumerable:true});", "Object.defineProperty(exports,'value',{value:1,enumerable:false});", "const api=(await import('./index.js')).default;if(!Object.keys(api).includes('value'))throw Error('enumeration lost');", true, true),
    ];
    let cases: Vec<_> = cases.into_iter().map(|(name, old, new, consumer, breaking, commonjs)| {
        let manifest = serde_json::json!({"name":"runtime-contracts","type":if commonjs {"commonjs"} else {"module"},"exports":"./index.js"});
        serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.js":old}},"new":{"manifest":manifest,"files":{"index.js":new}},"consumer":consumer,"breaking":breaking,"runtime":"node"})
    }).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn commonjs_assignment_scope_agrees_with_node_consumers() {
    let manifest =
        serde_json::json!({"name":"assignment-scope","exports":{"require":"./index.cjs"}});
    let public = "exports.real=Number(1);";
    let consumer = "const api=(await import('node:module')).createRequire(import.meta.url)('assignment-scope');api.real.toFixed();";
    let mut cases = Vec::new();
    for (name, old, new, breaking) in [
        ("escaped exports removal", r"\u0065xports.real=Number(1);", "", true),
        ("escaped module removal", r"m\u006fdule.exports.real=Number(1);", "", true),
        ("escaped member removal", r"module.\u0065xports.real=Number(1);", "", true),
        ("escaped public parameter removal", r"(function(\u0065xports){exports.real=Number(1)})(exports);", "", true),
        ("escaped helper removal", r"\u004fbject.defineProperty(exports,'real',{value:Number(1)});", "", true),
        ("escaped helper method removal", r"Object.\u0064efineProperty(exports,'real',{value:Number(1)});", "", true),
        ("escaped public parameter equivalence", public, r"(function(\u0065xports){exports.real=Number(1)})(exports);", false),
        ("escaped helper equivalence", "Object.defineProperty(exports,'real',{value:Number(1)});", r"\u004fbject.\u0064efineProperty(exports,'real',{value:Number(1)});", false),
        ("escaped helper shadow", public, r"const \u004fbject={defineProperty(){}};Object.defineProperty(exports,'real',{value:Number(1)});", true),
        ("escaped parameter shadow", public, r"(function(\u0065xports){exports.real=Number(1)})({});", true),
        ("escaped call method removal", r"(function(exports){exports.real=Number(1)}).\u0063all(null,exports);", "", true),
        ("escaped apply method removal", r"(function(exports){exports.real=Number(1)}).\u0061pply(null,[exports]);", "", true),
        ("escaped call method equivalence", public, r"(function(exports){exports.real=Number(1)}).\u0063all(null,exports);", false),
        ("escaped apply method equivalence", public, r"(function(exports){exports.real=Number(1)}).\u0061pply(null,[exports]);", false),
        ("escaped call unrelated object", public, r"(function(exports){exports.real=Number(1)}).\u0063all(null,{});", true),
        ("escaped apply unrelated object", public, r"(function(exports){exports.real=Number(1)}).\u0061pply(null,[{}]);", true),
        ("escaped require removal", r"module.exports=re\u0071uire('./side.cjs');", "", true),
        ("escaped require equivalence", "module.exports=require('./side.cjs');", r"module.exports=re\u0071uire('./side.cjs');", false),
        ("escaped require shadow", public, r"const \u0072equire=()=>({});module.exports=require('./side.cjs');", true),
        ("escaped UMD public parameter", public, r"(function(root,factory){factory(exports)})(this,function(\u0065xports){exports.real=Number(1)});", false),
        ("escaped UMD unrelated argument", public, r"(function(root,factory){factory({})})(this,function(\u0065xports){exports.real=Number(1)});", true),
        ("escaped bracket spelling", r"exports['re\u0061l']=Number(1);", public, false),
        ("deferred assignment", public, "function unused(){exports.real=Number(1);}", true),
        ("catch shadow", public, "try{throw {}}catch(exports){exports.real=1;}", true),
        ("destructured catch shadow", public, "try{throw {exports:{}}}catch({exports}){exports.real=1;}", true),
        ("destructured local shadow", public, "{const {exports}={exports:{}};exports.real=1;}", true),
        ("escaped local shadow", public, r"{const \u0065xports={};exports.real=1;}", true),
        ("rest parameter shadow", public, "(function(...exports){exports.real=1;})();", true),
        ("single arrow parameter shadow", public, "(exports=>{exports.real=1;})({});", true),
        ("named function shadow", public, "(function exports(){exports.real=1;})();", true),
        ("named class shadow", public, "const Api=class exports{static{exports.real=1;}};", true),
        ("uninvoked UMD factory", public, "(function(root,factory){})(this,function(exports){exports.real=1;});", true),
        ("UMD unrelated argument", public, "(function(root,factory){factory({});})(this,function(exports){exports.real=1;});", true),
        ("deferred UMD wrapper", public, "function unused(){(function(root,factory){factory(exports);})(this,function(exports){exports.real=1;});}", true),
        ("discarded UMD return", public, "(function(root,factory){factory();})(this,function(){return {real:1};});", true),
        ("static class block", public, "class Api {static{exports.real=Number(1);}}", false),
        ("static class field", public, "class Api {static field=(exports.real=Number(1));}", false),
        ("computed class method", public, "class Api {[(exports.real=Number(1),'method')](){}}", false),
        ("computed instance field", public, "class Api {[(exports.real=Number(1),'field')]=0;}", false),
        ("class heritage evaluation", public, "class Api extends (exports.real=Number(1),Object){}", false),
        ("UMD public return", public, "(function(root,factory){module.exports=factory();})(this,function(){return {real:Number(1)};});", false),
        ("shadowed export parameter", public, "(function(exports){exports.real=Number(1);})({});", true),
        ("private bundled assignment", "exports.real=Number(1);const bundled=(exports)=>{exports.internal=1;};", "exports.real=Number(1);const bundled=(exports)=>{};", false),
        ("shadowed module assignment", public, "(()=>{const module={};module.exports={real:Number(1)};})();", true),
        ("module export property", "module.exports.real=Number(1);", "", true),
        ("eager computed export", "(()=>{exports['re'+'al']=Number(1);})();", "", true),
        ("public IIFE parameter", public, "(function(exports){exports.real=Number(1);})(exports);", false),
        ("public call parameter", public, "(function(exports){exports.real=Number(1);}).call(null,exports);", false),
        ("public apply parameter", public, "(function(exports){exports.real=Number(1);}).apply(null,[exports]);", false),
        ("async before await", public, "(async()=>{exports.real=Number(1);await Promise.resolve();})();", false),
        ("direct eager equivalence", public, "(()=>{exports.real=Number(1);})();", false),
        ("call eager equivalence", public, "(function(){exports.real=Number(1);}).call(null);", false),
        ("apply eager equivalence", public, "(function(){exports.real=Number(1);}).apply(null,[]);", false),
        ("UMD export argument", public, "(function(root,factory){factory(exports);})(this,function(exports){exports.real=Number(1);});", false),
    ] {
        cases.push(serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.cjs":old,"side.cjs":"exports.real=Number(1);"}},"new":{"manifest":manifest,"files":{"index.cjs":new,"side.cjs":"exports.real=Number(1);"}},"consumer":consumer,"breaking":breaking,"runtime":"node"}));
    }
    cases.push(serde_json::json!({"name":"deferred default assignment","old":{"manifest":manifest,"files":{"index.cjs":"module.exports=function Api(){};"}},"new":{"manifest":manifest,"files":{"index.cjs":"function unused(){module.exports=function Api(){};}"}},"consumer":"const Api=(await import('node:module')).createRequire(import.meta.url)('assignment-scope');new Api();","breaking":true,"runtime":"node"}));
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn keyword_reexports_agree_with_node_namespaces() {
    let manifest =
        serde_json::json!({"name":"keyword-exports","type":"module","exports":"./index.js"});
    let cases: Vec<_> = [
        ("namespace reserved spelling", "export * as 'while' from './side.js';", "export * as while from './side.js';", "(await import('./index.js')).while.local.toFixed();", false),
        ("namespace default removal", "export * as default from './side.js';", "export {};", "(await import('./index.js')).default.local.toFixed();", true),
        ("named default spelling", "export {local as default} from './side.js';", "export {local as 'default'} from './side.js';", "(await import('./index.js')).default.toFixed();", false),
    ].into_iter().map(|(name, old, new, consumer, breaking)| {
        serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.js":old,"side.js":"export const local=Number(1);"}},"new":{"manifest":manifest,"files":{"index.js":new,"side.js":"export const local=Number(1);"}},"consumer":consumer,"breaking":breaking,"runtime":"node"})
    }).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

#[test]
fn post_await_commonjs_writes_are_not_available_to_immediate_require_consumers() {
    for (source, immediate) in [
        (
            "(async()=>{exports.real=1;await Promise.resolve();})();",
            true,
        ),
        (
            "(async()=>{await Promise.resolve();exports.real=1;})();",
            false,
        ),
        (
            "(async()=>{exports.real=await Promise.resolve(1);})();",
            false,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("package.json"),
            r#"{"name":"async-export","main":"index.cjs"}"#,
        )
        .unwrap();
        std::fs::write(root.path().join("index.cjs"), source).unwrap();
        let output = std::process::Command::new("node")
            .args([
                "--eval",
                "if(require('./index.cjs').real!==1)process.exitCode=1;",
            ])
            .current_dir(root.path())
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            immediate,
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (_, warnings) =
            pledge::ecosystems::node::discovery::discover_exports(root.path()).unwrap();
        assert_eq!(
            warnings
                .iter()
                .any(|warning| warning.incomplete && warning.message.contains("async suspension")),
            !immediate,
            "{source}: {warnings:?}"
        );
    }
}

#[tokio::test]
async fn inferred_commonjs_values_agree_with_require_consumers() {
    let cases = [
        ("whole module call", "function create(){return {api:Number(1)}}module.exports=create();", "function create(){return {renamed:Number(1)}}module.exports=create();", "api.api.toFixed()", true),
        ("whole module new", "class Api{api=Number(1)}module.exports=new Api();", "class Api{renamed=Number(1)}module.exports=new Api();", "api.api.toFixed()", true),
        ("whole module identifier", "const result={api:Number(1)};module.exports=result;", "const result={renamed:Number(1)};module.exports=result;", "api.api.toFixed()", true),
        ("whole module object", "module.exports={api:Number(1)};", "module.exports={renamed:Number(1)};", "api.api.toFixed()", true),
        ("whole module array", "module.exports=[Number(1)];", "module.exports=[String(1)];", "api[0].toFixed()", true),
        ("whole module primitive", "module.exports=Number(1);", "module.exports=String(1);", "api.toFixed()", true),
        ("whole module primitive-literal", "module.exports=1;", "module.exports=\"one\";", "api.toFixed()", true),
        ("whole module conditional", "module.exports=true?{api:Number(1)}:{api:Number(2)};", "module.exports=true?{renamed:Number(1)}:{renamed:Number(2)};", "api.api.toFixed()", true),
        ("factory-object", "function create(){return {api:Number(1)}}module.exports=create();", "module.exports={api:Number(1)};", "api.api.toFixed()", false),
        ("factory-property", "function create(){return {api:Number(1)}}module.exports=create();", "exports.api=Number(1);", "api.api.toFixed()", false),
        ("factory-identifier", "function create(){return {api:Number(1)}}module.exports=create();", "const value={api:Number(1)};module.exports=value;", "api.api.toFixed()", false),
        ("factory-renaming", "function create(){return {api:Number(1)}}module.exports=create();", "function build(){return {api:Number(1)}}module.exports=build();", "api.api.toFixed()", false),
        ("factory-default-field", "module.exports={default:Number(1),'a.b':Number(2)};", "function create(){return {default:Number(1),'a.b':Number(2)}}module.exports=create();", "api.default.toFixed();api['a.b'].toFixed()", false),
        ("factory-default-field-removed", "function create(){return {default:Number(1),'a.b':Number(2)}}module.exports=create();", "function create(){return {'a.b':Number(2)}}module.exports=create();", "api.default.toFixed();api['a.b'].toFixed()", true),
        ("factory-return-function", "function create(){return ()=>Number(1)}module.exports=create();", "function create(){return ()=>String(1)}module.exports=create();", "api().toFixed()", true),
        ("factory-return-class", "class Api{api=Number(1)}function create(){return new Api()}module.exports=create();", "class Api{api=String(1)}function create(){return new Api()}module.exports=create();", "api.api.toFixed()", true),
        ("literal-readonly", "module.exports={api:Number(1)};", "function create(){return {get api(){return Number(1)}}}module.exports=create();", "api.api=2;if(api.api!==2)throw Error('write lost')", true),
        ("readonly-compatible", "module.exports={get api(){return Number(1)}};", "function create(){return {get api(){return Number(1)}}}module.exports=create();", "api.api.toFixed()", false),
        ("factory-optional", "/** @returns {{api:number}} */function create(){return {api:Number(1)}}module.exports=create();", "/** @returns {{api?:number}} */function create(){return {}}module.exports=create();", "api.api.toFixed()", true),
        ("factory-setter-narrowed", "function create(){let state=1;return {get api(){return state},\n/** @param {number|string} value */set api(value){state=Number(value)}}}module.exports=create();", "function create(){let state=1;return {get api(){return state},\n/** @param {number} value */set api(value){if(typeof value!=='number')throw Error('number required');state=value}}}module.exports=create();", "api.api='2';if(api.api!==2)throw Error('write lost')", true),
        ("iterator-equivalent", "module.exports={[Symbol.iterator]:function*(){yield 1},'@@iterator':Number(2)};", "function create(){return {[Symbol.iterator]:function*(){yield 1},'@@iterator':Number(2)}}module.exports=create();", "if([...api][0]!==1||api['@@iterator']!==2)throw Error('identity');", false),
        ("registered-equivalent", "module.exports={[Symbol.for('key')]:Number(1),'[Symbol.for(\"key\")]':Number(2)};", "function create(){return {[Symbol.for('key')]:Number(1),'[Symbol.for(\"key\")]':Number(2)}}module.exports=create();", "api[Symbol.for('key')].toFixed();if(api['[Symbol.for(\"key\")]']!==2)throw Error('identity');", false),
        ("iterator-removal", "function create(){return {[Symbol.iterator]:function*(){yield 1},'@@iterator':Number(2)}}module.exports=create();", "function create(){return {'@@iterator':Number(2)}}module.exports=create();", "if([...api][0]!==1)throw Error('identity');", true),
        ("registered-removal", "function create(){return {[Symbol.for('key')]:Number(1),'@@unique(key)':Number(2)}}module.exports=create();", "function create(){return {'@@unique(key)':Number(2)}}module.exports=create();", "api[Symbol.for('key')].toFixed();", true),
        ("registered-key-rename", "function create(){return {[Symbol.for('key')]:Number(1)}}module.exports=create();", "function create(){return {[Symbol.for('renamed')]:Number(1)}}module.exports=create();", "api[Symbol.for('key')].toFixed();", true),
        ("iterator-string-removal", "function create(){return {[Symbol.iterator]:function*(){yield 1},'[Symbol.iterator]':function*(){yield 1}}}module.exports=create();", "function create(){return {[Symbol.iterator]:function*(){yield 1}}}module.exports=create();", "if([...api['[Symbol.iterator]']()][0]!==1)throw Error('string property');", true),
        ("registered-canonical-lookalike", "module.exports={[Symbol.for('key')]:Number(1),'@@for(\"key\")':Number(2)};", "function create(){return {[Symbol.for('key')]:Number(1),'@@for(\"key\")':Number(2)}}module.exports=create();", "api[Symbol.for('key')].toFixed();if(api['@@for(\"key\")']!==2)throw Error('string property');", false),
        ("iterator-string-collision", "function create(){return {[Symbol.iterator]:function*(){yield 1},'[Symbol.iterator]':function*(){yield 1}}}module.exports=create();", "function create(){return {'[Symbol.iterator]':function*(){yield 1}}}module.exports=create();", "if([...api][0]!==1)throw Error('iterator');", true),
    ];
    for (name, old, new, consumer, breaking) in cases {
        let mut maps = Vec::new();
        let mut directories = Vec::new();
        for (source, succeeds) in [(old, true), (new, !breaking)] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            std::fs::write(
                root.join("package.json"),
                r#"{"name":"commonjs-values","exports":{"require":"./index.cjs"}}"#,
            )
            .unwrap();
            std::fs::write(root.join("index.cjs"), source).unwrap();
            let output = std::process::Command::new("node")
                .current_dir(root)
                .args([
                    "--eval",
                    &format!("'use strict';const api=require('commonjs-values');{consumer}"),
                ])
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let (mut exports, warnings) =
                pledge::ecosystems::node::discovery::discover_exports(root).unwrap();
            assert!(warnings.is_empty(), "{name}: {warnings:?}");
            let enrichment =
                pledge::languages::typescript::compiler::enrich_exports(&mut exports, root)
                    .await
                    .unwrap();
            assert!(
                matches!(
                    enrichment,
                    pledge::languages::typescript::compiler::EnrichResult::Enriched
                ),
                "{name}: {enrichment:?}"
            );
            maps.push(exports);
            directories.push(directory);
        }
        let findings = pledge::languages::javascript::compare::compare_exports(
            &maps[0],
            &maps[1],
            directories[1].path(),
            true,
        );
        assert_eq!(
            pledge::core::findings::has_breaking_changes(&findings),
            breaking,
            "{name}: {findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{name}: {findings:?}"
        );
    }
}

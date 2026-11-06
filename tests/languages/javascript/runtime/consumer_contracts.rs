//! Real Node consumers independently verify static runtime-contract findings.

#[test]
fn exported_syntax_and_runtime_contracts_agree_with_node_consumers() {
    let cases = [
        ("direct var removal", "export var value=1;", "export {};", "(await import('./index.js')).value.toFixed();", true, false),
        ("generator removal", "export function* value(){yield 1;}", "export {};", "(await import('./index.js')).value().next();", true, false),
        ("string export rename", "const local=1;export{local as 'public-name'};", "const local=1;export{local as 'renamed'};", "(await import('./index.js'))['public-name'].toFixed();", true, false),
        ("function construction", "export const Api=function(){};", "export const Api=()=>{};", "new (await import('./index.js')).Api();", true, false),
        ("JavaScript inheritance", "export class BaseA{} export class BaseB{} export class Api extends BaseA{}", "export class BaseA{} export class BaseB{} export class Api extends BaseB{}", "const api=await import('./index.js');if(!(new api.Api() instanceof api.BaseA))throw Error('changed base');", true, false),
        ("constant base alias", "class Base{}export class Api extends Base{}", "class Base{}const Alias=Base;export class Api extends Alias{}", "const api=await import('./index.js');if(Object.getPrototypeOf(api.Api).name!=='Base')throw Error('changed base');", false, false),
        ("escaped export spelling", "export const value=1;", r"export const \u0076alue=1;", "(await import('./index.js')).value.toFixed();", false, false),
        ("discarded export copy", "exports.real=1;", "Object.assign({},exports,{real:1});", "(await import('./index.js')).default.real.toFixed();", true, true),
        ("descriptor key alias", "const key='value';Object.defineProperty(exports,key,{value:1});", "const renamed='value';Object.defineProperty(exports,renamed,{value:1});", "(await import('./index.js')).default.value.toFixed();", false, true),
        ("descriptor write removal", "Object.defineProperty(exports,'value',{value:1,writable:true});", "Object.defineProperty(exports,'value',{value:1,writable:false});", "const api=(await import('./index.js')).default;api.value=2;if(api.value!==2)throw Error('write lost');", true, true),
        ("descriptor enumeration removal", "Object.defineProperty(exports,'value',{value:1,enumerable:true});", "Object.defineProperty(exports,'value',{value:1,enumerable:false});", "const api=(await import('./index.js')).default;if(!Object.keys(api).includes('value'))throw Error('enumeration lost');", true, true),
    ];
    let cases: Vec<_> = cases.into_iter().map(|(name, old, new, consumer, breaking, commonjs)| {
        let manifest = serde_json::json!({"name":"runtime-contracts","type":if commonjs {"commonjs"} else {"module"},"exports":"./index.js"});
        serde_json::json!({"name":name,"old":{"manifest":manifest,"files":{"index.js":old}},"new":{"manifest":manifest,"files":{"index.js":new}},"consumer":consumer,"breaking":breaking,"runtime":"node"})
    }).collect();
    crate::node_consumers::assert_consumer_contracts(&serde_json::to_string(&cases).unwrap());
}

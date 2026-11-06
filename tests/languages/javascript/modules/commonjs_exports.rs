//! Commonjs exports compatibility checks.

use pledge::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use pledge::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn test_cjs_module_exports_bulk_assignment() {
    let source = b"const foo = 1;\nconst bar = 2;\nmodule.exports = { foo, bar };";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "CJS bulk export should capture foo; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "CJS bulk export should capture bar; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_exports_property_assignment() {
    let source = b"exports.hello = function() {};\nexports.world = 42;";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(exports
        .iter()
        .any(|e| e.name == "hello" && e.kind == RawExportKind::Named));
    assert!(exports
        .iter()
        .any(|e| e.name == "world" && e.kind == RawExportKind::Named));
}

#[test]
fn test_cjs_module_exports_single_default() {
    let source = b"module.exports = MyClass;";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
        "CJS single export should be treated as default; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_not_detected_in_typescript() {
    // CJS patterns should be ignored in TypeScript (which uses `export =` instead)
    let source = b"module.exports = { foo: 1 };";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(
        !exports.iter().any(|e| e.name == "foo"),
        "CJS should not fire in TypeScript; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn deferred_and_shadowed_commonjs_assignments_are_not_public_exports() {
    for source in [
        "function unused(){exports.real=1;}",
        "function unused(){exports['real']=1;}",
        "const bundled=(exports)=>{exports.real=1;};",
        "(function(exports){exports.real=1;})({});",
        "(()=>{const module={};module.exports={real:1};})();",
        "function unused(){module.exports=function Api(){};}",
        "function unused(){module.exports={real:1};}",
        "function unused(){exports=module.exports={real:1};}",
        "function* unused(){exports.real=1;}",
        "class Unused { field=(exports.real=1); method(){exports.real=1;} }",
        "(function(){exports.real=1;}).bind(null);",
        "try{throw {}}catch(exports){exports.real=1;}",
        "try{throw {exports:{}}}catch({exports}){exports.real=1;}",
        "{const {exports}={exports:{}};exports.real=1;}",
        r"{const \u0065xports={};exports.real=1;}",
        r"(function(\u0065xports){exports.real=1;})({});",
        r"const \u004fbject={defineProperty(){}};Object.defineProperty(exports,'real',{value:1});",
        "(function(...exports){exports.real=1;})();",
        "(exports=>{exports.real=1;})({});",
        "(function exports(){exports.real=1;})();",
        "const Api=class exports{static{exports.real=1;}};",
        "(function(root,factory){})(this,function(exports){exports.real=1;});",
        "(function(root,factory){factory({});})(this,function(exports){exports.real=1;});",
        r"(function(root,factory){factory({});})(this,function(\u0065xports){exports.real=1;});",
        "function unused(){(function(root,factory){factory(exports);})(this,function(exports){exports.real=1;});}",
        "(function(root,factory){factory();})(this,function(){return {real:1};});",
    ] {
        let exports = parse_exports(source.as_bytes(), SourceLanguage::JavaScript).unwrap();
        assert!(exports.is_empty(), "{source}: {exports:?}");
    }
}

#[test]
fn eager_commonjs_writes_preserve_public_property_identity() {
    for source in [
        "module.exports.real=1;",
        r"\u0065xports.real=1;",
        r"m\u006fdule.exports.real=1;",
        r"module.\u0065xports.real=1;",
        r"(function(\u0065xports){exports.real=1;})(exports);",
        r"\u004fbject.defineProperty(exports,'real',{value:1});",
        r"Object.\u0064efineProperty(exports,'real',{value:1});",
        r"(function(exports){exports.real=1;}).\u0063all(null,exports);",
        r"(function(exports){exports.real=1;}).\u0061pply(null,[exports]);",
        "(function(exports){exports.real=1;})(exports);",
        "(function(exports){exports.real=1;}).call(null,exports);",
        "(function(exports){exports.real=1;}).apply(null,[exports]);",
        "(exports=>{exports.real=1;})(exports);",
        "(async()=>{exports.real=1;await Promise.resolve();})();",
        "(async()=>{exports.real=1;})();",
        r"exports['re\u0061l']=1;",
        "exports.__esModule=true;exports.real=1;",
        "{const {exports:other}={exports:{}};exports.real=1;}",
        "(function(root,factory){module.exports=factory();})(this,function(){return {real:1};});",
        "class Api { static { exports.real=1; } }",
        "class Api { static field=(exports.real=1); }",
        "class Api { [(exports.real=1,'method')](){} }",
        "class Api { [(exports.real=1,'field')]=0; }",
        "class Api extends (exports.real=1,Object) {}",
        "module.exports['real']=1;",
        "module.exports={'real':1};",
        "module.exports={['re'+'al']:1};",
        "(()=>{exports.real=1;})();",
        "(()=>{exports['re'+'al']=1;})();",
        "(function(){exports.real=1;}).call(null);",
        "(function(){exports.real=1;}).apply(null,[]);",
        "function unused(){exports.real=1;}exports.real=2;",
        "(function(root,factory){factory(exports);})(this,function(exports){exports.real=1;});",
    ] {
        let exports = parse_exports(source.as_bytes(), SourceLanguage::JavaScript).unwrap();
        let names: Vec<_> = exports.iter().map(|export| export.name.as_str()).collect();
        assert_eq!(names, ["real"], "{source}: {exports:?}");
    }
}

#[test]
fn commonjs_writes_after_await_are_explicitly_incomplete() {
    for source in [
        "(async()=>{await Promise.resolve();exports.real=1;})();",
        "(async()=>{exports.real=await Promise.resolve(1);})();",
        "(async()=>{await Promise.resolve();Object.defineProperty(exports,'real',{value:1});})();",
    ] {
        let error = parse_exports(source.as_bytes(), SourceLanguage::JavaScript).unwrap_err();
        assert!(
            error.to_string().contains("async suspension"),
            "{source}: {error}"
        );
    }
}

//! Object helpers describe exports only when they mutate the public object.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::{compare_exports, structure::compare_structural_diff};
use pledge::languages::javascript::model::ExportKey;
use pledge::languages::javascript::syntax::{
    exports::{parse_exports, RawExportKind},
    grammar::SourceLanguage,
};

fn package(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"name":"object-helpers","main":"index.cjs"}"#,
    )
    .unwrap();
    std::fs::write(root.path().join("index.cjs"), source).unwrap();
    root
}

#[test]
fn copies_unrelated_targets_shadowed_builtins_and_deferred_calls_cannot_conceal_removal() {
    let old = package("exports.real=Number(1);");
    let (before, _) = discover_exports(old.path()).unwrap();
    for source in [
        "Object.assign({},exports,{real:1});",
        "function unused(){Object.assign(exports,{real:1});}",
        "module.other={};Object.defineProperty(module.other,'real',{value:1});",
        "const Object={assign(){}};Object.assign(exports,{real:1});",
        "function unused(){Object.defineProperties(exports,{real:{value:1}});}",
        "function unused(exports){Object.defineProperty(exports,'real',{value:1});}",
    ] {
        let new = package(source);
        let (after, _) = discover_exports(new.path()).unwrap();
        assert!(
            !after.contains_key(&ExportKey::new_root_value("real")),
            "{source}: {after:?}"
        );
        let findings = compare_exports(&before, &after, new.path(), false);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "export-removed"),
            "{source}: {findings:?}"
        );
    }
}

#[test]
fn helper_keys_use_constant_and_symbol_identity() {
    for source in [
        "const key='value';Object.defineProperty(exports,key,{value:1});",
        "const renamed='value';Object.defineProperty(exports,renamed,{value:1});",
        "Object.defineProperties(exports,{'value':{value:1}});",
        "Object.defineProperties(exports,{['val'+'ue']:{value:1}});",
        "Object.assign(exports,{'value':1});",
        "module.exports=Object.assign({},exports,{value:1});",
        "(()=>{Object.assign(exports,{value:1});})();",
    ] {
        let exports = parse_exports(source.as_bytes(), SourceLanguage::JavaScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|export| export.name == "value" && export.kind == RawExportKind::Named),
            "{source}: {exports:?}"
        );
    }
    let exports = parse_exports(
        b"Object.defineProperty(exports,Symbol.iterator,{value:function*(){}});",
        SourceLanguage::JavaScript,
    )
    .unwrap();
    assert!(
        exports
            .iter()
            .any(|export| export.name == "@@iterator" && export.kind == RawExportKind::Symbol),
        "{exports:?}"
    );
}

#[test]
fn removing_static_descriptor_capabilities_breaks_the_export_contract() {
    let old =
        package("Object.defineProperty(exports,'value',{value:1,writable:true,enumerable:true});");
    let (before, _) = discover_exports(old.path()).unwrap();
    for (source, expected) in [
        (
            "Object.defineProperty(exports,'value',{value:1,writable:false,enumerable:true});",
            "export-writability-removed",
        ),
        (
            "Object.defineProperty(exports,'value',{value:1,writable:true});",
            "export-enumerability-removed",
        ),
        (
            "Object.defineProperties(exports,{value:{value:1,writable:true,enumerable:false}});",
            "export-enumerability-removed",
        ),
    ] {
        let new = package(source);
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings = compare_structural_diff(old.path(), new.path(), &before, &after);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.rule == expected)
                .count(),
            1,
            "{source}: {findings:?}"
        );
    }
    let unchanged = package("const key='value';Object.defineProperties(exports,{[key]:{value:1,writable:true,enumerable:true}});");
    let (after, _) = discover_exports(unchanged.path()).unwrap();
    assert!(compare_structural_diff(old.path(), unchanged.path(), &before, &after).is_empty());
}

#[test]
fn unresolved_helper_keys_are_explicitly_incomplete() {
    for source in [
        "Object.defineProperty(exports,getKey(),{value:1});",
        "Object.defineProperties(exports,{[getKey()]:{value:1}});",
        "Object.assign(exports,{[getKey()]:1});",
    ] {
        let root = package(source);
        let (_, warnings) = discover_exports(root.path()).unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.incomplete && warning.message.contains("property key")),
            "{source}: {warnings:?}"
        );
    }
}

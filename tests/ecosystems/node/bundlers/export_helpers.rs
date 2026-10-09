//! Export helpers compatibility checks.

use interlint::discover::tree_query::{parse_exports, RawExportKind, SourceLanguage};

#[test]
fn esbuild_export_helper_captures_arrow_getters() {
    let source = br#"
        var __export = (target, all) => {
            for (var name in all) __defProp(target, name, { get: all[name], enumerable: true });
        };
        __export(exports, {
          foo: () => foo,
          bar: () => bar
        });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "BUN-01: foo not detected. Got: {:?}",
        names
    );
    assert!(
        names.contains(&"bar"),
        "BUN-01: bar not detected. Got: {:?}",
        names
    );
    assert!(
        !names.contains(&"__esModule"),
        "BUN-07: __esModule must be filtered. Got: {:?}",
        names
    );
}

#[test]
fn esbuild_minified_helper_name_recognized() {
    // Helper renamed to short name (minified)
    let source = br#"
        var l = (a, b) => {
            for (var k in b)
                Object.defineProperty(a, k, { get: b[k], enumerable: true });
        };
        l(exports, { foo: () => foo, bar: () => bar });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo") && names.contains(&"bar"),
        "BUN-02: minified helper call not detected. Got: {:?}",
        names
    );
}

#[test]
fn defprop_alias_declaration_with_object_defineproperty() {
    // BUN-03: `var __defProp = Object.defineProperty;` — alias declaration is captured
    // into the cjs_aliases map so BUN-06a can resolve it.
    // In real esbuild output, the alias is used inside the __export helper (BUN-02).
    // The direct-call form uses Object.defineProperty (captured by BUN-06a directly).
    let source = br#"
        var __defProp = Object.defineProperty;
        var __export = (target, all) => {
            for (var name in all) __defProp(target, name, { get: all[name], enumerable: true });
        };
        __export(exports, { foo: () => foo });
        Object.defineProperty(exports, "bar", { value: 2, enumerable: true });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    // BUN-02 captures foo from the __export call
    assert!(
        names.contains(&"foo"),
        "BUN-03: foo not captured via __export helper. Got: {:?}",
        names
    );
    // BUN-06a captures bar from Object.defineProperty directly
    assert!(
        names.contains(&"bar"),
        "BUN-03: bar not captured via Object.defineProperty. Got: {:?}",
        names
    );
}

#[test]
fn tsc_void_0_placeholders_captured() {
    let source = br#"
        "use strict";
        Object.defineProperty(exports, "__esModule", { value: true });
        exports.foo = exports.bar = void 0;
        exports.foo = function foo() {};
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "BUN-04: foo not captured. Got: {:?}",
        names
    );
    assert!(
        names.contains(&"bar"),
        "BUN-04: bar not captured (chained LHS). Got: {:?}",
        names
    );
    assert!(
        !names.contains(&"__esModule"),
        "BUN-07: __esModule must be filtered. Got: {:?}",
        names
    );
}

#[test]
fn swc_star_reexport_shim_emits_star_reexport() {
    let source = br#"
        var other = require("./other");
        Object.keys(other).forEach(function(key) {
            if (key === "default" || key === "__esModule") return;
            exports[key] = other[key];
        });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let has_star = raw
        .iter()
        .any(|e| matches!(e.kind, RawExportKind::StarReexport));
    assert!(
        has_star,
        "BUN-05: swc star re-export shim not detected. Got: {:?}",
        raw
    );
}

#[test]
fn object_defineproperty_captured() {
    let source = br#"
        Object.defineProperty(exports, "foo", { value: 1, enumerable: true });
        Object.defineProperties(exports, {
            bar: { value: 2, enumerable: true },
            baz: { value: 3, enumerable: true }
        });
        Object.assign(exports, { qux: 4 });
        exports["zap"] = 5;
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    for expected in ["foo", "bar", "baz", "qux", "zap"] {
        assert!(
            names.contains(&expected),
            "BUN-06: {} not captured. Got: {:?}",
            expected,
            names
        );
    }
}

#[test]
fn esmodule_always_filtered() {
    let source = br#"
        Object.defineProperty(exports, "__esModule", { value: true });
        exports.foo = 1;
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !names.contains(&"__esModule"),
        "BUN-07: __esModule not filtered. Got: {:?}",
        names
    );
    assert!(
        names.contains(&"foo"),
        "BUN-07: valid exports still retained. Got: {:?}",
        names
    );
}

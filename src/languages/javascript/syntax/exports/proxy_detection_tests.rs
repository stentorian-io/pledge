use crate::languages::javascript::syntax::exports::parse_exports;
use crate::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn proxy_module_exports_detected() {
    let source = b"module.exports = new Proxy({}, {});";
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let default_exp = raw
        .iter()
        .find(|e| e.name == "default")
        .expect("default not captured");
    assert!(
        !default_exp.is_proxy_dispatched,
        "an empty proxy handler forwards its target"
    );
}

#[test]
fn reflect_construct_module_exports_detected() {
    let source = b"module.exports = Reflect.construct(SomeClass, []);";
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let default_exp = raw
        .iter()
        .find(|e| e.name == "default")
        .expect("default not captured");
    assert!(
        !default_exp.is_proxy_dispatched,
        "ordinary Reflect.construct retains its target instance surface"
    );
}

#[test]
fn non_proxy_call_expression_not_flagged() {
    let source = b"module.exports = require('./other');";
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    // require() is a call_expression with a plain identifier function (not a member_expression).
    // The Proxy query's call_expression branch requires function: (member_expression...).
    // So this should NOT match the Proxy query; it falls through to existing identifier patterns.
    for e in &raw {
        if e.name == "default" {
            assert!(
                !e.is_proxy_dispatched,
                "require() must not flag is_proxy_dispatched"
            );
        }
    }
}

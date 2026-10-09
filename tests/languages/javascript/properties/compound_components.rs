//! Compound components compatibility checks.

use interlint::ecosystems::node::discovery::discover_exports;

fn write_pkg(dir: &std::path::Path, pkg: serde_json::Value) {
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
}

#[test]
fn compound_component_menu_item_attaches() {
    let dir = tempfile::tempdir().unwrap();
    // NOTE:  conservative rule — attach ONLY when target identifier is
    // itself an exported symbol declared in the same file. Direct property
    // assignment (`Menu.Item = ...`) and `Object.assign` attach members.
    // Mutate the function's properties without reassigning the const binding.
    let src = r#"
export const Menu = () => null;
Menu.Item = () => null;
Menu.Header = () => null;
Object.assign(Menu, { Footer: () => null });
"#;
    std::fs::write(dir.path().join("menu.ts"), src).unwrap();
    write_pkg(
        dir.path(),
        serde_json::json!({
            "name": "ui-lib",
            "version": "1.0.0",
            "main": "./menu.ts"
        }),
    );

    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    let menu_key = exports
        .keys()
        .find(|k| k.name == "Menu")
        .expect("Menu should be discovered");
    let menu_sym = &exports[menu_key];

    let has_item = menu_sym.compound_members.iter().any(|m| m == "Item");
    let has_header = menu_sym.compound_members.iter().any(|m| m == "Header");
    let has_footer = menu_sym.compound_members.iter().any(|m| m == "Footer");
    assert!(
        has_item && has_header && has_footer,
        "expected Item/Header/Footer in Menu.compound_members, got {:?}",
        menu_sym.compound_members
    );

    // Only parse/ERROR warnings are a concern; resolver-level warnings about
    // entry-point resolution etc. are acceptable.
    let parse_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("parse") || w.message.contains("ERROR"))
        .collect();
    assert!(
        parse_warnings.is_empty(),
        "no parse warnings expected, got {:?}",
        parse_warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

//! Prototype identity is observable even when base classes have equal member shapes.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::structure::compare_structural_diff;

fn package(source: &str, extension: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        format!(r#"{{"name":"heritage","type":"module","exports":"./index.{extension}"}}"#),
    )
    .unwrap();
    std::fs::write(root.path().join(format!("index.{extension}")), source).unwrap();
    root
}

#[test]
fn changing_the_base_class_changes_its_inheritance_contract() {
    for extension in ["js", "ts"] {
        let old = package(
            "class BaseA{};class BaseB{};export class Api extends BaseA {}",
            extension,
        );
        let new = package(
            "class BaseA{};class BaseB{};export class Api extends BaseB {}",
            extension,
        );
        let (before, _) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings = compare_structural_diff(old.path(), new.path(), &before, &after);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "class-inheritance-changed"),
            "{extension}: {findings:?}"
        );
    }
}

#[test]
fn a_constant_alias_of_the_same_base_preserves_inheritance() {
    for extension in ["js", "ts"] {
        let old = package("class Base{};export class Api extends Base {}", extension);
        let new = package(
            "class Base{};const Alias=Base;export class Api extends Alias {}",
            extension,
        );
        let (before, _) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings = compare_structural_diff(old.path(), new.path(), &before, &after);
        assert!(
            !findings
                .iter()
                .any(|finding| finding.rule == "class-inheritance-changed"),
            "{extension}: {findings:?}"
        );
    }
}

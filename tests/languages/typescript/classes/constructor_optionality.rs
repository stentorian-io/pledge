//! Constructor optionality compatibility checks.

use pledge::languages::javascript::compare::classes::compare_class_shapes;
use pledge::languages::javascript::model::{ClassMember, ClassShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn make_class(members: Vec<ClassMember>, ctor_optionality: Vec<bool>) -> ClassShape {
    ClassShape {
        members,
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: ctor_optionality,
        decorators: Vec::new(),
        has_dynamic_static_key: false,
    }
}

#[test]
fn constructor_param_optional_to_required_fires_major() {
    // old=[false, true] -> position 1 is optional
    // new=[false, false] -> position 1 is now required
    let old_shape = make_class(vec![], vec![false, true]);
    let new_shape = make_class(vec![], vec![false, false]);
    let findings = compare_class_shapes(&old_shape, &new_shape, "MyClass", &test_file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "constructor-param-made-required"),
        "IFT-03: expected constructor-param-made-required. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "constructor-param-made-required")
        .unwrap();
    assert_eq!(
        f.severity,
        pledge::core::findings::Severity::Major,
        "IFT-03: constructor-param-made-required should be Major"
    );
}

fn package(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"type":"module","exports":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(directory.path().join("index.ts"), source).unwrap();
    directory
}

fn compare_packages(
    old: &std::path::Path,
    new: &std::path::Path,
) -> Vec<pledge::core::findings::Finding> {
    let (old_exports, _) = pledge::ecosystems::node::discovery::discover_exports(old).unwrap();
    let (new_exports, _) = pledge::ecosystems::node::discovery::discover_exports(new).unwrap();
    let mut findings = pledge::languages::javascript::compare::compare_exports(
        &old_exports,
        &new_exports,
        new,
        false,
    );
    findings.extend(
        pledge::languages::javascript::compare::structure::compare_structural_diff(
            old,
            new,
            &old_exports,
            &new_exports,
        ),
    );
    findings
}

#[test]
fn implicit_base_constructors_are_not_removed_methods() {
    for old_source in [
        "export class C { constructor() {} }",
        "export class C { constructor(a = 1) {} }",
        "export class C { constructor(...args: string[]) {} }",
        "export declare class C { constructor(); }",
        "export declare class C { constructor(a?: number); }",
        "export declare class C { constructor(...args: string[]); }",
    ] {
        let old = package(old_source);
        let new = package("export class C {}");
        let findings = compare_packages(old.path(), new.path());
        assert!(
            !pledge::core::findings::has_breaking_changes(&findings),
            "{old_source}: {findings:?}"
        );
    }
}

#[test]
fn inherited_implicit_constructors_need_resolved_evidence() {
    let old = package("class Base { constructor(x: string) {} } export class C extends Base { constructor() { super('x'); } }");
    let new = package("class Base { constructor(x: string) {} } export class C extends Base {}");
    let findings = compare_packages(old.path(), new.path());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"),
        "{findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.rule == "class-method-removed"),
        "{findings:?}"
    );
}

#[test]
fn a_static_method_named_constructor_can_be_removed() {
    let old = package("export class C { static constructor() {} }");
    let new = package("export class C {}");
    let findings = compare_packages(old.path(), new.path());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "class-method-removed"),
        "{findings:?}"
    );
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn compiler_and_consumers_agree_on_implicit_constructors() {
    use pledge::ecosystems::node::discovery::discover_exports;
    use pledge::languages::javascript::compare::{
        compare_exports, structure::compare_structural_diff,
    };
    use pledge::languages::typescript::compiler::{enrich_prepared_exports, EnrichResult};
    for (old_source, new_source, breaks) in [
        ("export class C { constructor() {} }", "export class C {}", false),
        ("class Base { constructor() {} } export class C extends Base { constructor() { super(); } }", "class Base { constructor() {} } export class C extends Base {}", false),
        ("class Base { constructor(x: string) {} } export class C extends Base { constructor() { super('x'); } }", "class Base { constructor(x: string) {} } export class C extends Base {}", true),
    ] {
        let old = package(old_source);
        let new = package(new_source);
        for (directory, succeeds) in [(&old, true), (&new, !breaks)] {
            std::fs::write(directory.path().join("consumer.ts"), "import { C } from './index.js'; new C();").unwrap();
            let consumer = std::process::Command::new("tsc")
                .args(["--strict", "--noEmit", "--target", "ES2022", "--module", "NodeNext", "consumer.ts"])
                .current_dir(directory.path()).output().unwrap();
            assert_eq!(consumer.status.success(), succeeds, "{}", String::from_utf8_lossy(&consumer.stdout));
        }
        let (mut before, _) = discover_exports(old.path()).unwrap();
        let (mut after, _) = discover_exports(new.path()).unwrap();
        for (exports, directory) in [(&mut before, old.path()), (&mut after, new.path())] {
            let result = enrich_prepared_exports(exports, directory).await.unwrap();
            assert!(matches!(result, EnrichResult::Enriched), "{result:?}");
        }
        let mut findings = compare_exports(&before, &after, new.path(), true);
        findings.extend(compare_structural_diff(old.path(), new.path(), &before, &after));
        assert_eq!(pledge::core::findings::has_breaking_changes(&findings), breaks, "{findings:?}");
        assert!(!findings.iter().any(|finding| finding.rule == "analysis-incomplete"), "{findings:?}");
    }
}

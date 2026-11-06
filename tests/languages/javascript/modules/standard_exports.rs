//! Standard module names must survive grammar extraction and package discovery.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::model::ExportKey;
use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;

fn package(source: &str, extension: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        format!(r#"{{"name":"standard-exports","type":"module","exports":"./index.{extension}"}}"#),
    )
    .unwrap();
    std::fs::write(directory.path().join(format!("index.{extension}")), source).unwrap();
    directory
}

#[test]
fn removing_direct_variables_and_generators_removes_public_exports() {
    for extension in ["js", "ts"] {
        for source in [
            "export var value=1;",
            "export var {nested:{value}}={nested:{value:1}};",
            "export var [value]=[1];",
            "export function* value(){yield 1;}",
            "export async function* value(){yield 1;}",
        ] {
            let old = package(source, extension);
            let new = package("export {};", extension);
            let (before, warnings) = discover_exports(old.path()).unwrap();
            let (after, _) = discover_exports(new.path()).unwrap();

            assert!(warnings.is_empty(), "{source}: {warnings:?}");
            assert!(
                before.contains_key(&ExportKey::new_root_value("value")),
                "{source}: {before:?}"
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
}

#[test]
fn public_string_aliases_keep_local_and_exported_names_separate() {
    for extension in ["js", "ts"] {
        let old = package("const local=1;export{local as 'public-name'};", extension);
        let new = package("const local=1;export{local as 'renamed'};", extension);
        let (before, warnings) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();

        assert!(warnings.is_empty(), "{warnings:?}");
        let symbol = before
            .get(&ExportKey::new_root_value("public-name"))
            .unwrap();
        assert_eq!(symbol.local_name.as_deref(), Some("local"));
        assert!(!before.contains_key(&ExportKey::new_root_value("local")));
        let findings = compare_exports(&before, &after, new.path(), false);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "export-removed"
                    && finding.message.contains("public-name")),
            "{findings:?}"
        );
    }
}

#[test]
fn string_named_reexports_and_namespaces_preserve_their_public_identity() {
    for extension in ["js", "ts"] {
        let root = package("export * as 'public-name' from './dep.js';export {'original-name' as 'renamed'} from './dep.js';", extension);
        std::fs::write(
            root.path().join("dep.js"),
            "const value=1;export{value as 'original-name'};",
        )
        .unwrap();
        let (exports, warnings) = discover_exports(root.path()).unwrap();

        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(
            exports
                .keys()
                .any(|key| key.name == "public-name.original-name"),
            "{exports:?}"
        );
        assert!(
            exports.contains_key(&ExportKey::new_root_value("renamed")),
            "{exports:?}"
        );
    }
}

#[test]
fn escaped_identifiers_use_decoded_names_without_reinterpreting_literal_backslashes() {
    let source = br#"export const \u0076alue=1;const local=2;export {local as "\\u0076alue"};"#;
    for language in [SourceLanguage::JavaScript, SourceLanguage::TypeScript] {
        let exports = parse_exports(source, language).unwrap();
        let names: Vec<_> = exports.iter().map(|export| export.name.as_str()).collect();
        assert!(names.contains(&"value"), "{names:?}");
        assert!(names.contains(&r"\u0076alue"), "{names:?}");
    }
}

#[test]
fn reexport_attributes_preserve_the_export_and_original_coordinates() {
    for language in [SourceLanguage::JavaScript, SourceLanguage::TypeScript] {
        for source in [
            "export {default as config} from './data.json' with {type:'json'};",
            "export * as config from './data.json' with {type:'json'};",
            "export * from './data.json' with {type:'json'};",
            "export {default as config} from './data.json'\n/* attribute */ with {type:'json'};\nexport const next=1;",
        ] {
            let exports = parse_exports(source.as_bytes(), language).unwrap_or_else(|error| panic!("{language:?}: {source}: {error:#}"));
            assert!(exports.iter().any(|export| export.source.as_deref() == Some("./data.json")), "{source}: {exports:?}");
            if source.contains("next") {
                assert_eq!(exports.iter().find(|export| export.name == "next").unwrap().line, 3);
            }
        }
    }
}

#[test]
fn invalid_public_syntax_reports_incomplete_discovery() {
    let root = package("export function api(value: string:):string;", "ts");
    let (_, warnings) = discover_exports(root.path()).unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("parse failed") && warning.incomplete),
        "{warnings:?}"
    );
    assert!(parse_exports(
        b"export {default as config} from './data.json' with {type:};",
        SourceLanguage::JavaScript
    )
    .is_err());
}

#[test]
fn invalid_attribute_objects_are_not_masked_as_valid_module_syntax() {
    for attributes in [
        "{type:7}",
        "{...options}",
        "{type:'json',type:'json'}",
        "{['type']:'json'}",
    ] {
        let source = format!("export {{default as config}} from './data.json' with {attributes};");
        for language in [SourceLanguage::JavaScript, SourceLanguage::TypeScript] {
            assert!(
                parse_exports(source.as_bytes(), language).is_err(),
                "{source}"
            );
        }
    }
}

#[test]
fn reserved_word_aliases_retain_their_exact_public_names() {
    for alias in ["null", "function", "instanceof", "catch", "void"] {
        let source = format!("const local=1;export{{local as {alias}}};\nexport const next=2;");
        let exports = parse_exports(source.as_bytes(), SourceLanguage::JavaScript).unwrap();
        let export = exports.iter().find(|export| export.name == alias).unwrap();
        assert_eq!(export.local_name.as_deref(), Some("local"));
        assert_eq!(
            exports
                .iter()
                .find(|export| export.name == "next")
                .unwrap()
                .line,
            2
        );
    }
}

#[test]
fn anonymous_default_declaration_signatures_preserve_public_names_and_positions() {
    let old = package(
        "export default function (): number;\nexport declare const next:string;",
        "d.ts",
    );
    let new = package(
        "export default function (): string;\nexport declare const next:string;",
        "d.ts",
    );
    let (before, old_warnings) = discover_exports(old.path()).unwrap();
    let (after, new_warnings) = discover_exports(new.path()).unwrap();
    assert!(
        old_warnings.is_empty() && new_warnings.is_empty(),
        "{old_warnings:?} {new_warnings:?}"
    );
    assert!(
        before.contains_key(&ExportKey::new_root_value("default")),
        "{before:?}"
    );
    assert!(
        before.contains_key(&ExportKey::new_root_value("next")),
        "{before:?}"
    );
    assert_eq!(
        before.get(&ExportKey::new_root_value("next")).unwrap().line,
        2
    );
    assert!(after.contains_key(&ExportKey::new_root_value("default")));
}

#[test]
fn grammar_recovery_keeps_malformed_declarations_and_aliases_incomplete() {
    for (source, language) in [
        (
            "const local=1;export{local as 123};",
            SourceLanguage::JavaScript,
        ),
        (
            "const local=1;export{local as function as null};",
            SourceLanguage::JavaScript,
        ),
        (
            "export interface Box<out T U> {}",
            SourceLanguage::TypeScript,
        ),
        (
            "export interface Box<out out T> {}",
            SourceLanguage::TypeScript,
        ),
        (
            "export interface Box<in in T> {}",
            SourceLanguage::TypeScript,
        ),
        (
            "export interface Box<out in T> {}",
            SourceLanguage::TypeScript,
        ),
        ("export default function ():;", SourceLanguage::TypeScript),
        (
            "export default function (value: string:):string;",
            SourceLanguage::TypeScript,
        ),
    ] {
        assert!(
            parse_exports(source.as_bytes(), language).is_err(),
            "{source}"
        );
    }
}

#[test]
fn anonymous_default_signature_names_do_not_depend_on_whitespace_or_generics() {
    for source in [
        "export default function(x:string):string;",
        "export default function<T>(x:T):T;",
    ] {
        let exports = parse_exports(source.as_bytes(), SourceLanguage::TypeScript).unwrap();
        assert_eq!(exports.len(), 1, "{source}: {exports:?}");
        assert_eq!(exports[0].name, "default");
    }
}

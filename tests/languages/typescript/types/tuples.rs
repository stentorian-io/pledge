use interlint::compare::{has_breaking_changes, subtype::is_subtype_of, type_diff::diff_types};
use interlint::types::parser::parse_type_string;
use std::path::Path;

#[test]
fn tuple_requirements_affect_assignability() {
    for (old, new, compatible) in [
        ("[x?: string]", "[x: string]", false),
        ("[string?]", "[string]", false),
        ("[x: string]", "[x?: string]", true),
        (
            "[first: string, ...rest: number[]]",
            "[first: string, rest: number[]]",
            false,
        ),
        ("[string, ...number[]]", "[string, number[]]", false),
        ("[string]", "[string, number?]", true),
        ("[string, number, number]", "[string, ...number[]]", true),
        ("[string, ...number[]]", "[string, ...string[]]", false),
        ("[x: number, y: number]", "[y: number, x: number]", true),
        ("[x: number]", "[number]", true),
    ] {
        assert_eq!(
            is_subtype_of(&parse_type_string(old), &parse_type_string(new), 0),
            compatible,
            "{old} -> {new}"
        );
    }
}

#[test]
fn tuple_marker_changes_are_reported_and_labels_are_cosmetic() {
    for (old, new, breaking) in [
        ("[x?: string]", "[x: string]", true),
        ("[string?]", "[string]", true),
        (
            "[first: string, ...rest: number[]]",
            "[first: string, rest: number[]]",
            true,
        ),
        ("[x: number, y: number]", "[y: number, x: number]", false),
        ("[x: number]", "[number]", false),
    ] {
        let findings = diff_types(
            &parse_type_string(old),
            &parse_type_string(new),
            "Tuple",
            Path::new("index.d.ts"),
            1,
            1,
        );
        assert_eq!(
            has_breaking_changes(&findings),
            breaking,
            "{old} -> {new}: {findings:?}"
        );
    }
}

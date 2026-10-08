use interlint::discover::discover_exports;

#[test]
fn skipped_public_sources_report_incomplete_analysis() {
    for source in [
        "/*".to_owned() + &"x".repeat(11000) + "*/ export const api = 1;",
        "// padding\n".repeat(55000) + "export declare const api: number;",
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"types":"index.d.ts"}"#).unwrap();
        std::fs::write(dir.path().join("index.d.ts"), source).unwrap();
        let (exports, warnings) = discover_exports(dir.path()).unwrap();
        assert!(exports.is_empty());
        assert!(
            warnings
                .iter()
                .any(|w| w.incomplete && w.message.contains("processing limits")),
            "{warnings:?}"
        );
    }
}

// CSS diff: detect removed custom properties and class names in shipped stylesheets.
//
// AST-03: css-custom-property-removed (--var removed — Major)
// AST-04: css-class-removed (.classname removed — Major)
//
// Uses tree-sitter-css grammar for structural parsing.
// CSS file scope respects package.json style/css field,
// falls back to all.css files in packed output.
// Separate rules for custom properties vs class names.

use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::compare::files::scan_package_files;
use crate::ecosystems::node::compare::rules::{
    RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Parse a CSS source buffer using tree-sitter-css.
///
/// Returns `None` if the parser fails to initialize or parse.
fn parse_css(source: &[u8]) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let css_lang: tree_sitter::Language = tree_sitter_css::LANGUAGE.into();
    parser.set_language(&css_lang).ok()?;
    parser.parse(source, None)
}

/// Extract custom properties and class names from a CSS source buffer.
///
/// Returns `(custom_properties, class_names)` as `BTreeSet<String>`.
/// On parse failure: logs a warning and returns empty sets (warn-and-continue).
fn extract_css_apis(source: &[u8]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut custom_props = BTreeSet::new();
    let mut class_names = BTreeSet::new();

    match parse_css(source) {
        None => {
            tracing::warn!("css_diff: failed to parse CSS source, skipping");
        }
        Some(tree) => {
            walk_css_node(
                tree.root_node(),
                source,
                &mut custom_props,
                &mut class_names,
            );
        }
    }

    (custom_props, class_names)
}

/// Walk with a tree cursor so deeply nested rules do not consume the Rust stack.
fn walk_css_node(
    node: tree_sitter::Node,
    source: &[u8],
    custom_props: &mut BTreeSet<String>,
    class_names: &mut BTreeSet<String>,
) {
    let mut cursor = node.walk();
    loop {
        let current = cursor.node();
        match current.kind() {
            "declaration" => {
                if let Some(prop_node) = current.named_child(0) {
                    if prop_node.kind() == "property_name" {
                        if let Ok(text) = prop_node.utf8_text(source) {
                            if text.starts_with("--") {
                                custom_props.insert(text.to_string());
                            }
                        }
                    }
                }
            }
            "class_selector" => {
                let mut inner_cursor = current.walk();
                for inner_child in current.children(&mut inner_cursor) {
                    if inner_child.kind() == "class_name" {
                        if let Ok(text) = inner_child.utf8_text(source) {
                            class_names.insert(text.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

enum Stylesheets {
    EntryPoints(BTreeSet<String>),
    PackedFiles(BTreeSet<String>),
}

impl Stylesheets {
    fn paths(&self) -> &BTreeSet<String> {
        match self {
            Self::EntryPoints(paths) | Self::PackedFiles(paths) => paths,
        }
    }
}

fn normalize_css_path(path: &str) -> String {
    Path::new(path)
        .components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}

/// Prefer manifest stylesheet entry points; otherwise pair packed files by path.
fn discover_css_files(pkg_root: &Path, findings: &mut Vec<Finding>) -> Stylesheets {
    let pkg_json_path = pkg_root.join("package.json");

    if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            // Check "style" field first, then "css"
            for field in &["style", "css"] {
                match pkg.get(*field) {
                    Some(serde_json::Value::String(s)) => {
                        return Stylesheets::EntryPoints(BTreeSet::from([normalize_css_path(s)]));
                    }
                    Some(serde_json::Value::Array(arr)) => {
                        let paths: BTreeSet<String> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(normalize_css_path))
                            .collect();
                        if !paths.is_empty() {
                            return Stylesheets::EntryPoints(paths);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let scan = scan_package_files(pkg_root, |path| {
        path.extension().and_then(|extension| extension.to_str()) == Some("css")
    });
    findings.extend(scan.findings);
    Stylesheets::PackedFiles(scan.paths)
}

/// Compare CSS files between old and new package roots (AST-03, AST-04).
///
/// Discovers CSS files from both roots, compares custom properties and class
/// names for files present in both, and emits findings for removed names.
///
/// - Removed CSS custom property → `css-custom-property-removed` (Major)
/// - Removed CSS class name → `css-class-removed` (Major)
pub fn compare_css_files(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let old_css = discover_css_files(old_root, &mut findings);
    let new_css = discover_css_files(new_root, &mut findings);

    // Retained entry paths keep independent contracts even when another file
    // happens to expose the same class or custom property.
    for css_file in old_css.paths().intersection(new_css.paths()) {
        let files = BTreeSet::from([css_file.clone()]);
        findings.extend(compare_stylesheets(
            old_root, new_root, &files, &files, css_file,
        ));
    }
    if let (Stylesheets::EntryPoints(old), Stylesheets::EntryPoints(new)) = (&old_css, &new_css) {
        let redirected = old.difference(new).cloned().collect::<BTreeSet<_>>();
        if let Some(report_file) = new.first().filter(|_| !redirected.is_empty()) {
            findings.extend(compare_stylesheets(
                old_root,
                new_root,
                &redirected,
                new,
                report_file,
            ));
        }
    }
    findings
}

fn stylesheet_apis(
    root: &Path,
    files: &BTreeSet<String>,
    side: &str,
    findings: &mut Vec<Finding>,
) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    let mut properties = BTreeSet::new();
    let mut classes = BTreeSet::new();
    let mut complete = true;
    for file in files {
        let source = match read_package_stylesheet(root, file) {
            Ok(source) => source,
            Err(error) => {
                findings.push(Finding {
                    severity: Severity::Warning,
                    rule: "analysis-incomplete",
                    file: PathBuf::from(file),
                    line: 1,
                    col: 1,
                    message: format!("{side}: cannot read stylesheet `{file}`: {error}"),
                });
                complete = false;
                continue;
            }
        };
        let (file_properties, file_classes) = extract_css_apis(&source);
        properties.extend(file_properties);
        classes.extend(file_classes);
    }
    complete.then_some((properties, classes))
}

fn read_package_stylesheet(root: &Path, relative: &str) -> std::io::Result<Vec<u8>> {
    if Path::new(relative).is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "stylesheet path must be relative to the package",
        ));
    }
    let root = root.canonicalize()?;
    let stylesheet = root.join(relative).canonicalize()?;
    if !stylesheet.starts_with(&root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "stylesheet resolves outside the package",
        ));
    }
    std::fs::read(stylesheet)
}

fn compare_stylesheets(
    old_root: &Path,
    new_root: &Path,
    old_files: &BTreeSet<String>,
    new_files: &BTreeSet<String>,
    css_file: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let (Some((old_props, old_classes)), Some((new_props, new_classes))) = (
        stylesheet_apis(old_root, old_files, "baseline", &mut findings),
        stylesheet_apis(new_root, new_files, "current", &mut findings),
    ) else {
        return findings;
    };

    // Removed custom properties → AST-03
    for prop in old_props.difference(&new_props) {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_CSS_CUSTOM_PROPERTY_REMOVED,
            file: PathBuf::from(css_file),
            line: 1,
            col: 1,
            message: format!("CSS custom property `{}` removed from `{}`", prop, css_file),
        });
    }

    // Removed class names → AST-04
    for class in old_classes.difference(&new_classes) {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_CSS_CLASS_REMOVED,
            file: PathBuf::from(css_file),
            line: 1,
            col: 1,
            message: format!("CSS class `.{}` removed from `{}`", class, css_file),
        });
    }
    findings
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::ecosystems::node::compare::assets::css::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_custom_property_removed() {
        let old_css = b":root { --color-primary: blue; --spacing: 8px; }";
        let new_css = b":root { --spacing: 8px; }";

        let (old_props, _) = extract_css_apis(old_css);
        let (new_props, _) = extract_css_apis(new_css);

        // --color-primary removed
        let removed: Vec<_> = old_props.difference(&new_props).collect();
        assert_eq!(removed.len(), 1, "Expected 1 removed custom property");
        assert_eq!(removed[0], "--color-primary");
    }

    #[test]
    fn test_class_name_removed() {
        let old_css = b".btn { color: red; } .card { padding: 8px; }";
        let new_css = b".btn { color: red; }";

        let (_, old_classes) = extract_css_apis(old_css);
        let (_, new_classes) = extract_css_apis(new_css);

        let removed: Vec<_> = old_classes.difference(&new_classes).collect();
        assert_eq!(removed.len(), 1, "Expected 1 removed class");
        assert_eq!(removed[0], "card");
    }

    #[test]
    fn test_custom_property_in_media_query() {
        let old_css = b"@media (prefers-color-scheme: dark) { :root { --bg: black; } }";
        let new_css = b"@media (prefers-color-scheme: dark) { }";

        let (old_props, _) = extract_css_apis(old_css);
        let (new_props, _) = extract_css_apis(new_css);

        let removed: Vec<_> = old_props.difference(&new_props).collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected --bg to be detected inside @media"
        );
        assert_eq!(removed[0], "--bg");
    }

    #[test]
    fn test_nested_class_detected() {
        let old_css = b".parent .child { color: red; } .parent { display: flex; }";
        let new_css = b".parent { display: flex; }";

        let (_, old_classes) = extract_css_apis(old_css);
        let (_, new_classes) = extract_css_apis(new_css);

        let removed: Vec<_> = old_classes.difference(&new_classes).collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected .child to be detected in nested selector"
        );
        assert_eq!(removed[0], "child");
    }

    #[test]
    fn test_minified_css() {
        let old_css = b".a{color:red}.b{color:blue}";
        let new_css = b".a{color:red}";

        let (_, old_classes) = extract_css_apis(old_css);
        let (_, new_classes) = extract_css_apis(new_css);

        let removed: Vec<_> = old_classes.difference(&new_classes).collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected .b to be detected in minified CSS"
        );
        assert_eq!(removed[0], "b");
    }

    #[test]
    fn test_invalid_css_no_panic() {
        // Garbage bytes should not panic — return empty sets
        let garbage = b"\x00\xFF\xFE garbage not css at all !!@@##";
        let (props, classes) = extract_css_apis(garbage);
        // No panic, empty or non-panicking result
        // tree-sitter is error-tolerant so it may extract something or nothing
        // The key requirement is: no panic
        let _ = props;
        let _ = classes;
    }

    #[test]
    fn test_style_field_respected() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();

        // package.json with "style" field pointing to main.css
        let pkg_json = r#"{"name":"x","version":"1.0.0","style":"main.css"}"#;

        // Old: main.css has --primary, other.css has --other
        fs::write(old_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            old_dir.path().join("main.css"),
            ":root { --primary: blue; }",
        )
        .unwrap();
        fs::write(old_dir.path().join("other.css"), ":root { --other: red; }").unwrap();

        // New: main.css keeps --primary, other.css has --other removed
        fs::write(new_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            new_dir.path().join("main.css"),
            ":root { --primary: blue; }",
        )
        .unwrap();
        fs::write(new_dir.path().join("other.css"), ":root { }").unwrap();

        let findings = compare_css_files(old_dir.path(), new_dir.path());

        // --other removed from other.css, but style field points to main.css only
        // So there should be no finding
        assert!(
            findings.is_empty(),
            "style field should restrict CSS scope to main.css only, got: {:?}",
            findings.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_fallback_all_css() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();

        // package.json WITHOUT style/css field
        let pkg_json = r#"{"name":"x","version":"1.0.0"}"#;

        // Old: two css files with classes
        fs::write(old_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(old_dir.path().join("a.css"), ".foo { }").unwrap();
        fs::write(old_dir.path().join("b.css"), ".bar { }").unwrap();

        // New: b.css has.bar removed
        fs::write(new_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(new_dir.path().join("a.css"), ".foo { }").unwrap();
        fs::write(new_dir.path().join("b.css"), "/* empty */").unwrap();

        let findings = compare_css_files(old_dir.path(), new_dir.path());

        // b.css should be included in fallback scan
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_CSS_CLASS_REMOVED && f.message.contains("bar")),
            "Fallback scan should detect .bar removal in b.css, got: {:?}",
            findings.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }
}

// CSS diff: detect removed custom properties and class names in shipped stylesheets.
//
// AST-03: css-custom-property-removed (--var removed — Major)
// AST-04: css-class-removed (.classname removed — Major)
//
// Per D-04: Uses tree-sitter-css grammar for structural parsing.
// Per D-05: CSS file scope respects package.json style/css field,
//           falls back to all .css files in packed output.
// Per D-06: Separate rules for custom properties vs class names.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::types::{Finding, Severity};
use super::subtype::{RULE_CSS_CUSTOM_PROPERTY_REMOVED, RULE_CSS_CLASS_REMOVED};

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
            walk_css_node(tree.root_node(), source, &mut custom_props, &mut class_names);
        }
    }

    (custom_props, class_names)
}

/// Recursively walk a CSS parse tree, collecting custom properties and class names.
///
/// - `"declaration"` nodes whose first named child is `"property_name"` starting
///   with `"--"` → custom property
/// - `"class_selector"` nodes containing `"class_name"` children → class name
/// - All other nodes → recurse (handles @media, @layer, nested rule_sets)
fn walk_css_node(
    node: tree_sitter::Node,
    source: &[u8],
    custom_props: &mut BTreeSet<String>,
    class_names: &mut BTreeSet<String>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "declaration" => {
                // Check if the first named child is a property_name starting with "--"
                if let Some(prop_node) = child.named_child(0) {
                    if prop_node.kind() == "property_name" {
                        if let Ok(text) = prop_node.utf8_text(source) {
                            if text.starts_with("--") {
                                custom_props.insert(text.to_string());
                            }
                        }
                    }
                }
                // Recurse into declaration children (handles nested declarations)
                walk_css_node(child, source, custom_props, class_names);
            }
            "class_selector" => {
                // Collect all class_name children
                let mut inner_cursor = child.walk();
                for inner_child in child.children(&mut inner_cursor) {
                    if inner_child.kind() == "class_name" {
                        if let Ok(text) = inner_child.utf8_text(source) {
                            class_names.insert(text.to_string());
                        }
                    }
                }
                // Recurse into class_selector (handles compound selectors)
                walk_css_node(child, source, custom_props, class_names);
            }
            _ => {
                // Recurse into all other node kinds to handle @media, @layer,
                // nested rule_sets, selectors, etc.
                walk_css_node(child, source, custom_props, class_names);
            }
        }
    }
}

/// Discover CSS entry points for a package root.
///
/// Per D-05: Checks `package.json` `"style"` field first, then `"css"` field.
/// Falls back to scanning all `.css` files recursively if neither field exists.
///
/// Returns relative paths (relative to `pkg_root`).
fn discover_css_files(pkg_root: &Path) -> Vec<String> {
    let pkg_json_path = pkg_root.join("package.json");

    if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            // Check "style" field first, then "css"
            for field in &["style", "css"] {
                match pkg.get(*field) {
                    Some(serde_json::Value::String(s)) => {
                        return vec![s.clone()];
                    }
                    Some(serde_json::Value::Array(arr)) => {
                        let paths: Vec<String> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        if !paths.is_empty() {
                            return paths;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Fallback: scan all .css files recursively
    let mut files = BTreeSet::new();
    collect_css_files(pkg_root, pkg_root, &mut files);
    files.into_iter().collect()
}

/// Recursively collect CSS files from a package directory.
///
/// Returns relative paths (relative to `base`). Skips dotfiles and `node_modules/`.
fn collect_css_files(base: &Path, current: &Path, files: &mut BTreeSet<String>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip dotfiles
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Skip node_modules
            if file_name == "node_modules" {
                continue;
            }
            collect_css_files(base, &path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("css") {
            if let Ok(rel) = path.strip_prefix(base) {
                if let Some(rel_str) = rel.to_str() {
                    files.insert(rel_str.replace('\\', "/"));
                }
            }
        }
    }
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

    // Discover CSS files from both roots and union them for comparison
    let old_css: BTreeSet<String> = discover_css_files(old_root).into_iter().collect();
    let new_css: BTreeSet<String> = discover_css_files(new_root).into_iter().collect();

    // Compare files present in both old and new
    for css_file in old_css.intersection(&new_css) {
        let old_path = old_root.join(css_file);
        let new_path = new_root.join(css_file);

        let old_source = match std::fs::read(&old_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let new_source = match std::fs::read(&new_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let (old_props, old_classes) = extract_css_apis(&old_source);
        let (new_props, new_classes) = extract_css_apis(&new_source);

        // Removed custom properties → AST-03
        for prop in old_props.difference(&new_props) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CSS_CUSTOM_PROPERTY_REMOVED,
                file: PathBuf::from(css_file),
                line: 1,
                col: 1,
                message: format!(
                    "CSS custom property `{}` removed from `{}`",
                    prop, css_file
                ),
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
    }

    findings
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

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
        assert_eq!(removed.len(), 1, "Expected --bg to be detected inside @media");
        assert_eq!(removed[0], "--bg");
    }

    #[test]
    fn test_nested_class_detected() {
        let old_css = b".parent .child { color: red; } .parent { display: flex; }";
        let new_css = b".parent { display: flex; }";

        let (_, old_classes) = extract_css_apis(old_css);
        let (_, new_classes) = extract_css_apis(new_css);

        let removed: Vec<_> = old_classes.difference(&new_classes).collect();
        assert_eq!(removed.len(), 1, "Expected .child to be detected in nested selector");
        assert_eq!(removed[0], "child");
    }

    #[test]
    fn test_minified_css() {
        let old_css = b".a{color:red}.b{color:blue}";
        let new_css = b".a{color:red}";

        let (_, old_classes) = extract_css_apis(old_css);
        let (_, new_classes) = extract_css_apis(new_css);

        let removed: Vec<_> = old_classes.difference(&new_classes).collect();
        assert_eq!(removed.len(), 1, "Expected .b to be detected in minified CSS");
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
        fs::write(old_dir.path().join("main.css"), ":root { --primary: blue; }").unwrap();
        fs::write(old_dir.path().join("other.css"), ":root { --other: red; }").unwrap();

        // New: main.css keeps --primary, other.css has --other removed
        fs::write(new_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(new_dir.path().join("main.css"), ":root { --primary: blue; }").unwrap();
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

        // New: b.css has .bar removed
        fs::write(new_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(new_dir.path().join("a.css"), ".foo { }").unwrap();
        fs::write(new_dir.path().join("b.css"), "/* empty */").unwrap();

        let findings = compare_css_files(old_dir.path(), new_dir.path());

        // b.css should be included in fallback scan
        assert!(
            findings.iter().any(|f| f.rule == RULE_CSS_CLASS_REMOVED && f.message.contains("bar")),
            "Fallback scan should detect .bar removal in b.css, got: {:?}",
            findings.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }
}

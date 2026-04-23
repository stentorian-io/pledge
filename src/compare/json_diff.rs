// JSON diff: detect field changes in shipped JSON files.
//
// AST-01: json-field-removed (removed key or changed value type — Major)
// AST-02: json-required-field-added (added key in JSON Schema `required` array — Major)
// AST-06: json-field-added (added key, not in `required` — Minor)
//
// Per D-01: All shipped .json files are diffed except package.json.
// Per D-02: Recursive key-by-key diff using serde_json::Value.
// Per D-03: JSON Schema `required` array detection for AST-02.

use super::subtype::{
    RULE_JSON_COMPARISON_TRUNCATED, RULE_JSON_FIELD_ADDED, RULE_JSON_FIELD_REMOVED,
    RULE_JSON_REQUIRED_FIELD_ADDED,
};
use crate::types::{Finding, Severity};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 20;

/// Compare JSON files between old and new package roots (AST-01, AST-02, AST-06).
///
/// Scans both roots for .json files, excludes package.json, and diffs files
/// present in both versions key-by-key.
pub fn compare_json_files(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let mut old_files = BTreeSet::new();
    let mut new_files = BTreeSet::new();
    collect_json_files(old_root, old_root, &mut old_files);
    collect_json_files(new_root, new_root, &mut new_files);

    let mut findings = Vec::new();

    // Only diff files present in both versions.
    // Files in old but not new are handled by asset_diff.rs (no duplicate findings).
    for rel_path in old_files.intersection(&new_files) {
        // Skip package.json — already handled by metadata.rs (D-01)
        if rel_path == "package.json" {
            continue;
        }

        let old_path = old_root.join(rel_path);
        let new_path = new_root.join(rel_path);

        let old_str = match std::fs::read_to_string(&old_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let new_str = match std::fs::read_to_string(&new_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let old_val: Value = match serde_json::from_str(&old_str) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let new_val: Value = match serde_json::from_str(&new_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        diff_json_values(&old_val, &new_val, "", rel_path, &mut findings, 0);
    }

    findings
}

/// Recursively scan a directory for .json files, collecting relative paths.
///
/// Skips dotfiles, node_modules/, and non-.json files.
fn collect_json_files(base: &Path, current: &Path, files: &mut BTreeSet<String>) {
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
            collect_json_files(base, &path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(rel) = path.strip_prefix(base) {
                if let Some(rel_str) = rel.to_str() {
                    files.insert(rel_str.replace('\\', "/"));
                }
            }
        }
    }
}

/// Recursively diff two JSON values, emitting findings for structural differences.
///
/// `path` is the dotted key path to the current node (empty string at root).
/// `file_path` is the relative file path for finding messages.
/// `depth` tracks recursion depth; capped at MAX_DEPTH.
fn diff_json_values(
    old: &Value,
    new: &Value,
    path: &str,
    file_path: &str,
    findings: &mut Vec<Finding>,
    depth: usize,
) {
    if depth >= MAX_DEPTH {
        // WR-02 (Phase 17 review): mirror type_diff.rs and emit a Patch-severity
        // finding so consumers see a clear signal that analysis was incomplete at
        // MAX_DEPTH rather than a silent return.
        findings.push(json_finding(
            Severity::Patch,
            RULE_JSON_COMPARISON_TRUNCATED,
            file_path,
            format!("JSON comparison truncated at depth {MAX_DEPTH} for `{file_path}`"),
        ));
        return;
    }

    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            // Collect required keys from the new schema's `required` array (D-03)
            let required = extract_required_array(new_map);

            // Keys removed: old has, new doesn't
            for key in old_map.keys() {
                if !new_map.contains_key(key) {
                    let field_path = make_path(path, key);
                    findings.push(json_finding(
                        Severity::Major,
                        RULE_JSON_FIELD_REMOVED,
                        file_path,
                        format!("field `{}` removed from `{}`", field_path, file_path),
                    ));
                }
            }

            // Keys added: new has, old doesn't
            for key in new_map.keys() {
                if !old_map.contains_key(key) {
                    let field_path = make_path(path, key);
                    if required.contains(key) {
                        findings.push(json_finding(
                            Severity::Major,
                            RULE_JSON_REQUIRED_FIELD_ADDED,
                            file_path,
                            format!("required field `{}` added to `{}`", field_path, file_path),
                        ));
                    } else {
                        findings.push(json_finding(
                            Severity::Minor,
                            RULE_JSON_FIELD_ADDED,
                            file_path,
                            format!("field `{}` added to `{}`", field_path, file_path),
                        ));
                    }
                }
            }

            // Recurse into keys present in both
            for key in old_map.keys() {
                if let Some(new_val) = new_map.get(key) {
                    let child_path = make_path(path, key);
                    diff_json_values(
                        &old_map[key],
                        new_val,
                        &child_path,
                        file_path,
                        findings,
                        depth + 1,
                    );
                }
            }
        }

        // Type change: discriminant differs — treat as removal (D-02)
        (old_val, new_val)
            if std::mem::discriminant(old_val) != std::mem::discriminant(new_val) =>
        {
            findings.push(json_finding(
                Severity::Major,
                RULE_JSON_FIELD_REMOVED,
                file_path,
                format!("field `{}` type changed in `{}`", path, file_path),
            ));
        }

        // Same type, non-object: no finding
        // Arrays compared by discriminant only — content changes are not breaking
        // per JSON import semantics.
        _ => {}
    }
}

/// Extract the `required` array from a JSON Schema object map.
///
/// Returns the set of required field names, or empty BTreeSet if not present.
fn extract_required_array(obj: &serde_json::Map<String, Value>) -> BTreeSet<String> {
    match obj.get("required") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// Build a dotted path from parent path and key.
fn make_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", parent, key)
    }
}

/// Create a Finding anchored to a JSON file (line 1, col 1).
fn json_finding(
    severity: Severity,
    rule: &'static str,
    file_path: &str,
    message: String,
) -> Finding {
    Finding {
        severity,
        rule,
        file: PathBuf::from(file_path),
        line: 1,
        col: 1,
        message,
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: write a JSON string to a temp dir file and return the dir.
    fn write_json(dir: &TempDir, name: &str, content: &str) {
        fs::write(dir.path().join(name), content).unwrap();
    }

    #[test]
    fn test_json_field_removed() {
        let old = serde_json::json!({"name": "alice", "age": 30});
        let new = serde_json::json!({"name": "alice"});
        let mut findings = Vec::new();
        diff_json_values(&old, &new, "", "schema.json", &mut findings, 0);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_JSON_FIELD_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(findings[0].message.contains("age"));
    }

    #[test]
    fn test_json_type_changed() {
        let old = serde_json::json!({"count": "five"});
        let new = serde_json::json!({"count": 5});
        let mut findings = Vec::new();
        diff_json_values(&old, &new, "", "schema.json", &mut findings, 0);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_JSON_FIELD_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(findings[0].message.contains("type changed"));
    }

    #[test]
    fn test_json_field_added_optional() {
        let old = serde_json::json!({"name": "alice"});
        let new = serde_json::json!({"name": "alice", "email": "a@b.com"});
        let mut findings = Vec::new();
        diff_json_values(&old, &new, "", "schema.json", &mut findings, 0);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_JSON_FIELD_ADDED);
        assert_eq!(findings[0].severity, Severity::Minor);
        assert!(findings[0].message.contains("email"));
    }

    #[test]
    fn test_json_required_field_added() {
        // The `required` array at the SAME level as the added key triggers AST-02.
        // This matches JSON Schema semantics where `required` and added fields
        // are at the same object level.
        let old = serde_json::json!({"name": "alice"});
        let new = serde_json::json!({
            "name": "alice",
            "email": "a@b.com",
            "required": ["email"]
        });
        let mut findings = Vec::new();
        diff_json_values(&old, &new, "", "schema.json", &mut findings, 0);
        // Should find: email added as required field
        let required_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_JSON_REQUIRED_FIELD_ADDED)
            .collect();
        assert_eq!(required_findings.len(), 1);
        assert_eq!(required_findings[0].severity, Severity::Major);
        assert!(required_findings[0].message.contains("email"));
    }

    #[test]
    fn test_json_nested_path_reporting() {
        let old = serde_json::json!({
            "properties": {
                "name": {"type": "string", "minLength": 1}
            }
        });
        let new = serde_json::json!({
            "properties": {
                "name": {"type": "string"}
            }
        });
        let mut findings = Vec::new();
        diff_json_values(&old, &new, "", "schema.json", &mut findings, 0);
        assert!(!findings.is_empty());
        // Path should use dotted notation
        assert!(
            findings[0].message.contains("properties.name.minLength"),
            "Expected dotted path in message, got: {}",
            findings[0].message
        );
    }

    #[test]
    fn test_json_identical_no_findings() {
        let val = serde_json::json!({"name": "alice", "age": 30});
        let mut findings = Vec::new();
        diff_json_values(&val, &val, "", "schema.json", &mut findings, 0);
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn test_json_package_json_excluded() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        write_json(
            &old_dir,
            "package.json",
            r#"{"name":"pkg","version":"1.0.0"}"#,
        );
        write_json(
            &new_dir,
            "package.json",
            r#"{"name":"pkg","version":"2.0.0","extra":"field"}"#,
        );
        let findings = compare_json_files(old_dir.path(), new_dir.path());
        assert_eq!(
            findings.len(),
            0,
            "package.json should be excluded from JSON diff"
        );
    }

    #[test]
    fn test_json_depth_cap() {
        // Build a 25-level deep nested JSON value
        let mut val = serde_json::json!({"leaf": "value"});
        for _ in 0..25 {
            val = serde_json::json!({"nested": val});
        }
        let old = val.clone();
        // Change the leaf at depth 25 — should not panic; findings may be truncated
        let mut findings = Vec::new();
        // This should complete without stack overflow
        diff_json_values(&old, &old, "", "deep.json", &mut findings, 0);
        // Per WR-02, a truncation finding is expected when recursion crosses MAX_DEPTH.
        // Identical subtrees still trigger it because the cap fires before per-key
        // comparison decides whether there are real differences.
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_JSON_COMPARISON_TRUNCATED),
            "WR-02: depth cap should produce a truncation finding"
        );
    }

    #[test]
    fn wr_02_truncation_finding_emitted_for_deep_tree() {
        // WR-02 (Phase 17 review): pathologically deep JSON schemas must produce
        // a Patch-severity truncation finding, matching type_diff.rs. Silent depth
        // capping meant users never saw that analysis was incomplete.
        let mut old = serde_json::json!({"leaf": "old"});
        for _ in 0..25 {
            old = serde_json::json!({"nested": old});
        }
        let mut new = serde_json::json!({"leaf": "new"});
        for _ in 0..25 {
            new = serde_json::json!({"nested": new});
        }
        let mut findings = Vec::new();
        diff_json_values(&old, &new, "", "deep.json", &mut findings, 0);
        let truncation: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_JSON_COMPARISON_TRUNCATED)
            .collect();
        assert!(
            !truncation.is_empty(),
            "WR-02: expected at least one truncation finding; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
        assert_eq!(
            truncation[0].severity,
            Severity::Patch,
            "WR-02: truncation should be Patch severity"
        );
        assert!(truncation[0].message.contains("truncated at depth"));
    }

    #[test]
    fn test_json_files_compared() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        write_json(
            &old_dir,
            "schema.json",
            r#"{"type":"object","properties":{"id":{"type":"number"},"name":{"type":"string"}}}"#,
        );
        write_json(
            &new_dir,
            "schema.json",
            r#"{"type":"object","properties":{"id":{"type":"number"}}}"#,
        );
        let findings = compare_json_files(old_dir.path(), new_dir.path());
        assert!(!findings.is_empty());
        let removed = findings.iter().any(|f| f.rule == RULE_JSON_FIELD_REMOVED);
        assert!(removed, "Should detect removed field");
    }
}

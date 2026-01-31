//! Extract PackageConfig from raw package.json data.
//!
//! This module provides functions to parse a raw JSON package.json file
//! and extract the structured [`PackageConfig`] type for API submission.

use interlint_format::{ExportsField, ExportsValue, PackageConfig, PeerDepMeta};
use json::JsonValue;
use std::collections::HashMap;

/// Extract a structured [`PackageConfig`] from raw package.json data.
///
/// This function parses the relevant fields from a package.json file
/// and returns a typed structure suitable for API submission or local
/// comparison.
///
/// # Example
///
/// ```rust
/// use breakpoint::pkg::package_config_extractor::extract_package_config;
///
/// let pkg_json = json::parse(r#"{
///     "name": "my-package",
///     "version": "1.0.0",
///     "engines": { "node": ">=18.0.0" },
///     "main": "./dist/index.js"
/// }"#).unwrap();
///
/// let config = extract_package_config(&pkg_json);
/// assert_eq!(config.name, "my-package");
/// assert_eq!(config.version, "1.0.0");
/// assert!(config.has_engine_requirements());
/// ```
pub fn extract_package_config(pkg_json: &JsonValue) -> PackageConfig {
    PackageConfig {
        name: pkg_json["name"].as_str().unwrap_or("").to_string(),
        version: pkg_json["version"].as_str().unwrap_or("").to_string(),
        engines: extract_string_map(&pkg_json["engines"]),
        peer_dependencies: extract_string_map(&pkg_json["peerDependencies"]),
        peer_dependencies_meta: extract_peer_meta(&pkg_json["peerDependenciesMeta"]),
        exports: extract_exports(&pkg_json["exports"]),
        main: pkg_json["main"].as_str().map(|s| s.to_string()),
        module: pkg_json["module"].as_str().map(|s| s.to_string()),
        module_type: pkg_json["type"].as_str().map(|s| s.to_string()),
        bin: extract_string_map(&pkg_json["bin"]),
        types: extract_types(pkg_json),
        types_versions: extract_types_versions(&pkg_json["typesVersions"]),
        dependencies: extract_string_map(&pkg_json["dependencies"]),
    }
}

/// Extract a string map from an object field (e.g., dependencies, peerDependencies, engines, bin).
fn extract_string_map(obj: &JsonValue) -> HashMap<String, String> {
    let mut map = HashMap::new();

    if obj.is_object() {
        for (key, value) in obj.entries() {
            if let Some(v) = value.as_str() {
                map.insert(key.to_string(), v.to_string());
            }
        }
    }

    map
}

/// Extract peer dependency metadata from `peerDependenciesMeta`.
fn extract_peer_meta(meta: &JsonValue) -> HashMap<String, PeerDepMeta> {
    let mut map = HashMap::new();

    if meta.is_object() {
        for (name, value) in meta.entries() {
            if value.is_object() {
                let optional = value["optional"].as_bool().unwrap_or(false);
                map.insert(name.to_string(), PeerDepMeta { optional });
            }
        }
    }

    map
}

/// Extract exports field (can be string, object, or nested conditional exports).
fn extract_exports(exports: &JsonValue) -> Option<ExportsField> {
    if exports.is_null() {
        return None;
    }

    Some(parse_exports_field(exports))
}

/// Parse exports field from JSON.
fn parse_exports_field(value: &JsonValue) -> ExportsField {
    if let Some(s) = value.as_str() {
        return ExportsField::Simple(s.to_string());
    }

    if value.is_object() {
        let mut map = HashMap::new();
        for (key, val) in value.entries() {
            map.insert(key.to_string(), parse_exports_value(val));
        }
        return ExportsField::Conditional(map);
    }

    // Fallback for arrays or other types - convert to string
    ExportsField::Simple(value.dump())
}

/// Recursively parse an export value.
fn parse_exports_value(value: &JsonValue) -> ExportsValue {
    if value.is_null() {
        return ExportsValue::Null;
    }

    if let Some(s) = value.as_str() {
        return ExportsValue::Path(s.to_string());
    }

    if value.is_object() {
        let mut conditions = HashMap::new();
        for (key, val) in value.entries() {
            conditions.insert(key.to_string(), parse_exports_value(val));
        }
        return ExportsValue::Conditional(conditions);
    }

    // Fallback for arrays or other types - convert to string
    ExportsValue::Path(value.dump())
}

/// Extract types/typings field.
fn extract_types(pkg_json: &JsonValue) -> Option<String> {
    // Check both "types" and "typings" (legacy)
    if let Some(s) = pkg_json["types"].as_str() {
        return Some(s.to_string());
    }
    if let Some(s) = pkg_json["typings"].as_str() {
        return Some(s.to_string());
    }
    None
}

/// Extract typesVersions field.
fn extract_types_versions(
    types_versions: &JsonValue,
) -> HashMap<String, HashMap<String, Vec<String>>> {
    let mut outer_map: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

    if !types_versions.is_object() {
        return outer_map;
    }

    for (version_range, mappings) in types_versions.entries() {
        if mappings.is_object() {
            let mut inner_map: HashMap<String, Vec<String>> = HashMap::new();

            for (pattern, paths) in mappings.entries() {
                let mut path_list = Vec::new();

                if paths.is_array() {
                    for path in paths.members() {
                        if let Some(s) = path.as_str() {
                            path_list.push(s.to_string());
                        }
                    }
                } else if let Some(s) = paths.as_str() {
                    path_list.push(s.to_string());
                }

                if !path_list.is_empty() {
                    inner_map.insert(pattern.to_string(), path_list);
                }
            }

            if !inner_map.is_empty() {
                outer_map.insert(version_range.to_string(), inner_map);
            }
        }
    }

    outer_map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_basic_config() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0"
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert_eq!(config.name, "my-pkg");
        assert_eq!(config.version, "1.0.0");
        assert!(!config.has_engine_requirements());
    }

    #[test]
    fn test_extract_engines() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "engines": {
                "node": ">=18.0.0",
                "npm": ">=9.0.0"
            }
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert!(config.has_engine_requirements());
        assert_eq!(config.engines.get("node"), Some(&">=18.0.0".to_string()));
        assert_eq!(config.engines.get("npm"), Some(&">=9.0.0".to_string()));
    }

    #[test]
    fn test_extract_peer_dependencies() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "peerDependencies": {
                "react": "^18.0.0",
                "react-dom": "^18.0.0"
            },
            "peerDependenciesMeta": {
                "react-dom": {
                    "optional": true
                }
            }
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert!(config.has_peer_dependencies());
        assert_eq!(
            config.peer_dependencies.get("react"),
            Some(&"^18.0.0".to_string())
        );
        assert!(!config.is_peer_optional("react"));
        assert!(config.is_peer_optional("react-dom"));
    }

    #[test]
    fn test_extract_exports_simple() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "exports": "./dist/index.js"
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert!(config.has_exports());
        match config.exports.unwrap() {
            ExportsField::Simple(p) => assert_eq!(p, "./dist/index.js"),
            _ => panic!("Expected Simple variant"),
        }
    }

    #[test]
    fn test_extract_exports_conditional() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "exports": {
                ".": {
                    "import": "./dist/index.mjs",
                    "require": "./dist/index.cjs"
                }
            }
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert!(config.has_exports());
        match config.exports.unwrap() {
            ExportsField::Conditional(map) => {
                assert!(map.contains_key("."));
            }
            _ => panic!("Expected Conditional variant"),
        }
    }

    #[test]
    fn test_extract_bin() {
        let pkg = json::parse(
            r#"{
            "name": "my-cli",
            "version": "1.0.0",
            "bin": {
                "cli": "./bin/cli.js",
                "helper": "./bin/helper.js"
            }
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert!(config.has_bin());
        assert_eq!(config.bin.get("cli"), Some(&"./bin/cli.js".to_string()));
        assert_eq!(
            config.bin.get("helper"),
            Some(&"./bin/helper.js".to_string())
        );
    }

    #[test]
    fn test_extract_types() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "types": "./dist/index.d.ts"
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert_eq!(config.types, Some("./dist/index.d.ts".to_string()));
    }

    #[test]
    fn test_extract_typings_legacy() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "typings": "./dist/index.d.ts"
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert_eq!(config.types, Some("./dist/index.d.ts".to_string()));
    }

    #[test]
    fn test_extract_module_type() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "type": "module"
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert_eq!(config.module_type, Some("module".to_string()));
    }

    #[test]
    fn test_extract_types_versions() {
        let pkg = json::parse(
            r#"{
            "name": "my-pkg",
            "version": "1.0.0",
            "typesVersions": {
                ">=4.0": {
                    "*": ["./types/ts4/*"]
                },
                "<4.0": {
                    "*": ["./types/ts3/*"]
                }
            }
        }"#,
        )
        .unwrap();

        let config = extract_package_config(&pkg);
        assert!(config.types_versions.contains_key(">=4.0"));
        assert!(config.types_versions.contains_key("<4.0"));
    }
}

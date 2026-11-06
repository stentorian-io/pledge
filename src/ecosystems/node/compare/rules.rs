pub const RULE_PEER_DEP_BUMPED: &str = "peer-dep-version-bumped";

pub const RULE_PEER_DEP_ADDED: &str = "peer-dep-added";

pub const RULE_PEER_DEP_REMOVED: &str = "peer-dep-removed";

pub const RULE_ENGINE_BUMPED: &str = "engine-version-bumped";

pub const RULE_ENGINE_ADDED: &str = "engine-added";

pub const RULE_ENTRY_POINT_CHANGED: &str = "entry-point-changed";

pub const RULE_MODULE_TYPE_CHANGED: &str = "module-type-changed";

pub const RULE_SUBPATH_REMOVED: &str = "subpath-removed";

pub const RULE_EXPORTS_FIELD_ADDED: &str = "exports-field-added";

pub const RULE_CONDITION_REMOVED: &str = "condition-removed";

pub const RULE_CONDITION_TARGET_CHANGED: &str = "condition-target-changed";

pub const RULE_TYPES_CONDITION_REMOVED: &str = "types-condition-removed";

pub const RULE_SUBPATH_PATTERN_CHANGED: &str = "subpath-pattern-changed";

pub const RULE_SUBPATH_ADDED: &str = "subpath-added";

pub const RULE_CJS_TO_ESM: &str = "cjs-to-esm";

pub const RULE_ESM_TO_CJS: &str = "esm-to-cjs";

pub const RULE_TYPES_RUNTIME_MISMATCH: &str = "types-runtime-mismatch";

pub const RULE_BIN_ENTRY_CHANGED: &str = "bin-entry-changed";

pub const RULE_PLATFORM_NARROWED: &str = "platform-narrowed";

pub const RULE_IMPORTABLE_FILE_REMOVED: &str = "importable-file-removed";

pub const RULE_TYPES_VERSIONS_CHANGED: &str = "types-versions-changed";

pub const RULE_IMPORT_ADDED: &str = "import-added";

pub const RULE_IMPORT_REMOVED: &str = "import-removed";

pub const RULE_IMPORT_CONDITION_CHANGED: &str = "import-condition-changed";

pub const RULE_IMPORT_PATTERN_REFINED: &str = "import-pattern-refined";

pub const RULE_IMPORT_REROUTE: &str = "import-reroute";

pub const RULE_JSON_FIELD_REMOVED: &str = "json-field-removed";

pub const RULE_JSON_REQUIRED_FIELD_ADDED: &str = "json-required-field-added";

pub const RULE_CSS_CUSTOM_PROPERTY_REMOVED: &str = "css-custom-property-removed";

pub const RULE_CSS_CLASS_REMOVED: &str = "css-class-removed";

pub const RULE_ASSET_FILE_REMOVED: &str = "asset-file-removed";

pub const RULE_JSON_FIELD_ADDED: &str = "json-field-added";

/// Emitted (Patch severity) when JSON diff recursion hits the depth cap and
/// further nested differences are not reported — mirrors `type-comparison-truncated`.
pub const RULE_JSON_COMPARISON_TRUNCATED: &str = "json-comparison-truncated";

pub const RULE_CJS_BUNDLER_EXPORT_DETECTED: &str = "cjs-bundler-export-detected";

pub const RULE_CHUNK_PATH_NORMALIZED: &str = "chunk-path-normalized";

pub const RULE_SIDE_EFFECTS_WIDENED: &str = "side-effects-widened";

pub const RULE_PEER_DEP_OPTIONAL_REMOVED: &str = "peer-dep-optional-removed";

pub const RULE_BARREL_UNRESOLVABLE: &str = "barrel-unresolvable";
pub const RULE_NODE_SYNTAX_REQUIREMENT_RAISED: &str = "node-syntax-requirement-raised";

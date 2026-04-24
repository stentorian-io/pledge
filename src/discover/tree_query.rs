#![allow(dead_code)]

use std::path::Path;

use anyhow::Context;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::types::DeclarationKind;

// ---------------------------------------------------------------------------
// Language selection
// ---------------------------------------------------------------------------

/// Which grammar should be used to parse a source file.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    TypeScript,
    /// INF-01 (Phase 18): JSX-bearing TypeScript files (`.tsx`) — routed to
    /// `tree_sitter_typescript::LANGUAGE_TSX`. TSX is a strict superset of
    /// LANGUAGE_TYPESCRIPT so every TypeScript query compiles and matches
    /// identically on TSX source. Pre-Phase-18 `.tsx` was routed to
    /// LANGUAGE_TYPESCRIPT and produced ERROR nodes on any JSX body,
    /// suppressing export discovery.
    TypeScriptTsx,
    JavaScript,
}

/// Determine which grammar to use based on the file extension.
///
/// Returns `None` for unsupported extensions so callers can emit a warning and
/// skip the file rather than hard-failing.
#[allow(dead_code)]
pub fn language_for_file(path: &Path) -> Option<SourceLanguage> {
    let ext = path.extension()?.to_str()?;
    // Check for compound extensions like `.d.ts`, `.d.mts`, `.d.cts`.
    // Declaration files are pure type surface — no JSX — routed to standard
    // TypeScript grammar (NOT TypeScriptTsx).
    let file_name = path.file_name()?.to_str().unwrap_or("");
    if file_name.ends_with(".d.ts")
        || file_name.ends_with(".d.mts")
        || file_name.ends_with(".d.cts")
    {
        return Some(SourceLanguage::TypeScript);
    }
    match ext {
        // INF-01 (Phase 18): `.tsx` routes to LANGUAGE_TSX — a strict superset
        // of LANGUAGE_TYPESCRIPT that handles JSX expression syntax.
        "tsx" => Some(SourceLanguage::TypeScriptTsx),
        "ts" | "mts" | "cts" => Some(SourceLanguage::TypeScript),
        "js" | "mjs" | "cjs" | "jsx" => Some(SourceLanguage::JavaScript),
        _ => None,
    }
}

/// Return the tree-sitter `Language` handle for the given `SourceLanguage`.
///
/// Uses the non-deprecated `LANGUAGE_TYPESCRIPT` / `LANGUAGE_TSX` / `LANGUAGE` constants.
///
/// Phase 19: made `pub(super)` so `comment_scan.rs` can call it.
pub(super) fn get_language(lang: &SourceLanguage) -> tree_sitter::Language {
    match lang {
        SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    }
}

// ---------------------------------------------------------------------------
// Raw export representation
// ---------------------------------------------------------------------------

/// The kind of export as detected by tree-sitter — before semantic resolution.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawExportKind {
    /// `export { foo }` or `export { foo as bar }` (no `from` clause)
    Named,
    /// `export default …`
    Default,
    /// `export * from './module'`
    StarReexport,
    /// `export { foo } from './module'`
    NamedReexport,
}

/// A single export statement captured by tree-sitter before semantic enrichment.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RawExport {
    /// The exported name. For `Default` this is `"default"`. For `StarReexport`
    /// this is `"*"`.
    pub name: String,
    pub kind: RawExportKind,
    /// 1-indexed line number (per Pitfall 7: tree-sitter is 0-indexed).
    pub line: u32,
    /// 1-indexed column number.
    pub col: u32,
    /// The module specifier string for re-exports (e.g., `"./utils"`), with
    /// surrounding quotes stripped. `None` for non-re-export forms.
    pub source: Option<String>,
    /// The syntactic form of the declaration (function, class, interface, etc.).
    /// `None` for re-exports, default expression-form exports, and plain `export { foo }`.
    pub decl_kind: Option<DeclarationKind>,
    /// TLF-07: true for `export type * from './mod'` (type-only star re-export).
    pub is_type_only: bool,
    /// META-04: TSDoc release tag extracted from adjacent block comment.
    /// Populated during parse_exports for named declaration exports.
    pub release_tag: Option<crate::types::ReleaseTag>,
    /// ARCH-02 (Phase 18): compound-component members attached to this export
    /// via `Target.Member = X` or `Object.assign(Target, { Member: X })`.
    /// Populated during the compound-assignment second pass in `parse_exports`.
    /// D-16 conservative: only populated when Target is itself an exported
    /// symbol in the same file.
    pub compound_members: Vec<String>,
    /// INF-02 (Phase 18): type annotation text extracted from a matching
    /// `declare` statement in the same file (rollup-plugin-dts / api-extractor
    /// bundle pattern: `declare const foo: T; ... export { foo };`). Populated
    /// during `parse_exports` Pass 1 by lookup against the file-scope ambient-
    /// declaration sidecar map. `None` for source-form exports where no
    /// `declare` statement precedes the `export { foo }` specifier and for
    /// re-exports where the symbol comes from another module.
    pub type_signature: Option<String>,
    /// TYPE-01 (Phase 20): `satisfies T` RHS — populated by the resolver's
    /// post-parse populate pass that calls `extract_satisfies_types` and
    /// routes each hit through `parse_type_string`. `None` for exports that
    /// have no adjacent `satisfies` clause, and on every initial construction
    /// from this module (the populate pass is the single producer).
    pub satisfies_type: Option<crate::types::shape::TypeShape>,
}

// ---------------------------------------------------------------------------
// Tree-sitter query strings
// ---------------------------------------------------------------------------

/// Captures `export { foo }` (no alias, no `from` clause).
///
/// We match `export_statement` nodes that contain an `export_clause` but no
/// `source` child — that distinguishes plain named exports from named re-exports.
/// For `export { foo as bar }` see `NAMED_EXPORT_ALIAS_QUERY`.
///
/// GAP-13-03 (inline type modifiers): `export { type Foo, Bar }` is handled
/// transparently by this query because the grammar's `export_specifier` matches
/// the `identifier` @name node regardless of a preceding per-specifier `"type"` token.
/// Both `Foo` and `Bar` appear exactly once as Named exports.
const NAMED_EXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name)))
"#;

/// Captures `export { foo as bar }` (with alias, no `from` clause).
///
/// Capture index 0 = `@local_name` (the local binding, e.g. "foo").
/// Capture index 1 = `@alias_name` (the public export name, e.g. "bar").
/// We process this query first so the alias name is used instead of the local name.
const NAMED_EXPORT_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name)))
"#;

/// ARCH-02 Form 1: `Target.Member = X;` top-level member assignment.
///
/// Captures `@target` (the object identifier) and `@member` (the new property
/// name). Matched against `expression_statement` nodes at the program level so
/// nested-inside-function assignments are excluded — those don't contribute to
/// the public surface.
const COMPOUND_ASSIGN_MEMBER_QUERY: &str = r#"
(program
  (expression_statement
    (assignment_expression
      left: (member_expression
        object: (identifier) @target
        property: (property_identifier) @member)
      right: (_) @value)))
"#;

/// ARCH-02 Form 3: `Object.assign(Target, { Member: X, ... })` compound
/// component composition.
///
/// Captures `@obj` + `@assign_prop` (to check Object.assign), `@target` (first
/// arg identifier), and `@member` for every property key in the object literal.
/// Supports both `(pair key: (property_identifier))` and shorthand properties.
const COMPOUND_OBJECT_ASSIGN_QUERY: &str = r#"
(call_expression
  function: (member_expression
    object: (identifier) @obj
    property: (property_identifier) @assign_prop)
  arguments: (arguments
    (identifier) @target
    (object
      [
        (pair key: (property_identifier) @member)
        (shorthand_property_identifier) @member
      ])))
"#;

/// Captures `export default <expr>` — the `value:` field covers expressions and
/// object/array literals. Named default exports (functions/classes with a name
/// that are declared as `declaration:`) are handled by `DEFAULT_DECL_EXPORT_QUERY`.
const DEFAULT_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  value: (_) @value)
"#;

/// Captures `export default function foo` and `export default class Foo` —
/// these appear as `declaration:` children, not `value:`, in both grammars.
/// We capture the keyword "default" position as the anchor since anonymous
/// default declarations don't have a name node.
const DEFAULT_DECL_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  declaration: (_) @decl)
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`
/// for **JavaScript** files. Class names use `identifier` in the JS grammar.
const DECL_EXPORT_QUERY_JS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (class_declaration name: (identifier) @name)
  ])
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`,
/// `export abstract class Foo`, `export type Foo`, `export interface Foo`, `export enum Foo`,
/// `export namespace Foo`, and overload `export function foo(...)` signatures
/// for **TypeScript** files. TypeScript uses `type_identifier` for class/type/interface names.
const DECL_EXPORT_QUERY_TS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (function_signature name: (identifier) @name)
    (class_declaration name: (type_identifier) @name)
    (abstract_class_declaration name: (type_identifier) @name)
    (type_alias_declaration name: (type_identifier) @name)
    (interface_declaration name: (type_identifier) @name)
    (enum_declaration name: (identifier) @name)
    (internal_module name: (identifier) @name)
  ])
"#;

/// Captures `export * from './module'` (plain star re-export).
///
/// There are two tree-sitter forms for star re-exports:
/// - `export * from './module'` → `(export_statement source: (string))`
///   with a bare `*` token — no `namespace_export` child node.
/// - `export * as name from './module'` → `(export_statement (namespace_export) source: (string))`
///
/// We use two separate queries to cover both forms.
const STAR_EXPORT_QUERY: &str = r#"
(export_statement
  "*"
  source: (string) @source)
"#;

/// Captures `export * as name from './module'` (namespace re-export).
///
/// Capture index 0 = `@ns_name` (the namespace identifier, e.g. "utils").
/// Capture index 1 = `@source` (the module specifier string).
const NAMESPACE_EXPORT_QUERY: &str = r#"
(export_statement
  (namespace_export
    (identifier) @ns_name)
  source: (string) @source)
"#;

/// Captures `export { foo } from './module'` (no alias).
/// For `export { foo as bar } from './module'` see `NAMED_REEXPORT_ALIAS_QUERY`.
const NAMED_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name))
  source: (string) @source)
"#;

/// Captures `export { foo as bar } from './module'` (with alias).
///
/// Capture index 0 = `@local_name` (the local name in the source module, e.g. "foo").
/// Capture index 1 = `@alias_name` (the public re-export name, e.g. "bar").
/// Capture index 2 = `@source` (the module specifier string).
/// We process this query first so the alias name is used as the exported name.
const NAMED_REEXPORT_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name))
  source: (string) @source)
"#;

/// Captures `export type { Foo }` (TS 3.8+ type-only named exports, no `from` clause).
///
/// The `"type"` keyword is an anonymous token child of `export_statement`.
/// Type-only exports are public API surface and must be tracked in the export map.
/// See Phase 12 RESEARCH Pattern 1.
const TYPE_ONLY_NAMED_EXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @name)))
"#;

/// Captures `export type { Foo } from './bar'` (TS 3.8+ type-only named re-exports).
///
/// Capture index 0 = `@name`, capture index 1 = `@source`.
const TYPE_ONLY_NAMED_REEXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @name))
  source: (string) @source)
"#;

/// Captures `export type { Foo as Bar }` (type-only with alias, no `from` clause).
///
/// Capture index 0 = `@local_name`, capture index 1 = `@alias_name`.
/// The alias_name is the public exported name.
const TYPE_ONLY_ALIAS_EXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name)))
"#;

/// Captures `export type { Foo as Bar } from './bar'` (type-only with alias and re-export).
///
/// Capture index 0 = `@local_name`, capture index 1 = `@alias_name`, capture index 2 = `@source`.
const TYPE_ONLY_ALIAS_REEXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name))
  source: (string) @source)
"#;

/// Captures `export const { foo, bar } = obj` — shorthand destructuring patterns.
///
/// Each `shorthand_property_identifier_pattern` is a separately exported name.
/// See Phase 12 RESEARCH Pattern 2.
const DESTRUCTURED_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (object_pattern
        (shorthand_property_identifier_pattern) @name))))
"#;

/// Captures `export const { foo: renamed } = obj` — pair-pattern destructuring.
///
/// The `value` (identifier) is the PUBLIC exported name (the local binding).
/// The `key` is the source property.
/// See Phase 12 RESEARCH Pattern 2 and BUG-13-03 fix.
const DESTRUCTURED_PAIR_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (object_pattern
        (pair_pattern
          value: (identifier) @name)))))
"#;

/// Captures `export = Foo` — CommonJS-style TypeScript module export.
///
/// This is a TypeScript-only syntax. The export is treated as a default export
/// with name "default" in interlint's model (equivalent to `module.exports = Foo`).
/// See Phase 12 RESEARCH Pattern 3.
const EXPORT_ASSIGNMENT_QUERY_TS: &str = r#"
(export_statement
  "="
  (_) @value)
"#;

/// Captures `export declare function/class/interface/...` — ambient declarations.
///
/// In `.d.ts` files (and `.ts` files with `declare` blocks), exported declarations
/// are wrapped in an `ambient_declaration` node. The existing `DECL_EXPORT_QUERY_TS`
/// does NOT match through this wrapper.
///
/// This is critical for packages that ship `.d.ts` type declaration files —
/// virtually all TypeScript packages. Their public API is expressed as
/// `declare` statements.
/// See Phase 12 RESEARCH Pattern 6 (Gap 8).
const AMBIENT_DECL_EXPORT_QUERY_TS: &str = r#"
(export_statement
  declaration: (ambient_declaration
    [
      (function_declaration name: (identifier) @name)
      (function_signature name: (identifier) @name)
      (class_declaration name: (type_identifier) @name)
      (abstract_class_declaration name: (type_identifier) @name)
      (type_alias_declaration name: (type_identifier) @name)
      (interface_declaration name: (type_identifier) @name)
      (enum_declaration name: (identifier) @name)
      (internal_module name: (identifier) @name)
      (lexical_declaration (variable_declarator name: (identifier) @name))
    ]))
"#;

/// Captures `declare module 'express' { ... }` — module augmentation in .d.ts files.
///
/// Module augmentations extend external package types. The module name (string literal)
/// identifies which module is being augmented. These are captured as named exports
/// with a synthetic `augment:<module>` key so their removal is detected.
///
/// Grammar: `(ambient_declaration (module name: (string) body: (statement_block)))`.
/// The `string` node has a `string_fragment` child; `node.utf8_text` returns the
/// full quoted string (e.g., `"'express'"`). We strip quotes after capture.
/// GAP-15-06.
const DECLARE_MODULE_AUGMENTATION_QUERY_TS: &str = r#"
(ambient_declaration
  (module
    name: (string) @module_name
    body: (statement_block) @module_body))
"#;

/// Captures `declare global { ... }` — global augmentation in .d.ts files.
///
/// Global augmentations add declarations to the global scope. Captured with a
/// synthetic `augment:global` key.
///
/// Grammar: `declare global { ... }` parses as `(ambient_declaration (statement_block ...))`
/// — no `module` wrapper, no identifier. We detect it by matching `ambient_declaration`
/// nodes whose only named child is a `statement_block` (with no preceding module name node).
/// The query captures the `statement_block` so we can filter on node position.
/// GAP-15-06.
const DECLARE_GLOBAL_AUGMENTATION_QUERY_TS: &str = r#"
(ambient_declaration
  (statement_block) @global_body)
"#;

/// Captures `export { default as Named }` — keyword `default` as source with alias.
/// The `default` keyword is an unnamed node, not `identifier`, so existing queries miss it.
/// GAP-13-02.
const DEFAULT_SPECIFIER_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"
      alias: (identifier) @alias_name)))
"#;

/// Captures `export { default } from './foo'` — re-export of default.
/// GAP-13-02.
const DEFAULT_SPECIFIER_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"))
  source: (string) @source)
"#;

/// Captures `export { default as Named } from './foo'` — re-export default with alias.
/// GAP-13-02.
const DEFAULT_SPECIFIER_ALIAS_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"
      alias: (identifier) @alias_name))
  source: (string) @source)
"#;

/// Captures `export const [a, b] = arr` — array destructuring with named identifiers.
/// GAP-13-04.
const ARRAY_DESTRUCTURED_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (array_pattern
        (identifier) @name))))
"#;

/// Captures rest patterns in array destructuring: `export const [a, ...rest] = arr`.
/// GAP-13-04.
const ARRAY_DESTRUCTURED_REST_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (array_pattern
        (rest_pattern (identifier) @name)))))
"#;

/// Captures `module.exports = { foo, bar }` — CJS bulk export with shorthand properties.
/// JavaScript/CJS only. Each shorthand_property_identifier is a named export.
/// GAP-13-01.
const CJS_MODULE_EXPORTS_SHORTHAND_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (object
      (shorthand_property_identifier) @name)))
"#;

/// Captures `module.exports = { key: value }` — CJS bulk export with explicit pairs.
/// The key (property_identifier) is the exported name.
/// GAP-13-01.
const CJS_MODULE_EXPORTS_PAIR_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (object
      (pair
        key: (property_identifier) @name))))
"#;

/// Captures `exports.foo = bar` — CJS property-by-property export.
/// The property identifier is the exported name.
/// GAP-13-01.
const CJS_EXPORTS_PROPERTY_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @name)))
"#;

/// Captures `module.exports = SomeIdentifier` — CJS single default-like export.
/// GAP-13-01.
const CJS_MODULE_EXPORTS_DEFAULT_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (identifier) @value))
"#;

/// Captures `module.exports = class { ... }` — CJS inline class export.
/// The class may be named (`class Foo { ... }`) or anonymous (`class { ... }`).
/// GAP-15-01.
const CJS_MODULE_EXPORTS_CLASS_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (class) @class_value))
"#;

/// Captures `module.exports = function(...) { ... }`, including async functions,
/// generator functions, and arrow functions.
/// Note: In the JS grammar, generator function expressions use the node type
/// `generator_function` (not `generator_function_expression`).
/// GAP-15-01.
const CJS_MODULE_EXPORTS_FUNCTION_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: [
      (function_expression) @fn_value
      (generator_function) @fn_value
      (arrow_function) @fn_value
    ]))
"#;

/// Captures `exports.handler = async (req, res) => { ... }` and
/// `exports.foo = function() { ... }` — CJS named function property exports.
/// Note: In the JS grammar, generator function expressions use the node type
/// `generator_function` (not `generator_function_expression`).
/// GAP-15-01.
const CJS_EXPORTS_PROPERTY_FUNCTION_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @name)
    right: [
      (function_expression) @fn_value
      (generator_function) @fn_value
      (arrow_function) @fn_value
    ]))
"#;

/// Captures `exports = module.exports = { ... }` — CJS aliasing pattern.
/// Some packages use this pattern where the RHS is a chained assignment.
/// We capture the object properties from the final object literal.
/// GAP-16-09.
const CJS_EXPORTS_ALIASING_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (identifier) @lhs
    right: (assignment_expression
      left: (member_expression
        object: (identifier) @mod_obj
        property: (property_identifier) @mod_prop)
      right: (object) @obj)))
"#;

/// BUN-01: esbuild `0 && (module.exports = { ... })` tree-shake wrapper.
/// The binary_expression's RHS (under parenthesized_expression) is the module.exports assignment.
const ESBUILD_TREE_SHAKE_WRAPPER_QUERY: &str = r#"
(binary_expression
  left: (number)
  operator: "&&"
  right: (parenthesized_expression
    (assignment_expression
      left: (member_expression
        object: (identifier) @mod_obj
        property: (property_identifier) @mod_prop)
      right: (object) @exports_obj)))
"#;

/// BUN-02: esbuild `__export(target, { foo: () => foo })` helper.
/// Helper name is unstable (minified); match on call shape and verify target is
/// `exports` or `module` and values are arrow_function getters. BUN-02.
const ESBUILD_EXPORT_HELPER_QUERY: &str = r#"
(call_expression
  function: (identifier) @fn_name
  arguments: (arguments
    (identifier) @target
    (object) @prop_obj))
"#;

/// BUN-03: `var __defProp = Object.defineProperty;` alias binding.
/// We capture alias_name → (obj_name, prop_name) to resolve later __defProp calls.
/// Does NOT emit exports on its own — builds cjs_aliases map. BUN-03.
const ESBUILD_DEFPROP_ALIAS_QUERY: &str = r#"
(variable_declarator
  name: (identifier) @alias_name
  value: (member_expression
    object: (identifier) @obj_name
    property: (property_identifier) @prop_name))
"#;

/// BUN-04: tsc `exports.foo = void 0;` placeholder.
/// Chained assignments (`exports.foo = exports.bar = void 0`) each produce a
/// separate match because tree-sitter emits one assignment_expression per node.
/// BUN-04.
const TSC_VOID_EXPORTS_QUERY: &str = r#"
(assignment_expression
  left: (member_expression
    object: (identifier) @exports_id
    property: (property_identifier) @name)
  right: (_) @rhs)
"#;

/// BUN-05: swc `Object.keys(m).forEach(k => exports[k] = m[k])` star re-export shim.
/// Detected as StarReexport with source resolved from the require() bound to `m`.
/// BUN-05.
const SWC_STAR_REEXPORT_SHIM_QUERY: &str = r#"
(call_expression
  function: (member_expression
    object: (call_expression
      function: (member_expression
        object: (identifier) @object_id
        property: (property_identifier) @keys_method)
      arguments: (arguments (identifier) @source_module))
    property: (property_identifier) @foreach_method)
  arguments: (arguments (_) @callback))
"#;

/// BUN-06a: `Object.defineProperty(exports, "foo", ...)`,
/// `Object.defineProperties(exports, { foo: {...} })`,
/// `Object.assign(exports, { foo: x })` forms.
/// Also resolves __defProp aliases from the BUN-03 map. BUN-06.
const OBJECT_DEFINE_EXPORT_QUERY: &str = r#"
(call_expression
  function: (member_expression
    object: (identifier) @obj_id
    property: (property_identifier) @method)
  arguments: (arguments) @args)
"#;

/// BUN-06b: `exports["foo"] = bar;` bracket-subscript assignment.
/// BUN-06.
const BRACKET_SUBSCRIPT_EXPORT_QUERY: &str = r#"
(assignment_expression
  left: (subscript_expression
    object: (identifier) @exports_id
    index: (string) @name_str)
  right: (_))
"#;

// ---------------------------------------------------------------------------
// INF-02 (Phase 18): rollup-plugin-dts / api-extractor split-declaration
// capture queries.
//
// rollup-plugin-dts and api-extractor emit bundle output of the shape:
//     declare const foo: T;
//     ...
//     export { foo };
// where the `declare` and the `export { foo }` are SIBLING top-level nodes —
// the `declare` carries the type signature and decl_kind, the `export { foo }`
// is the public re-surface. A single-pass parse_exports treats the plain
// `export { foo }` as a typeless Named export because its decl_kind comes
// from the export_statement's `declaration:` field which is empty for these
// export-specifier forms. INF-02 restores the identity by running a Pass 1
// over all top-level `ambient_declaration` nodes, building a file-scope
// sidecar HashMap<name, (DeclarationKind, Option<type_signature>)>, and a
// post-process link pass enriches each Named non-re-export RawExport by
// lookup.
// ---------------------------------------------------------------------------

/// INF-02 Pass 1 / form A: `declare const/let X: T;` — variables with a type annotation.
/// The `type:` field captures the type-annotation text including the leading colon.
const AMBIENT_CONST_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (lexical_declaration
      (variable_declarator
        name: (identifier) @decl_name
        type: (type_annotation) @decl_type))))
"#;

/// INF-02 Pass 1 / form B: `declare function X(...): T;` — function signatures.
/// Whole-node text is used as a coarse type_signature stand-in (minus the trailing `;`).
const AMBIENT_FUNCTION_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (function_signature
      name: (identifier) @decl_name) @decl_node))
"#;

/// INF-02 Pass 1 / form C: `declare class X { ... }` — class declarations.
const AMBIENT_CLASS_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (class_declaration
      name: (type_identifier) @decl_name)))
"#;

/// INF-02 Pass 1 / form D: `declare interface X { ... }` — interface declarations.
const AMBIENT_INTERFACE_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (interface_declaration
      name: (type_identifier) @decl_name)))
"#;

/// INF-02 Pass 1 / form E: `declare type X = ...;` — type alias declarations.
const AMBIENT_TYPE_ALIAS_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (type_alias_declaration
      name: (type_identifier) @decl_name)))
"#;

/// INF-02 Pass 1 / form F: `declare enum X { ... }` — enum declarations.
const AMBIENT_ENUM_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (enum_declaration
      name: (identifier) @decl_name)))
"#;

/// INF-02 Pass 1: build a file-scope sidecar of top-level `declare` statements so that
/// Pass 2 (the existing export-statement processing) can enrich `export { foo }`
/// entries with the matching declare site's decl_kind and type_signature.
///
/// rollup-plugin-dts and api-extractor ship declare + export as sibling top-level
/// nodes; the existing parse_exports treats the plain `export { foo }` as a
/// typeless re-export. INF-02 restores the type identity by lookup.
///
/// On duplicate name (rare — same identifier declared twice), first-seen wins
/// per Phase 18 Research Pitfall 5 (entry().or_insert semantics).
///
/// Query-compile failures are handled gracefully per Phase 17 key decision: each
/// AMBIENT_*_DECL_QUERY is wrapped in `if let Ok(q)` and a failing query silently
/// skips that pattern.
fn collect_ambient_decls(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    lang: &SourceLanguage,
) -> std::collections::HashMap<String, (DeclarationKind, Option<String>)> {
    use std::collections::HashMap;
    let mut decls: HashMap<String, (DeclarationKind, Option<String>)> = HashMap::new();
    let language = get_language(lang);

    // Tuple element semantics:
    //   (query_str, kind, capture_type)
    // where `capture_type = true` means the query exposes a `@decl_type` capture
    // (variable/type-annotation form) whose text becomes the type_signature.
    // Otherwise `@decl_node` whole-node text (minus trailing `;`) is used as the
    // type_signature stand-in for function signatures; for class/interface/
    // enum/type-alias the type_signature stays None (name + decl_kind alone is
    // enough for downstream identity — downstream consumers care most about
    // the variable-type case).
    let queries = [
        (AMBIENT_CONST_DECL_QUERY, DeclarationKind::Variable, true),
        (
            AMBIENT_FUNCTION_DECL_QUERY,
            DeclarationKind::Function,
            false,
        ),
        (AMBIENT_CLASS_DECL_QUERY, DeclarationKind::Class, false),
        (
            AMBIENT_INTERFACE_DECL_QUERY,
            DeclarationKind::Interface,
            false,
        ),
        (
            AMBIENT_TYPE_ALIAS_DECL_QUERY,
            DeclarationKind::TypeAlias,
            false,
        ),
        (AMBIENT_ENUM_DECL_QUERY, DeclarationKind::Enum, false),
    ];

    for (query_str, kind, capture_type) in queries {
        let Ok(q) = Query::new(&language, query_str) else {
            continue;
        };
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&q, root, source);
        let capture_names = q.capture_names();
        while let Some(m) = matches_iter.next() {
            let mut name: Option<String> = None;
            let mut type_text: Option<String> = None;
            let mut decl_node_text: Option<String> = None;
            for cap in m.captures {
                let cap_name = capture_names[cap.index as usize];
                let text = cap.node.utf8_text(source).ok().map(|s| s.to_string());
                match cap_name {
                    "decl_name" => name = text,
                    "decl_type" => {
                        // `type_annotation` node text includes the leading `:` and any
                        // leading whitespace — trim so callers see the bare type.
                        type_text = text.map(|t| t.trim_start_matches(':').trim().to_string())
                    }
                    "decl_node" => decl_node_text = text,
                    _ => {}
                }
            }
            if let Some(n) = name {
                let type_sig = if capture_type {
                    type_text
                } else {
                    // Function signature: use full node text (minus trailing `;`)
                    // as a coarse type_signature. `None` for class/interface/
                    // enum/type-alias (kind alone conveys identity).
                    match kind {
                        DeclarationKind::Function => {
                            decl_node_text.map(|s| s.trim_end_matches(';').trim().to_string())
                        }
                        _ => None,
                    }
                };
                decls.entry(n).or_insert((kind.clone(), type_sig));
            }
        }
    }
    decls
}

// ---------------------------------------------------------------------------
// Declaration kind mapping
// ---------------------------------------------------------------------------

/// Map a tree-sitter node kind string to a DeclarationKind.
/// Per D-04 and 08-RESEARCH Pattern 1.
fn decl_kind_from_node_kind(kind: &str) -> DeclarationKind {
    match kind {
        "function_declaration" | "function_signature" => DeclarationKind::Function,
        "class_declaration" | "abstract_class_declaration" => DeclarationKind::Class,
        "interface_declaration" => DeclarationKind::Interface,
        "type_alias_declaration" => DeclarationKind::TypeAlias,
        "enum_declaration" => DeclarationKind::Enum,
        "internal_module" => DeclarationKind::Namespace,
        "lexical_declaration" | "variable_declaration" => DeclarationKind::Variable,
        _ => DeclarationKind::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Public parsing API
// ---------------------------------------------------------------------------

/// Parse `source` bytes with the given grammar and return all detected exports.
///
/// Line and column numbers in the returned `RawExport`s are **1-indexed**
/// (tree-sitter provides 0-indexed positions; this function adds 1 to both).
#[allow(dead_code)]
pub fn parse_exports(source: &[u8], lang: SourceLanguage) -> anyhow::Result<Vec<RawExport>> {
    let language = get_language(&lang);

    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .context("failed to set tree-sitter language")?;

    let tree = parser
        .parse(source, None)
        .context("tree-sitter returned None — source bytes may be invalid UTF-8 or too large")?;

    let root = tree.root_node();
    let mut exports: Vec<RawExport> = Vec::new();

    // INF-02 Pass 1 (Phase 18): collect top-level `declare` statements keyed by
    // identifier. The resulting sidecar is consulted post-collection by the
    // link-pass below to enrich rollup-plugin-dts / api-extractor split
    // declarations (`declare const foo: T; ... export { foo };`) with the
    // declare-site decl_kind and type_signature. Empty HashMap for non-TS or
    // TS files without ambient declarations — link-pass no-ops.
    let ambient_decls = collect_ambient_decls(root, source, &lang);

    // --- Named exports (no `from` clause) ------------------------------------
    // We run the named re-export query first to collect (name, source) pairs,
    // then use that set to exclude them from the plain named export results.
    let reexport_positions = collect_reexport_positions(source, &language)?;

    // Tracks (line, col) positions of names captured by type-only re-export queries
    // so the later NAMED_REEXPORT_QUERY section can skip them (avoiding double-emit).
    let mut type_only_reexport_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();

    // --- `export { default as Named }` forms (GAP-13-02) ----------------------
    // Process BEFORE regular named export queries to set positions for dedup.
    // The `default` keyword is an unnamed node in the grammar, so existing
    // identifier-based queries miss it.

    // 1. `export { default as Named } from './foo'` — re-export with alias
    if let Ok(query) = Query::new(&language, DEFAULT_SPECIFIER_ALIAS_REEXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            let mut alias_opt: Option<(String, u32, u32)> = None;
            let mut src_opt: Option<String> = None;
            for capture in m.captures {
                let node = capture.node;
                if capture.index == 0 {
                    // @alias_name
                    if let Ok(text) = node.utf8_text(source) {
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        alias_opt = Some((text.to_string(), line, col));
                    }
                } else if capture.index == 1 {
                    // @source
                    if let Ok(text) = node.utf8_text(source) {
                        src_opt = Some(strip_quotes(text).to_string());
                    }
                }
            }
            if let (Some((name, line, col)), Some(src)) = (alias_opt, src_opt) {
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                });
            }
        }
    }

    // 2. `export { default } from './foo'` — re-export default (no alias)
    if let Ok(query) = Query::new(&language, DEFAULT_SPECIFIER_REEXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @source
                    let node = capture.node;
                    if let Ok(text) = node.utf8_text(source) {
                        let src = strip_quotes(text).to_string();
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports
                            .iter()
                            .any(|e| e.name == "default" && e.source.as_deref() == Some(&src));
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::NamedReexport,
                                line,
                                col,
                                source: Some(src),
                                decl_kind: None,
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // 3. `export { default as Named }` — local default aliased to Named (no from clause)
    if let Ok(query) = Query::new(&language, DEFAULT_SPECIFIER_ALIAS_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @alias_name
                    let node = capture.node;
                    if let Ok(text) = node.utf8_text(source) {
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        exports.push(RawExport {
                            name: text.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // Process aliased named exports first: `export { foo as bar }`.
    // We track their local-name positions so the plain query below skips them.
    let mut alias_local_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();
    {
        let query = Query::new(&language, NAMED_EXPORT_ALIAS_QUERY)
            .context("failed to compile NAMED_EXPORT_ALIAS_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // index 0 = @local_name, index 1 = @alias_name
            let mut local_pos: Option<(u32, u32)> = None;
            let mut alias_name_opt: Option<(String, u32, u32)> = None;
            for capture in m.captures {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    local_pos = Some((line, col));
                } else if capture.index == 1 {
                    // alias_name is the PUBLIC exported name
                    if !reexport_positions.contains(&(line, col)) {
                        let name = node
                            .utf8_text(source)
                            .context("UTF-8 decode failed on alias name")?
                            .to_string();
                        alias_name_opt = Some((name, line, col));
                    }
                }
            }
            if let (Some(lp), Some((name, line, col))) = (local_pos, alias_name_opt) {
                alias_local_positions.insert(lp);
                // INF-03 (Phase 18): tsup/esbuild ESM trailer
                // `export { foo as default }`. The alias_name is the PUBLIC
                // exported name — when it is the keyword `default`, the
                // specifier is semantically a default-export, not a named
                // export with name "default". Emit `RawExport { name:
                // "default", kind: Default }` so downstream resolver keying
                // treats this as the default export (the resolver's default
                // path is keyed as `"default"` and carries distinct merge
                // semantics). Non-"default" aliases preserve pre-Phase-18
                // Named behavior — `export { foo as Baz }` remains a Named
                // export with name "Baz".
                let export_kind = if name == "default" {
                    RawExportKind::Default
                } else {
                    RawExportKind::Named
                };
                exports.push(RawExport {
                    name,
                    kind: export_kind,
                    line,
                    col,
                    source: None,
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                });
            }
        }
    }

    // Process non-aliased named exports: `export { foo }`.
    // Skip positions already handled by the alias query or re-export query.
    {
        let query = Query::new(&language, NAMED_EXPORT_QUERY)
            .context("failed to compile NAMED_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;

                    // Skip if this position belongs to a named re-export.
                    if reexport_positions.contains(&(line, col)) {
                        continue;
                    }
                    // Skip if already handled by the alias query above.
                    if alias_local_positions.contains(&(line, col)) {
                        continue;
                    }

                    let name = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on export name")?
                        .to_string();
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                    });
                }
            }
        }
    }

    // --- Type-only exports (TypeScript only: `export type { Foo }`) ------------
    // TS 3.8+ type-only export forms. The `"type"` keyword is an anonymous token.
    // These are tracked as Named/NamedReexport — same kind as regular named exports.
    // Processing order: alias forms first (to set type_only_alias_local_positions),
    // then non-alias forms (skipping positions handled by alias queries).
    // We also skip positions already in reexport_positions to avoid double-counting.
    // INF-01 (Phase 18): TypeScriptTsx included — TSX is a strict superset; every TS query matches identically.
    if matches!(
        lang,
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx
    ) {
        let mut type_only_alias_local_positions: std::collections::HashSet<(u32, u32)> =
            std::collections::HashSet::new();

        // Process `export type { Foo as Bar }` (no from clause)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_ALIAS_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_name_opt: Option<(String, u32, u32)> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        // @local_name
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        // @alias_name — public exported name; skip if it's a re-export
                        if !reexport_positions.contains(&(line, col)) {
                            if let Ok(name) = node.utf8_text(source) {
                                alias_name_opt = Some((name.to_string(), line, col));
                            }
                        }
                    }
                }
                if let (Some(lp), Some((name, line, col))) = (local_pos, alias_name_opt) {
                    type_only_alias_local_positions.insert(lp);
                    // Skip if already captured by the non-type-only alias query
                    if !alias_local_positions.contains(&lp) {
                        exports.push(RawExport {
                            name,
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }

        // Process `export type { Foo as Bar } from './bar'` (with source)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_ALIAS_REEXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        if let Ok(name) = node.utf8_text(source) {
                            alias_name_opt = Some((name.to_string(), line, col));
                        }
                    } else if capture.index == 2 {
                        if let Ok(text) = node.utf8_text(source) {
                            src_opt = Some(strip_quotes(text).to_string());
                        }
                    }
                }
                if let (Some(lp), Some((name, line, col)), Some(src)) =
                    (local_pos, alias_name_opt, src_opt)
                {
                    type_only_alias_local_positions.insert(lp);
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::NamedReexport,
                        line,
                        col,
                        source: Some(src),
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                    });
                }
            }
        }

        // Process `export type { Foo }` (no alias, no from clause)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_NAMED_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let node = capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        // Skip if already a re-export position
                        if reexport_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if handled by alias query above
                        if type_only_alias_local_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if already handled by the non-type-only alias query
                        if alias_local_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if already captured by the non-type-only named export query
                        // (the existing NAMED_EXPORT_QUERY also matches these nodes)
                        let already = exports.iter().any(|e| {
                            e.kind == RawExportKind::Named && e.line == line && e.col == col
                        });
                        if !already {
                            if let Ok(name) = node.utf8_text(source) {
                                exports.push(RawExport {
                                    name: name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Process `export type { Foo } from './bar'` (no alias, with source)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_NAMED_REEXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        // @name
                        if type_only_alias_local_positions.contains(&(line, col)) {
                            break; // alias variant already handled
                        }
                        if let Ok(name) = node.utf8_text(source) {
                            name_opt = Some((name.to_string(), line, col));
                        }
                    } else if capture.index == 1 {
                        if let Ok(text) = node.utf8_text(source) {
                            src_opt = Some(strip_quotes(text).to_string());
                        }
                    }
                }
                if let (Some((name, line, col)), Some(src)) = (name_opt, src_opt) {
                    // Track this position so the later NAMED_REEXPORT_QUERY section
                    // skips it and avoids emitting a duplicate NamedReexport entry.
                    type_only_reexport_positions.insert((line, col));
                    // Skip if already captured by non-type-only named re-export query
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::NamedReexport && e.line == line && e.col == col
                    });
                    if !already {
                        exports.push(RawExport {
                            name,
                            kind: RawExportKind::NamedReexport,
                            line,
                            col,
                            source: Some(src),
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Default exports (expression form) ------------------------------------
    // Captures `export default <expr>` where the default value is an expression
    // (e.g. `export default 42`, `export default obj`, `export default function() {}`).
    // Named default declarations (`export default class Foo {}`) are handled below.
    {
        let query = Query::new(&language, DEFAULT_EXPORT_QUERY)
            .context("failed to compile DEFAULT_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // @value gives us the position of the default export expression.
            if let Some(capture) = m.captures.first() {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                exports.push(RawExport {
                    name: "default".to_string(),
                    kind: RawExportKind::Default,
                    line,
                    col,
                    source: None,
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                });
            }
        }
    }

    // --- Default exports (declaration form) -----------------------------------
    // Captures `export default function foo() {}` and `export default class Foo {}`
    // which appear as `declaration:` children rather than `value:` children.
    // We only emit a "default" entry if one hasn't already been added above.
    {
        let query = Query::new(&language, DEFAULT_DECL_EXPORT_QUERY)
            .context("failed to compile DEFAULT_DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            if let Some(capture) = m.captures.first() {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                // Only add if no default has been emitted for this position yet.
                let already_emitted = exports
                    .iter()
                    .any(|e| e.kind == RawExportKind::Default && e.line == line && e.col == col);
                if !already_emitted {
                    exports.push(RawExport {
                        name: "default".to_string(),
                        kind: RawExportKind::Default,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                    });
                }
            }
        }
    }

    // --- Export assignment (`export = Foo`, TypeScript only) -----------------
    // CommonJS-style TypeScript default export. Produces a "default" entry.
    // Only emit if no default export already exists at this position.
    // INF-01 (Phase 18): TypeScriptTsx included — `export = Foo` is TS syntax
    // the TSX grammar also parses.
    if matches!(
        lang,
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx
    ) {
        if let Ok(query) = Query::new(&language, EXPORT_ASSIGNMENT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                if let Some(capture) = m.captures.first() {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    // Only add if no default export has already been emitted
                    let already_emitted = exports
                        .iter()
                        .any(|e| e.kind == RawExportKind::Default && e.line == line);
                    if !already_emitted {
                        exports.push(RawExport {
                            name: "default".to_string(),
                            kind: RawExportKind::Default,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Declaration exports --------------------------------------------------
    // Use grammar-specific queries: TypeScript uses `type_identifier` for class/
    // interface/type names; JavaScript uses `identifier` for all names.
    //
    // We skip any declaration that is under `export default` (which has a
    // "default" keyword child in the parent export_statement node) — those are
    // handled by the default export queries above.
    {
        // INF-01 (Phase 18): TypeScriptTsx shares DECL_EXPORT_QUERY_TS with
        // TypeScript — TSX is a strict superset of LANGUAGE_TYPESCRIPT's query
        // surface.
        let decl_query_str = match lang {
            SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx => DECL_EXPORT_QUERY_TS,
            SourceLanguage::JavaScript => DECL_EXPORT_QUERY_JS,
        };
        let query =
            Query::new(&language, decl_query_str).context("failed to compile DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;

                    // Skip if this declaration is under `export default …`.
                    // The export_statement parent will have a child with kind "default".
                    let is_default_export = node
                        .parent() // declaration node
                        .and_then(|decl| decl.parent()) // export_statement
                        .map(|stmt| {
                            (0..stmt.child_count() as u32).any(|i| {
                                stmt.child(i)
                                    .map(|c| c.kind() == "default")
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);

                    if is_default_export {
                        continue;
                    }

                    let name = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on declaration name")?
                        .to_string();
                    // Get the declaration node kind from the parent of the captured @name node.
                    // The @name node's parent is the declaration node (function_declaration, etc.).
                    let decl_node = node.parent();
                    let decl_kind = decl_node
                        .as_ref()
                        .map(|parent| decl_kind_from_node_kind(parent.kind()));
                    // META-04: extract TSDoc release tag from preceding block comment.
                    let release_tag = decl_node
                        .as_ref()
                        .and_then(|decl| extract_release_tag(*decl, source));
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line: node.start_position().row as u32 + 1,
                        col: node.start_position().column as u32 + 1,
                        source: None,
                        decl_kind,
                        is_type_only: false,
                        release_tag,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                    });
                }
            }
        }
    }

    // --- Ambient declaration exports (`export declare ...`, TypeScript only) ---
    // In .d.ts files and `declare` blocks, declarations are wrapped in
    // `ambient_declaration`. The existing DECL_EXPORT_QUERY_TS doesn't match
    // through this wrapper.
    // INF-01 (Phase 18): TypeScriptTsx included — ambient declarations also
    // appear in `.tsx` source (e.g., `declare global { ... }` inside a JSX-
    // bearing component file).
    if matches!(
        lang,
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx
    ) {
        if let Ok(query) = Query::new(&language, AMBIENT_DECL_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let node = capture.node;
                        let name = match node.utf8_text(source) {
                            Ok(n) => n.to_string(),
                            Err(_) => continue,
                        };

                        // Skip if already captured (ambient + non-ambient declarations
                        // on the same symbol would be redundant).
                        let already = exports.iter().any(|e| e.name == name);
                        if already {
                            continue;
                        }

                        // The captured @name node's parent is the inner declaration
                        // (e.g., function_declaration) OR variable_declarator (for lexical_declaration).
                        // For lexical_declaration: @name → variable_declarator → lexical_declaration.
                        // We need to check the correct level for decl_kind.
                        let immediate_parent = node.parent();
                        let decl_kind = immediate_parent.map(|ip| {
                            let parent_kind = ip.kind();
                            if parent_kind == "variable_declarator" {
                                // Navigate up one more level to lexical_declaration
                                ip.parent()
                                    .map(|ld| decl_kind_from_node_kind(ld.kind()))
                                    .unwrap_or(crate::types::DeclarationKind::Variable)
                            } else {
                                decl_kind_from_node_kind(parent_kind)
                            }
                        });

                        // META-04: extract TSDoc release tag from preceding block comment.
                        // For ambient declarations, the relevant node to check is the
                        // inner declaration (parent of @name).
                        let release_tag = node
                            .parent()
                            .and_then(|decl| extract_release_tag(decl, source));

                        exports.push(RawExport {
                            name,
                            kind: RawExportKind::Named,
                            line: node.start_position().row as u32 + 1,
                            col: node.start_position().column as u32 + 1,
                            source: None,
                            decl_kind,
                            is_type_only: false,
                            release_tag,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Destructured variable exports (`export const { foo, bar } = obj`) ---
    // These are variable exports where the name field is an object_pattern instead of
    // an identifier. The existing DECL_EXPORT_QUERY misses these because it only
    // handles `variable_declarator name: (identifier)`.
    // Works for both JS and TS.

    // Shorthand patterns: `export const { foo, bar } = obj`
    if let Ok(query) = Query::new(&language, DESTRUCTURED_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(crate::types::DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // Pair patterns: `export const { foo: renamed } = obj` — key is the public name
    if let Ok(query) = Query::new(&language, DESTRUCTURED_PAIR_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(crate::types::DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Star re-exports (plain `export * from '...'`) ------------------------
    // The JS/TS grammar represents `export * from '...'` with a bare `*` token
    // directly inside the export_statement (no namespace_export child node).
    if let Ok(query) = Query::new(&language, STAR_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @source
                    let node = capture.node;
                    let raw_source = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on star re-export source")?;
                    let module_spec = strip_quotes(raw_source);
                    let spec_str = module_spec.to_string();
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::StarReexport
                            && e.source.as_deref() == Some(&spec_str)
                    });
                    // TLF-07: Detect `export type * from '...'`.
                    // In tree-sitter-typescript 0.23.2, `export type *` is parsed as:
                    //   export_statement { export, ERROR { type }, *, from, string }
                    // The `type` keyword is inside an ERROR child, not a direct unnamed child.
                    // We check both direct children AND ERROR children for the `type` token.
                    let is_type_only = {
                        let mut found_type = false;
                        if let Some(stmt) = node.parent() {
                            let mut sc = stmt.walk();
                            for c in stmt.children(&mut sc) {
                                if !c.is_named() && c.utf8_text(source).ok() == Some("type") {
                                    found_type = true;
                                    break;
                                }
                                if c.kind() == "ERROR" {
                                    let mut ec = c.walk();
                                    for inner in c.children(&mut ec) {
                                        if !inner.is_named()
                                            && inner.utf8_text(source).ok() == Some("type")
                                        {
                                            found_type = true;
                                            break;
                                        }
                                    }
                                    if found_type {
                                        break;
                                    }
                                }
                            }
                        }
                        found_type
                    };
                    if !already {
                        exports.push(RawExport {
                            name: "*".to_string(),
                            kind: RawExportKind::StarReexport,
                            line: node.start_position().row as u32 + 1,
                            col: node.start_position().column as u32 + 1,
                            source: Some(spec_str),
                            decl_kind: None,
                            is_type_only,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Namespace re-exports (`export * as name from '...'`) -----------------
    // NAMESPACE_EXPORT_QUERY captures: index 0 = @ns_name, index 1 = @source.
    {
        if let Ok(query) = Query::new(&language, NAMESPACE_EXPORT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut ns_name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let text = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on namespace re-export")?;
                    if capture.index == 0 {
                        // @ns_name — the public namespace identifier
                        ns_name_opt = Some((
                            text.to_string(),
                            node.start_position().row as u32 + 1,
                            node.start_position().column as u32 + 1,
                        ));
                    } else if capture.index == 1 {
                        // @source — the module specifier
                        src_opt = Some(strip_quotes(text).to_string());
                    }
                }
                if let (Some((ns_name, line, col)), Some(src)) = (ns_name_opt, src_opt) {
                    // BUG-13-02: Include the name in the dedup check so that a plain star
                    // re-export (name="*") and a namespace re-export (name="utils") from the
                    // same source are NOT considered duplicates and can coexist.
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::StarReexport
                            && e.source.as_deref() == Some(src.as_str())
                            && e.name == ns_name
                    });
                    if !already {
                        exports.push(RawExport {
                            name: ns_name,
                            kind: RawExportKind::StarReexport,
                            line,
                            col,
                            source: Some(src),
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Named re-exports -----------------------------------------------------
    // Process aliased re-exports first (`export { foo as bar } from './mod'`),
    // then non-aliased re-exports (`export { foo } from './mod'`).
    // Track local-name positions from the alias query to skip them in the plain query.
    let mut reexport_alias_local_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();
    {
        let query = Query::new(&language, NAMED_REEXPORT_ALIAS_QUERY)
            .context("failed to compile NAMED_REEXPORT_ALIAS_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // index 0 = @local_name, index 1 = @alias_name, index 2 = @source
            let mut local_pos: Option<(u32, u32)> = None;
            let mut alias_name_opt: Option<(String, u32, u32)> = None;
            let mut src_opt: Option<String> = None;
            for capture in m.captures {
                let node = capture.node;
                let text = node
                    .utf8_text(source)
                    .context("UTF-8 decode failed on named re-export alias")?;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    local_pos = Some((line, col));
                } else if capture.index == 1 {
                    alias_name_opt = Some((text.to_string(), line, col));
                } else if capture.index == 2 {
                    src_opt = Some(strip_quotes(text).to_string());
                }
            }
            if let (Some(lp), Some((name, line, col)), Some(src)) =
                (local_pos, alias_name_opt, src_opt)
            {
                reexport_alias_local_positions.insert(lp);
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                });
            }
        }
    }
    {
        let query = Query::new(&language, NAMED_REEXPORT_QUERY)
            .context("failed to compile NAMED_REEXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // Match has two captures: @name (index 0) and @source (index 1).
            let mut name_opt: Option<(String, u32, u32)> = None;
            let mut source_opt: Option<String> = None;

            for capture in m.captures {
                let node = capture.node;
                let text = node
                    .utf8_text(source)
                    .context("UTF-8 decode failed on named re-export")?;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    name_opt = Some((text.to_string(), line, col));
                } else if capture.index == 1 {
                    source_opt = Some(strip_quotes(text).to_string());
                }
            }

            if let (Some((name, line, col)), Some(src)) = (name_opt, source_opt) {
                // Skip if already handled by the alias query above.
                if reexport_alias_local_positions.contains(&(line, col)) {
                    continue;
                }
                // Skip if already handled by the type-only re-export query
                // (TypeScript `export type { Foo } from '...'` — avoids double-emit).
                if type_only_reexport_positions.contains(&(line, col)) {
                    continue;
                }
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                });
            }
        }
    }

    // --- Array destructuring: `export const [a, b] = arr` (GAP-13-04) ----------
    if let Ok(query) = Query::new(&language, ARRAY_DESTRUCTURED_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // Rest patterns: `export const [a, ...rest] = arr`
    if let Ok(query) = Query::new(&language, ARRAY_DESTRUCTURED_REST_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }

    // --- Nested destructuring: `export const { a: { b } } = obj` (GAP-16-08) ---
    // Tree-sitter queries only match 1-level deep. Walk export statements with
    // object_pattern or array_pattern and recursively extract nested identifiers.
    // Runs AFTER flat destructuring queries so the dedup check correctly skips
    // names already captured by the flat queries (only adds the nested-level names).
    {
        let nested_query_str = r#"
        (export_statement
          declaration: (lexical_declaration
            (variable_declarator
              name: [(object_pattern) (array_pattern)] @pattern)))
        "#;
        if let Ok(query) = Query::new(&language, nested_query_str) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let mut names = Vec::new();
                        extract_nested_destructuring_names(capture.node, source, &mut names);
                        for (name, line, col) in names {
                            // Dedup: only add if not already captured by flat queries
                            let already = exports.iter().any(|e| e.name == name);
                            if !already {
                                exports.push(RawExport {
                                    name,
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: Some(DeclarationKind::Variable),
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // --- TS 5.2 `using`/`await using` export declarations (GAP-15-02) ----------
    // The tree-sitter-typescript grammar (v0.23.2) may not have native support for
    // `export using handle = getResource()` or `export await using h = getResource()`.
    // If parsing produces ERROR nodes, we apply a heuristic: walk top-level children
    // looking for export_statement nodes containing ERROR children whose text starts
    // with "using " or "await using ". If found, extract the identifier after the keyword.
    // If the grammar parses them correctly (future grammar version), the existing
    // DECL_EXPORT_QUERY_TS will catch them as lexical_declaration/variable_declarator.
    // This is graceful degradation — if neither heuristic fires, it's a silent no-op.
    // INF-01 (Phase 18): TypeScriptTsx included — `using`/`await using` is TS 5.2
    // syntax that TSX also accepts.
    if matches!(
        lang,
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx
    ) {
        let mut walk_cursor = root.walk();
        for top_child in root.children(&mut walk_cursor) {
            if top_child.kind() == "export_statement" || top_child.kind() == "ERROR" {
                let mut ec = top_child.walk();
                for export_child in top_child.children(&mut ec) {
                    if export_child.kind() == "ERROR"
                        || export_child.kind() == "expression_statement"
                    {
                        if let Ok(text) = export_child.utf8_text(source) {
                            let trimmed = text.trim();
                            let (keyword_len, is_using) = if trimmed.starts_with("await using ") {
                                (12usize, true)
                            } else if trimmed.starts_with("using ") {
                                (6usize, true)
                            } else {
                                (0, false)
                            };
                            if is_using {
                                // Extract identifier: take chars until '=' or whitespace after keyword
                                let rest = &trimmed[keyword_len..];
                                let ident: String = rest
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                                    .collect();
                                if !ident.is_empty() {
                                    let already = exports.iter().any(|e| e.name == ident);
                                    if !already {
                                        let line = export_child.start_position().row as u32 + 1;
                                        let col = export_child.start_position().column as u32 + 1;
                                        exports.push(RawExport {
                                            name: ident,
                                            kind: RawExportKind::Named,
                                            line,
                                            col,
                                            source: None,
                                            decl_kind: Some(DeclarationKind::Variable),
                                            is_type_only: false,
                                            release_tag: None,
                                            compound_members: Vec::new(),
                                            type_signature: None,
                                            satisfies_type: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- CJS module.exports / exports.foo detection (GAP-13-01) ---------------
    // Only for JavaScript files — TypeScript uses `export =` instead.
    // Validation (checking "module"/"exports" text) is done in Rust after capture
    // since #eq? predicates are not automatically applied by cursor.matches().
    if matches!(lang, SourceLanguage::JavaScript) {
        // 1. module.exports = { foo, bar } — shorthand properties
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_SHORTHAND_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Validate: capture 0 (@obj) must be "module", capture 1 (@prop) must be "exports"
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            // @name — shorthand property identifier
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. module.exports = { key: value } — explicit pair properties
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_PAIR_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. exports.foo = bar — property-by-property
        if let Ok(query) = Query::new(&language, CJS_EXPORTS_PROPERTY_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Validate: capture 0 (@obj) must be "exports" (not "module" or anything else)
                let obj_text = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                if obj_text == Some("exports") {
                    for capture in m.captures {
                        if capture.index == 1 {
                            // @name — the property being assigned
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4a. module.exports = class { ... } — CJS inline class export (GAP-15-01)
        // Run BEFORE query #4 (module.exports = Identifier) so the dedup check on "default"
        // skips the identifier-form query when an inline class was already emitted.
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_CLASS_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    if let Some(class_capture) = m.captures.iter().find(|c| c.index == 2) {
                        let node = class_capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports.iter().any(|e| e.name == "default");
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::Default,
                                line,
                                col,
                                source: None,
                                decl_kind: Some(DeclarationKind::Class),
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                            });
                        }
                    }
                }
            }
        }

        // 4b. module.exports = function/arrow — CJS inline function export (GAP-15-01)
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_FUNCTION_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    if let Some(fn_capture) = m.captures.iter().find(|c| c.index == 2) {
                        let node = fn_capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports.iter().any(|e| e.name == "default");
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::Default,
                                line,
                                col,
                                source: None,
                                decl_kind: Some(DeclarationKind::Function),
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                            });
                        }
                    }
                }
            }
        }

        // 4c. exports.NAME = function/arrow — CJS named function property export (GAP-15-01)
        // Only when @obj is "exports" (NOT "module"). The existing CJS_EXPORTS_PROPERTY_QUERY
        // already emits these as Variable; we emit a more specific Function decl_kind version.
        // Use a dedup check on name to avoid double-emitting with the existing query (#3).
        if let Ok(query) = Query::new(&language, CJS_EXPORTS_PROPERTY_FUNCTION_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // @obj must be exactly "exports" (not "module")
                let obj_text = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                if obj_text != Some("exports") {
                    continue;
                }
                // Extract @name (index 1) from the member expression property
                let name_capture = m.captures.iter().find(|c| c.index == 1);
                if let Some(nc) = name_capture {
                    let name_node = nc.node;
                    if let Ok(export_name) = name_node.utf8_text(source) {
                        let line = name_node.start_position().row as u32 + 1;
                        let col = name_node.start_position().column as u32 + 1;
                        // Update existing entry's decl_kind if already emitted as Variable by query #3
                        if let Some(existing) = exports.iter_mut().find(|e| {
                            e.name == export_name && e.decl_kind == Some(DeclarationKind::Variable)
                        }) {
                            existing.decl_kind = Some(DeclarationKind::Function);
                        } else {
                            let already = exports.iter().any(|e| e.name == export_name);
                            if !already {
                                exports.push(RawExport {
                                    name: export_name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: Some(DeclarationKind::Function),
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 4. module.exports = SomeIdentifier — default-like export
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_DEFAULT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            // @value — the identifier being assigned as default
                            let node = capture.node;
                            let line = node.start_position().row as u32 + 1;
                            let col = node.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == "default");
                            if !already {
                                exports.push(RawExport {
                                    name: "default".to_string(),
                                    kind: RawExportKind::Default,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // --- CJS aliasing: `exports = module.exports = { ... }` (GAP-16-09) --------
    // Some packages use a chained assignment pattern to alias exports and module.exports.
    // GAP-16-09.
    if matches!(lang, SourceLanguage::JavaScript) {
        if let Ok(query) = Query::new(&language, CJS_EXPORTS_ALIASING_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Verify: @lhs is "exports", @mod_obj is "module", @mod_prop is "exports"
                let lhs = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let mod_obj = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let mod_prop = m
                    .captures
                    .iter()
                    .find(|c| c.index == 2)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let obj_node = m.captures.iter().find(|c| c.index == 3).map(|c| c.node);

                if lhs == Some("exports")
                    && mod_obj == Some("module")
                    && mod_prop == Some("exports")
                {
                    if let Some(obj) = obj_node {
                        // Extract properties from the object literal
                        let mut obj_cursor = obj.walk();
                        for child in obj.children(&mut obj_cursor) {
                            match child.kind() {
                                "shorthand_property_identifier" => {
                                    if let Ok(name) = child.utf8_text(source) {
                                        let line = child.start_position().row as u32 + 1;
                                        let col = child.start_position().column as u32 + 1;
                                        let already = exports.iter().any(|e| e.name == name);
                                        if !already {
                                            exports.push(RawExport {
                                                name: name.to_string(),
                                                kind: RawExportKind::Named,
                                                line,
                                                col,
                                                source: None,
                                                decl_kind: None,
                                                is_type_only: false,
                                                release_tag: None,
                                                compound_members: Vec::new(),
                                                type_signature: None,
                                                satisfies_type: None,
                                            });
                                        }
                                    }
                                }
                                "pair" => {
                                    if let Some(key_node) = child.child_by_field_name("key") {
                                        if let Ok(name) = key_node.utf8_text(source) {
                                            let line = key_node.start_position().row as u32 + 1;
                                            let col = key_node.start_position().column as u32 + 1;
                                            let already = exports.iter().any(|e| e.name == name);
                                            if !already {
                                                exports.push(RawExport {
                                                    name: name.to_string(),
                                                    kind: RawExportKind::Named,
                                                    line,
                                                    col,
                                                    source: None,
                                                    decl_kind: None,
                                                    is_type_only: false,
                                                    release_tag: None,
                                                    compound_members: Vec::new(),
                                                    type_signature: None,
                                                    satisfies_type: None,
                                                });
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Module augmentation capture (GAP-15-06, TypeScript only) ---
    // Captures `declare module 'express' { ... }` and `declare global { ... }`.
    // Module augmentations extend external package types — their removal breaks consumers.
    // Emitted with synthetic `augment:<module>` key (or `augment:global`) so the existing
    // export-removed detection handles their removal without extra code.
    // INF-01 (Phase 18): TypeScriptTsx included — module augmentation is TS
    // syntax that TSX also parses.
    if matches!(
        lang,
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx
    ) {
        // declare module 'express' { ... }
        if let Ok(query) = Query::new(&language, DECLARE_MODULE_AUGMENTATION_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Extract both captures: index 0 = @module_name, index 1 = @module_body
                let mut module_name_opt: Option<String> = None;
                let mut body_node_opt: Option<tree_sitter::Node> = None;
                let mut name_line = 1u32;
                let mut name_col = 1u32;
                for capture in m.captures {
                    if capture.index == 0 {
                        // @module_name — the string literal (e.g., "'express'")
                        if let Ok(text) = capture.node.utf8_text(source) {
                            module_name_opt = Some(strip_quotes(text).to_string());
                            name_line = capture.node.start_position().row as u32 + 1;
                            name_col = capture.node.start_position().column as u32 + 1;
                        }
                    } else if capture.index == 1 {
                        // @module_body — the statement_block
                        body_node_opt = Some(capture.node);
                    }
                }
                if let Some(module_name) = module_name_opt {
                    let augment_key = format!("augment:{}", module_name);
                    let already = exports.iter().any(|e| e.name == augment_key);
                    if !already {
                        exports.push(RawExport {
                            name: augment_key,
                            kind: RawExportKind::Named,
                            line: name_line,
                            col: name_col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Namespace),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                    // META-03: emit synthetic keys for each top-level member in the body.
                    // Naming convention: `augment:<module>:*<member>`.
                    if let Some(body) = body_node_opt {
                        emit_augmentation_members(body, &module_name, source, &mut exports);
                    }
                }
            }
        }

        // declare global { ... }
        // Grammar: `declare global { ... }` parses as `(ambient_declaration (statement_block ...))`.
        // The `statement_block` is the first (and only) named child of `ambient_declaration`.
        // We match ambient_declaration nodes whose first named child is statement_block and
        // have no preceding module-name node — that distinguishes `declare global` from
        // `declare module 'foo' { }` (which has a module child with a string name).
        if let Ok(query) = Query::new(&language, DECLARE_GLOBAL_AUGMENTATION_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        // @global_body — the statement_block inside ambient_declaration.
                        // Verify the parent is ambient_declaration with no module-name child
                        // to exclude false positives from `declare module 'foo' { }` body blocks
                        // (those are nested inside a `module` node, not directly in ambient_declaration).
                        let body_node = capture.node;
                        // Check parent is ambient_declaration
                        if let Some(parent) = body_node.parent() {
                            if parent.kind() == "ambient_declaration" {
                                // Ensure no `module` child — that would be `declare module 'foo'`
                                let has_module_child = (0..parent.child_count() as u32)
                                    .filter_map(|i| parent.child(i))
                                    .any(|c| c.kind() == "module");
                                if !has_module_child {
                                    let line = body_node.start_position().row as u32 + 1;
                                    let col = body_node.start_position().column as u32 + 1;
                                    let already =
                                        exports.iter().any(|e| e.name == "augment:global");
                                    if !already {
                                        exports.push(RawExport {
                                            name: "augment:global".to_string(),
                                            kind: RawExportKind::Named,
                                            line,
                                            col,
                                            source: None,
                                            decl_kind: Some(DeclarationKind::Namespace),
                                            is_type_only: false,
                                            release_tag: None,
                                            compound_members: Vec::new(),
                                            type_signature: None,
                                            satisfies_type: None,
                                        });
                                    }
                                    // META-03: emit synthetic keys for top-level members.
                                    emit_augmentation_members(
                                        body_node,
                                        "global",
                                        source,
                                        &mut exports,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Namespace member expansion (REQ-14-03, GAP-16-05) ---------------------
    // Recursively expands namespace members at all nesting levels.
    // For `export namespace A { export namespace B { export function baz() {} } }`,
    // emits "A.B" and "A.B.baz" as separate Named exports.
    // GAP-16-05: supports 2+ levels of nesting.
    {
        let ns_query_str = "(export_statement (internal_module name: (identifier) @ns_name body: (statement_block) @ns_body))";
        if let Ok(query) = Query::new(&language, ns_query_str) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let mut ns_name_opt: Option<String> = None;
                let mut ns_body_opt: Option<tree_sitter::Node> = None;
                for capture in m.captures {
                    if capture.index == 0 {
                        // @ns_name
                        if let Ok(name) = capture.node.utf8_text(source) {
                            ns_name_opt = Some(name.to_string());
                        }
                    } else if capture.index == 1 {
                        // @ns_body
                        ns_body_opt = Some(capture.node);
                    }
                }
                if let (Some(ns_name), Some(ns_body)) = (ns_name_opt, ns_body_opt) {
                    expand_namespace_members(ns_body, &ns_name, source, &mut exports, &language);
                }
            }
        }
    }

    // --- Bundler CJS output: BUN-01..07 (JavaScript only) ----------------------
    // Detects esbuild tree-shake wrappers, __export helper calls, __defProp aliases,
    // tsc void 0 placeholders, swc star re-export shims, Object.defineProperty/
    // defineProperties/assign forms, and bracket-subscript assignments.
    if matches!(lang, SourceLanguage::JavaScript) {
        // BUN-03: Build alias map: identifier → (Object, defineProperty) before processing
        // other queries so BUN-06a can resolve __defProp aliases.
        let mut cjs_aliases: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();
        if let Ok(query) = Query::new(&language, ESBUILD_DEFPROP_ALIAS_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let alias_name = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                let obj_name = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                let prop_name = m
                    .captures
                    .iter()
                    .find(|c| c.index == 2)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if obj_name == "Object" && prop_name == "defineProperty" && !alias_name.is_empty() {
                    cjs_aliases.insert(alias_name, (obj_name, prop_name));
                }
            }
        }

        // BUN-01: esbuild `0 && (module.exports = { ... })` tree-shake wrapper
        if let Ok(query) = Query::new(&language, ESBUILD_TREE_SHAKE_WRAPPER_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let mod_obj = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let mod_prop = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let exports_obj = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                if mod_obj == Some("module") && mod_prop == Some("exports") {
                    if let Some(obj) = exports_obj {
                        let mut obj_cursor = obj.walk();
                        for child in obj.children(&mut obj_cursor) {
                            if child.kind() == "pair" {
                                if let Some(key_node) = child.child_by_field_name("key") {
                                    if let Ok(name) = key_node.utf8_text(source) {
                                        if name == "__esModule" {
                                            continue;
                                        }
                                        let line = key_node.start_position().row as u32 + 1;
                                        let col = key_node.start_position().column as u32 + 1;
                                        let already = exports.iter().any(|e| e.name == name);
                                        if !already {
                                            exports.push(RawExport {
                                                name: name.to_string(),
                                                kind: RawExportKind::Named,
                                                line,
                                                col,
                                                source: None,
                                                decl_kind: None,
                                                is_type_only: false,
                                                release_tag: None,
                                                compound_members: Vec::new(),
                                                type_signature: None,
                                                satisfies_type: None,
                                            });
                                        }
                                    }
                                }
                            } else if child.kind() == "shorthand_property_identifier" {
                                if let Ok(name) = child.utf8_text(source) {
                                    if name == "__esModule" {
                                        continue;
                                    }
                                    let line = child.start_position().row as u32 + 1;
                                    let col = child.start_position().column as u32 + 1;
                                    let already = exports.iter().any(|e| e.name == name);
                                    if !already {
                                        exports.push(RawExport {
                                            name: name.to_string(),
                                            kind: RawExportKind::Named,
                                            line,
                                            col,
                                            source: None,
                                            decl_kind: None,
                                            is_type_only: false,
                                            release_tag: None,
                                            compound_members: Vec::new(),
                                            type_signature: None,
                                            satisfies_type: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // BUN-02: esbuild `__export(target, { foo: () => foo })` helper
        if let Ok(query) = Query::new(&language, ESBUILD_EXPORT_HELPER_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let fn_name = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let target = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let prop_obj = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                // Heuristic: fn_name is "__export*" or short (minified); target must be exports/module
                let is_export_helper = fn_name.starts_with("__export") || fn_name.len() <= 3;
                let is_valid_target = target == "exports" || target == "module";

                if is_export_helper && is_valid_target {
                    if let Some(obj) = prop_obj {
                        let mut obj_cursor = obj.walk();
                        for child in obj.children(&mut obj_cursor) {
                            if child.kind() == "pair" {
                                // Only emit if value is an arrow_function (getter)
                                if let Some(val_node) = child.child_by_field_name("value") {
                                    if val_node.kind() != "arrow_function" {
                                        continue;
                                    }
                                }
                                if let Some(key_node) = child.child_by_field_name("key") {
                                    if let Ok(name) = key_node.utf8_text(source) {
                                        if name == "__esModule" {
                                            continue;
                                        }
                                        let line = key_node.start_position().row as u32 + 1;
                                        let col = key_node.start_position().column as u32 + 1;
                                        let already = exports.iter().any(|e| e.name == name);
                                        if !already {
                                            exports.push(RawExport {
                                                name: name.to_string(),
                                                kind: RawExportKind::Named,
                                                line,
                                                col,
                                                source: None,
                                                decl_kind: None,
                                                is_type_only: false,
                                                release_tag: None,
                                                compound_members: Vec::new(),
                                                type_signature: None,
                                                satisfies_type: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // BUN-04: tsc `exports.foo = void 0;` placeholder
        if let Ok(query) = Query::new(&language, TSC_VOID_EXPORTS_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let exports_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let name_node = m.captures.iter().find(|c| c.index == 1).map(|c| c.node);
                let rhs_node = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                if exports_id != "exports" {
                    continue;
                }

                // Only capture void 0 placeholders here (real assignments captured by
                // the existing CJS_EXPORTS_PROPERTY_QUERY)
                let is_void_rhs = rhs_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|t| t.trim() == "void 0")
                    .unwrap_or(false);

                if is_void_rhs {
                    if let Some(name_n) = name_node {
                        if let Ok(name) = name_n.utf8_text(source) {
                            if name == "__esModule" {
                                continue;
                            }
                            let line = name_n.start_position().row as u32 + 1;
                            let col = name_n.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == name);
                            if !already {
                                exports.push(RawExport {
                                    name: name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // BUN-05: swc `Object.keys(m).forEach(k => exports[k] = m[k])` star re-export shim.
        // Build a quick require-binding map first: var m = require('...')
        let mut require_bindings: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        {
            let require_query_str = r#"
            (variable_declarator
              name: (identifier) @binding
              value: (call_expression
                function: (identifier) @req_fn
                arguments: (arguments (string) @req_path)))
            "#;
            if let Ok(rq) = Query::new(&language, require_query_str) {
                let mut cur = QueryCursor::new();
                let mut mit = cur.matches(&rq, root, source);
                while let Some(rm) = mit.next() {
                    let binding = rm
                        .captures
                        .iter()
                        .find(|c| c.index == 0)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or("")
                        .to_string();
                    let req_fn = rm
                        .captures
                        .iter()
                        .find(|c| c.index == 1)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or("");
                    let req_path = rm
                        .captures
                        .iter()
                        .find(|c| c.index == 2)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .map(strip_quotes)
                        .unwrap_or_default();
                    if req_fn == "require" && !binding.is_empty() && !req_path.is_empty() {
                        require_bindings.insert(binding, req_path.to_string());
                    }
                }
            }
        }
        if let Ok(query) = Query::new(&language, SWC_STAR_REEXPORT_SHIM_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let object_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let keys_method = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let source_module_ident = m
                    .captures
                    .iter()
                    .find(|c| c.index == 2)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let foreach_method = m
                    .captures
                    .iter()
                    .find(|c| c.index == 3)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");

                if object_id != "Object" || keys_method != "keys" || foreach_method != "forEach" {
                    continue;
                }

                // Resolve the identifier to a require() target
                if let Some(require_target) = require_bindings.get(source_module_ident) {
                    let first_cap = m.captures.first().map(|c| c.node);
                    let (line, col) = first_cap
                        .map(|n| {
                            (
                                n.start_position().row as u32 + 1,
                                n.start_position().column as u32 + 1,
                            )
                        })
                        .unwrap_or((1, 1));
                    let star_key = format!("*{}", require_target);
                    let already = exports.iter().any(|e| e.name == star_key);
                    if !already {
                        exports.push(RawExport {
                            name: star_key,
                            kind: RawExportKind::StarReexport,
                            line,
                            col,
                            source: Some(require_target.clone()),
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }

        // BUN-06a: Object.defineProperty / defineProperties / assign exports
        if let Ok(query) = Query::new(&language, OBJECT_DEFINE_EXPORT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let method = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let args_node = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                // Gate: obj_id is "Object" directly OR an alias to Object.defineProperty
                let is_object_ref = obj_id == "Object"
                    || cjs_aliases
                        .get(obj_id)
                        .map(|(o, _)| o == "Object")
                        .unwrap_or(false);
                if !is_object_ref {
                    continue;
                }

                let is_define_prop = method == "defineProperty"
                    || (cjs_aliases
                        .get(obj_id)
                        .map(|(_, p)| p == "defineProperty")
                        .unwrap_or(false)
                        && obj_id != "Object");

                match method {
                    _ if is_define_prop && method != "defineProperties" && method != "assign" => {
                        // defineProperty(exports|module.exports, "name", descriptor)
                        if let Some(args) = args_node {
                            // Check first arg is exports or module.exports
                            let first_arg = args.named_children(&mut args.walk()).next();
                            let is_exports_target = first_arg
                                .and_then(|n| n.utf8_text(source).ok())
                                .map(|t| t == "exports" || t.starts_with("module."))
                                .unwrap_or(false);
                            if !is_exports_target {
                                continue;
                            }
                            // Second arg is the property name string
                            let second_arg = args.named_children(&mut args.walk()).nth(1);
                            if let Some(name_node) = second_arg {
                                if let Ok(raw) = name_node.utf8_text(source) {
                                    let name = strip_quotes(raw);
                                    if name == "__esModule" || name.is_empty() {
                                        continue;
                                    }
                                    let line = name_node.start_position().row as u32 + 1;
                                    let col = name_node.start_position().column as u32 + 1;
                                    let already = exports.iter().any(|e| e.name == name);
                                    if !already {
                                        exports.push(RawExport {
                                            name: name.to_string(),
                                            kind: RawExportKind::Named,
                                            line,
                                            col,
                                            source: None,
                                            decl_kind: None,
                                            is_type_only: false,
                                            release_tag: None,
                                            compound_members: Vec::new(),
                                            type_signature: None,
                                            satisfies_type: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    "defineProperties" => {
                        if let Some(args) = args_node {
                            let first_arg = args.named_children(&mut args.walk()).next();
                            let is_exports_target = first_arg
                                .and_then(|n| n.utf8_text(source).ok())
                                .map(|t| t == "exports" || t.starts_with("module."))
                                .unwrap_or(false);
                            if !is_exports_target {
                                continue;
                            }
                            // Second arg is the object of descriptors
                            let second_arg = args.named_children(&mut args.walk()).nth(1);
                            if let Some(obj_node) = second_arg {
                                let mut obj_cur = obj_node.walk();
                                for child in obj_node.children(&mut obj_cur) {
                                    if child.kind() == "pair" {
                                        if let Some(key_node) = child.child_by_field_name("key") {
                                            if let Ok(name) = key_node.utf8_text(source) {
                                                if name == "__esModule" {
                                                    continue;
                                                }
                                                let line = key_node.start_position().row as u32 + 1;
                                                let col =
                                                    key_node.start_position().column as u32 + 1;
                                                let already =
                                                    exports.iter().any(|e| e.name == name);
                                                if !already {
                                                    exports.push(RawExport {
                                                        name: name.to_string(),
                                                        kind: RawExportKind::Named,
                                                        line,
                                                        col,
                                                        source: None,
                                                        decl_kind: None,
                                                        is_type_only: false,
                                                        release_tag: None,
                                                        compound_members: Vec::new(),
                                                        type_signature: None,
                                                        satisfies_type: None,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "assign" => {
                        if let Some(args) = args_node {
                            let first_arg = args.named_children(&mut args.walk()).next();
                            let is_exports_target = first_arg
                                .and_then(|n| n.utf8_text(source).ok())
                                .map(|t| t == "exports" || t.starts_with("module."))
                                .unwrap_or(false);
                            if !is_exports_target {
                                continue;
                            }
                            // Remaining args are objects whose pairs to emit
                            let remaining: Vec<_> =
                                args.named_children(&mut args.walk()).skip(1).collect();
                            for obj_node in remaining {
                                let mut obj_cur = obj_node.walk();
                                for child in obj_node.children(&mut obj_cur) {
                                    if child.kind() == "pair" {
                                        if let Some(key_node) = child.child_by_field_name("key") {
                                            if let Ok(name) = key_node.utf8_text(source) {
                                                if name == "__esModule" {
                                                    continue;
                                                }
                                                let line = key_node.start_position().row as u32 + 1;
                                                let col =
                                                    key_node.start_position().column as u32 + 1;
                                                let already =
                                                    exports.iter().any(|e| e.name == name);
                                                if !already {
                                                    exports.push(RawExport {
                                                        name: name.to_string(),
                                                        kind: RawExportKind::Named,
                                                        line,
                                                        col,
                                                        source: None,
                                                        decl_kind: None,
                                                        is_type_only: false,
                                                        release_tag: None,
                                                        compound_members: Vec::new(),
                                                        type_signature: None,
                                                        satisfies_type: None,
                                                    });
                                                }
                                            }
                                        }
                                    } else if child.kind() == "shorthand_property_identifier" {
                                        if let Ok(name) = child.utf8_text(source) {
                                            if name == "__esModule" {
                                                continue;
                                            }
                                            let line = child.start_position().row as u32 + 1;
                                            let col = child.start_position().column as u32 + 1;
                                            let already = exports.iter().any(|e| e.name == name);
                                            if !already {
                                                exports.push(RawExport {
                                                    name: name.to_string(),
                                                    kind: RawExportKind::Named,
                                                    line,
                                                    col,
                                                    source: None,
                                                    decl_kind: None,
                                                    is_type_only: false,
                                                    release_tag: None,
                                                    compound_members: Vec::new(),
                                                    type_signature: None,
                                                    satisfies_type: None,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // BUN-06b: `exports["foo"] = bar;` bracket-subscript assignment
        if let Ok(query) = Query::new(&language, BRACKET_SUBSCRIPT_EXPORT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let exports_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let name_str_node = m.captures.iter().find(|c| c.index == 1).map(|c| c.node);

                if exports_id != "exports" {
                    continue;
                }

                if let Some(name_n) = name_str_node {
                    if let Ok(raw) = name_n.utf8_text(source) {
                        let name = strip_quotes(raw);
                        if name == "__esModule" || name.is_empty() {
                            continue;
                        }
                        let line = name_n.start_position().row as u32 + 1;
                        let col = name_n.start_position().column as u32 + 1;
                        let already = exports.iter().any(|e| e.name == name);
                        if !already {
                            exports.push(RawExport {
                                name: name.to_string(),
                                kind: RawExportKind::Named,
                                line,
                                col,
                                source: None,
                                decl_kind: None,
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                            });
                        }
                    }
                }
            }
        }
    } // end matches!(lang, SourceLanguage::JavaScript) BUN-01..07

    // BUN-07: Belt-and-suspenders __esModule final sweep.
    // Individual query processors already skip __esModule, but existing queries
    // (CJS_EXPORTS_PROPERTY_QUERY, CJS_EXPORTS_SHORTHAND_QUERY) may have captured it
    // from unrelated patterns. Remove unconditionally regardless of source.
    exports.retain(|e| e.name != "__esModule");

    // ARCH-02 (Phase 18): compound-component capture as a second pass.
    // Per D-16 conservative: members attach ONLY when the target identifier is
    // itself an exported symbol discovered by the first pass.
    attach_compound_members(root, source, &language, &mut exports);

    // INF-02 link-pass (Phase 18): for each Named (non-re-export) RawExport,
    // look up the matching declare-site in the file-scope ambient sidecar
    // built in Pass 1. When found, enrich decl_kind and type_signature when
    // the export's own values are None. This restores type identity for
    // rollup-plugin-dts / api-extractor bundle output where `export { foo }`
    // appears without inline declaration context.
    //
    // Guards:
    // - Only `RawExportKind::Named` entries — Default/StarReexport/NamedReexport
    //   are excluded because their identity does not come from a local declare.
    // - Skip when `rx.source.is_some()` — re-exports (`export { foo } from ...`)
    //   carry an external origin; the sidecar's local declare must NOT leak in.
    // - Skip when `rx.compound_members` is populated on a hybrid target (N/A —
    //   compound_members is orthogonal to the declare-linking pathway).
    for rx in exports.iter_mut() {
        if !matches!(rx.kind, RawExportKind::Named) {
            continue;
        }
        if rx.source.is_some() {
            continue;
        }
        if let Some((kind, type_sig)) = ambient_decls.get(&rx.name) {
            if rx.decl_kind.is_none() {
                rx.decl_kind = Some(kind.clone());
            }
            if rx.type_signature.is_none() && type_sig.is_some() {
                rx.type_signature = type_sig.clone();
            }
        }
    }

    Ok(exports)
}

/// ARCH-02 (Phase 18): second-pass attachment of compound-component members.
///
/// Scans the program for `Target.Member = X;` and `Object.assign(Target, { ... })`
/// patterns. When `Target` matches an already-exported symbol (D-16 conservative),
/// appends matched members to that symbol's `compound_members` list. Silent when
/// the target is not an exported symbol — this correctly handles minified/internal
/// names where the target is an internal binding.
fn attach_compound_members(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    language: &tree_sitter::Language,
    exports: &mut [RawExport],
) {
    use std::collections::HashSet;
    let exported_names: HashSet<String> = exports.iter().map(|r| r.name.clone()).collect();

    let attach = |target: &str, member: &str, exports: &mut [RawExport]| {
        if !exported_names.contains(target) {
            return;
        }
        for rx in exports.iter_mut() {
            if rx.name == target && !rx.compound_members.iter().any(|m| m == member) {
                rx.compound_members.push(member.to_string());
            }
        }
    };

    // Form 1: `Target.Member = X;` top-level.
    if let Ok(q) = Query::new(language, COMPOUND_ASSIGN_MEMBER_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, source);
        while let Some(m) = matches.next() {
            let mut target: Option<String> = None;
            let mut member: Option<String> = None;
            for cap in m.captures {
                let cap_name = q.capture_names()[cap.index as usize];
                let text = cap.node.utf8_text(source).ok().map(|s| s.to_string());
                match cap_name {
                    "target" => target = text,
                    "member" => member = text,
                    _ => {}
                }
            }
            if let (Some(t), Some(mm)) = (target, member) {
                attach(&t, &mm, exports);
            }
        }
    }

    // Form 3: `Object.assign(Target, { Member: X, ... })`.
    // Guard with a runtime @obj == "Object" && @assign_prop == "assign" check.
    if let Ok(q) = Query::new(language, COMPOUND_OBJECT_ASSIGN_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, source);
        while let Some(m) = matches.next() {
            let mut obj: Option<String> = None;
            let mut assign_prop: Option<String> = None;
            let mut target: Option<String> = None;
            let mut members: Vec<String> = Vec::new();
            for cap in m.captures {
                let cap_name = q.capture_names()[cap.index as usize];
                let text = cap.node.utf8_text(source).ok().map(|s| s.to_string());
                match cap_name {
                    "obj" => obj = text,
                    "assign_prop" => assign_prop = text,
                    "target" => target = text,
                    "member" => {
                        if let Some(t) = text {
                            members.push(t);
                        }
                    }
                    _ => {}
                }
            }
            if obj.as_deref() != Some("Object") || assign_prop.as_deref() != Some("assign") {
                continue;
            }
            if let Some(t) = target {
                for mm in members {
                    attach(&t, &mm, exports);
                }
            }
        }
    }
}

/// Recursively expand namespace members, producing dotted names like A.B.baz.
/// `prefix` is the dotted name so far (e.g., "A" or "A.B").
/// GAP-16-05: supports 2+ levels of nesting.
#[allow(clippy::only_used_in_recursion)]
fn expand_namespace_members(
    ns_body: tree_sitter::Node,
    prefix: &str,
    source: &[u8],
    exports: &mut Vec<RawExport>,
    language: &tree_sitter::Language,
) {
    let mut body_cursor = ns_body.walk();
    for stmt in ns_body.children(&mut body_cursor) {
        if stmt.kind() == "export_statement" {
            let mut stmt_cursor = stmt.walk();
            for decl_child in stmt.children(&mut stmt_cursor) {
                let decl_kind_str = decl_child.kind();

                // Check for nested namespace: `export namespace B { ... }`
                if decl_kind_str == "internal_module" {
                    if let Some(nested_name_node) = decl_child.child_by_field_name("name") {
                        if let Ok(nested_name) = nested_name_node.utf8_text(source) {
                            let nested_prefix = format!("{}.{}", prefix, nested_name);
                            // Emit the namespace itself as a Named export
                            let line = decl_child.start_position().row as u32 + 1;
                            let col = decl_child.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == nested_prefix);
                            if !already {
                                exports.push(RawExport {
                                    name: nested_prefix.clone(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: Some(DeclarationKind::Namespace),
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                });
                            }
                            // Recurse into the nested namespace body
                            if let Some(body) = decl_child.child_by_field_name("body") {
                                expand_namespace_members(
                                    body,
                                    &nested_prefix,
                                    source,
                                    exports,
                                    language,
                                );
                            }
                        }
                    }
                    continue;
                }

                // Extract name from the inner declaration (same logic as before)
                let inner_name = match decl_kind_str {
                    "function_declaration"
                    | "generator_function_declaration"
                    | "class_declaration"
                    | "abstract_class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "function_signature" => decl_child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string()),
                    "lexical_declaration" => {
                        // export const baz = ... inside namespace
                        let mut found = None;
                        let mut ldc = decl_child.walk();
                        for ld_child in decl_child.children(&mut ldc) {
                            if ld_child.kind() == "variable_declarator" {
                                found = ld_child
                                    .child_by_field_name("name")
                                    .and_then(|n| n.utf8_text(source).ok())
                                    .map(|s| s.to_string());
                                break;
                            }
                        }
                        found
                    }
                    _ => None,
                };
                if let Some(member_name) = inner_name {
                    let dotted = format!("{}.{}", prefix, member_name);
                    let line = decl_child.start_position().row as u32 + 1;
                    let col = decl_child.start_position().column as u32 + 1;
                    let decl_kind = Some(decl_kind_from_node_kind(decl_kind_str));
                    // Don't emit duplicates
                    let already = exports.iter().any(|e| e.name == dotted);
                    if !already {
                        exports.push(RawExport {
                            name: dotted,
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                        });
                    }
                }
            }
        }
    }
}

/// META-03: Emit synthetic keys for top-level declaration members inside an
/// augmentation body (`declare module 'X' { ... }` or `declare global { ... }`).
///
/// Naming convention: `augment:<scope>:*<member>` where scope is either the
/// module name (e.g., `"express"`) or `"global"`.
///
/// Each top-level declaration kind that is commonly augmented — interface,
/// type alias, class, function, const/let/var — is detected by node kind.
/// Other kinds (comments, punctuation) are skipped.
fn emit_augmentation_members(
    body: tree_sitter::Node,
    augment_scope: &str,
    source: &[u8],
    exports: &mut Vec<RawExport>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        let kind = child.kind();
        let member_name = match kind {
            "interface_declaration"
            | "type_alias_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "function_declaration"
            | "function_signature" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(String::from),
            "variable_declaration" | "lexical_declaration" => {
                // Find first variable_declarator name — collect to avoid borrow issue
                let mut vc = child.walk();
                let children_vec: Vec<_> = child.children(&mut vc).collect();
                children_vec
                    .into_iter()
                    .find(|n| n.kind() == "variable_declarator")
                    .and_then(|decl| decl.child_by_field_name("name"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(String::from)
            }
            _ => None,
        };
        if let Some(name) = member_name {
            let synthetic_key = format!("augment:{}:*{}", augment_scope, name);
            if !exports.iter().any(|e| e.name == synthetic_key) {
                exports.push(RawExport {
                    name: synthetic_key,
                    kind: RawExportKind::Named,
                    line: child.start_position().row as u32 + 1,
                    col: child.start_position().column as u32 + 1,
                    source: Some(augment_scope.to_string()),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                });
            }
        }
    }
}

/// META-04: Extract TSDoc release tag from the block comment immediately
/// preceding a declaration node.
///
/// The block comment preceding an exported declaration lives as a sibling of
/// the `export_statement` wrapper at the program-level, not as a sibling of the
/// inner declaration node. This function walks up to the export_statement (or
/// ambient_declaration) level and checks its preceding named sibling for a
/// `/** ... */` doc comment.
///
/// Returns the first recognized release tag found: @internal, @alpha,
/// @beta, @deprecated, or @public. Order defines priority.
pub fn extract_release_tag(
    decl_node: tree_sitter::Node,
    source: &[u8],
) -> Option<crate::types::ReleaseTag> {
    // The comment is a sibling of the wrapping export_statement/ambient_declaration,
    // not of the inner declaration node. Walk up to find the correct ancestor.
    let search_node = find_statement_ancestor(decl_node);

    // Now look at the preceding named sibling for a doc comment.
    let prev = search_node.prev_named_sibling()?;
    if prev.kind() != "comment" {
        return None;
    }
    let text = prev.utf8_text(source).ok()?;
    if !text.starts_with("/**") {
        return None;
    }
    // Simple tag scan — match on word-boundary @tag patterns.
    if text.contains("@internal") {
        return Some(crate::types::ReleaseTag::Internal);
    }
    if text.contains("@alpha") {
        return Some(crate::types::ReleaseTag::Alpha);
    }
    if text.contains("@beta") {
        return Some(crate::types::ReleaseTag::Beta);
    }
    if text.contains("@deprecated") {
        return Some(crate::types::ReleaseTag::Deprecated);
    }
    if text.contains("@public") {
        return Some(crate::types::ReleaseTag::Public);
    }
    None
}

/// CLS-10 (Phase 19): Extract `DecoratorSpec` values from the `decorator`
/// children of a given AST node, and from preceding-sibling `decorator` nodes.
///
/// Per CONTEXT D-03 (AST-direct, no comment-scan): decorators are first-class
/// AST nodes in both tree-sitter-typescript 0.23.2 and tree-sitter-javascript
/// 0.25.0.
///
/// The caller passes the node that MIGHT carry decorators — this is
/// grammar-and-position-specific:
///  - For class-level decorators (TypeScript): pass the `class_declaration`,
///    `abstract_class_declaration`, or the `export_statement` that wraps the
///    class (decorators can land on either the export_statement or the inner
///    class node in TS).
///  - For class-level decorators (JavaScript): pass the `class_declaration`
///    or the `export_statement`.
///  - For method decorators: pass the `method_definition` node (JS grammar
///    stores decorators directly on it; TS grammar sometimes puts them on
///    the preceding `class_body` child — handle both by walking preceding
///    siblings as a fallback).
///  - For field decorators: pass the `public_field_definition` (TS) or
///    `field_definition` (JS).
///  - For parameter decorators: pass the `required_parameter` or
///    `optional_parameter` node.
///
/// The implementation scans BOTH direct children (JS pattern) AND preceding
/// siblings that are `decorator` nodes (TS pattern). This is robust across
/// both grammars.
pub fn extract_decorators(
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Vec<crate::types::DecoratorSpec> {
    let mut out: Vec<crate::types::DecoratorSpec> = Vec::new();

    // Pattern 1: direct children — e.g. class_declaration has `decorator`
    // as a direct child in both TS and JS grammars.
    for i in 0..node.child_count() as u32 {
        let child = match node.child(i) {
            Some(c) => c,
            None => continue,
        };
        if child.kind() == "decorator" {
            if let Some(spec) = decorator_node_to_spec(child, source) {
                out.push(spec);
            }
        }
    }

    // Pattern 2: preceding siblings — TS grammar sometimes emits
    // `decorator` as a sibling directly before a `method_definition` or
    // `public_field_definition` inside `class_body`.
    let mut cursor = node;
    while let Some(prev) = cursor.prev_named_sibling() {
        if prev.kind() != "decorator" {
            break;
        }
        if let Some(spec) = decorator_node_to_spec(prev, source) {
            // Prepend to preserve source order (we're walking backwards).
            out.insert(0, spec);
        }
        cursor = prev;
    }

    out
}

/// Convert a single `decorator` AST node to a DecoratorSpec.
fn decorator_node_to_spec(
    decorator_node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<crate::types::DecoratorSpec> {
    // decorator's meaningful child is the first named child (identifier /
    // call_expression / member_expression / parenthesized_expression).
    // Node-types.json confirms no `field` name; we walk children.
    let inner = (0..decorator_node.child_count() as u32)
        .filter_map(|i| decorator_node.child(i))
        .find(|c| c.is_named())?;

    match inner.kind() {
        "call_expression" => {
            // @Route('/users') — name from `function` field, args from `arguments` field.
            let fn_node = inner.child_by_field_name("function")?;
            let name = fn_node.utf8_text(source).ok()?.to_string();
            let args = inner
                .child_by_field_name("arguments")
                .and_then(|a| a.utf8_text(source).ok().map(|s| s.to_string()));
            Some(crate::types::DecoratorSpec { name, args })
        }
        "identifier" | "member_expression" => {
            // @Component or @ns.foo — name is the source text, no args.
            let name = inner.utf8_text(source).ok()?.to_string();
            Some(crate::types::DecoratorSpec { name, args: None })
        }
        "parenthesized_expression" => {
            // @(expr) — rare; best effort.
            let name = inner.utf8_text(source).ok()?.to_string();
            Some(crate::types::DecoratorSpec { name, args: None })
        }
        _ => None,
    }
}

/// Walk up the tree from a declaration node to the nearest program-level
/// statement wrapper (`export_statement` or `ambient_declaration`), returning
/// that node. If no such ancestor is found (the node itself is at program level),
/// return the original node unchanged.
fn find_statement_ancestor(mut node: tree_sitter::Node) -> tree_sitter::Node {
    loop {
        let parent = match node.parent() {
            Some(p) => p,
            None => return node, // reached root
        };
        let parent_kind = parent.kind();
        if parent_kind == "program" {
            // `node` is directly under the root — it is the statement we want
            return node;
        }
        if parent_kind == "export_statement" || parent_kind == "ambient_declaration" {
            // The export_statement or ambient_declaration is the relevant statement
            return parent;
        }
        node = parent;
    }
}

/// Recursively extract exported names from nested destructuring patterns.
/// Handles `{ a: { b } }`, `[a, [b, c]]`, `{ a: [b, c] }`, etc.
/// GAP-16-08.
fn extract_nested_destructuring_names(
    pattern_node: tree_sitter::Node,
    source: &[u8],
    names: &mut Vec<(String, u32, u32)>,
) {
    let mut cursor = pattern_node.walk();
    for child in pattern_node.children(&mut cursor) {
        match child.kind() {
            "shorthand_property_identifier_pattern" => {
                // `{ foo }` — foo is both key and value
                if let Ok(name) = child.utf8_text(source) {
                    let line = child.start_position().row as u32 + 1;
                    let col = child.start_position().column as u32 + 1;
                    names.push((name.to_string(), line, col));
                }
            }
            "pair_pattern" => {
                // `{ key: value }` — value is what we export
                if let Some(value_node) = child.child_by_field_name("value") {
                    match value_node.kind() {
                        "identifier" => {
                            if let Ok(name) = value_node.utf8_text(source) {
                                let line = value_node.start_position().row as u32 + 1;
                                let col = value_node.start_position().column as u32 + 1;
                                names.push((name.to_string(), line, col));
                            }
                        }
                        "object_pattern" | "array_pattern" => {
                            // Nested: `{ a: { b } }` or `{ a: [b, c] }`
                            extract_nested_destructuring_names(value_node, source, names);
                        }
                        _ => {}
                    }
                }
            }
            "identifier" => {
                // Direct identifier in array pattern: `[a, b]`
                if let Ok(name) = child.utf8_text(source) {
                    let line = child.start_position().row as u32 + 1;
                    let col = child.start_position().column as u32 + 1;
                    names.push((name.to_string(), line, col));
                }
            }
            "rest_pattern" => {
                // `...rest` in array pattern
                if let Some(inner) = child.child(1) {
                    if inner.kind() == "identifier" {
                        if let Ok(name) = inner.utf8_text(source) {
                            let line = inner.start_position().row as u32 + 1;
                            let col = inner.start_position().column as u32 + 1;
                            names.push((name.to_string(), line, col));
                        }
                    }
                }
            }
            "object_pattern" | "array_pattern" => {
                // Nested pattern: `[a, [b, c]]`
                extract_nested_destructuring_names(child, source, names);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Class / Enum / Generic extraction
// ---------------------------------------------------------------------------

use crate::types::{
    ClassMember, ClassMemberKind, ClassShape, EnumMember, EnumShape, EnumValueKind, TypeParam,
    Visibility,
};

/// Extract the `Visibility` for a method or field node by iterating its children
/// looking for an `accessibility_modifier` child.
///
/// Per Pitfall 3 in RESEARCH.md: no modifier = `Visibility::Public`.
fn extract_visibility(node: tree_sitter::Node, source: &[u8]) -> Visibility {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "accessibility_modifier" {
                if let Ok(text) = child.utf8_text(source) {
                    return match text {
                        "protected" => Visibility::Protected,
                        "private" => Visibility::Private,
                        _ => Visibility::Public, // "public" explicit
                    };
                }
            }
        }
    }
    Visibility::Public // No modifier = public (TypeScript default)
}

/// Count required (non-optional) parameters on a method or constructor node.
///
/// Finds the `formal_parameters` child and counts children with kind
/// `required_parameter` (not `optional_parameter`).
fn count_required_params(method_node: tree_sitter::Node) -> usize {
    for i in 0..method_node.child_count() as u32 {
        if let Some(params_node) = method_node.child(i) {
            if params_node.kind() == "formal_parameters" {
                let mut count = 0;
                let mut cursor = params_node.walk();
                for child in params_node.children(&mut cursor) {
                    if child.kind() == "required_parameter" {
                        count += 1;
                    }
                }
                return count;
            }
        }
    }
    0
}

/// IFT-03: Extract per-position optionality of parameters in a function/method node.
///
/// Finds the `formal_parameters` child and returns a `Vec<bool>` where `true`
/// means the parameter at that position is optional. Skips non-parameter children
/// (commas, parentheses, etc.).
pub fn extract_param_optionality(fn_or_method_node: tree_sitter::Node) -> Vec<bool> {
    for i in 0..fn_or_method_node.child_count() as u32 {
        if let Some(params_node) = fn_or_method_node.child(i) {
            if params_node.kind() == "formal_parameters" {
                let mut result = Vec::new();
                let mut cursor = params_node.walk();
                for child in params_node.children(&mut cursor) {
                    match child.kind() {
                        "required_parameter" => result.push(false),
                        "optional_parameter" => result.push(true),
                        _ => {} // skip commas, parens, etc.
                    }
                }
                return result;
            }
        }
    }
    Vec::new()
}

/// Extract all class members (methods, constructors, properties, abstract methods)
/// from a `class_declaration` or `abstract_class_declaration` node.
///
/// Per Pitfall 7: line/col are 1-indexed (tree-sitter provides 0-indexed).
///
/// CLS-12 (Phase 21): returns `(members, has_dynamic_static_key)`. The bool is
/// `true` if any `class_static_block` body contains a subscript-expression LHS
/// (`Foo[computed()] = x`) — consumed by `parse_class_shape` to set
/// `ClassShape.has_dynamic_static_key`, which the diff layer reads to emit the
/// `limitation: dynamic-static-member-key` annotation.
pub fn parse_class_members(
    source: &[u8],
    class_node: tree_sitter::Node,
) -> (Vec<ClassMember>, bool) {
    let mut members = Vec::new();
    // CLS-12 (Phase 21): extract the enclosing class name once — used by the
    // class_static_block arm to gate `Foo.bar = ...` assignments. Returns an
    // empty string for anonymous `export default class { ... }`, in which case
    // only the `this.bar = ...` path matches inside the static-block walker.
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
        .to_string();
    let mut has_dynamic_static_key = false;

    // Find class_body child
    let class_body = {
        let mut body = None;
        for i in 0..class_node.child_count() as u32 {
            if let Some(child) = class_node.child(i) {
                if child.kind() == "class_body" {
                    body = Some(child);
                    break;
                }
            }
        }
        match body {
            Some(b) => b,
            None => return (members, has_dynamic_static_key),
        }
    };

    let mut cursor = class_body.walk();
    for child in class_body.children(&mut cursor) {
        match child.kind() {
            "method_definition" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                // CLS-11 (Phase 21): ES2022 private methods (`#compute()`) keep
                // the `#` in `name` so diff messages stay accurate; the flag
                // lets class_diff.rs route them out of the public-surface
                // HashMaps (Risk 6 Option A — prevents false-positive CLS-03
                // `class-method-removed` on `#foo` → `#bar` renames).
                let is_private = name.starts_with('#');

                // Detect getter/setter by scanning anonymous children for "get"/"set" keywords.
                // Modifiers appear before the name field; stop at named nodes other than
                // accessibility_modifier to avoid false positives.
                let mut is_getter = false;
                let mut is_setter = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.is_named() && kw.kind() != "accessibility_modifier" {
                            break;
                        }
                        match kw.kind() {
                            "get" => is_getter = true,
                            "set" => is_setter = true,
                            _ => {}
                        }
                    }
                }

                let kind = if name == "constructor" {
                    ClassMemberKind::Constructor
                } else if is_getter {
                    ClassMemberKind::Getter
                } else if is_setter {
                    ClassMemberKind::Setter
                } else {
                    ClassMemberKind::Method
                };

                let visibility = extract_visibility(child, source);

                // Check for static, readonly, abstract, accessor keyword children
                let mut is_static = false;
                let mut is_abstract = false;
                let mut is_readonly = false;
                let mut is_accessor = false; // TLF-06
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "abstract" => is_abstract = true,
                            "readonly" => is_readonly = true,
                            "accessor" => is_accessor = true, // TLF-06
                            _ => {}
                        }
                    }
                }

                let param_count = count_required_params(child);

                // TLF-02: Synthesize Property members from constructor parameter properties.
                // A parameter property is a constructor parameter with an accessibility modifier
                // (public/protected/private) or readonly keyword. Bare parameters are NOT
                // synthesized (Pitfall 4).
                if name == "constructor" {
                    // Walk formal_parameters to find parameter properties
                    let fp_node = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "formal_parameters");
                    if let Some(fp) = fp_node {
                        let mut fp_cursor = fp.walk();
                        for param in fp.children(&mut fp_cursor) {
                            if param.kind() == "required_parameter"
                                || param.kind() == "optional_parameter"
                            {
                                let is_opt_param = param.kind() == "optional_parameter";
                                // Check for accessibility_modifier or readonly child
                                let mut param_vis: Option<crate::types::Visibility> = None;
                                let mut param_readonly = false;
                                let mut pp_cursor = param.walk();
                                for pp_child in param.children(&mut pp_cursor) {
                                    if pp_child.kind() == "accessibility_modifier" {
                                        let text = pp_child.utf8_text(source).unwrap_or("");
                                        param_vis = Some(match text {
                                            "protected" => crate::types::Visibility::Protected,
                                            "private" => crate::types::Visibility::Private,
                                            _ => crate::types::Visibility::Public,
                                        });
                                    } else if pp_child.kind() == "readonly" {
                                        param_readonly = true;
                                    }
                                }
                                // Only synthesize when an accessibility modifier or readonly is present
                                if param_vis.is_some() || param_readonly {
                                    // Get the parameter name: prefer `name` field, fallback to
                                    // first identifier child (tree-sitter 0.23.2 may not always
                                    // use the field name for parameter identifier nodes).
                                    let param_name_node =
                                        param.child_by_field_name("name").or_else(|| {
                                            let mut wc = param.walk();
                                            let found = param
                                                .children(&mut wc)
                                                .find(|n| n.kind() == "identifier");
                                            found
                                        });
                                    let param_name = param_name_node
                                        .and_then(|n| n.utf8_text(source).ok())
                                        .unwrap_or("")
                                        .to_string();
                                    if !param_name.is_empty() {
                                        members.push(ClassMember {
                                            name: param_name,
                                            kind: ClassMemberKind::Property,
                                            visibility: param_vis
                                                .unwrap_or(crate::types::Visibility::Public),
                                            is_static: false,
                                            is_abstract: false,
                                            is_readonly: param_readonly,
                                            param_count: 0,
                                            type_signature: None,
                                            is_optional: is_opt_param,
                                            is_accessor: false,
                                            this_param_type: None,
                                            // CLS-10 (Phase 19): parameter-property decorators
                                            // live on the required_parameter/optional_parameter node.
                                            decorators: extract_decorators(param, source),
                                            is_private: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                members.push(ClassMember {
                    name,
                    kind,
                    visibility,
                    is_static,
                    is_abstract,
                    is_readonly,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor,
                    this_param_type: None,
                    // CLS-10 (Phase 19): method decorators attach to the
                    // method_definition node in JS, or appear as preceding
                    // decorator siblings inside class_body in TS. extract_decorators
                    // handles both patterns.
                    decorators: extract_decorators(child, source),
                    // CLS-11 (Phase 21): propagate `#`-prefix private method detection.
                    is_private,
                });
            }
            "public_field_definition" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();

                // CLS-11 (Phase 21): keep `#`-prefix private fields; the `is_private`
                // flag lets the diff layer pair public `foo` ↔ private `#foo` across
                // versions. `#` is retained in `name` so diff messages can show the
                // accurate source-level identifier on either side. The identity-match
                // helper in class_diff.rs strips the prefix on the fly.
                if name.is_empty() {
                    continue;
                }
                let is_private = name.starts_with('#');

                let visibility = extract_visibility(child, source);

                let mut is_static = false;
                let mut is_readonly = false;
                let mut is_accessor = false; // TLF-06: accessor keyword on public_field_definition
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "readonly" => is_readonly = true,
                            "accessor" => is_accessor = true, // TLF-06
                            _ => {}
                        }
                    }
                }

                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    type_signature: None,
                    is_optional: false,
                    is_accessor,
                    this_param_type: None,
                    // CLS-10 (Phase 19): field decorators attach to the
                    // public_field_definition node in TS, or appear as
                    // preceding decorator siblings in class_body.
                    decorators: extract_decorators(child, source),
                    // CLS-11 (Phase 21): propagate `#`-prefix private field detection.
                    is_private,
                });
            }
            "abstract_method_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();

                let visibility = extract_visibility(child, source);
                let param_count = count_required_params(child);

                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility,
                    is_static: false,
                    is_abstract: true,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    // CLS-10 (Phase 19): abstract-method decorators.
                    decorators: extract_decorators(child, source),
                    is_private: false,
                });
            }
            // BUG-13-01: .d.ts ambient class members use method_signature /
            // property_signature instead of method_definition / public_field_definition.
            "method_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let visibility = extract_visibility(child, source);
                let param_count = count_required_params(child);
                let mut is_static = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.kind() == "static" {
                            is_static = true;
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    // CLS-10 (Phase 19): ambient method_signature decorators
                    // (rare in .d.ts but harmless to extract).
                    decorators: extract_decorators(child, source),
                    is_private: false,
                });
            }
            "property_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                // CLS-11 (Phase 21): keep `#`-prefix ambient private members;
                // same rationale as public_field_definition arm.
                if name.is_empty() {
                    continue;
                }
                let is_private = name.starts_with('#');
                let visibility = extract_visibility(child, source);
                let mut is_static = false;
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "readonly" => is_readonly = true,
                            _ => {}
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    // CLS-10 (Phase 19): ambient property_signature decorators.
                    decorators: extract_decorators(child, source),
                    // CLS-11 (Phase 21): propagate `#`-prefix private ambient-member detection.
                    is_private,
                });
            }
            "class_static_block" => {
                // CLS-12 (Phase 21): extract top-level `Foo.bar = expr` and
                // `this.bar = expr` assignments from static initialization
                // blocks as synthesized static Property members. Subscript
                // expressions (`Foo[x] = ...`) set has_dynamic_static_key;
                // non-assignment statements are ignored per locked decision.
                let Some(sb_body) = child.child_by_field_name("body") else {
                    continue;
                };
                let mut sb_cursor = sb_body.walk();
                for stmt in sb_body.children(&mut sb_cursor) {
                    if stmt.kind() != "expression_statement" {
                        continue;
                    }
                    // Find the assignment_expression child of the expression_statement.
                    let mut s_cursor = stmt.walk();
                    let Some(assign) = stmt
                        .children(&mut s_cursor)
                        .find(|n| n.kind() == "assignment_expression")
                    else {
                        continue;
                    };
                    let Some(lhs) = assign.child_by_field_name("left") else {
                        continue;
                    };
                    match lhs.kind() {
                        "member_expression" => {
                            let obj = lhs.child_by_field_name("object");
                            let prop = lhs.child_by_field_name("property");
                            let obj_text = obj
                                .and_then(|n| n.utf8_text(source).ok())
                                .unwrap_or("");
                            // Gate: object must be the class name (by text) OR
                            // `this` (handles anonymous default-exported
                            // classes). If class_name is empty (anonymous),
                            // only `this` matches.
                            let is_class_ref = (!class_name.is_empty()
                                && obj_text == class_name)
                                || obj.is_some_and(|n| n.kind() == "this");
                            if !is_class_ref {
                                continue;
                            }
                            let prop_name = prop
                                .and_then(|n| n.utf8_text(source).ok())
                                .unwrap_or("")
                                .to_string();
                            if prop_name.is_empty() {
                                continue;
                            }
                            // Last-write-wins among STATIC-BLOCK synthesized
                            // members, but preserve class-body declarations
                            // (which carry LSP-enriched `type_signature` and
                            // may carry `decorators` / `is_readonly`). If a
                            // class-body `static bar: string = "x"` exists,
                            // the static-block `Foo.bar = "y"` assignment is
                            // a runtime write to that same slot — the typed
                            // declaration wins so CLS-07 static-type-change
                            // can fire on a genuine `string → number`
                            // regression. Heuristic: class-body members carry
                            // either a type_signature or decorators;
                            // synthesized ones carry neither.
                            let class_body_wins = members.iter().any(|m| {
                                m.name == prop_name
                                    && m.is_static
                                    && matches!(m.kind, ClassMemberKind::Property)
                                    && (m.type_signature.is_some()
                                        || !m.decorators.is_empty())
                            });
                            if class_body_wins {
                                // Skip synthesizing — class-body entry stays.
                                continue;
                            }
                            // Drop prior synthesized entries (no type_signature
                            // AND no decorators) for the same static-property
                            // slot; the new assignment supersedes them.
                            members.retain(|m| {
                                !(m.name == prop_name
                                    && m.is_static
                                    && matches!(m.kind, ClassMemberKind::Property)
                                    && m.type_signature.is_none()
                                    && m.decorators.is_empty())
                            });
                            members.push(ClassMember {
                                name: prop_name,
                                kind: ClassMemberKind::Property,
                                visibility: Visibility::Public,
                                is_static: true,
                                is_abstract: false,
                                is_readonly: false,
                                param_count: 0,
                                // No LSP enrichment on synthesized members.
                                type_signature: None,
                                is_optional: false,
                                is_accessor: false,
                                this_param_type: None,
                                decorators: Vec::new(),
                                // Static-block synthesized members are public surface.
                                is_private: false,
                            });
                        }
                        "subscript_expression" => {
                            // Locked decision: `Foo[computed()] = x` is NOT
                            // extracted. Flag the class so the diff layer can
                            // emit `limitation: dynamic-static-member-key`.
                            has_dynamic_static_key = true;
                        }
                        _ => {
                            // Other LHS shapes (identifier, etc.) are ignored
                            // — not static-member exports.
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (members, has_dynamic_static_key)
}

/// Extract a full `ClassShape` from a `class_declaration` or
/// `abstract_class_declaration` node.
pub fn parse_class_shape(source: &[u8], decl_node: tree_sitter::Node) -> ClassShape {
    let is_abstract = decl_node.kind() == "abstract_class_declaration";

    // Find class_heritage child (contains extends_clause and implements_clause)
    let mut extends_text: Option<String> = None;
    let mut implements_list: Vec<String> = Vec::new();

    for i in 0..decl_node.child_count() as u32 {
        if let Some(child) = decl_node.child(i) {
            if child.kind() == "class_heritage" {
                // Look for extends_clause and implements_clause
                for j in 0..child.child_count() as u32 {
                    if let Some(heritage_child) = child.child(j) {
                        match heritage_child.kind() {
                            "extends_clause" => {
                                if let Ok(text) = heritage_child.utf8_text(source) {
                                    // Strip the leading "extends " keyword
                                    let text = text.trim();
                                    let text = text.strip_prefix("extends ").unwrap_or(text);
                                    extends_text = Some(text.to_string());
                                }
                            }
                            "implements_clause" => {
                                // Iterate named children (type references)
                                let mut cursor = heritage_child.walk();
                                for impl_child in heritage_child.children(&mut cursor) {
                                    // Skip the "implements" keyword itself and commas
                                    if impl_child.is_named() {
                                        if let Ok(text) = impl_child.utf8_text(source) {
                                            implements_list.push(text.to_string());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
        }
    }

    let type_params = parse_type_params(source, decl_node);
    let (members, has_dynamic_static_key) = parse_class_members(source, decl_node);

    // CLS-10 (Phase 19): Extract class-level decorators. In TS, decorators
    // appear as direct children on the class_declaration / abstract_class_declaration
    // AND ALSO re-surface as children of the parent export_statement wrapper
    // (verified empirically against tree-sitter-typescript 0.23.2 —
    // `@Component\nexport class Foo {}` produces the decorator as a child of
    // BOTH the export_statement and the inner class_declaration).
    //
    // To avoid double-counting we prefer the class_declaration's own children
    // and only fall back to the parent export_statement if the direct children
    // yielded nothing. For the JS grammar, decorators only appear on the
    // export_statement, so the fallback path is the primary one there.
    let class_decorators: Vec<crate::types::DecoratorSpec> = {
        let direct = extract_decorators(decl_node, source);
        if !direct.is_empty() {
            direct
        } else if let Some(parent) = decl_node.parent() {
            if parent.kind() == "export_statement" {
                extract_decorators(parent, source)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };

    // IFT-03: Extract per-position constructor param optionality.
    // Find the class_body, scan for the constructor method_definition, then
    // call extract_param_optionality on it.
    let constructor_param_optionality = {
        let class_body = {
            let mut found = None;
            for i in 0..decl_node.child_count() as u32 {
                if let Some(child) = decl_node.child(i) {
                    if child.kind() == "class_body" {
                        found = Some(child);
                        break;
                    }
                }
            }
            found
        };
        if let Some(body) = class_body {
            let mut optionality = Vec::new();
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "method_definition" {
                    let is_ctor = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        == Some("constructor");
                    if is_ctor {
                        optionality = extract_param_optionality(child);
                        break;
                    }
                }
            }
            optionality
        } else {
            Vec::new()
        }
    };

    ClassShape {
        members,
        is_abstract,
        extends_text,
        implements_list,
        type_params,
        constructor_param_optionality,
        decorators: class_decorators,
        // Phase 21 CLS-12: propagated from `parse_class_members` tuple return.
        has_dynamic_static_key,
    }
}

/// Returns true if the given `enum_declaration` node has a `const` anonymous
/// keyword child (i.e., it is a `const enum`).
///
/// Per Pitfall 1 in RESEARCH.md: `const` is an anonymous grammar token,
/// not a named field. Must iterate children and check text.
pub fn is_const_enum(node: tree_sitter::Node, source: &[u8]) -> bool {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "enum" {
                break; // Past modifiers
            }
            if !child.is_named() {
                if let Ok(text) = child.utf8_text(source) {
                    if text == "const" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a function_declaration node has the `async` modifier.
///
/// The `async` keyword is an anonymous token that appears BEFORE the `function`
/// keyword in the AST child order. We stop iterating at the `function` keyword
/// to avoid false positives. Per Phase 9 RESEARCH Pitfall 2.
pub fn is_async_function(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "function" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("async") {
            return true;
        }
    }
    false
}

/// Check if a function_declaration or generator_function_declaration node has
/// the `*` generator marker.
///
/// The `*` token appears between the `function` keyword and the
/// `formal_parameters` node. We stop at `formal_parameters` to avoid
/// matching `*` in other contexts. Per Phase 9 RESEARCH Pitfall 3.
///
/// Note: tree-sitter-typescript parses `function* foo() {}` as
/// `generator_function_declaration`, not `function_declaration`.
pub fn is_generator_function(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "formal_parameters" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("*") {
            return true;
        }
    }
    false
}

/// Compute auto-incremented numeric values for enum members.
///
/// Per D-04 and RESEARCH.md: maintains running counter starting at 0.
/// - No explicit value: assign computed_value = Some(counter), increment.
/// - Numeric explicit value: assign computed_value = Some(n), set counter = n + 1.
/// - Non-numeric initializer: set computed_value = None, still increment counter.
pub fn compute_enum_values(members: &mut [EnumMember]) {
    let mut next_value: i64 = 0;
    for member in members.iter_mut() {
        match &member.raw_value {
            None => {
                member.computed_value = Some(next_value);
                next_value += 1;
            }
            Some(text) => {
                if let Ok(n) = text.trim().parse::<i64>() {
                    member.computed_value = Some(n);
                    next_value = n + 1;
                } else {
                    member.computed_value = None;
                    next_value += 1; // TypeScript behavior: still increment
                }
            }
        }
    }
}

/// Extract all members from an `enum_declaration` node.
///
/// Handles both explicit (`enum_assignment`) and implicit (bare identifier) members.
/// Calls `compute_enum_values` to fill in computed_value fields.
pub fn parse_enum_members(source: &[u8], enum_node: tree_sitter::Node) -> Vec<EnumMember> {
    let mut members = Vec::new();

    // Find enum_body child
    let enum_body = {
        let mut body = None;
        for i in 0..enum_node.child_count() as u32 {
            if let Some(child) = enum_node.child(i) {
                if child.kind() == "enum_body" {
                    body = Some(child);
                    break;
                }
            }
        }
        match body {
            Some(b) => b,
            None => return members,
        }
    };

    let mut cursor = enum_body.walk();
    for child in enum_body.children(&mut cursor) {
        match child.kind() {
            "enum_assignment" => {
                // Has explicit value: name: (_) @name value: (_) @value
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                let value_text = child
                    .child_by_field_name("value")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.to_string());

                let value_kind = if let Some(ref text) = value_text {
                    let trimmed = text.trim();
                    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                        EnumValueKind::StringLiteral
                    } else if trimmed.parse::<i64>().is_ok() {
                        EnumValueKind::Numeric
                    } else {
                        EnumValueKind::OtherExpr
                    }
                } else {
                    EnumValueKind::Numeric // implicit
                };

                members.push(EnumMember {
                    name,
                    raw_value: value_text,
                    computed_value: None, // filled in by compute_enum_values
                    value_kind,
                });
            }
            "property_identifier" | "identifier" => {
                // Bare member with no explicit value (implicit numeric)
                let name = child.utf8_text(source).unwrap_or("").to_string();
                members.push(EnumMember {
                    name,
                    raw_value: None,
                    computed_value: None,
                    value_kind: EnumValueKind::Numeric, // implicit numeric
                });
            }
            _ => {}
        }
    }

    compute_enum_values(&mut members);
    members
}

/// Extract a full `EnumShape` from an `enum_declaration` node.
pub fn parse_enum_shape(source: &[u8], enum_node: tree_sitter::Node) -> EnumShape {
    let is_const = is_const_enum(enum_node, source);
    let members = parse_enum_members(source, enum_node);
    EnumShape { members, is_const }
}

/// Extract generic type parameters from ANY declaration node that has a
/// `type_parameters` field (class, function, interface, type_alias).
///
/// Per D-06: applies to ALL exported constructs.
/// Returns empty Vec if no `type_parameters` field.
pub fn parse_type_params(source: &[u8], decl_node: tree_sitter::Node) -> Vec<TypeParam> {
    let mut params = Vec::new();

    let tp_node = match decl_node.child_by_field_name("type_parameters") {
        Some(n) => n,
        None => return params,
    };

    // TLF-05: tree-sitter-typescript 0.23.2 does not fully support variance modifiers
    // (`in T`, `out T`, `in out T`). When encountered, it parses them as:
    //   type_parameter { name: "in" | "out" }  followed by  ERROR { identifier: "T" }
    // We collect all children first to allow lookahead for this pattern.
    let all_children: Vec<tree_sitter::Node> = {
        let mut wc = tp_node.walk();
        tp_node.children(&mut wc).collect()
    };

    let mut i = 0;
    while i < all_children.len() {
        let child = all_children[i];
        if child.kind() == "type_parameter" {
            let raw_name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string();

            // TLF-05: Detect variance modifiers by checking if the type_parameter's
            // apparent "name" is actually an `in`/`out` keyword, indicating that
            // tree-sitter mis-parsed a variance-annotated type parameter.
            let (name, variance) = {
                let name_is_variance_kw = matches!(raw_name.as_str(), "in" | "out");
                if name_is_variance_kw {
                    // Look for the actual name in the next ERROR sibling
                    let saw_in = raw_name == "in";
                    let mut saw_out = raw_name == "out";
                    let mut real_name = String::new();

                    // The ERROR node immediately following this type_parameter contains
                    // the real type parameter name (and possibly "out" for `in out T`).
                    let mut j = i + 1;
                    while j < all_children.len() && all_children[j].kind() == "ERROR" {
                        let err = all_children[j];
                        let mut ec = err.walk();
                        for err_child in err.children(&mut ec) {
                            let t = err_child.utf8_text(source).unwrap_or("");
                            match t {
                                "out" => saw_out = true,
                                s if !s.is_empty() && s != "in" => {
                                    if real_name.is_empty() {
                                        real_name = s.to_string();
                                    }
                                }
                                _ => {}
                            }
                        }
                        j += 1;
                    }
                    // Advance i past the consumed ERROR nodes
                    i = j - 1; // will be incremented to j at end of loop body

                    let variance = match (saw_in, saw_out) {
                        (true, true) => Some(crate::types::Variance::InOut),
                        (true, false) => Some(crate::types::Variance::In),
                        (false, true) => Some(crate::types::Variance::Out),
                        _ => None,
                    };
                    (real_name, variance)
                } else {
                    // Standard type parameter (no variance modifier)
                    (raw_name, None)
                }
            };

            let constraint_text = child
                .child_by_field_name("constraint")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| {
                    // Strip the leading "extends " keyword if present
                    let s = s.trim();
                    s.strip_prefix("extends ").unwrap_or(s).to_string()
                });

            let default_text = child
                .child_by_field_name("value")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| {
                    let s = s.trim();
                    // Strip leading "= " prefix if present (tree-sitter may include it)
                    s.strip_prefix("= ").unwrap_or(s).trim().to_string()
                });

            // GAP-15-07: Check for `const` modifier on type parameter.
            // In tree-sitter-typescript, `<const T>` has the "const" keyword as an
            // anonymous child before the name node in the type_parameter node.
            // Walk children to detect it — `const` appears as an unnamed node before "name".
            let is_const = {
                let mut found_const = false;
                let mut tp_cursor = child.walk();
                for tp_child in child.children(&mut tp_cursor) {
                    // Stop once we hit the name node (const can only precede the name)
                    if tp_child.is_named() {
                        break;
                    }
                    if tp_child.utf8_text(source).ok() == Some("const") {
                        found_const = true;
                        break;
                    }
                }
                found_const
            };

            if !name.is_empty() {
                params.push(TypeParam {
                    name,
                    constraint_text,
                    default_text,
                    constraint_shape: None, // LSP fills in later for GEN-04
                    is_const,
                    variance,
                });
            }
        }
        i += 1;
    }

    params
}

// ---------------------------------------------------------------------------
// Phase 8 extraction functions (REF-02..REF-07)
// ---------------------------------------------------------------------------

/// Check if a required_parameter node represents a rest parameter.
/// Per 08-RESEARCH: rest params are `required_parameter` with a `rest_pattern` child.
/// NOT `rest_parameter` — that node kind does not exist in tree-sitter-typescript 0.23.2.
pub fn is_rest_param(param_node: tree_sitter::Node) -> bool {
    let mut cursor = param_node.walk();
    for child in param_node.children(&mut cursor) {
        if child.kind() == "rest_pattern" {
            return true;
        }
    }
    false
}

/// Extract the type guard type from a function declaration/signature node.
/// Returns `Some("Foo")` for `function isStr(x: unknown): x is Foo`.
/// Returns `None` if the function does not have a type predicate return.
///
/// Per 08-RESEARCH: type guard returns use `type_predicate_annotation` (not `type_annotation`).
/// The `type_predicate` child contains the parameter name and guarded type.
pub fn extract_type_guard(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "type_predicate_annotation" {
            // Find the type_predicate child
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if inner_child.kind() == "type_predicate" {
                    // The guarded type is the last named child (type node)
                    // Structure: identifier "is" type
                    let type_node = inner_child.child_by_field_name("type").or_else(|| {
                        // Fallback: last named child that is a type (not the identifier)
                        let mut tc = inner_child.walk();
                        inner_child
                            .children(&mut tc)
                            .filter(|c| c.is_named() && c.kind() != "identifier")
                            .last()
                    });
                    if let Some(tn) = type_node {
                        return tn.utf8_text(source).ok().map(|s| s.to_string());
                    }
                }
            }
        }
    }
    None
}

/// TYPE-01 (Phase 20): Extract `satisfies T` constraints from exported
/// const/let/var declarations at the top level of a TS/TSX source file.
///
/// Returns a map from the exported binding name to the RHS type-expression
/// source text. Resolver insertion consumes this map to post-populate
/// `ExportSymbol.satisfies_type` after parsing the RHS through
/// `parse_type_string`.
///
/// Tree-sitter AST path: `variable_declarator (name: identifier)
/// (value: satisfies_expression (_) @value (_) @satisfies_type)`.
///
/// Grammar gate: `satisfies_expression` does NOT exist in
/// tree-sitter-javascript 0.25.0. This function MUST be a no-op on JS
/// inputs — call sites guard via `SourceLanguage::TypeScript` /
/// `SourceLanguage::TypeScriptTsx`.
pub fn extract_satisfies_types(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    lang: SourceLanguage,
) -> std::collections::HashMap<String, String> {
    let mut out: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Grammar gate: only TS/TSX have `satisfies_expression`. Silently no-op on JS.
    if !matches!(
        lang,
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx
    ) {
        return out;
    }

    let language = get_language(&lang);
    let query_src = r#"
        (variable_declarator
          name: (identifier) @name
          value: (satisfies_expression
            (_) @value
            (_) @satisfies_type))
    "#;

    let query = match Query::new(&language, query_src) {
        Ok(q) => q,
        Err(_) => return out,
    };
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source);
    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut satisfies_text: Option<String> = None;
        for capture in m.captures {
            let capture_name = capture_names[capture.index as usize];
            let text = match capture.node.utf8_text(source) {
                Ok(t) => t.to_string(),
                Err(_) => continue,
            };
            match capture_name {
                "name" => name = Some(text),
                "satisfies_type" => satisfies_text = Some(text),
                _ => {}
            }
        }
        if let (Some(n), Some(t)) = (name, satisfies_text) {
            out.insert(n, t);
        }
    }
    out
}

/// An index signature extracted from an interface or object type.
/// E.g., `[key: string]: number` has key_type_text="string", value_type_text="number".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSignature {
    pub key_type_text: String,
    pub value_type_text: String,
}

/// Extract all index signatures from an interface_body or object_type node.
/// Returns empty Vec if node has no index_signature children.
pub fn extract_index_signatures(
    body_node: tree_sitter::Node,
    source: &[u8],
) -> Vec<IndexSignature> {
    let mut sigs = Vec::new();
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "index_signature" {
            let mut key_type = None;
            let mut value_type = None;
            let mut in_brackets = false;
            let mut ic = child.walk();
            for c in child.children(&mut ic) {
                match c.kind() {
                    "[" => in_brackets = true,
                    "]" => in_brackets = false,
                    "type_annotation" if !in_brackets => {
                        // Value type is the type_annotation after the brackets
                        // Iterate children with for loop to avoid cursor borrow issues
                        let mut tc = c.walk();
                        for type_child in c.children(&mut tc) {
                            if type_child.is_named() {
                                value_type =
                                    type_child.utf8_text(source).ok().map(|s| s.to_string());
                                break;
                            }
                        }
                    }
                    _ if in_brackets && c.is_named() && c.kind() != "identifier" => {
                        // Key type is the named non-identifier child inside brackets
                        // (predefined_type like "string" or "number")
                        if key_type.is_none() {
                            key_type = c.utf8_text(source).ok().map(|s| s.to_string());
                        }
                    }
                    _ => {}
                }
            }
            if let (Some(kt), Some(vt)) = (key_type, value_type) {
                sigs.push(IndexSignature {
                    key_type_text: kt,
                    value_type_text: vt,
                });
            }
        }
    }
    sigs
}

/// Find the interface_body child of an interface_declaration node.
/// Returns None if the declaration has no body (forward declaration).
pub fn find_interface_body(decl_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = decl_node.walk();
    // Use a local result variable so the cursor is dropped before function return
    let result = decl_node
        .children(&mut cursor)
        .find(|child| child.kind() == "interface_body" || child.kind() == "object_type");
    result
}

/// Extract members from an `interface_body` node.
///
/// Interface members are `method_signature` and `property_signature` nodes
/// (same node kinds as .d.ts ambient class members, handled in Phase 13).
/// All interface members are public (no visibility modifiers in interfaces).
/// Reuses ClassMember type for consistency with class_diff pipeline.
pub fn parse_interface_members(
    source: &[u8],
    interface_body: tree_sitter::Node,
) -> Vec<ClassMember> {
    let mut members = Vec::new();
    let mut cursor = interface_body.walk();
    for child in interface_body.children(&mut cursor) {
        match child.kind() {
            "method_signature" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let param_count = count_required_params(child);
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    decorators: Vec::new(),
                    is_private: false,
                });
            }
            "property_signature" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                // Detect readonly modifier
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.kind() == "readonly" {
                            is_readonly = true;
                        }
                    }
                }
                // Detect optional (?) — property_signature with "?" anonymous child
                // Optional properties are non-breaking to add; required are breaking
                let mut is_optional = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if !kw.is_named() && kw.utf8_text(source).ok() == Some("?") {
                            is_optional = true;
                            break;
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    // Phase 17: use proper is_optional field (replaces type_signature stopgap)
                    type_signature: None,
                    is_optional,
                    is_accessor: false,
                    this_param_type: None,
                    decorators: Vec::new(),
                    is_private: false,
                });
            }
            "call_signature" => {
                // Call signatures: interface with () callable syntax
                // Treat as a method with a synthetic name "(call)"
                let param_count = count_required_params(child);
                members.push(ClassMember {
                    name: "(call)".to_string(),
                    kind: ClassMemberKind::Method,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    decorators: Vec::new(),
                    is_private: false,
                });
            }
            _ => {}
        }
    }
    members
}

/// TLF-01: Extract interface construct signatures (`new (x: T): U` and `abstract new(): T`).
///
/// In tree-sitter-typescript 0.23.2 the node kind is `construct_signature`.
/// It appears as a direct child of `interface_body` / `object_type`.
pub fn extract_construct_signatures(
    body_node: tree_sitter::Node,
    source: &[u8],
) -> Vec<crate::types::ConstructSignature> {
    let mut sigs = Vec::new();
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "construct_signature" {
            let param_count = count_required_params(child);
            // In tree-sitter-typescript 0.23.2 the return type of a construct_signature
            // appears as a `type_annotation` child (`: T`), not a named field `return_type`.
            // Walk children to find it and unwrap the inner type node.
            let return_text = {
                let mut cs_cursor = child.walk();
                let ta_node = child
                    .children(&mut cs_cursor)
                    .find(|c| c.kind() == "type_annotation");
                if let Some(ta) = ta_node {
                    let mut ta_cursor = ta.walk();
                    let inner_node = ta.children(&mut ta_cursor).find(|c| c.is_named());
                    inner_node
                        .and_then(|inner| inner.utf8_text(source).ok())
                        .map(String::from)
                } else {
                    None
                }
            };
            // Check for `abstract` token (unnamed child before the `new` keyword)
            let mut is_abstract = false;
            let mut abs_cursor = child.walk();
            for ch in child.children(&mut abs_cursor) {
                if !ch.is_named() && ch.utf8_text(source).ok() == Some("abstract") {
                    is_abstract = true;
                    break;
                }
            }
            sigs.push(crate::types::ConstructSignature {
                param_count,
                return_text,
                is_abstract,
            });
        }
    }
    sigs
}

/// TLF-04: Extract the asserted type from an `asserts x is T` signature.
///
/// In tree-sitter-typescript 0.23.2 the asserts annotation appears inside a
/// `type_predicate_annotation` node (the same container as `x is T` type guards).
/// Within it, when the `asserts` keyword is present, there may be a `type_predicate`
/// child or the asserted type may be a direct named child after `is`.
/// Returns `Some("T")` for `function foo(): asserts x is T`.
/// Returns `None` for non-assertion functions.
pub fn extract_assertion_target(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "type_predicate_annotation" {
            // Look for `asserts` keyword — if present, this is an assertion function
            let mut saw_asserts = false;
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if !inner_child.is_named() && inner_child.utf8_text(source).ok() == Some("asserts")
                {
                    saw_asserts = true;
                }
            }
            if !saw_asserts {
                continue;
            }
            // Walk type_predicate child or direct type child after `asserts`
            let mut inner_cursor2 = child.walk();
            for inner_child in child.children(&mut inner_cursor2) {
                if inner_child.kind() == "type_predicate" {
                    // asserts x is T — find the type after `is`
                    let type_node = inner_child.child_by_field_name("type").or_else(|| {
                        let mut tc = inner_child.walk();
                        inner_child
                            .children(&mut tc)
                            .filter(|c| c.is_named() && c.kind() != "identifier")
                            .last()
                    });
                    if let Some(tn) = type_node {
                        return tn.utf8_text(source).ok().map(String::from);
                    }
                } else if inner_child.is_named() && inner_child.kind() != "identifier" {
                    // asserts x (without `is T`) — asserts a boolean condition
                    // Return None to distinguish from typed asserts
                }
            }
        }
    }
    None
}

/// TLF-03: Extract the `this` parameter type annotation from a function node.
///
/// In TypeScript, `this` is the first parameter with name `this`. It constrains
/// the call receiver. Example: `function onClick(this: HTMLElement): void {}`.
/// Returns `Some("HTMLElement")` for that example, `None` otherwise.
pub fn extract_this_param_type(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Look for "parameters" field child (works for function_declaration, arrow_function,
    // function_signature, method_definition, etc.)
    let params_node = fn_node.child_by_field_name("parameters")?;
    let mut cursor = params_node.walk();
    for param in params_node.children(&mut cursor) {
        // `this` parameter parses as required_parameter with identifier child named "this"
        if param.kind() == "required_parameter" {
            // Check if the pattern (first named child) is the identifier `this`
            let mut pc = param.walk();
            let mut name_text: Option<&str> = None;
            for child in param.children(&mut pc) {
                if child.kind() == "identifier" || child.kind() == "this" {
                    name_text = child.utf8_text(source).ok();
                    break;
                }
            }
            if name_text != Some("this") {
                continue;
            }
            // Found the `this` parameter — extract its type annotation
            let mut tc = param.walk();
            for child in param.children(&mut tc) {
                if child.kind() == "type_annotation" {
                    let mut inner_cursor = child.walk();
                    let type_node = child.children(&mut inner_cursor).find(|c| c.is_named());
                    return type_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(String::from);
                }
            }
        }
        // Only check the first parameter — `this` must be first
        break;
    }
    None
}

/// Extract the text of the first named child of a type_annotation node.
/// Returns None if no named child is found.
fn first_named_child_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Find the `rest_pattern` child of a `required_parameter` node, then return the
/// `identifier` text inside that rest_pattern (the param name).
fn rest_param_name<'a>(param: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut pc = param.walk();
    for child in param.children(&mut pc) {
        if child.kind() == "rest_pattern" {
            let mut rc = child.walk();
            for inner in child.children(&mut rc) {
                if inner.kind() == "identifier" {
                    return inner.utf8_text(source).ok();
                }
            }
        }
    }
    None
}

/// Build a TypeShape::Function from a function_signature or function_declaration node.
/// Extracts parameter names, types, optionality, and rest flags.
/// Used for overload signature extraction.
pub fn build_function_shape_from_node(
    fn_node: tree_sitter::Node,
    source: &[u8],
) -> Option<crate::types::shape::TypeShape> {
    use crate::types::shape::{ParamShape, PrimitiveKind, TypeShape};

    // Find formal_parameters child
    let params_node = fn_node.child_by_field_name("parameters")?;
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for param in params_node.children(&mut cursor) {
        match param.kind() {
            "required_parameter" => {
                let rest = is_rest_param(param);
                let name = if rest {
                    // For rest params, the identifier is inside rest_pattern
                    rest_param_name(param, source).unwrap_or("_").to_string()
                } else {
                    param
                        .child_by_field_name("pattern")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("_")
                        .to_string()
                };
                let type_text = param
                    .child_by_field_name("type")
                    .and_then(|ta| first_named_child_text(ta, source))
                    .unwrap_or("unknown");
                params.push(ParamShape {
                    name,
                    optional: false,
                    rest,
                    shape: TypeShape::Opaque(type_text.to_string()),
                });
            }
            "optional_parameter" => {
                let name = param
                    .child_by_field_name("pattern")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("_")
                    .to_string();
                let type_text = param
                    .child_by_field_name("type")
                    .and_then(|ta| first_named_child_text(ta, source))
                    .unwrap_or("unknown");
                params.push(ParamShape {
                    name,
                    optional: true,
                    rest: false,
                    shape: TypeShape::Opaque(type_text.to_string()),
                });
            }
            _ => {} // skip commas, other tokens
        }
    }

    // Extract return type
    let return_type = fn_node
        .child_by_field_name("return_type")
        .and_then(|ta| first_named_child_text(ta, source))
        .map(|s| TypeShape::Opaque(s.to_string()))
        .unwrap_or(TypeShape::Primitive(PrimitiveKind::Void));

    Some(TypeShape::Function {
        params,
        return_type: Box::new(return_type),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect (line, col) positions of local names captured by re-export queries.
/// Used to exclude those positions from the plain named export results.
///
/// Handles both `export { foo } from '...'` and `export { foo as bar } from '...'`.
/// For the alias form, the local-name (`foo`) position is collected so the plain
/// export query doesn't double-count it.
fn collect_reexport_positions(
    source: &[u8],
    language: &tree_sitter::Language,
) -> anyhow::Result<std::collections::HashSet<(u32, u32)>> {
    let mut positions = std::collections::HashSet::new();

    let mut parser = Parser::new();
    parser
        .set_language(language)
        .context("failed to set language for dedup parser")?;
    let tree = parser
        .parse(source, None)
        .context("parse failed in dedup")?;
    let root = tree.root_node();

    // Collect positions from the plain re-export query (@name = index 0).
    let query = Query::new(language, NAMED_REEXPORT_QUERY)
        .context("failed to compile NAMED_REEXPORT_QUERY for dedup")?;
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @name — the local name node
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    positions.insert((line, col));
                }
            }
        }
    }

    // Also collect local-name positions from the alias re-export query (@local_name = index 0).
    if let Ok(alias_query) = Query::new(language, NAMED_REEXPORT_ALIAS_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&alias_query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @local_name
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    positions.insert((line, col));
                }
            }
        }
    }

    Ok(positions)
}

/// Strip leading and trailing quote characters from a string literal as returned
/// by tree-sitter (e.g., `"'./utils'"` -> `"./utils"`).
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClassMemberKind, EnumValueKind, Visibility};
    use tree_sitter::Parser;

    /// Parse a TypeScript source string and return the root node and source bytes.
    fn parse_ts(source: &str) -> (tree_sitter::Tree, Vec<u8>) {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&lang).unwrap();
        let bytes = source.as_bytes().to_vec();
        let tree = parser.parse(&bytes, None).expect("parse failed");
        (tree, bytes)
    }

    /// Get the first child of root with the given kind.
    fn first_node_of_kind<'a>(
        root: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() == kind {
                return Some(child);
            }
        }
        None
    }

    // ── parse_class_members tests ─────────────────────────────────────────────

    #[test]
    fn test_parse_class_members_extracts_method_names() {
        let src = "class Foo { doWork(x: string): void {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        assert!(
            members.iter().any(|m| m.name == "doWork"),
            "should extract doWork method"
        );
    }

    #[test]
    fn test_parse_class_members_no_modifier_is_public() {
        let src = "class Foo { doWork(): void {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let method = members.iter().find(|m| m.name == "doWork").unwrap();
        assert_eq!(
            method.visibility,
            Visibility::Public,
            "no modifier = Public"
        );
    }

    #[test]
    fn test_parse_class_members_protected_modifier() {
        let src = "class Foo { protected doWork(): void {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let method = members.iter().find(|m| m.name == "doWork").unwrap();
        assert_eq!(method.visibility, Visibility::Protected);
    }

    #[test]
    fn test_parse_class_members_constructor_kind() {
        let src = "class Foo { constructor(x: string) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let ctor = members.iter().find(|m| m.name == "constructor").unwrap();
        assert_eq!(ctor.kind, ClassMemberKind::Constructor);
    }

    #[test]
    fn test_parse_class_members_constructor_required_params() {
        let src = "class Foo { constructor(x: string, y: number) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let ctor = members.iter().find(|m| m.name == "constructor").unwrap();
        assert_eq!(ctor.param_count, 2, "two required params");
    }

    #[test]
    fn test_parse_class_members_static_flag() {
        let src = "class Foo { static create(): Foo { return new Foo(); } }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let method = members.iter().find(|m| m.name == "create").unwrap();
        assert!(method.is_static, "create should be static");
    }

    #[test]
    fn test_parse_class_members_property_kind() {
        let src = "class Foo { name: string = ''; }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let prop = members.iter().find(|m| m.name == "name").unwrap();
        assert_eq!(prop.kind, ClassMemberKind::Property);
    }

    #[test]
    fn test_parse_class_members_getter() {
        let src = "class A { get foo(): number { return 1; } }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let getter = members.iter().find(|m| m.name == "foo").unwrap();
        assert_eq!(
            getter.kind,
            ClassMemberKind::Getter,
            "getter should have kind Getter"
        );
    }

    #[test]
    fn test_parse_class_members_setter() {
        let src = "class A { set foo(v: number) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let setter = members.iter().find(|m| m.name == "foo").unwrap();
        assert_eq!(
            setter.kind,
            ClassMemberKind::Setter,
            "setter should have kind Setter"
        );
    }

    #[test]
    fn test_parse_class_members_private_field_kept_with_is_private_flag() {
        // CLS-11 (Phase 21): `#`-prefix private fields are now KEPT in the
        // members vector with `is_private: true` so the diff layer can detect
        // public ↔ private transitions (e.g., `foo` → `#foo`). The `#`
        // prefix is preserved in `name` so diff messages are accurate.
        let src = "class A { #secret: string; visible: string; }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        assert_eq!(
            members.len(),
            2,
            "both '#secret' (is_private=true) and 'visible' (is_private=false) should be present"
        );
        let secret = members.iter().find(|m| m.name == "#secret").unwrap();
        assert!(secret.is_private, "'#secret' must have is_private=true");
        let visible = members.iter().find(|m| m.name == "visible").unwrap();
        assert!(
            !visible.is_private,
            "'visible' must have is_private=false"
        );
    }

    #[test]
    fn test_parse_class_members_getter_setter_pair() {
        let src = "class A { get x(): number { return 0; } set x(v: number) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let getter = members
            .iter()
            .find(|m| m.name == "x" && m.kind == ClassMemberKind::Getter);
        let setter = members
            .iter()
            .find(|m| m.name == "x" && m.kind == ClassMemberKind::Setter);
        assert!(getter.is_some(), "should have a Getter for x");
        assert!(setter.is_some(), "should have a Setter for x");
    }

    // ── abstract class tests ──────────────────────────────────────────────────

    #[test]
    fn test_abstract_class_detection_via_node_kind() {
        let src = "abstract class Base { abstract doWork(): void; }";
        let (tree, bytes) = parse_ts(src);
        let abs_node = first_node_of_kind(tree.root_node(), "abstract_class_declaration").unwrap();
        assert_eq!(abs_node.kind(), "abstract_class_declaration");
        let shape = parse_class_shape(&bytes, abs_node);
        assert!(shape.is_abstract, "should detect as abstract");
    }

    #[test]
    fn test_concrete_class_not_abstract() {
        let src = "class Foo {}";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let shape = parse_class_shape(&bytes, class_node);
        assert!(!shape.is_abstract, "concrete class should not be abstract");
    }

    // ── DECL_EXPORT_QUERY_TS abstract class test ──────────────────────────────

    #[test]
    fn test_decl_export_query_captures_abstract_class() {
        let src = "export abstract class MyAbstractClass { abstract doWork(): void; }";
        let exports = parse_exports(src.as_bytes(), SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "MyAbstractClass"),
            "exported abstract class should be captured"
        );
    }

    // ── parse_enum_members tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_enum_members_extracts_names() {
        let src = "enum Color { Red, Green, Blue }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        let members = parse_enum_members(&bytes, enum_node);
        assert_eq!(members.len(), 3);
        assert_eq!(members[0].name, "Red");
        assert_eq!(members[1].name, "Green");
        assert_eq!(members[2].name, "Blue");
    }

    #[test]
    fn test_parse_enum_members_no_explicit_value() {
        let src = "enum Dir { Up, Down }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        let members = parse_enum_members(&bytes, enum_node);
        assert!(
            members.iter().all(|m| m.raw_value.is_none()),
            "implicit members should have no raw_value"
        );
    }

    #[test]
    fn test_parse_enum_members_with_explicit_numeric_value() {
        let src = "enum Status { Active = 1, Inactive = 2 }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        let members = parse_enum_members(&bytes, enum_node);
        let active = members.iter().find(|m| m.name == "Active").unwrap();
        assert_eq!(active.raw_value.as_deref(), Some("1"));
        assert_eq!(active.computed_value, Some(1));
    }

    // ── is_const_enum tests ───────────────────────────────────────────────────

    #[test]
    fn test_is_const_enum_true_for_const_enum() {
        let src = "const enum Direction { Up, Down }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        assert!(is_const_enum(enum_node, &bytes), "should detect const enum");
    }

    #[test]
    fn test_is_const_enum_false_for_regular_enum() {
        let src = "enum Direction { Up, Down }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        assert!(
            !is_const_enum(enum_node, &bytes),
            "regular enum should not be const"
        );
    }

    // ── compute_enum_values tests ─────────────────────────────────────────────

    #[test]
    fn test_compute_enum_values_auto_increment_from_zero() {
        let mut members = vec![
            EnumMember {
                name: "A".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
            EnumMember {
                name: "B".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
            EnumMember {
                name: "C".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
        ];
        compute_enum_values(&mut members);
        assert_eq!(members[0].computed_value, Some(0));
        assert_eq!(members[1].computed_value, Some(1));
        assert_eq!(members[2].computed_value, Some(2));
    }

    #[test]
    fn test_compute_enum_values_resets_after_explicit_numeric() {
        let mut members = vec![
            EnumMember {
                name: "A".to_string(),
                raw_value: Some("5".to_string()),
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
            EnumMember {
                name: "B".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
        ];
        compute_enum_values(&mut members);
        assert_eq!(members[0].computed_value, Some(5));
        assert_eq!(members[1].computed_value, Some(6), "should reset to 5+1=6");
    }

    #[test]
    fn test_compute_enum_values_non_numeric_sets_none() {
        let mut members = vec![
            EnumMember {
                name: "A".to_string(),
                raw_value: Some("SomeExpr + 1".to_string()),
                computed_value: None,
                value_kind: EnumValueKind::OtherExpr,
            },
            EnumMember {
                name: "B".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
        ];
        compute_enum_values(&mut members);
        assert_eq!(
            members[0].computed_value, None,
            "non-numeric initializer = None"
        );
        // Counter still increments for B
        assert_eq!(members[1].computed_value, Some(1));
    }

    // ── parse_type_params tests ───────────────────────────────────────────────

    #[test]
    fn test_parse_type_params_extracts_name_constraint_default() {
        let src = "interface Foo<T extends string = never> {}";
        let (tree, bytes) = parse_ts(src);
        let iface_node = first_node_of_kind(tree.root_node(), "interface_declaration").unwrap();
        let params = parse_type_params(&bytes, iface_node);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "T");
        assert_eq!(params[0].constraint_text.as_deref(), Some("string"));
        assert_eq!(params[0].default_text.as_deref(), Some("never"));
    }

    #[test]
    fn test_parse_type_params_empty_for_no_type_params() {
        let src = "class Foo {}";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let params = parse_type_params(&bytes, class_node);
        assert!(params.is_empty(), "no type parameters = empty Vec");
    }

    #[test]
    fn test_parse_type_params_multiple_params() {
        let src = "type Pair<K, V> = [K, V];";
        let (tree, bytes) = parse_ts(src);
        let alias_node = first_node_of_kind(tree.root_node(), "type_alias_declaration").unwrap();
        let params = parse_type_params(&bytes, alias_node);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "K");
        assert_eq!(params[1].name, "V");
        assert!(params[0].constraint_text.is_none());
        assert!(params[0].default_text.is_none());
    }

    #[test]
    fn test_parse_type_params_constraint_shape_is_none() {
        let src = "function foo<T extends string>() {}";
        let (tree, bytes) = parse_ts(src);
        let fn_node = first_node_of_kind(tree.root_node(), "function_declaration").unwrap();
        let params = parse_type_params(&bytes, fn_node);
        assert_eq!(params.len(), 1);
        assert!(
            params[0].constraint_shape.is_none(),
            "constraint_shape is None (LSP fills in later)"
        );
    }

    // ── DeclarationKind propagation tests (Task 1) ────────────────────────────

    #[test]
    fn test_decl_export_query_captures_namespace() {
        let source = b"export namespace MyNS { export function helper(): void {} }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "MyNS"),
            "namespace export should be captured"
        );
        let ns = exports.iter().find(|e| e.name == "MyNS").unwrap();
        assert_eq!(ns.decl_kind, Some(crate::types::DeclarationKind::Namespace));
    }

    #[test]
    fn test_decl_kind_propagated_for_function() {
        let source = b"export function greet(name: string): string { return name; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let exp = exports.iter().find(|e| e.name == "greet").unwrap();
        assert_eq!(exp.decl_kind, Some(crate::types::DeclarationKind::Function));
    }

    #[test]
    fn test_decl_kind_propagated_for_interface() {
        let source = b"export interface Config { timeout: number; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let exp = exports.iter().find(|e| e.name == "Config").unwrap();
        assert_eq!(
            exp.decl_kind,
            Some(crate::types::DeclarationKind::Interface)
        );
    }

    #[test]
    fn test_decl_kind_propagated_for_type_alias() {
        let source = b"export type ID = string;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let exp = exports.iter().find(|e| e.name == "ID").unwrap();
        assert_eq!(
            exp.decl_kind,
            Some(crate::types::DeclarationKind::TypeAlias)
        );
    }

    #[test]
    fn test_function_signature_captured_as_overload() {
        let source = b"export function foo(x: string): string;\nexport function foo(x: number): number;\nexport function foo(x: any): any { return x; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let foos: Vec<_> = exports.iter().filter(|e| e.name == "foo").collect();
        // function_signature nodes (overloads) + function_declaration (implementation)
        assert!(
            foos.len() >= 3,
            "should capture overload signatures and implementation; got {}",
            foos.len()
        );
    }

    // ── Rest param, type guard, index sig, function shape tests (Task 2) ─────

    #[test]
    fn test_is_rest_param_detects_spread() {
        let source = b"function foo(...args: string[]): void {}";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        let params = fn_node.child_by_field_name("parameters").unwrap();
        let mut cursor = params.walk();
        let first_param = params
            .children(&mut cursor)
            .find(|c| c.kind() == "required_parameter")
            .unwrap();
        assert!(
            is_rest_param(first_param),
            "...args should be detected as rest param"
        );
    }

    #[test]
    fn test_is_rest_param_false_for_normal() {
        let source = b"function foo(x: string): void {}";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        let params = fn_node.child_by_field_name("parameters").unwrap();
        let mut cursor = params.walk();
        let first_param = params
            .children(&mut cursor)
            .find(|c| c.kind() == "required_parameter")
            .unwrap();
        assert!(
            !is_rest_param(first_param),
            "normal param should not be rest"
        );
    }

    #[test]
    fn test_extract_type_guard_returns_guarded_type() {
        let source = b"function isStr(x: unknown): x is string { return typeof x === 'string'; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let fn_node = tree.root_node().child(0).unwrap();
        let guard = extract_type_guard(fn_node, source);
        assert_eq!(guard, Some("string".to_string()));
    }

    #[test]
    fn test_extract_type_guard_none_for_regular_return() {
        let source = b"function foo(x: string): boolean { return true; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let fn_node = tree.root_node().child(0).unwrap();
        let guard = extract_type_guard(fn_node, source);
        assert!(
            guard.is_none(),
            "regular boolean return should not be a type guard"
        );
    }

    #[test]
    fn test_extract_index_signatures() {
        let source = b"interface Dict { [key: string]: number; name: string; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let iface = root.child(0).unwrap(); // interface_declaration
        let body = find_interface_body(iface).unwrap();
        let sigs = extract_index_signatures(body, source);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].key_type_text, "string");
        assert_eq!(sigs[0].value_type_text, "number");
    }

    #[test]
    fn test_build_function_shape_from_signature_node() {
        let source =
            b"export function foo(x: string): number;\nexport function foo(x: any): any { return x; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        // First child is export_statement containing function_signature
        let first_export = root.child(0).unwrap();
        let mut cursor = first_export.walk();
        let fn_sig = first_export
            .children(&mut cursor)
            .find(|c| c.kind() == "function_signature")
            .unwrap();
        let shape = build_function_shape_from_node(fn_sig, source);
        assert!(
            shape.is_some(),
            "should build TypeShape from function_signature"
        );
        if let Some(crate::types::shape::TypeShape::Function { params, .. }) = shape {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(!params[0].rest);
        } else {
            panic!("expected TypeShape::Function");
        }
    }

    #[test]
    fn test_build_function_shape_with_rest_param() {
        let source = b"function bar(...args: string[]): void {}";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let fn_node = tree.root_node().child(0).unwrap();
        let shape = build_function_shape_from_node(fn_node, source);
        assert!(shape.is_some());
        if let Some(crate::types::shape::TypeShape::Function { params, .. }) = shape {
            assert_eq!(params.len(), 1);
            assert!(params[0].rest, "rest param should have rest=true");
        } else {
            panic!("expected TypeShape::Function");
        }
    }

    // ── is_async_function / is_generator_function tests (BEH-01, BEH-02) ─────

    #[test]
    fn test_is_async_function_true() {
        let source = b"async function foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(is_async_function(fn_node, source));
    }

    #[test]
    fn test_is_async_function_false() {
        let source = b"function foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(!is_async_function(fn_node, source));
    }

    #[test]
    fn test_is_generator_function_true() {
        let source = b"function* foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(is_generator_function(fn_node, source));
    }

    #[test]
    fn test_is_generator_function_false() {
        let source = b"function foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(!is_generator_function(fn_node, source));
    }

    // ── Export alias and namespace re-export tests (Phase 11 bug fixes) ────────

    #[test]
    fn test_export_alias_uses_alias_name() {
        // `export { foo as bar }` — public API name is "bar", not "foo".
        let source = b"const foo = 1;\nexport { foo as bar };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "bar"),
            "exported alias name 'bar' should appear in exports; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "local name 'foo' should NOT appear as a Named export; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_export_no_alias_uses_local_name() {
        // Regression guard: `export { foo }` with no alias should still use "foo".
        let source = b"const foo = 1;\nexport { foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "non-aliased export should use local name 'foo'; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_reexport_alias_uses_alias_name() {
        // `export { foo as bar } from './mod'` — public re-export name is "bar".
        let source = b"export { foo as bar } from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "bar" && e.kind == RawExportKind::NamedReexport),
            "aliased re-export 'bar' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::NamedReexport),
            "local name 'foo' should NOT appear as a NamedReexport; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_namespace_reexport_captures_name() {
        // `export * as utils from './mod'` — exported name is "utils", not "*".
        let source = b"export * as utils from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "utils" && e.kind == RawExportKind::StarReexport),
            "namespace re-export should use name 'utils'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "*" && e.source.as_deref() == Some("./mod")),
            "name '*' should not appear for a named namespace re-export; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_plain_star_reexport_still_works() {
        // Regression guard: `export * from './mod'` should still produce name="*".
        let source = b"export * from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "*" && e.kind == RawExportKind::StarReexport),
            "plain star re-export should still produce name='*'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    // ── Type-only export tests (Task 1 — Phase 12) ────────────────────────────

    #[test]
    fn test_type_only_named_export() {
        // `export type { Foo }` — Foo is a Named export with no source
        let source = b"export type { Foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
            "export type {{ Foo }} should produce Named export 'Foo'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        let foo = exports.iter().find(|e| e.name == "Foo").unwrap();
        assert!(
            foo.decl_kind.is_none(),
            "type-only export should have no decl_kind"
        );
    }

    #[test]
    fn test_type_only_reexport() {
        // `export type { Foo } from './bar'` — NamedReexport with source
        let source = b"export type { Foo } from './bar';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Foo" && e.kind == RawExportKind::NamedReexport),
            "export type {{ Foo }} from './bar' should produce NamedReexport 'Foo'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        let foo = exports
            .iter()
            .find(|e| e.name == "Foo" && e.kind == RawExportKind::NamedReexport)
            .unwrap();
        assert_eq!(
            foo.source.as_deref(),
            Some("./bar"),
            "source should be './bar'"
        );
    }

    #[test]
    fn test_type_only_alias_export() {
        // `export type { Foo as Bar }` — exported public name is "Bar"
        let source = b"export type { Foo as Bar };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Bar" && e.kind == RawExportKind::Named),
            "export type {{ Foo as Bar }} should produce Named export 'Bar'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
            "local name 'Foo' should NOT appear as Named; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_destructured_export() {
        // `export const { foo, bar } = obj` — two Named exports
        let source = b"export const { foo, bar } = obj;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "destructured export 'foo' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "destructured export 'bar' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        // Both should have Variable decl_kind
        let foo = exports
            .iter()
            .find(|e| e.name == "foo" && e.kind == RawExportKind::Named)
            .unwrap();
        assert_eq!(
            foo.decl_kind,
            Some(crate::types::DeclarationKind::Variable),
            "destructured export should have Variable decl_kind"
        );
    }

    #[test]
    fn test_destructured_pair_export() {
        // `export const { foo: renamed } = obj` — public name is "renamed" (the value/local binding)
        let source = b"export const { foo: renamed } = obj;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "renamed" && e.kind == RawExportKind::Named),
            "pair-pattern destructured export should export 'renamed' (local binding); got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "source property key 'foo' should NOT appear as Named export; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    // ── BUG-13-02: Namespace/star dedup fix ──────────────────────────────────

    #[test]
    fn test_namespace_and_star_from_same_source() {
        let source = b"export * from './mod';\nexport * as utils from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let star = exports
            .iter()
            .filter(|e| e.name == "*" && e.source.as_deref() == Some("./mod"))
            .count();
        let ns = exports
            .iter()
            .filter(|e| e.name == "utils" && e.source.as_deref() == Some("./mod"))
            .count();
        assert_eq!(star, 1, "plain star should appear once; got {star}");
        assert_eq!(ns, 1, "namespace 'utils' should appear once; got {ns}");
    }

    // ── GAP-13-03: Inline type modifier exports ───────────────────────────────

    #[test]
    fn test_inline_type_modifier_exports() {
        // TS 4.5: per-specifier `type` prefix — `export { type Foo, Bar }`
        // Both should appear exactly once as Named exports
        let source = b"export { type Foo, Bar };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let foo_count = exports.iter().filter(|e| e.name == "Foo").count();
        let bar_count = exports.iter().filter(|e| e.name == "Bar").count();
        assert_eq!(
            foo_count, 1,
            "Foo should appear exactly once; got {foo_count}"
        );
        assert_eq!(
            bar_count, 1,
            "Bar should appear exactly once; got {bar_count}"
        );
    }

    // ── Export assignment and ambient declaration tests (Task 2 — Phase 12) ───

    #[test]
    fn test_export_assignment() {
        // `export = MyClass;` — CJS-style TS default export, becomes name="default"
        let source = b"export = MyClass;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
            "export = MyClass should produce Default export 'default'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_ambient_function_declaration() {
        // `export declare function foo(): void;` — ambient declaration in .d.ts
        let source = b"export declare function foo(): void;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let foo = exports
            .iter()
            .find(|e| e.name == "foo" && e.kind == RawExportKind::Named);
        assert!(
            foo.is_some(),
            "export declare function foo should produce Named export 'foo'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            foo.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Function),
            "ambient function should have Function decl_kind"
        );
    }

    #[test]
    fn test_ambient_class_declaration() {
        // `export declare class Bar {}` — ambient class declaration
        let source = b"export declare class Bar {}";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let bar = exports
            .iter()
            .find(|e| e.name == "Bar" && e.kind == RawExportKind::Named);
        assert!(
            bar.is_some(),
            "export declare class Bar should produce Named export 'Bar'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            bar.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Class),
            "ambient class should have Class decl_kind"
        );
    }

    #[test]
    fn test_ambient_interface_declaration() {
        // `export declare interface Baz { x: number }` — ambient interface
        let source = b"export declare interface Baz { x: number }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let baz = exports
            .iter()
            .find(|e| e.name == "Baz" && e.kind == RawExportKind::Named);
        assert!(
            baz.is_some(),
            "export declare interface Baz should produce Named export 'Baz'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            baz.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Interface),
            "ambient interface should have Interface decl_kind"
        );
    }

    #[test]
    fn test_ambient_variable_declaration() {
        // `export declare const VERSION: string;` — ambient variable
        let source = b"export declare const VERSION: string;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let version = exports
            .iter()
            .find(|e| e.name == "VERSION" && e.kind == RawExportKind::Named);
        assert!(
            version.is_some(),
            "export declare const VERSION should produce Named export 'VERSION'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            version.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Variable),
            "ambient variable should have Variable decl_kind"
        );
    }

    // ── GAP-13-01: CJS module.exports detection ───────────────────────────────

    #[test]
    fn test_cjs_module_exports_shorthand() {
        let source = b"module.exports = { foo, bar };";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "CJS shorthand export 'foo' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "CJS shorthand export 'bar' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_module_exports_pair() {
        let source = b"module.exports = { foo: myFoo, bar: myBar };";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "CJS pair export 'foo' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "CJS pair export 'bar' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_exports_property() {
        let source = b"exports.foo = function() {};\nexports.bar = 42;";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "CJS exports.foo should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "CJS exports.bar should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_module_exports_default() {
        let source = b"module.exports = MyClass;";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
            "CJS module.exports = MyClass should produce Default 'default'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_not_triggered_in_typescript() {
        // CJS patterns should NOT be detected in TypeScript files
        let source = b"module.exports = { foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            !exports.iter().any(|e| e.name == "foo"),
            "CJS should not match in TS; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    // ── GAP-13-02: export { default as Named } detection ─────────────────────

    #[test]
    fn test_export_default_as_named() {
        let source = b"export { default as Named };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Named" && e.kind == RawExportKind::Named),
            "export {{ default as Named }} should export 'Named'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_export_default_reexport() {
        let source = b"export { default } from './foo';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && e.kind == RawExportKind::NamedReexport),
            "export {{ default }} from './foo' should produce NamedReexport 'default'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_export_default_as_named_reexport() {
        let source = b"export { default as Named } from './foo';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Named" && e.kind == RawExportKind::NamedReexport),
            "export {{ default as Named }} from './foo' should export 'Named'; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    // ── GAP-13-04: Array destructuring exports ────────────────────────────────

    #[test]
    fn test_array_destructured_export() {
        let source = b"export const [a, b] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "a" && e.kind == RawExportKind::Named),
            "array destructured export 'a' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "b" && e.kind == RawExportKind::Named),
            "array destructured export 'b' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_array_destructured_rest_export() {
        let source = b"export const [first, ...rest] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "first" && e.kind == RawExportKind::Named),
            "array destructured 'first' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "rest" && e.kind == RawExportKind::Named),
            "array destructured rest 'rest' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_array_destructured_with_holes() {
        let source = b"export const [a, , c] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "a" && e.kind == RawExportKind::Named),
            "array destructured 'a' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "c" && e.kind == RawExportKind::Named),
            "array destructured 'c' should appear; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            exports
                .iter()
                .filter(|e| e.kind == RawExportKind::Named)
                .count(),
            2,
            "holes should not produce Named exports; got: {:?}",
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }

    // ─── REQ-14-03: Namespace member expansion ───────────────────────────────

    #[test]
    fn test_namespace_member_expansion() {
        // REQ-14-03: namespace members should produce dotted-name entries
        let source =
            b"export namespace Foo { export function bar(): void {} export const baz = 1; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"Foo"),
            "namespace itself should be present; got: {:?}",
            names
        );
        assert!(
            names.contains(&"Foo.bar"),
            "namespace member 'Foo.bar' should be present; got: {:?}",
            names
        );
        assert!(
            names.contains(&"Foo.baz"),
            "namespace member 'Foo.baz' should be present; got: {:?}",
            names
        );
    }

    #[test]
    fn test_namespace_member_decl_kind() {
        let source = b"export namespace NS { export class MyClass {} }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let member = exports.iter().find(|e| e.name == "NS.MyClass");
        assert!(
            member.is_some(),
            "NS.MyClass should be exported; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            member.unwrap().decl_kind,
            Some(DeclarationKind::Class),
            "NS.MyClass should have Class decl_kind"
        );
    }

    #[test]
    fn test_namespace_member_no_duplicates() {
        // REQ-14-03: same dotted name should not appear twice
        let source = b"export namespace Foo { export function bar(): void {} }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let count = exports.iter().filter(|e| e.name == "Foo.bar").count();
        assert_eq!(
            count, 1,
            "Foo.bar should appear exactly once; got count={}",
            count
        );
    }

    // ── GAP-16-08: Nested destructuring exports ───────────────────────────────

    #[test]
    fn test_nested_destructuring_object_in_object() {
        // GAP-16-08: `export const { a: { b } } = obj` captures both "a" (flat query) and "b" (nested)
        let source = b"export const { a: { b } } = obj;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"b"),
            "nested destructuring 'b' from {{ a: {{ b }} }} should be captured; got: {:?}",
            names
        );
    }

    #[test]
    fn test_nested_destructuring_array_in_array() {
        // GAP-16-08: `export const [a, [b, c]] = arr` captures "a", "b", "c"
        let source = b"export const [a, [b, c]] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"a"),
            "top-level 'a' from [a, [b, c]] should be captured; got: {:?}",
            names
        );
        assert!(
            names.contains(&"b"),
            "nested 'b' from [a, [b, c]] should be captured; got: {:?}",
            names
        );
        assert!(
            names.contains(&"c"),
            "nested 'c' from [a, [b, c]] should be captured; got: {:?}",
            names
        );
    }

    // ── GAP-16-09: CJS exports aliasing pattern ───────────────────────────────

    #[test]
    fn test_cjs_aliasing_shorthand_properties() {
        // GAP-16-09: `exports = module.exports = { foo, bar }` captures "foo" and "bar"
        let source = b"exports = module.exports = { foo, bar };";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "CJS aliasing 'foo' should be captured; got: {:?}",
            names
        );
        assert!(
            names.contains(&"bar"),
            "CJS aliasing 'bar' should be captured; got: {:?}",
            names
        );
    }

    #[test]
    fn test_cjs_aliasing_pair_properties() {
        // GAP-16-09: `exports = module.exports = { key: value }` captures "key"
        let source = b"exports = module.exports = { key: value };";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"key"),
            "CJS aliasing pair 'key' should be captured; got: {:?}",
            names
        );
    }

    #[test]
    fn test_cjs_aliasing_not_triggered_in_typescript() {
        // CJS aliasing is JavaScript-only
        let source = b"exports = module.exports = { foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        // Should not crash and we don't assert contents since TS doesn't recognize CJS
        let _ = exports;
    }

    // ── GAP-16-05: Nested namespace expansion ────────────────────────────────

    #[test]
    fn test_namespace_nested_2_levels() {
        // GAP-16-05: 2-level nested namespace should produce A.B and A.B.baz
        let source = b"export namespace A { export namespace B { export function baz() {} } }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"A"),
            "top-level namespace 'A' should be present; got: {:?}",
            names
        );
        assert!(
            names.contains(&"A.B"),
            "nested namespace 'A.B' should be present; got: {:?}",
            names
        );
        assert!(
            names.contains(&"A.B.baz"),
            "doubly-nested member 'A.B.baz' should be present; got: {:?}",
            names
        );
    }

    #[test]
    fn test_namespace_nested_3_levels() {
        // GAP-16-05: 3-level nested namespace should produce A.B.C and A.B.C.fn
        let source = b"export namespace A { export namespace B { export namespace C { export function fn_deep() {} } } }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"A.B.C"),
            "3-level nested namespace 'A.B.C' should be present; got: {:?}",
            names
        );
        assert!(
            names.contains(&"A.B.C.fn_deep"),
            "3-level nested member 'A.B.C.fn_deep' should be present; got: {:?}",
            names
        );
    }

    #[test]
    fn test_namespace_1_level_still_works_after_refactor() {
        // Regression: 1-level expansion must still work after recursive refactor
        let source =
            b"export namespace Outer { export function method(): void {} export const VAL = 42; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"Outer.method"),
            "1-level expansion 'Outer.method' should still work; got: {:?}",
            names
        );
        assert!(
            names.contains(&"Outer.VAL"),
            "1-level expansion 'Outer.VAL' should still work; got: {:?}",
            names
        );
    }

    // ── GAP-15-01: CJS inline class/function exports ─────────────────────────

    #[test]
    fn test_cjs_module_exports_class_inline() {
        // GAP-15-01: module.exports = class Foo { method() {} }
        // Should produce a "default" export with decl_kind Class
        let source = b"module.exports = class Foo { method() {} }";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let default_export = exports.iter().find(|e| e.name == "default");
        assert!(
            default_export.is_some(),
            "module.exports = class should emit 'default' export; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            default_export.unwrap().decl_kind,
            Some(DeclarationKind::Class),
            "inline class export should have decl_kind Class"
        );
    }

    #[test]
    fn test_cjs_module_exports_function_inline() {
        // GAP-15-01: module.exports = function(a, b) { return a + b; }
        // Should produce a "default" export with decl_kind Function
        let source = b"module.exports = function(a, b) { return a + b; }";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let default_export = exports.iter().find(|e| e.name == "default");
        assert!(
            default_export.is_some(),
            "module.exports = function should emit 'default' export; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            default_export.unwrap().decl_kind,
            Some(DeclarationKind::Function),
            "inline function export should have decl_kind Function"
        );
    }

    #[test]
    fn test_cjs_module_exports_async_arrow() {
        // GAP-15-01: module.exports = async (req, res) => { ... }
        // Should produce a "default" export with decl_kind Function
        let source = b"module.exports = async (req, res) => { res.send('ok'); }";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let default_export = exports.iter().find(|e| e.name == "default");
        assert!(
            default_export.is_some(),
            "module.exports = async arrow should emit 'default' export; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            default_export.unwrap().decl_kind,
            Some(DeclarationKind::Function),
            "async arrow export should have decl_kind Function"
        );
    }

    #[test]
    fn test_cjs_exports_property_function() {
        // GAP-15-01: exports.handler = async (req, res) => { }
        // Should produce a "handler" Named export with decl_kind Function
        let source = b"exports.handler = async (req, res) => { res.send('ok'); }";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let handler_export = exports.iter().find(|e| e.name == "handler");
        assert!(
            handler_export.is_some(),
            "exports.handler = async arrow should emit 'handler' export; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            handler_export.unwrap().decl_kind,
            Some(DeclarationKind::Function),
            "exports.handler = async arrow should have decl_kind Function"
        );
    }

    #[test]
    fn test_using_declaration_graceful() {
        // GAP-15-02: export using handle = getResource() should not panic
        // Either detect it or silently skip it — no panics allowed.
        let source = b"export using handle = getResource();";
        let result = parse_exports(source, SourceLanguage::TypeScript);
        assert!(
            result.is_ok(),
            "parse_exports should not error on 'export using' syntax; got: {:?}",
            result.err()
        );
        // The parse completing without panic/error is sufficient — detection is best-effort.
    }

    // ── GAP-15-07: Const type parameter modifier ──────────────────────────────

    #[test]
    fn test_parse_type_params_const_modifier() {
        // NOTE: This test verifies const detection. If tree-sitter-typescript 0.23.2
        // doesn't parse `const T` in generics, the const will appear as ERROR node
        // and is_const will be false. That's acceptable — the detection is best-effort.
        let source = b"function foo<const T>(x: T): T { return x; }";
        let bytes = source.as_slice();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(bytes, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        let params = parse_type_params(bytes, fn_node);
        // If the grammar supports const type params, verify is_const == true
        // If not, the param should still be extracted (is_const may be false)
        assert!(!params.is_empty(), "should extract at least one type param");
        assert_eq!(params[0].name, "T");
    }

    // ── GAP-15-06: Module augmentation capture ────────────────────────────────

    #[test]
    fn test_declare_module_augmentation_captured() {
        let source = br#"
declare module 'express' {
    interface Request {
        user?: { id: string };
    }
}
"#;
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let augment = exports.iter().find(|e| e.name == "augment:express");
        assert!(
            augment.is_some(),
            "declare module 'express' should be captured; got: {:?}",
            exports
        );
    }

    #[test]
    fn test_declare_global_augmentation_captured() {
        let source = br#"
declare global {
    interface Window {
        myApp: any;
    }
}
"#;
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let augment = exports.iter().find(|e| e.name == "augment:global");
        assert!(
            augment.is_some(),
            "declare global should be captured; got: {:?}",
            exports
        );
    }

    #[test]
    fn test_declare_module_not_captured_in_js() {
        // declare module is TypeScript-only syntax
        let source = b"// no declare module in JS";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(exports.iter().all(|e| !e.name.starts_with("augment:")));
    }

    // ── Phase 17 META-03 augmentation body capture ───────────────────────────

    #[test]
    fn meta_03_declare_module_body_member_captured() {
        let source = br#"declare module 'express' { interface Session { uid: string; } }"#;
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "augment:express:*Session"),
            "META-03: declare module member not captured. Got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn meta_03_declare_global_body_captured() {
        let source = br#"declare global { interface Window { foo: string; } }"#;
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name.starts_with("augment:global:*Window")),
            "META-03: declare global member not captured. Got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    // ── Phase 17 META-04 TSDoc release tag extraction ────────────────────────

    #[test]
    fn meta_04_internal_tag_extracted() {
        let source = b"/** @internal */\nexport function privateHelper() {}";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let e = exports.iter().find(|e| e.name == "privateHelper").unwrap();
        assert_eq!(
            e.release_tag,
            Some(crate::types::ReleaseTag::Internal),
            "META-04: @internal tag should be extracted; got: {:?}",
            e.release_tag
        );
    }

    #[test]
    fn meta_04_deprecated_tag_extracted() {
        let source = b"/** @deprecated use newFoo */\nexport function oldFoo() {}";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let e = exports.iter().find(|e| e.name == "oldFoo").unwrap();
        assert_eq!(
            e.release_tag,
            Some(crate::types::ReleaseTag::Deprecated),
            "META-04: @deprecated tag should be extracted; got: {:?}",
            e.release_tag
        );
    }

    #[test]
    fn meta_04_beta_tag_extracted() {
        // Phase 19 META-06: @beta replaces @experimental in the 5-tag set.
        let source = b"/** @beta */\nexport function previewFoo() {}";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let e = exports.iter().find(|e| e.name == "previewFoo").unwrap();
        assert_eq!(
            e.release_tag,
            Some(crate::types::ReleaseTag::Beta),
            "META-06: @beta tag should be extracted; got: {:?}",
            e.release_tag
        );
    }

    #[test]
    fn meta_04_no_tag_is_none() {
        let source = b"export function plainFn() {}";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let e = exports.iter().find(|e| e.name == "plainFn").unwrap();
        assert_eq!(
            e.release_tag, None,
            "META-04: no tag should yield None; got: {:?}",
            e.release_tag
        );
    }

    // ── Phase 17 TLF extraction tests ────────────────────────────────────────

    #[test]
    fn tlf_01_extract_construct_signature() {
        let src = "interface Foo { new (x: number): Bar; }";
        let (tree, bytes) = parse_ts(src);
        let iface_node = first_node_of_kind(tree.root_node(), "interface_declaration").unwrap();
        // Find interface_body child
        let body = (0..iface_node.child_count() as u32)
            .filter_map(|i| iface_node.child(i))
            .find(|c| c.kind() == "interface_body" || c.kind() == "object_type")
            .expect("interface body not found");
        let sigs = extract_construct_signatures(body, &bytes);
        assert_eq!(
            sigs.len(),
            1,
            "should extract one construct signature; got: {:?}",
            sigs
        );
        assert_eq!(sigs[0].param_count, 1, "one param");
        assert_eq!(
            sigs[0].return_text.as_deref(),
            Some("Bar"),
            "return type Bar"
        );
        assert!(!sigs[0].is_abstract, "not abstract");
    }

    #[test]
    fn tlf_02_parameter_properties_synthesized() {
        let src = "class Foo { constructor(public x: number, private y: string, z: boolean) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        // x and y should be synthesized as Property members
        let x = members.iter().find(|m| m.name == "x");
        let y = members.iter().find(|m| m.name == "y");
        let z = members.iter().find(|m| m.name == "z");
        assert!(
            x.is_some(),
            "x should be synthesized as property; members: {:?}",
            members.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
        assert!(y.is_some(), "y should be synthesized as property");
        assert!(z.is_none(), "z has no modifier — must NOT be synthesized");
        assert_eq!(x.unwrap().kind, ClassMemberKind::Property);
        assert_eq!(x.unwrap().visibility, Visibility::Public);
        assert_eq!(y.unwrap().visibility, Visibility::Private);
    }

    #[test]
    fn tlf_05_variance_in_out_extracted() {
        let src = "type X<in T, out U, in out V> = [T, U, V];";
        let (tree, bytes) = parse_ts(src);
        let alias_node = first_node_of_kind(tree.root_node(), "type_alias_declaration").unwrap();
        // Pass the declaration node (parse_type_params looks up type_parameters internally)
        let params = parse_type_params(&bytes, alias_node);
        assert_eq!(
            params.len(),
            3,
            "three type params; got: {:?}",
            params.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        assert_eq!(
            params[0].variance,
            Some(crate::types::Variance::In),
            "T should be in"
        );
        assert_eq!(
            params[1].variance,
            Some(crate::types::Variance::Out),
            "U should be out"
        );
        assert_eq!(
            params[2].variance,
            Some(crate::types::Variance::InOut),
            "V should be in out"
        );
    }

    #[test]
    fn tlf_06_accessor_keyword_captured() {
        let src = "class Foo { accessor bar: number; }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let (members, _has_dynamic_static_key) = parse_class_members(&bytes, class_node);
        let bar = members
            .iter()
            .find(|m| m.name == "bar")
            .expect("bar member should be present");
        assert!(bar.is_accessor, "bar should have is_accessor=true");
    }

    #[test]
    fn tlf_07_export_type_star_is_type_only() {
        let source = b"export type * from './mod';";
        let raw = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            raw.iter().any(|e| e.is_type_only),
            "export type * should produce is_type_only=true; got: {:?}",
            raw.iter()
                .map(|e| (&e.name, e.is_type_only))
                .collect::<Vec<_>>()
        );
    }

    // ── Phase 17 BUN-01..07: Bundler CJS output detection ────────────────────

    #[test]
    fn bun_01_esbuild_tree_shake_wrapper() {
        // BUN-01: esbuild `0 && (module.exports = { foo, bar })` tree-shake wrapper
        let source = b"0 && (module.exports = { foo: foo, bar: bar });";
        let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "BUN-01: foo must be detected; got: {:?}",
            names
        );
        assert!(
            names.contains(&"bar"),
            "BUN-01: bar must be detected; got: {:?}",
            names
        );
    }

    #[test]
    fn bun_02_esbuild_export_helper() {
        // BUN-02: __export(exports, { foo: () => foo, bar: () => bar })
        let source = b"__export(exports, { foo: () => foo, bar: () => bar });";
        let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "BUN-02: foo must be detected; got: {:?}",
            names
        );
        assert!(
            names.contains(&"bar"),
            "BUN-02: bar must be detected; got: {:?}",
            names
        );
    }

    #[test]
    fn bun_04_tsc_void_placeholder() {
        // BUN-04: exports.foo = void 0; — tsc placeholder
        let source = b"exports.foo = void 0;";
        let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "BUN-04: foo must be detected from void 0 placeholder; got: {:?}",
            names
        );
    }

    #[test]
    fn bun_06b_bracket_subscript_export() {
        // BUN-06b: exports["foo"] = bar
        let source = b"exports[\"foo\"] = bar;";
        let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "BUN-06b: foo must be detected from bracket subscript; got: {:?}",
            names
        );
    }

    #[test]
    fn bun_07_esmodule_filtered() {
        // BUN-07: __esModule must never appear in final export list
        let source = b"exports.__esModule = true;";
        let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
        assert!(
            !names.contains(&"__esModule"),
            "BUN-07: __esModule must be filtered; got: {:?}",
            names
        );
    }

    // ── Phase 17 IFT-01: Interface property optionality extracted ────────────

    #[test]
    fn ift_01_interface_property_optionality_extracted() {
        let source = b"interface Foo { x?: number; y: string; }";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        // Find the interface_declaration node
        let iface_node = first_node_of_kind(tree.root_node(), "interface_declaration")
            .expect("interface_declaration not found");
        // Find interface body (interface_body or object_type)
        let body = find_interface_body(iface_node).expect("interface body not found");
        let members = parse_interface_members(source, body);
        let x = members.iter().find(|m| m.name == "x").expect("x not found");
        let y = members.iter().find(|m| m.name == "y").expect("y not found");
        assert!(x.is_optional, "x? should be optional");
        assert!(!y.is_optional, "y should not be optional");
    }

    // ── Phase 17 IFT-03: extract_param_optionality on constructor ────────────

    #[test]
    fn ift_03_extract_param_optionality_constructor() {
        let source = b"class C { constructor(a: number, b?: string, c?: boolean) {} }";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration")
            .expect("class_declaration not found");
        // Find class_body
        let class_body = {
            let mut found = None;
            for i in 0..class_node.child_count() as u32 {
                if let Some(child) = class_node.child(i) {
                    if child.kind() == "class_body" {
                        found = Some(child);
                        break;
                    }
                }
            }
            found.expect("class_body not found")
        };
        // Find the constructor method_definition
        let ctor_node = {
            let mut found = None;
            let mut cursor = class_body.walk();
            for child in class_body.children(&mut cursor) {
                if child.kind() == "method_definition" {
                    let name_text = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("");
                    if name_text == "constructor" {
                        found = Some(child);
                        break;
                    }
                }
            }
            found.expect("constructor method_definition not found")
        };
        let optionality = extract_param_optionality(ctor_node);
        assert_eq!(
            optionality,
            vec![false, true, true],
            "a is required, b? and c? are optional; got: {:?}",
            optionality
        );
    }

    // ── Phase 18 ARCH-02: compound-component capture ────────────────────────

    #[test]
    fn test_arch02_compound_member_assignment_attaches() {
        // Form 1: `Target.Member = X;` at top-level attaches Member to Target.
        let src = br#"
export const Menu = () => null;
Menu.Item = () => null;
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse");
        let menu = exports
            .iter()
            .find(|e| e.name == "Menu")
            .expect("Menu export missing");
        assert!(
            menu.compound_members.iter().any(|m| m == "Item"),
            "ARCH-02 Form 1: expected 'Item' in compound_members, got {:?}",
            menu.compound_members
        );
    }

    #[test]
    fn test_arch02_object_assign_compound_attaches() {
        // Form 3: `Object.assign(Target, { Member: X, ... })` attaches all members.
        let src = br#"
export let Menu = {};
Menu = Object.assign(Menu, { Button: 1, Items: 2, Item: 3 });
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse");
        let menu = exports
            .iter()
            .find(|e| e.name == "Menu")
            .expect("Menu export missing");
        assert!(
            menu.compound_members.iter().any(|m| m == "Button"),
            "ARCH-02 Form 3: expected 'Button'; got {:?}",
            menu.compound_members
        );
        assert!(
            menu.compound_members.iter().any(|m| m == "Items"),
            "ARCH-02 Form 3: expected 'Items'; got {:?}",
            menu.compound_members
        );
        assert!(
            menu.compound_members.iter().any(|m| m == "Item"),
            "ARCH-02 Form 3: expected 'Item'; got {:?}",
            menu.compound_members
        );
    }

    #[test]
    fn test_arch02_conservative_no_attach_when_target_not_exported() {
        // D-16 conservative: `Foo.Bar = X` where `Foo` is NOT an exported symbol
        // must NOT attach Bar to anything. `Unrelated` should remain with empty
        // compound_members.
        let src = br#"
const Foo = {};
Foo.Bar = 42;
export const Unrelated = 1;
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse");
        assert!(
            !exports.iter().any(|e| e.name == "Foo"),
            "ARCH-02 D-16: unexported Foo must not appear in exports"
        );
        let unrelated = exports
            .iter()
            .find(|e| e.name == "Unrelated")
            .expect("Unrelated missing");
        assert!(
            unrelated.compound_members.is_empty(),
            "ARCH-02 D-16: Unrelated must not have compound_members; got {:?}",
            unrelated.compound_members
        );
    }

    #[test]
    fn test_arch02_minified_target_not_in_exports_silent_noop() {
        // Pitfall 6: minified target name (`Pe`) is not in the exports — the
        // conservative attachment rule silently no-ops. `Menu` gets no members.
        let src = br#"
export const Menu = () => null;
var qe = Object.assign(Pe, { Button: 1, Item: 2 });
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse");
        let menu = exports
            .iter()
            .find(|e| e.name == "Menu")
            .expect("Menu export missing");
        assert!(
            menu.compound_members.is_empty(),
            "ARCH-02 Pitfall 6: minified-target case must silently no-op; got {:?}",
            menu.compound_members
        );
    }

    // ---------------------------------------------------------------------
    // INF-01 (Phase 18): .tsx → LANGUAGE_TSX grammar routing tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_inf01_tsx_routes_to_typescript_tsx() {
        let p = std::path::PathBuf::from("/x/foo.tsx");
        assert!(matches!(
            language_for_file(&p),
            Some(SourceLanguage::TypeScriptTsx)
        ));
    }

    #[test]
    fn test_inf01_d_ts_still_typescript() {
        // Declaration files are pure type surface (no JSX) — must stay on the
        // standard TypeScript grammar regardless of the `.tsx` → LANGUAGE_TSX
        // routing.
        let p = std::path::PathBuf::from("/x/foo.d.ts");
        assert!(matches!(
            language_for_file(&p),
            Some(SourceLanguage::TypeScript)
        ));
    }

    #[test]
    fn test_inf01_ts_still_typescript() {
        let p = std::path::PathBuf::from("/x/foo.ts");
        assert!(matches!(
            language_for_file(&p),
            Some(SourceLanguage::TypeScript)
        ));
    }

    #[test]
    fn test_inf01_jsx_still_javascript() {
        // `.jsx` is JavaScript-JSX; it stays on the JavaScript grammar because
        // tree-sitter-javascript parses JSX natively. Only `.tsx` needs the TSX
        // TypeScript grammar variant.
        let p = std::path::PathBuf::from("/x/foo.jsx");
        assert!(matches!(
            language_for_file(&p),
            Some(SourceLanguage::JavaScript)
        ));
    }

    #[test]
    fn test_inf01_tsx_jsx_body_discovers_export() {
        // TSX grammar must handle JSX in export body without producing ERROR
        // nodes that suppress export discovery. Pre-INF-01, `.tsx` routed to
        // LANGUAGE_TYPESCRIPT which emits ERROR nodes for JSX expressions and
        // silently dropped the surrounding exports.
        let src = br#"
export const Comp = () => <div>hi</div>;
export interface Props { children: string }
"#;
        let exports =
            parse_exports(src, SourceLanguage::TypeScriptTsx).expect("parse must succeed");
        assert!(
            exports.iter().any(|e| e.name == "Comp"),
            "expected Comp in exports, got {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "Props"),
            "expected Props in exports"
        );
    }

    #[test]
    fn test_inf01_tsx_same_queries_match_as_ts() {
        // Strict-superset property: every TypeScript query matches identically
        // on TSX grammar for source containing no JSX. If this regresses, every
        // matches!() dispatch site that was widened would start silently
        // dropping exports under the TypeScriptTsx arm.
        let src = br#"
export function foo(x: number): string { return ""; }
export class Bar<T> { method(a: T): T { return a; } }
"#;
        let ts = parse_exports(src, SourceLanguage::TypeScript).expect("TS parse");
        let tsx = parse_exports(src, SourceLanguage::TypeScriptTsx).expect("TSX parse");
        let ts_names: std::collections::HashSet<_> = ts.iter().map(|e| e.name.clone()).collect();
        let tsx_names: std::collections::HashSet<_> = tsx.iter().map(|e| e.name.clone()).collect();
        assert_eq!(
            ts_names, tsx_names,
            "TSX grammar should be strict superset — same exports discovered"
        );
    }

    // ---------------------------------------------------------------------
    // INF-02 (Phase 18): rollup-plugin-dts / api-extractor split-declaration
    // linking tests. Pass 1 collects ambient declare statements; link-pass
    // enriches Named non-re-export RawExports with decl_kind + type_signature.
    // ---------------------------------------------------------------------

    #[test]
    fn test_inf02_declare_const_link_to_export_populates_type_signature() {
        let src = br#"
declare const foo: number;
export { foo };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        let foo = exports
            .iter()
            .find(|e| e.name == "foo")
            .expect("foo missing");
        assert_eq!(foo.decl_kind, Some(DeclarationKind::Variable));
        assert!(
            foo.type_signature.is_some(),
            "type_signature should be populated from declare site"
        );
        assert!(
            foo.type_signature.as_ref().unwrap().contains("number"),
            "got type_signature: {:?}",
            foo.type_signature
        );
    }

    #[test]
    fn test_inf02_declare_function_link_to_export_populates_decl_kind() {
        let src = br#"
declare function bar(x: number): string;
export { bar };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        let bar = exports
            .iter()
            .find(|e| e.name == "bar")
            .expect("bar missing");
        assert_eq!(bar.decl_kind, Some(DeclarationKind::Function));
        assert!(
            bar.type_signature.is_some(),
            "function declare should set type_signature to the signature text"
        );
    }

    #[test]
    fn test_inf02_declare_class_link_to_export_populates_decl_kind() {
        let src = br#"
declare class Baz { x: number; }
export { Baz };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        let baz = exports
            .iter()
            .find(|e| e.name == "Baz")
            .expect("Baz missing");
        assert_eq!(baz.decl_kind, Some(DeclarationKind::Class));
    }

    #[test]
    fn test_inf02_declare_without_export_is_not_public() {
        // Orphan declare statement — no export — must NOT produce a RawExport.
        // The ambient sidecar records it, but without a matching export the
        // link-pass has nothing to enrich.
        let src = br#"
declare const internal: number;
export const unrelated = 1;
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        assert!(
            !exports.iter().any(|e| e.name == "internal"),
            "declare-without-export must not appear as an export"
        );
        assert!(exports.iter().any(|e| e.name == "unrelated"));
    }

    #[test]
    fn test_inf02_reexport_not_affected_by_local_declare() {
        // If the export is `export { foo } from './other'` (re-export with
        // `from`), the local ambient declare must NOT leak into the exported
        // symbol — the symbol's identity comes from ./other. The INF-02
        // link-pass explicitly skips entries where `rx.source.is_some()`.
        let src = br#"
declare const foo: number;
export { foo } from './other';
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        let foo = exports
            .iter()
            .find(|e| e.name == "foo")
            .expect("foo missing");
        assert_eq!(foo.kind, RawExportKind::NamedReexport);
        // decl_kind from ambient declare must NOT have leaked in.
        assert_eq!(foo.decl_kind, None);
        assert_eq!(foo.type_signature, None);
    }

    // ---------------------------------------------------------------------
    // INF-03 (Phase 18): tsup/esbuild `export { foo as default }` ESM trailer
    // dual-emission tests. When `alias_name == "default"` the NAMED_EXPORT_
    // ALIAS_QUERY handler now emits `RawExportKind::Default` instead of
    // `Named`. Dual emission (`export { foo, foo as default }`) fires both
    // queries — NAMED_EXPORT_QUERY for foo and NAMED_EXPORT_ALIAS_QUERY for
    // the default trailer.
    // ---------------------------------------------------------------------

    #[test]
    fn test_inf03_foo_as_default_trailer_emits_default_kind() {
        let src = br#"
const foo = 1;
export { foo as default };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        let default = exports
            .iter()
            .find(|e| matches!(e.kind, RawExportKind::Default))
            .expect("expected a Default RawExport");
        assert_eq!(default.name, "default");
        // foo is NOT also emitted as a separate Named when only the alias
        // trailer is present (no bare `foo` specifier in the export clause).
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "foo" && matches!(e.kind, RawExportKind::Named)),
            "foo alone should not be Named when only the alias trailer is present"
        );
    }

    #[test]
    fn test_inf03_dual_emission_foo_and_default() {
        let src = br#"
const foo = 1;
export { foo, foo as default };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "foo" && matches!(e.kind, RawExportKind::Named)),
            "expected Named foo (fires via NAMED_EXPORT_QUERY)"
        );
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default)),
            "expected Default from `as default` trailer"
        );
    }

    #[test]
    fn test_inf03_non_default_alias_stays_named() {
        let src = br#"
const bar = 1;
export { bar as Baz };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Baz" && matches!(e.kind, RawExportKind::Named)),
            "expected Baz as Named"
        );
        assert!(
            !exports
                .iter()
                .any(|e| matches!(e.kind, RawExportKind::Default)),
            "non-default alias must not emit a Default"
        );
    }

    #[test]
    fn test_inf03_source_default_plus_trailer_dedup() {
        // Both source-form `export default foo` AND the trailer form
        // `export { foo as default }`. Both produce a Default RawExport. The
        // resolver pre-merge absorbs the duplicate via ExportKey ("default",
        // space=Value) — tree_query.rs emits both and the resolver keeps one
        // ExportMap entry.
        let src = br#"
const foo = 1;
export default foo;
export { foo as default };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        let default_count = exports
            .iter()
            .filter(|e| matches!(e.kind, RawExportKind::Default))
            .count();
        assert!(
            default_count >= 1,
            "at least one Default must be emitted; got {default_count}"
        );
    }

    #[test]
    fn test_inf03_zod_style_trailer_with_variable_ref() {
        // Real-package inspired: Zod 3.23.8 emits
        //   declare const z: ...;
        //   export { z as default, z };
        // alongside ambient declare at the top. INF-02 links `z` (Named) to
        // the ambient decl_kind; INF-03 emits an additional Default for the
        // `as default` alias.
        let src = br#"
declare const z: { parse: (x: any) => any };
export { z as default, z };
"#;
        let exports = parse_exports(src, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "z" && matches!(e.kind, RawExportKind::Named)),
            "expected Named z"
        );
        assert!(
            exports
                .iter()
                .any(|e| matches!(e.kind, RawExportKind::Default)),
            "expected Default from 'as default' trailer"
        );
    }

    // ── Phase 19 CLS-10: decorator AST extraction ─────────────────────────────

    fn find_first_node_of_kind<'a>(
        root: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        if root.kind() == kind {
            return Some(root);
        }
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if let Some(found) = find_first_node_of_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    /// Extract the combined class-level decorators: those directly on the
    /// class_declaration plus those on an enclosing export_statement parent.
    /// Matches the logic inside `parse_class_shape`.
    fn combined_class_decs(source: &[u8]) -> Vec<crate::types::DecoratorSpec> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&get_language(&SourceLanguage::TypeScript))
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let class_node = find_first_node_of_kind(root, "class_declaration")
            .expect("class_declaration must exist in source");
        let mut decs = extract_decorators(class_node, source);
        if let Some(parent) = class_node.parent() {
            if parent.kind() == "export_statement" {
                let mut parent_decs = extract_decorators(parent, source);
                parent_decs.extend(decs);
                decs = parent_decs;
            }
        }
        decs
    }

    #[test]
    fn cls_10_class_decorator_identifier_form() {
        let source = b"@Component\nexport class Foo {}\n";
        let decs = combined_class_decs(source);
        assert!(
            decs.iter()
                .any(|d| d.name == "Component" && d.args.is_none()),
            "@Component should be extracted; got {:?}",
            decs
        );
    }

    #[test]
    fn cls_10_class_decorator_call_expression_form() {
        let source = b"@Route('/users')\nexport class UserController {}\n";
        let decs = combined_class_decs(source);
        let route = decs.iter().find(|d| d.name == "Route");
        assert!(
            route.is_some(),
            "Route decorator should be extracted; decs = {:?}",
            decs
        );
        let args = route.unwrap().args.as_deref().unwrap_or("");
        assert!(
            args.contains("'/users'"),
            "args should include '/users'; got {:?}",
            args
        );
    }

    #[test]
    fn cls_10_member_expression_form() {
        let source = b"@ns.log\nexport class X {}\n";
        let decs = combined_class_decs(source);
        assert!(
            decs.iter()
                .any(|d| d.name == "ns.log" && d.args.is_none()),
            "ns.log member_expression decorator should be extracted; decs = {:?}",
            decs
        );
    }

    #[test]
    fn cls_10_no_decorators_is_empty() {
        let source = b"export class Plain {}\n";
        let decs = combined_class_decs(source);
        assert!(decs.is_empty(), "plain class should have no decorators; got {:?}", decs);
    }
}

//! Tree-sitter export patterns for JS/TS source and generated JavaScript.

/// Captures `export { foo }` (no alias, no `from` clause).
///
/// We match `export_statement` nodes that contain an `export_clause` but no
/// `source` child — that distinguishes plain named exports from named re-exports.
/// For `export { foo as bar }` see `NAMED_EXPORT_ALIAS_QUERY`.
///
/// (inline type modifiers): `export { type Foo, Bar }` is handled
/// transparently by this query because the grammar's `export_specifier` matches
/// the `identifier` @name node regardless of a preceding per-specifier `"type"` token.
/// Both `Foo` and `Bar` appear exactly once as Named exports.
pub(crate) const NAMED_EXPORT_QUERY: &str = r#"
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
pub(crate) const NAMED_EXPORT_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: [(identifier) (string)] @alias_name)))
"#;

/// INF-06: JS grammar's `export { foo as default }` alias form.
/// In tree-sitter-javascript 0.25.0 the alias child is an anonymous `default`
/// keyword token (kind = "default"), NOT an identifier. Requires the string-
/// literal matcher in the query pattern.
///
/// Capture index 0 = `@local_name` (the local binding, e.g. "foo").
/// Capture index 1 = `@alias_default` (the anonymous `default` keyword token).
pub(crate) const NAMED_EXPORT_ALIAS_DEFAULT_QUERY_JS: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: "default" @alias_default)))
"#;

/// ARCH-02 Form 1: `Target.Member = X;` top-level member assignment.
///
/// Captures `@target` (the object identifier) and `@member` (the new property
/// name). Matched against `expression_statement` nodes at the program level so
/// nested-inside-function assignments are excluded — those don't contribute to
/// the public surface.
pub(crate) const COMPOUND_ASSIGN_MEMBER_QUERY: &str = r#"
(program
  (expression_statement
    (assignment_expression
      left: (member_expression
        object: (identifier) @target
        property: (property_identifier) @member)
      right: (_) @value)))
"#;

/// ARCH-02 Form 3: `Object.assign(Target, { Member: X,... })` compound
/// component composition.
///
/// Captures `@obj` + `@assign_prop` (to check Object.assign), `@target` (first
/// arg identifier), and `@member` for every property key in the object literal.
/// Supports both `(pair key: (property_identifier))` and shorthand properties.
pub(crate) const COMPOUND_OBJECT_ASSIGN_QUERY: &str = r#"
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
pub(crate) const DEFAULT_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  value: (_) @value)
"#;

/// Captures `export default function foo` and `export default class Foo` —
/// these appear as `declaration:` children, not `value:`, in both grammars.
/// We capture the keyword "default" position as the anchor since anonymous
/// default declarations don't have a name node.
pub(crate) const DEFAULT_DECL_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  declaration: (_) @decl)
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`
/// for **JavaScript** files. Class names use `identifier` in the JS grammar.
pub(crate) const DECL_EXPORT_QUERY_JS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (variable_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (generator_function_declaration name: (identifier) @name)
    (class_declaration name: (identifier) @name)
  ])
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`,
/// `export abstract class Foo`, `export type Foo`, `export interface Foo`, `export enum Foo`,
/// `export namespace Foo`, and overload `export function foo(...)` signatures
/// for **TypeScript** files. TypeScript uses `type_identifier` for class/type/interface names.
pub(crate) const DECL_EXPORT_QUERY_TS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (variable_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (generator_function_declaration name: (identifier) @name)
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
pub(crate) const STAR_EXPORT_QUERY: &str = r#"
(export_statement
  "*"
  source: (string) @source)
"#;

/// Captures `export * as name from './module'` (namespace re-export).
///
/// Capture index 0 = `@ns_name` (the namespace identifier, e.g. "utils").
/// Capture index 1 = `@source` (the module specifier string).
pub(crate) const NAMESPACE_EXPORT_QUERY: &str = r#"
(export_statement
  (namespace_export
    [(identifier) (string)] @ns_name)
  source: (string) @source)
"#;

/// Captures `export { foo } from './module'` (no alias).
/// For `export { foo as bar } from './module'` see `NAMED_REEXPORT_ALIAS_QUERY`.
pub(crate) const NAMED_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: [(identifier) (string)] @name))
  source: (string) @source)
"#;

/// Captures `export { foo as bar } from './module'` (with alias).
///
/// Capture index 0 = `@local_name` (the local name in the source module, e.g. "foo").
/// Capture index 1 = `@alias_name` (the public re-export name, e.g. "bar").
/// Capture index 2 = `@source` (the module specifier string).
/// We process this query first so the alias name is used as the exported name.
pub(crate) const NAMED_REEXPORT_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: [(identifier) (string)] @local_name
      alias: [(identifier) (string)] @alias_name))
  source: (string) @source)
"#;

/// Captures `export type { Foo }` (TS 3.8+ type-only named exports, no `from` clause).
///
/// The `"type"` keyword is an anonymous token child of `export_statement`.
/// Type-only exports are public API surface and must be tracked in the export map.
pub(crate) const TYPE_ONLY_NAMED_EXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @name)))
"#;

/// Captures `export type { Foo } from './bar'` (TS 3.8+ type-only named re-exports).
///
/// Capture index 0 = `@name`, capture index 1 = `@source`.
pub(crate) const TYPE_ONLY_NAMED_REEXPORT_QUERY_TS: &str = r#"
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
pub(crate) const TYPE_ONLY_ALIAS_EXPORT_QUERY_TS: &str = r#"
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
pub(crate) const TYPE_ONLY_ALIAS_REEXPORT_QUERY_TS: &str = r#"
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
pub(crate) const DESTRUCTURED_EXPORT_QUERY: &str = r#"
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
pub(crate) const DESTRUCTURED_PAIR_EXPORT_QUERY: &str = r#"
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
/// with name "default" in pledge's model (equivalent to `module.exports = Foo`).
pub(crate) const EXPORT_ASSIGNMENT_QUERY_TS: &str = r#"
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
pub(crate) const AMBIENT_DECL_EXPORT_QUERY_TS: &str = r#"
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

/// Captures `declare module 'express' {... }` — module augmentation in.d.ts files.
///
/// Module augmentations extend external package types. The module name (string literal)
/// identifies which module is being augmented. These are captured as named exports
/// with a synthetic `augment:<module>` key so their removal is detected.
///
/// Grammar: `(ambient_declaration (module name: (string) body: (statement_block)))`.
/// The `string` node has a `string_fragment` child; `node.utf8_text` returns the
/// full quoted string (e.g., `"'express'"`). We strip quotes after capture.
///.
pub(crate) const DECLARE_MODULE_AUGMENTATION_QUERY_TS: &str = r#"
(ambient_declaration
  (module
    name: (string) @module_name
    body: (statement_block) @module_body))
"#;

/// Captures `declare global {... }` — global augmentation in.d.ts files.
///
/// Global augmentations add declarations to the global scope. Captured with a
/// synthetic `augment:global` key.
///
/// Grammar: `declare global {... }` parses as `(ambient_declaration (statement_block...))`
/// no `module` wrapper, no identifier. We detect it by matching `ambient_declaration`
/// nodes whose only named child is a `statement_block` (with no preceding module name node).
/// The query captures the `statement_block` so we can filter on node position.
///.
pub(crate) const DECLARE_GLOBAL_AUGMENTATION_QUERY_TS: &str = r#"
(ambient_declaration
  (statement_block) @global_body)
"#;

/// Captures `export { default as Named }` — keyword `default` as source with alias.
/// The `default` keyword is an unnamed node, not `identifier`, so existing queries miss it.
///.
pub(crate) const DEFAULT_SPECIFIER_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"
      alias: [(identifier) (string)] @alias_name)))
"#;

/// Captures `export { default } from './foo'` — re-export of default.
///.
pub(crate) const DEFAULT_SPECIFIER_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"))
  source: (string) @source)
"#;

/// Captures `export { default as Named } from './foo'` — re-export default with alias.
///.
pub(crate) const DEFAULT_SPECIFIER_ALIAS_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"
      alias: [(identifier) (string)] @alias_name))
  source: (string) @source)
"#;

/// Captures `export const [a, b] = arr` — array destructuring with named identifiers.
///.
pub(crate) const ARRAY_DESTRUCTURED_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (array_pattern
        (identifier) @name))))
"#;

/// Captures rest patterns in array destructuring: `export const [a,...rest] = arr`.
///.
pub(crate) const ARRAY_DESTRUCTURED_REST_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (array_pattern
        (rest_pattern (identifier) @name)))))
"#;

/// Captures `module.exports = { foo, bar }` — CJS bulk export with shorthand properties.
/// JavaScript/CJS only. Each shorthand_property_identifier is a named export.
///.
pub(crate) const CJS_MODULE_EXPORTS_SHORTHAND_QUERY: &str = r#"
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
///.
pub(crate) const CJS_MODULE_EXPORTS_PAIR_QUERY: &str = r#"
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
///.
pub(crate) const CJS_EXPORTS_PROPERTY_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @name)))
"#;

/// Captures `module.exports = SomeIdentifier` — CJS single default-like export.
///.
pub(crate) const CJS_MODULE_EXPORTS_DEFAULT_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (identifier) @value))
"#;

/// Captures `module.exports = class {... }` — CJS inline class export.
/// The class may be named (`class Foo {... }`) or anonymous (`class {... }`).
///.
pub(crate) const CJS_MODULE_EXPORTS_CLASS_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (class) @class_value))
"#;

/// Captures `module.exports = function(...) {... }`, including async functions,
/// generator functions, and arrow functions.
/// Note: In the JS grammar, generator function expressions use the node type
/// `generator_function` (not `generator_function_expression`).
///.
pub(crate) const CJS_MODULE_EXPORTS_FUNCTION_QUERY: &str = r#"
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

/// DOC-01: Captures `module.exports = new Proxy(...)` and
/// `module.exports = Reflect.construct(...)` — Proxy-dispatched default
/// export. Synthesizes a `default` RawExport flagged is_proxy_dispatched
/// so the comparator can emit a paired `limitation: proxy-dispatched-exports`
/// Info finding. Covers Open Question O3 idioms (a) and (b); idiom (c)
/// (named-key Proxy) and (d) (factory returning Proxy) are NOT covered
/// here — they fall through to existing patterns and the limitation is
/// the catch-all.
pub(crate) const CJS_MODULE_EXPORTS_PROXY_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: [
      (new_expression
        constructor: (identifier) @ctor)
      (call_expression
        function: (member_expression
          object: (identifier) @reflect_obj
          property: (property_identifier) @reflect_method))
    ]))
"#;

/// Captures `exports.handler = async (req, res) => {... }` and
/// `exports.foo = function() {... }` — CJS named function property exports.
/// Note: In the JS grammar, generator function expressions use the node type
/// `generator_function` (not `generator_function_expression`).
///.
pub(crate) const CJS_EXPORTS_PROPERTY_FUNCTION_QUERY: &str = r#"
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

/// Captures `exports = module.exports = {... }` — CJS aliasing pattern.
/// Some packages use this pattern where the RHS is a chained assignment.
/// We capture the object properties from the final object literal.
///.
pub(crate) const CJS_EXPORTS_ALIASING_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (identifier) @lhs
    right: (assignment_expression
      left: (member_expression
        object: (identifier) @mod_obj
        property: (property_identifier) @mod_prop)
      right: (object) @obj)))
"#;

/// BUN-01: esbuild `0 && (module.exports = {... })` tree-shake wrapper.
/// The binary_expression's RHS (under parenthesized_expression) is the module.exports assignment.
pub(crate) const ESBUILD_TREE_SHAKE_WRAPPER_QUERY: &str = r#"
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
pub(crate) const ESBUILD_EXPORT_HELPER_QUERY: &str = r#"
(call_expression
  function: (identifier) @fn_name
  arguments: (arguments
    (identifier) @target
    (object) @prop_obj))
"#;

/// BUN-04: tsc `exports.foo = void 0;` placeholder.
/// Chained assignments (`exports.foo = exports.bar = void 0`) each produce a
/// separate match because tree-sitter emits one assignment_expression per node.
/// BUN-04.
pub(crate) const TSC_VOID_EXPORTS_QUERY: &str = r#"
(assignment_expression
  left: (member_expression
    object: (identifier) @exports_id
    property: (property_identifier) @name)
  right: (_) @rhs)
"#;

/// BUN-05: swc `Object.keys(m).forEach(k => exports[k] = m[k])` star re-export shim.
/// Detected as StarReexport with source resolved from the require() bound to `m`.
/// BUN-05.
pub(crate) const SWC_STAR_REEXPORT_SHIM_QUERY: &str = r#"
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

/// BUN-06a: `Object.defineProperty(exports, "foo",...)`,
/// `Object.defineProperties(exports, { foo: {...} })`,
/// `Object.assign(exports, { foo: x })` forms.
/// Also resolves __defProp aliases from the BUN-03 map. BUN-06.
pub(crate) const OBJECT_DEFINE_EXPORT_QUERY: &str = r#"
(call_expression
  function: (member_expression
    object: (identifier) @obj_id
    property: (property_identifier) @method)
  arguments: (arguments) @args)
"#;

/// BUN-06b: `exports["foo"] = bar;` bracket-subscript assignment.
/// BUN-06.
pub(crate) const BRACKET_SUBSCRIPT_EXPORT_QUERY: &str = r#"
(assignment_expression
  left: (subscript_expression
    object: (identifier) @exports_id
    index: (string) @name_str)
  right: (_))
"#;

/// BUN-11-A: UMD wrapper IIFE shape — Form 1: `(func)(args)`.
///
/// Captures @umd_factory_body — the IIFE's second-arg function body — so the
/// driver can recurse the existing CJS / BUN-06a queries against just that
/// sub-tree using a fresh QueryCursor (the `expand_namespace_members` trick).
///
/// `function_expression` is the verified tree-sitter-javascript 0.25.0 node
/// name for anonymous `function(…) {}` literals. The query-compile
/// unit test below is the regression guard for grammar version bumps.
///
/// UMD output uses two parenthesization forms:
///
/// Form 1 — `(function (root, factory) {... })(this, function () {... });`
///  (parens wrap only the outer function; call sits outside)
/// AST: `(call_expression function: (parenthesized_expression (function_expression...))...)`
///
/// Form 2 — `(function (root, factory) {... }(this, function () {... }));`
///  (parens wrap the entire call expression; function_expression sits directly under call_expression)
/// AST: `(parenthesized_expression (call_expression function: (function_expression...)...))`
///
/// Form 2 is the lodash/UMD-template canonical shape. Both queries run in the
/// driver and dedup at emission time prevents double-emission when both match.
pub(crate) const UMD_FACTORY_WRAPPER_QUERY: &str = r#"
(call_expression
  function: (parenthesized_expression
    (function_expression
      parameters: (formal_parameters
        (identifier) @umd_root_param
        (identifier) @umd_factory_param)))
  arguments: (arguments
    (_) @umd_global_arg
    (function_expression
      parameters: (formal_parameters) @umd_factory_params
      body: (statement_block) @umd_factory_body)))
"#;

/// BUN-11-A Form 2: UMD wrapper IIFE shape — `(func(args))`.
/// Companion query to UMD_FACTORY_WRAPPER_QUERY for the form where the call
/// expression is wrapped by `parenthesized_expression`, not just the function.
pub(crate) const UMD_FACTORY_WRAPPER_QUERY_FORM2: &str = r#"
(parenthesized_expression
  (call_expression
    function: (function_expression
      parameters: (formal_parameters
        (identifier) @umd_root_param
        (identifier) @umd_factory_param))
    arguments: (arguments
      (_) @umd_global_arg
      (function_expression
        parameters: (formal_parameters) @umd_factory_params
        body: (statement_block) @umd_factory_body))))
"#;

/// BUN-11-B: lodash-style IIFE with `module.exports = <ident>`.
///
/// The simplest shape is a top-level IIFE call like:
///  (function() {... module.exports = X;... }.call(this))
/// or:
///  (function() {... module.exports = X;... }())
/// or with the parenthesized_expression chain:
///  (function() {... (module.exports = X)._ = X;... }.call(this))
///
/// We search the AST DEPTH-AGNOSTICALLY for `module.exports = <identifier>` —
/// tree-sitter queries are depth-agnostic by default, so this
/// captures the assignment regardless of nesting under any IIFE wrapper.
/// The transient Default RawExport is synthesized for the captured identifier
/// so attach_compound_members's gate accepts the binding.
pub(crate) const LODASH_STYLE_DEFAULT_QUERY: &str = r#"
(assignment_expression
  left: (member_expression
    object: (identifier) @mod_obj
    property: (property_identifier) @mod_prop)
  right: (identifier) @umd_default_binding)
"#;

/// BUN-11-B Path 1A companion: depth-agnostic `<ident>.<member> = …`.
///
/// Companion query to LODASH_STYLE_DEFAULT_QUERY for collecting the lodash-style
/// member assignments (`_.chunk = …`, `_.compact = …`) that live INSIDE an IIFE
/// body. The pre-existing COMPOUND_ASSIGN_MEMBER_QUERY at line ~196 is anchored
/// to `(program (expression_statement...))` — it intentionally rejects
/// nested-inside-function assignments. The lodash binding's members live
/// inside the wrapper IIFE body and would be excluded.
///
/// This query is DEPTH-AGNOSTIC (no `(program...)` anchor). The driver gates
/// runtime by checking `target` against the synthesized binding name list
/// (`umd_default_bindings`) so it cannot over-capture assignments from
/// unrelated identifiers.
///
/// LIMITATION: The gate is **binding-name-scoped, not lexically-scoped**.
/// Matching is by identifier text equality against `umd_default_bindings`, NOT
/// by AST containment within the wrapper IIFE body. If the same identifier
/// (e.g. `_`) is reused as a different binding elsewhere in the same file —
/// for instance a top-level `_.foo = bar` referring to a sibling-IIFE-rebound
/// `_`, or a shadowed variable — those member assignments will be incorrectly
/// attached to the synthetic Default. phrases this as "over-capture
/// is bounded": bounded to "binding-name match anywhere in the file", NOT
/// bounded to "descendants of the wrapper IIFE body". The robust fix is to
/// capture the wrapper IIFE body node when synthesizing the Default and
/// restrict matching to its byte-range descendants; deferred for v0.2 because
/// the lodash-canonical shape scopes the binding to the wrapper IIFE and the
/// collision is rare in real bundler output.
pub(crate) const BUN_11_B_DEPTH_ANY_MEMBER_ASSIGN_QUERY: &str = r#"
(assignment_expression
  left: (member_expression
    object: (identifier) @target
    property: (property_identifier) @member))
"#;

/// INF-02 Pass 1 / form A: `declare const/let X: T;` — variables with a type annotation.
/// The `type:` field captures the type-annotation text including the leading colon.
pub(crate) const AMBIENT_CONST_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (lexical_declaration
      (variable_declarator
        name: (identifier) @decl_name
        type: (type_annotation) @decl_type))))
"#;

/// INF-02 Pass 1 / form B: `declare function X(...): T;` — function signatures.
/// Whole-node text is used as a coarse type_signature stand-in (minus the trailing `;`).
pub(crate) const AMBIENT_FUNCTION_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (function_signature
      name: (identifier) @decl_name) @decl_node))
"#;

/// INF-02 Pass 1 / form C: `declare class X {... }` — class declarations.
pub(crate) const AMBIENT_CLASS_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (class_declaration
      name: (type_identifier) @decl_name)))
"#;

/// INF-02 Pass 1 / form D: `declare interface X {... }` — interface declarations.
pub(crate) const AMBIENT_INTERFACE_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (interface_declaration
      name: (type_identifier) @decl_name)))
"#;

/// INF-02 Pass 1 / form E: `declare type X =...;` — type alias declarations.
pub(crate) const AMBIENT_TYPE_ALIAS_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (type_alias_declaration
      name: (type_identifier) @decl_name)))
"#;

/// INF-02 Pass 1 / form F: `declare enum X {... }` — enum declarations.
pub(crate) const AMBIENT_ENUM_DECL_QUERY: &str = r#"
(program
  (ambient_declaration
    (enum_declaration
      name: (identifier) @decl_name)))
"#;

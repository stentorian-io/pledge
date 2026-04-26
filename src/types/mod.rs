pub mod parser;
pub mod shape;

use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Major,
    Minor,
    Patch,
    Warning,
    Info,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub rule: &'static str,
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub message: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportKind {
    Named,
    Default,
}

/// Syntactic form of a declaration — what it IS, not how it's exported.
/// Orthogonal to ExportKind (Named|Default) which tracks the export mechanism.
/// Per D-03/D-04.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeclarationKind {
    Function,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Namespace,
    Variable,
    Unknown,
}

impl std::fmt::Display for DeclarationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationKind::Function => write!(f, "function"),
            DeclarationKind::Class => write!(f, "class"),
            DeclarationKind::Interface => write!(f, "interface"),
            DeclarationKind::TypeAlias => write!(f, "type alias"),
            DeclarationKind::Enum => write!(f, "enum"),
            DeclarationKind::Namespace => write!(f, "namespace"),
            DeclarationKind::Variable => write!(f, "variable"),
            DeclarationKind::Unknown => write!(f, "unknown"),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ExportSymbol {
    pub name: String,
    pub kind: ExportKind,
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub type_signature: Option<String>,
    pub overloads: Vec<shape::TypeShape>, // replaces type_shape per D-06
    pub decl_kinds: Vec<DeclarationKind>, // D-14 (Phase 18): Vec preserves declaration-merging hybrids like [Class, Namespace]
    pub type_guard_type: Option<String>,  // for REF-03 — guarded type name
    pub is_async: bool,                   // for BEH-01/BEH-02 — async detection
    pub is_generator: bool,               // for BEH-03 — generator detection
    pub is_reexport: bool,                // for GAP-15-05 — tracks external re-export entries
    // Phase 17 fields:
    pub is_type_only: bool,              // TLF-07 — export type * vs export *
    pub release_tag: Option<ReleaseTag>, // META-04 — TSDoc release tag (populated in Plan 17-05)
    /// TYPE-01 (Phase 20): captured `satisfies T` form of the exported value.
    /// Populated during the resolver post-parse populate pass from
    /// `extract_satisfies_types`; LSP hover enrichment fills the optional
    /// `narrowed` slot inside. `None` when the export has no `satisfies`
    /// clause. Diff engine compares this field in
    /// `type_diff.rs::diff_satisfies_types`; transition from Some→None
    /// fires Minor `type-narrowing-widened`.
    pub satisfies_type: Option<shape::TypeShape>,
    /// ARCH-02 (Phase 18): compound-component members attached via
    /// `Target.Member = X` or `Object.assign(Target, { Member: X })` patterns.
    /// Populated during the compound-assignment second pass in `parse_exports`
    /// (D-16 conservative: only when the target identifier is itself an exported
    /// symbol in the same file).
    pub compound_members: Vec<String>,
    /// INF-04 (Phase 22): `typeof import('./y').Z` references discovered in this
    /// symbol's type position. Populated by `extract_typeof_import_refs` in the
    /// resolver's post-parse populate phase (mirrors META-06 release-tag populate
    /// at resolver.rs:423-448 and Phase 20's satisfies_type sidecar at line 90).
    ///
    /// Consumed by the diff pipeline in type_diff.rs::diff_typeof_import_refs:
    ///   - LSP-available path: each ref resolved via textDocument/definition
    ///     (see CONTEXT.md D-05 kickoff spike for method choice); if the
    ///     resolved symbol is absent from the new-version ExportMap, fires
    ///     Major `type-reference-removed`.
    ///   - Degraded path: finding message embeds `(limitation: lsp-required)`.
    ///
    /// Empty `Vec` is the default — populated only when the type RHS contains
    /// `typeof import(...)` or `import(...)` member-access forms.
    pub typeof_import_refs: Vec<TypeofImportRef>,
    /// DOC-01 (Phase 25): true when this ExportSymbol was synthesized from a
    /// `module.exports = new Proxy(...)` or `module.exports = Reflect.construct(...)`
    /// RHS. Forwarded from RawExport.is_proxy_dispatched by the resolver.
    /// The comparator uses this flag to emit a paired Severity::Info finding
    /// with rule `limitation: proxy-dispatched-exports`. Default `false`.
    pub is_proxy_dispatched: bool,
}

/// INF-04 (Phase 22): a single `typeof import('./x').Y` or `import('./x').Y`
/// type reference captured during parse. Positions are 1-indexed per the
/// project's ExportSymbol convention; the LSP call site adjusts to 0-indexed.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TypeofImportRef {
    /// The import specifier, e.g., `"./y"` or `"../shared/types"`.
    pub specifier: String,
    /// The property accessed, e.g., `"Z"` in `typeof import('./y').Z`.
    pub property: String,
    /// 1-indexed line of the `property` token — used as the LSP request position.
    pub property_line: u32,
    /// 1-indexed column of the `property` token.
    pub property_col: u32,
}

/// D-03 (Phase 18): SymbolSpace is a first-class key dimension — `export type Foo` and
/// `export const Foo` get distinct map entries and diff independently against
/// their own space.
///
/// Primary-assignment convention (per Phase 18 Research Open Q2):
/// - `interface` / `type alias` / `export type { X }` -> `SymbolSpace::Type`
/// - `const` / `let` / `var` / `function` / `default export` / `class` / `enum` -> `SymbolSpace::Value`
/// - `namespace` -> `SymbolSpace::Namespace`
///
/// Rationale: consumers import values (`import { Foo } from 'pkg'`); class/enum
/// are primarily accessed at the value level, and their namespace-like behavior
/// is carried by `decl_kinds` on the merged symbol, not by the primary key space.
#[allow(dead_code)]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum SymbolSpace {
    Type,
    Value,
    Namespace,
}

/// D-01 (Phase 18): Composite key for ExportMap, preserving multi-subpath/condition/space identity.
///
/// Derived Hash/Eq means field-wise hashing of the composite — no manual impl needed.
/// - `subpath`: canonical `"."` for root (D-02). Fallback entry points (main/module/
///   types/typings/browser) all map to `"."`.
/// - `condition`: `None` for fallback entry points; `Some("import"/"require"/"types"/...)`
///   for conditional-export keys.
/// - `space`: see SymbolSpace (D-03).
/// - `name`: the exported symbol name, unchanged from the pre-Phase-18 flat key.
#[allow(dead_code)]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExportKey {
    pub subpath: String,
    pub condition: Option<String>,
    pub space: SymbolSpace,
    pub name: String,
}

impl ExportKey {
    /// Root-subpath + no-condition + Value-space constructor — the common case
    /// for tests and for fallback entry-point insertions. Per D-02 + D-03.
    #[allow(dead_code)]
    pub fn new_root_value(name: &str) -> Self {
        ExportKey {
            subpath: ".".to_string(),
            condition: None,
            space: SymbolSpace::Value,
            name: name.to_string(),
        }
    }

    /// Full constructor. Use when subpath, condition, or space differ from the
    /// root-value defaults.
    #[allow(dead_code)]
    pub fn new(subpath: &str, condition: Option<&str>, space: SymbolSpace, name: &str) -> Self {
        ExportKey {
            subpath: subpath.to_string(),
            condition: condition.map(|c| c.to_string()),
            space,
            name: name.to_string(),
        }
    }
}

/// INF-05 (Phase 19): primary export symbol map plus a per-source-file
/// triple-slash directive sidecar.
///
/// Internally a `HashMap<ExportKey, ExportSymbol>` (preserved from pre-Phase-19
/// behavior) with a parallel `BTreeMap<PathBuf, Vec<DirectiveRecord>>` for
/// directives. `Deref`/`DerefMut` to the inner HashMap keep every existing
/// call site (`.get`, `.insert`, `.keys`, `.iter`, `.retain`, `.contains_key`,
/// `.clone`, `for (k, v) in &map`, etc.) working without change.
///
/// Files with no directives are absent from the map.
///
/// # Invariants (WR-01, Phase 19 review)
///
/// - `inner` maps by `ExportKey` (subpath/condition/space/name) — per-symbol.
/// - `directives` maps by source-file `PathBuf` — per-file.
///
/// The two maps are independently keyed; mutating one does NOT require a
/// matching mutation on the other. A file can appear in `directives` with
/// zero corresponding `ExportSymbol`s (pure-directive file, e.g. a rollup
/// chunk that only contains `/// <reference />`), and a file can back
/// symbols in `inner` without any directives. Callers reaching through
/// `Deref`/`DerefMut` to mutate the inner `HashMap` therefore never need
/// to touch `directives`, and vice versa: the two maps cannot drift
/// because they index different keys.
///
/// The `From<HashMap<ExportKey, ExportSymbol>>` impl intentionally
/// constructs with `directives: BTreeMap::new()`. Test-only constructions
/// that go through `From` are accepting "no directive coverage" as an
/// explicit design choice; production discovery always populates
/// `directives` via `map.directives.insert(...)` in the resolver.
///
/// In debug builds, `debug_assert_invariants` enforces the
/// `typeof_targets ⊆ inner` direction at every public mutation entry
/// (`record_typeof_target`, `From<HashMap>` construction). The
/// `directives ⊥ inner` independence is NOT checked at runtime — it is
/// a keying-disjoint property documented as a design choice above.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ExportMap {
    inner: HashMap<ExportKey, ExportSymbol>,
    /// INF-05 (Phase 19): per-source-file triple-slash directive records.
    ///
    /// Key: relative path to the source file (same as `ExportSymbol.file`).
    /// Value: ordered directive records in document order.
    ///
    /// Populated by `CommentIndex::extract_directives` during resolver
    /// insertion. See the struct-level `# Invariants` block for the
    /// keying-disjoint reasoning that lets `inner` and `directives`
    /// be mutated independently without drift.
    pub directives: std::collections::BTreeMap<
        std::path::PathBuf,
        Vec<crate::discover::comment_scan::DirectiveRecord>,
    >,
    /// INF-04 (Phase 22): resolved LSP `textDocument/definition` Locations
    /// for each ExportSymbol's `typeof_import_refs` entries.
    ///
    /// Outer key: ExportKey of the symbol whose `typeof_import_refs` is
    /// non-empty.
    /// Inner key: the index of the ref within that symbol's
    /// `typeof_import_refs` Vec (stable iteration order).
    /// Inner value: `Some(Location)` = LSP resolved to a concrete declaration
    /// site; `None` = LSP ran but returned no result (timeout / no
    /// definition); absent = LSP was never run for this ref (degraded path).
    ///
    /// Populated by `enrich_exports` second pass (`src/lsp/mod.rs`). Consumed
    /// by `diff_typeof_import_refs` (`src/compare/type_diff.rs`).
    pub typeof_targets: std::collections::HashMap<
        ExportKey,
        std::collections::HashMap<usize, Option<async_lsp::lsp_types::Location>>,
    >,
}

impl ExportMap {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// WR-01 (Phase 19 review): runtime guard for the documented invariants.
    /// Cheap enough for debug builds — O(typeof_targets keys). Production
    /// builds compile this out via `debug_assert!`.
    ///
    /// Invariants checked:
    /// - Every key in `typeof_targets` is also present in `inner` (a typeof
    ///   sidecar entry without a backing ExportSymbol is a desync).
    ///
    /// NOT checked (intentional, per the # Invariants block above):
    /// - `directives` keys do NOT need to correspond to any
    ///   `ExportSymbol.file` — pure-directive files (rollup chunks with
    ///   `/// <reference />` and zero exports) are legal.
    fn debug_assert_invariants(&self) {
        debug_assert!(
            self.typeof_targets.keys().all(|k| self.inner.contains_key(k)),
            "ExportMap WR-01 invariant violated: typeof_targets contains key absent from inner"
        );
    }

    /// INF-04 (Phase 22): record the resolved Location for a given
    /// ExportSymbol's `typeof_import_refs` entry at the given ref index.
    /// Called by the second-pass LSP dispatch in `enrich_exports`.
    #[allow(dead_code)]
    pub fn record_typeof_target(
        &mut self,
        key: &ExportKey,
        ref_idx: usize,
        location: Option<async_lsp::lsp_types::Location>,
    ) {
        self.typeof_targets
            .entry(key.clone())
            .or_default()
            .insert(ref_idx, location);
        self.debug_assert_invariants();
    }

    /// INF-04 (Phase 22): look up a recorded target. Returns:
    /// - `Some(&Some(loc))` — LSP resolved to `loc`
    /// - `Some(&None)`      — LSP ran but returned no result
    /// - `None`             — LSP was never run (degraded path)
    #[allow(dead_code)]
    pub fn typeof_target(
        &self,
        key: &ExportKey,
        ref_idx: usize,
    ) -> Option<&Option<async_lsp::lsp_types::Location>> {
        self.typeof_targets.get(key).and_then(|m| m.get(&ref_idx))
    }
}

impl std::ops::Deref for ExportMap {
    type Target = HashMap<ExportKey, ExportSymbol>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ExportMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Preserve the ergonomic of `ExportMap: HashMap<...> = HashMap::new()`
/// that appears at ~4 test construction sites in `src/compare/mod.rs`.
impl From<HashMap<ExportKey, ExportSymbol>> for ExportMap {
    fn from(inner: HashMap<ExportKey, ExportSymbol>) -> Self {
        let out = ExportMap {
            inner,
            directives: std::collections::BTreeMap::new(),
            typeof_targets: std::collections::HashMap::new(),
        };
        out.debug_assert_invariants();
        out
    }
}

/// Enable `for (k, v) in &export_map { ... }`. `Deref` on its own does NOT
/// make the struct iterable because `IntoIterator` is only attempted on the
/// concrete type in `for` loops.
impl<'a> IntoIterator for &'a ExportMap {
    type Item = (&'a ExportKey, &'a ExportSymbol);
    type IntoIter = std::collections::hash_map::Iter<'a, ExportKey, ExportSymbol>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

/// Enable `for (k, v) in &mut export_map { ... }`.
impl<'a> IntoIterator for &'a mut ExportMap {
    type Item = (&'a ExportKey, &'a mut ExportSymbol);
    type IntoIter = std::collections::hash_map::IterMut<'a, ExportKey, ExportSymbol>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

// ─── Class shape model ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassMemberKind {
    Constructor,
    Method,
    Property,
    Getter,
    Setter,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ClassMember {
    pub name: String,
    pub kind: ClassMemberKind,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_readonly: bool,
    pub param_count: usize,
    pub type_signature: Option<String>,
    // Phase 17 TLF fields:
    pub is_optional: bool, // TLF-02/IFT-01 — replaces type_signature=="optional" stopgap
    pub is_accessor: bool, // TLF-06 — TS 4.9 accessor modifier
    pub this_param_type: Option<String>, // TLF-03 — this parameter type annotation
    // Phase 19 CLS-10 foundation (Plan 01):
    pub decorators: Vec<DecoratorSpec>, // CLS-10 — populated by extract_decorators in Plan 03
    // Phase 21 CLS-11:
    pub is_private: bool, // CLS-11 — `#`-prefixed ES2022 private field or method (distinct from TS `private` keyword)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ClassShape {
    pub members: Vec<ClassMember>,
    pub is_abstract: bool,
    pub extends_text: Option<String>,
    pub implements_list: Vec<String>,
    pub type_params: Vec<TypeParam>,
    pub constructor_param_optionality: Vec<bool>, // IFT-03 — per-position optional flag for constructor params
    // Phase 19 CLS-10 foundation (Plan 01):
    pub decorators: Vec<DecoratorSpec>, // CLS-10 — class-level decorators; populated by Plan 03
    // Phase 21 CLS-12:
    pub has_dynamic_static_key: bool, // CLS-12 — true if any `static { Foo[x] = ... }` subscript assignment was seen; consumed by Plan 21-02
}

// ─── Enum shape model ───────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnumValueKind {
    Numeric,
    StringLiteral,
    OtherExpr,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: String,
    pub raw_value: Option<String>,
    pub computed_value: Option<i64>,
    pub value_kind: EnumValueKind,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EnumShape {
    pub members: Vec<EnumMember>,
    pub is_const: bool,
}

// ─── Generic type parameter model ───────────────────────────────────────────

/// TLF-05: Generic type parameter variance modifier.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Variance {
    In,
    Out,
    InOut,
}

/// META-04 / META-06 (Phase 19): TSDoc release tag extracted from `@internal`,
/// `@alpha`, `@beta`, `@public`, `@deprecated` tags in block comments adjacent
/// to exported symbols. Populated by the comment_scan post-process pass at
/// resolver insertion (src/discover/resolver.rs:~420).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseTag {
    Internal,
    Alpha,
    Beta,
    Public,
    Deprecated,
}

/// CLS-10 (Phase 19): a single decorator on a class, method, field, or parameter.
/// `name` is the decorator's primary identifier (for `@ns.foo` takes the last
/// member). `args` is the source-text bytes of the `arguments` field of the
/// decorator's call_expression child, or None for bare decorators like `@Component`.
/// Populated by `src/discover/tree_query.rs::extract_decorators` in Plan 03.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoratorSpec {
    pub name: String,
    pub args: Option<String>,
}

/// TLF-01: Interface construct signature shape (`new (params): ReturnType`).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ConstructSignature {
    pub param_count: usize,
    pub return_text: Option<String>,
    pub is_abstract: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub constraint_text: Option<String>,
    pub default_text: Option<String>,
    pub constraint_shape: Option<shape::TypeShape>,
    pub is_const: bool, // GAP-15-07 — const type parameter modifier (TS 4.9+)
    pub variance: Option<Variance>, // TLF-05
}

// ─── Tests for new data model types ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_rule_class_constructor_param_added_value() {
        assert_eq!(
            crate::compare::subtype::RULE_CLASS_CONSTRUCTOR_PARAM_ADDED,
            "class-constructor-param-added"
        );
    }

    #[test]
    fn test_rule_enum_member_removed_value() {
        assert_eq!(
            crate::compare::subtype::RULE_ENUM_MEMBER_REMOVED,
            "enum-member-removed"
        );
    }

    #[test]
    fn test_rule_generic_param_added_value() {
        assert_eq!(
            crate::compare::subtype::RULE_GENERIC_PARAM_ADDED,
            "generic-param-added"
        );
    }

    #[test]
    fn test_class_shape_construction() {
        let member = ClassMember {
            name: "doWork".to_string(),
            kind: ClassMemberKind::Method,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count: 1,
            type_signature: None,
            is_optional: false,
            is_accessor: false,
            this_param_type: None,
            decorators: Vec::new(),
            is_private: false,
        };
        let shape = ClassShape {
            members: vec![member],
            is_abstract: false,
            extends_text: Some("Base".to_string()),
            implements_list: vec!["IFoo".to_string()],
            type_params: vec![],
            constructor_param_optionality: vec![],
            decorators: Vec::new(),
            has_dynamic_static_key: false,
        };
        assert_eq!(shape.members.len(), 1);
        assert!(!shape.is_abstract);
        assert_eq!(shape.extends_text.as_deref(), Some("Base"));
        assert_eq!(shape.implements_list.len(), 1);
    }

    #[test]
    fn test_enum_shape_construction() {
        let member = EnumMember {
            name: "Foo".to_string(),
            raw_value: None,
            computed_value: Some(0),
            value_kind: EnumValueKind::Numeric,
        };
        let shape = EnumShape {
            members: vec![member],
            is_const: true,
        };
        assert!(shape.is_const);
        assert_eq!(shape.members.len(), 1);
    }

    #[test]
    fn test_type_param_construction() {
        let param = TypeParam {
            name: "T".to_string(),
            constraint_text: Some("string".to_string()),
            default_text: Some("never".to_string()),
            constraint_shape: None,
            is_const: false,
            variance: None,
        };
        assert_eq!(param.name, "T");
        assert_eq!(param.constraint_text.as_deref(), Some("string"));
        assert_eq!(param.default_text.as_deref(), Some("never"));
        assert!(param.constraint_shape.is_none());
    }

    #[test]
    fn test_visibility_public_is_default() {
        // Default visibility is Visibility::Public (TypeScript default)
        let vis = Visibility::Public;
        assert_eq!(vis, Visibility::Public);
        assert_ne!(vis, Visibility::Protected);
        assert_ne!(vis, Visibility::Private);
    }

    #[test]
    fn test_enum_value_kind_variants() {
        let numeric = EnumValueKind::Numeric;
        let string_lit = EnumValueKind::StringLiteral;
        let other = EnumValueKind::OtherExpr;
        assert_eq!(numeric, EnumValueKind::Numeric);
        assert_eq!(string_lit, EnumValueKind::StringLiteral);
        assert_eq!(other, EnumValueKind::OtherExpr);
    }

    #[test]
    fn test_class_member_kind_variants() {
        assert_eq!(ClassMemberKind::Constructor, ClassMemberKind::Constructor);
        assert_eq!(ClassMemberKind::Method, ClassMemberKind::Method);
        assert_eq!(ClassMemberKind::Property, ClassMemberKind::Property);
        assert_ne!(ClassMemberKind::Constructor, ClassMemberKind::Method);
        assert_eq!(ClassMemberKind::Getter, ClassMemberKind::Getter);
        assert_eq!(ClassMemberKind::Setter, ClassMemberKind::Setter);
        assert_ne!(ClassMemberKind::Getter, ClassMemberKind::Method);
        assert_ne!(ClassMemberKind::Setter, ClassMemberKind::Getter);
    }

    #[test]
    fn inf_04_typeof_import_ref_constructs_cleanly() {
        // INF-04 (Phase 22): TypeofImportRef sidecar struct construction +
        // default `typeof_import_refs == Vec::new()` on ExportSymbol.
        let r = TypeofImportRef {
            specifier: "./y".into(),
            property: "Z".into(),
            property_line: 1,
            property_col: 40,
        };
        assert_eq!(r.specifier, "./y");
        assert_eq!(r.property, "Z");
        assert_eq!(r.property_line, 1);
        assert_eq!(r.property_col, 40);
    }

    #[test]
    fn inf_04_export_symbol_default_typeof_refs_empty() {
        // INF-04 (Phase 22): a freshly-constructed ExportSymbol has an empty
        // typeof_import_refs Vec by default.
        let sym = ExportSymbol {
            name: "foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        };
        assert!(sym.typeof_import_refs.is_empty());
    }

    #[test]
    fn test_export_symbol_unchanged() {
        // Ensure existing ExportSymbol still compiles correctly after changes
        let sym = ExportSymbol {
            name: "foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("src/index.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        };
        assert_eq!(sym.name, "foo");
    }

    #[test]
    fn test_severity_warning_sorts_last() {
        let mut severities = vec![
            Severity::Warning,
            Severity::Major,
            Severity::Patch,
            Severity::Minor,
        ];
        severities.sort();
        assert_eq!(
            severities,
            vec![
                Severity::Major,
                Severity::Minor,
                Severity::Patch,
                Severity::Warning
            ]
        );
    }

    #[test]
    fn test_severity_info_sorts_last() {
        let mut severities = vec![
            Severity::Info,
            Severity::Warning,
            Severity::Major,
            Severity::Patch,
            Severity::Minor,
        ];
        severities.sort();
        assert_eq!(
            severities,
            vec![
                Severity::Major,
                Severity::Minor,
                Severity::Patch,
                Severity::Warning,
                Severity::Info
            ]
        );
    }

    #[test]
    fn test_export_symbol_async_generator_fields() {
        let sym = ExportSymbol {
            name: "foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: vec![DeclarationKind::Function],
            type_guard_type: None,
            is_async: true,
            is_generator: true,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        };
        assert!(sym.is_async);
        assert!(sym.is_generator);
    }

    #[test]
    fn tlf_05_variance_variants_exist() {
        let _in_var = Variance::In;
        let _out_var = Variance::Out;
        let _inout_var = Variance::InOut;
        assert_eq!(Variance::In, Variance::In);
        assert_ne!(Variance::In, Variance::Out);
        assert_ne!(Variance::In, Variance::InOut);
        assert_ne!(Variance::Out, Variance::InOut);
    }

    #[test]
    fn tlf_06_class_member_is_accessor_defaults_false() {
        let m = ClassMember {
            name: "foo".into(),
            kind: ClassMemberKind::Property,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count: 0,
            type_signature: None,
            is_optional: false,
            is_accessor: false,
            this_param_type: None,
            decorators: Vec::new(),
            is_private: false,
        };
        assert!(!m.is_accessor);
        assert!(!m.is_optional);
        assert!(m.this_param_type.is_none());
    }

    #[test]
    fn tlf_01_construct_signature_struct_shape() {
        let cs = ConstructSignature {
            param_count: 2,
            return_text: Some("Foo".into()),
            is_abstract: false,
        };
        assert_eq!(cs.param_count, 2);
        assert_eq!(cs.return_text.as_deref(), Some("Foo"));
        assert!(!cs.is_abstract);
    }

    #[test]
    fn tlf_07_export_symbol_is_type_only_field_exists() {
        let sym = ExportSymbol {
            name: "mod".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: true,
            is_type_only: true,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        };
        assert!(sym.is_type_only);
    }

    #[test]
    fn tlf_type_param_variance_field_exists() {
        let tp = TypeParam {
            name: "T".to_string(),
            constraint_text: None,
            default_text: None,
            constraint_shape: None,
            is_const: false,
            variance: Some(Variance::In),
        };
        assert_eq!(tp.variance, Some(Variance::In));
    }

    #[test]
    fn tlf_release_tag_variants_exist() {
        let _ = ReleaseTag::Internal;
        let _ = ReleaseTag::Alpha;
        let _ = ReleaseTag::Beta;
        let _ = ReleaseTag::Public;
        let _ = ReleaseTag::Deprecated;
        assert_eq!(ReleaseTag::Internal, ReleaseTag::Internal);
        assert_ne!(ReleaseTag::Internal, ReleaseTag::Public);
    }

    #[test]
    fn cls_10_decorator_spec_construction() {
        let d = DecoratorSpec {
            name: "Component".to_string(),
            args: None,
        };
        assert_eq!(d.name, "Component");
        assert_eq!(d.args, None);
        let d2 = DecoratorSpec {
            name: "Route".to_string(),
            args: Some("('/users')".to_string()),
        };
        assert_eq!(d2.args.as_deref(), Some("('/users')"));
    }

    // ─── Phase 18: ExportKey, SymbolSpace, decl_kinds ───────────────────────

    #[test]
    fn test_export_key_new_root_value_canonical_shape() {
        let k = ExportKey::new_root_value("foo");
        assert_eq!(k.subpath, ".");
        assert_eq!(k.condition, None);
        assert_eq!(k.space, SymbolSpace::Value);
        assert_eq!(k.name, "foo");
    }

    #[test]
    fn test_export_key_hash_eq_same_key_equal() {
        let a = ExportKey::new("./sub", Some("import"), SymbolSpace::Value, "foo");
        let b = ExportKey::new("./sub", Some("import"), SymbolSpace::Value, "foo");
        assert_eq!(a, b);
        use std::collections::HashMap;
        let mut m: HashMap<ExportKey, u32> = HashMap::new();
        m.insert(a.clone(), 1);
        m.insert(b, 2);
        assert_eq!(m.len(), 1); // same key overwrites
        assert_eq!(m.get(&a), Some(&2));
    }

    #[test]
    fn test_export_key_subpath_distinguishes() {
        let a = ExportKey::new(".", None, SymbolSpace::Value, "foo");
        let b = ExportKey::new("./sub", None, SymbolSpace::Value, "foo");
        assert_ne!(a, b);
    }

    #[test]
    fn test_export_key_space_distinguishes() {
        let v = ExportKey::new(".", None, SymbolSpace::Value, "Foo");
        let t = ExportKey::new(".", None, SymbolSpace::Type, "Foo");
        assert_ne!(v, t);
    }

    #[test]
    fn test_export_key_condition_distinguishes() {
        let import = ExportKey::new(".", Some("import"), SymbolSpace::Value, "foo");
        let require = ExportKey::new(".", Some("require"), SymbolSpace::Value, "foo");
        assert_ne!(import, require);
    }

    #[test]
    fn test_symbol_space_variants_derive_hash() {
        use std::collections::HashSet;
        let mut s: HashSet<SymbolSpace> = HashSet::new();
        s.insert(SymbolSpace::Type);
        s.insert(SymbolSpace::Value);
        s.insert(SymbolSpace::Namespace);
        s.insert(SymbolSpace::Value); // duplicate insert
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn test_hybrid_decl_kinds_carries_multiple() {
        // D-14: Vec<DeclarationKind> preserves hybrid identity (class+namespace).
        let sym = ExportSymbol {
            name: "User".to_string(),
            kind: ExportKind::Named,
            file: std::path::PathBuf::from("user.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: vec![DeclarationKind::Class, DeclarationKind::Namespace],
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        };
        assert_eq!(sym.decl_kinds.len(), 2);
        assert!(sym.decl_kinds.contains(&DeclarationKind::Class));
        assert!(sym.decl_kinds.contains(&DeclarationKind::Namespace));
    }

    // ─── WR-01 (Phase 19 review): ExportMap structural guard tests ──────────

    fn make_sym(file: &str) -> ExportSymbol {
        ExportSymbol {
            name: "x".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from(file),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        }
    }

    /// WR-01: A directives entry whose key matches no ExportSymbol.file is
    /// legal (pure-directive rollup chunk). Must NOT panic.
    #[test]
    fn wr_01_directive_only_file_is_legal() {
        let mut map = ExportMap::new();
        // Insert a symbol for "src/x.ts"
        let key = ExportKey::new_root_value("x");
        map.inner.insert(key, make_sym("src/x.ts"));
        // Insert a directive for a DIFFERENT file (no matching ExportSymbol)
        map.directives.insert(
            PathBuf::from("dist/chunks/chunk-*.js"),
            vec![],
        );
        // debug_assert_invariants is not called by directives mutation (by design);
        // call it explicitly to assert it does not panic.
        map.debug_assert_invariants();
    }

    /// WR-01: Overwriting the same (ExportKey, ExportSymbol) via inner.insert
    /// (HashMap semantics: overwrite, not desync) must NOT panic.
    #[test]
    fn wr_01_overwrite_inner_is_safe() {
        let mut map = ExportMap::new();
        let key = ExportKey::new_root_value("y");
        map.inner.insert(key.clone(), make_sym("src/y.ts"));
        // Overwrite with a different symbol at the same key
        map.inner.insert(key, make_sym("src/y2.ts"));
        map.debug_assert_invariants();
    }

    /// WR-01: record_typeof_target called with a key that IS present in inner
    /// must NOT panic.
    #[test]
    fn wr_01_record_typeof_target_with_inner_present_ok() {
        let mut map = ExportMap::new();
        let key = ExportKey::new_root_value("z");
        map.inner.insert(key.clone(), make_sym("src/z.ts"));
        map.record_typeof_target(&key, 0, None);
        // record_typeof_target calls debug_assert_invariants internally;
        // reaching here means it did not panic.
    }

    /// WR-01: typeof_targets referencing a key NOT in inner is a desync;
    /// debug_assert_invariants MUST panic in debug builds.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "WR-01 invariant violated")]
    fn wr_01_orphan_typeof_target_panics() {
        let mut map = ExportMap::new();
        let key = ExportKey::new_root_value("orphan");
        // Inject directly into typeof_targets WITHOUT adding to inner
        map.typeof_targets
            .entry(key)
            .or_default()
            .insert(0, None);
        // This call must panic because typeof_targets has a key absent from inner
        map.debug_assert_invariants();
    }
}

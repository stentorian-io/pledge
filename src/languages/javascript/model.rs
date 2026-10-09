//! JS/TS export identity and class surfaces, with explicit TypeScript type evidence.

use crate::languages::typescript::model as ts_model;
use crate::languages::typescript::syntax::comments;
use crate::languages::typescript::types::shape as type_shapes;
use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportKind {
    Named,
    Default,
}

/// Syntactic form of a declaration — what it IS, not how it's exported.
/// Orthogonal to ExportKind (Named|Default) which tracks the export mechanism.
/// Per /.
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
            DeclarationKind::Interface => {
                write!(f, "interface")
            }
            DeclarationKind::TypeAlias => {
                write!(f, "type alias")
            }
            DeclarationKind::Enum => write!(f, "enum"),
            DeclarationKind::Namespace => {
                write!(f, "namespace")
            }
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
    pub overloads: Vec<type_shapes::TypeShape>, // All callable overloads.
    pub decl_kinds: Vec<DeclarationKind>, // Preserves declaration-merging hybrids like [Class, Namespace].
    pub type_guard_type: Option<String>,  // for REF-03 — guarded type name
    pub is_async: bool,                   // for BEH-01/BEH-02 — async detection
    pub is_generator: bool,               // for BEH-03 — generator detection
    pub is_reexport: bool,                // for tracks external re-export entries
    pub is_type_only: bool,               // TLF-07 — export type * vs export *
    pub release_tag: Option<ts_model::ReleaseTag>, // META-04 — TSDoc release tag.
    /// TYPE-01: captured `satisfies T` form of the exported value.
    /// Populated during the resolver post-parse populate pass from
    /// `extract_satisfies_types`; LSP hover enrichment fills the optional
    /// `narrowed` slot inside. `None` when the export has no `satisfies`
    /// clause. Diff engine compares this field in
    /// `typescript::compare::types::diff_satisfies_types`; transition from Some→None
    /// fires Minor `type-narrowing-widened`.
    pub satisfies_type: Option<type_shapes::TypeShape>,
    /// ARCH-02: compound-component members attached via
    /// `Target.Member = X` or `Object.assign(Target, { Member: X })` patterns.
    /// Populated during the compound-assignment second pass in `parse_exports`
    /// (conservative: only when the target identifier is itself an exported
    /// symbol in the same file).
    pub compound_members: Vec<String>,
    /// INF-04: `typeof import('./y').Z` references discovered in this
    /// symbol's type position. Populated by `extract_typeof_import_refs` in the
    /// resolver's post-parse pass (mirrors META-06 release-tag populate
    /// at resolver.rs:423-448 and satisfies_type sidecar at line 90).
    ///
    /// Consumed by the diff pipeline in typescript::compare::types::diff_typeof_import_refs:
    ///  - LSP-available path: each ref is resolved via textDocument/definition.
    ///    If the resolved symbol is absent from the new-version ExportMap, fires
    ///    Major `type-reference-removed`.
    ///  - Degraded path: finding message embeds `(limitation: lsp-required)`.
    ///
    /// Empty `Vec` is the default — populated only when the type RHS contains
    /// `typeof import(...)` or `import(...)` member-access forms.
    pub typeof_import_refs: Vec<ts_model::TypeofImportRef>,
    /// DOC-01: true when this ExportSymbol was synthesized from a
    /// `module.exports = new Proxy(...)` or `module.exports = Reflect.construct(...)`
    /// RHS. Forwarded from RawExport.is_proxy_dispatched by the resolver.
    /// The comparator uses this flag to emit a paired Severity::Info finding
    /// with rule `limitation: proxy-dispatched-exports`. Default `false`.
    pub is_proxy_dispatched: bool,
    /// Original declaration name, independent of the public export alias.
    pub local_name: Option<String>,
}

///: SymbolSpace is a first-class key dimension — `export type Foo` and
/// `export const Foo` get distinct map entries and diff independently against
/// their own space.
///
/// Primary-assignment convention:
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
    /// Runtime symbol properties, distinct from string-named exports.
    Symbol,
    Type,
    Value,
    Namespace,
}

///: Composite key for ExportMap, preserving multi-subpath/condition/space identity.
///
/// Derived Hash/Eq means field-wise hashing of the composite — no manual impl needed.
/// - `subpath`: canonical `"."` for root. Fallback entry points (main/module/
///   types/typings/browser) all map to `"."`.
/// - `condition`: `None` for fallback entry points; `Some("import"/"require"/"types"/...)`
///   for conditional-export keys.
/// - `space`: see SymbolSpace.
/// - `name`: the exported symbol name, unchanged from the flat key.
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
    /// for tests and for fallback entry-point insertions. Per +.
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

/// INF-05: primary export symbol map plus a per-source-file
/// triple-slash directive sidecar.
///
/// Internally a `HashMap<ExportKey, ExportSymbol>` (preserved from
/// behavior) with a parallel `BTreeMap<PathBuf, Vec<DirectiveRecord>>` for
/// directives. `Deref`/`DerefMut` to the inner HashMap keep every existing
/// call site (`.get`, `.insert`, `.keys`, `.iter`, `.retain`, `.contains_key`,
/// `.clone`, `for (k, v) in &map`, etc.) working without change.
///
/// Files with no directives are absent from the map.
///
/// # Invariants
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
    /// INF-05: per-source-file triple-slash directive records.
    ///
    /// Key: relative path to the source file (same as `ExportSymbol.file`).
    /// Value: ordered directive records in document order.
    ///
    /// Populated by `CommentIndex::extract_directives` during resolver
    /// insertion. See the struct-level `# Invariants` block for the
    /// keying-disjoint reasoning that lets `inner` and `directives`
    /// be mutated independently without drift.
    pub directives: std::collections::BTreeMap<std::path::PathBuf, Vec<comments::DirectiveRecord>>,
    /// INF-04: resolved LSP `textDocument/definition` Locations
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
    /// Populated by `enrich_exports` second pass (`src/languages/typescript/compiler/mod.rs`). Consumed
    /// by `diff_typeof_import_refs` (`src/languages/typescript/compare/types.rs`).
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

    ///: runtime guard for the documented invariants.
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
            self.typeof_targets
                .keys()
                .all(|k| self.inner.contains_key(k)),
            "ExportMap WR-01 invariant violated: typeof_targets contains key absent from inner"
        );
    }

    /// INF-04: record the resolved Location for a given
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

    /// INF-04: look up a recorded target. Returns:
    /// - `Some(&Some(loc))` — LSP resolved to `loc`
    /// - `Some(&None)` — LSP ran but returned no result
    /// - `None` — LSP was never run (degraded path)
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

/// Construct an export map from symbols, with empty directive and type-reference metadata.
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

/// Enable `for (k, v) in &export_map {... }`. `Deref` on its own does NOT
/// make the struct iterable because `IntoIterator` is only attempted on the
/// concrete type in `for` loops.
impl<'a> IntoIterator for &'a ExportMap {
    type Item = (&'a ExportKey, &'a ExportSymbol);
    type IntoIter = std::collections::hash_map::Iter<'a, ExportKey, ExportSymbol>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

/// Enable `for (k, v) in &mut export_map {... }`.
impl<'a> IntoIterator for &'a mut ExportMap {
    type Item = (&'a ExportKey, &'a mut ExportSymbol);
    type IntoIter = std::collections::hash_map::IterMut<'a, ExportKey, ExportSymbol>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

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
    // TLF fields:
    pub is_optional: bool, // TLF-02/IFT-01 — replaces type_signature=="optional" stopgap
    pub is_accessor: bool, // TLF-06 — TS 4.9 accessor modifier
    pub this_param_type: Option<String>, // TLF-03 — this parameter type annotation
    pub decorators: Vec<ts_model::DecoratorSpec>, // CLS-10 — member decorators from the AST.
    // CLS-11:
    pub is_private: bool, // CLS-11 — `#`-prefixed ES2022 private field or method (distinct from TS `private` keyword)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ClassShape {
    pub members: Vec<ClassMember>,
    pub is_abstract: bool,
    pub extends_text: Option<String>,
    pub implements_list: Vec<String>,
    pub type_params: Vec<ts_model::TypeParam>,
    pub constructor_param_optionality: Vec<bool>, // IFT-03 — per-position optional flag for constructor params
    pub decorators: Vec<ts_model::DecoratorSpec>, // CLS-10 — class-level decorators.
    // CLS-12:
    pub has_dynamic_static_key: bool, // CLS-12 — static assignment with an unresolved key.
}

#[cfg(test)]
mod tests;

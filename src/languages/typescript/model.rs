//! TypeScript declaration metadata carried through discovery and comparison.

use crate::languages::typescript::types::shape as type_shapes;
/// INF-04: a single `typeof import('./x').Y` or `import('./x').Y`
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

/// TLF-05: Generic type parameter variance modifier.
#[allow(dead_code)]
#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Variance {
    In,
    Out,
    InOut,
}

/// META-04 / META-06: TSDoc release tag extracted from `@internal`,
/// `@alpha`, `@beta`, `@public`, `@deprecated` tags in block comments adjacent
/// to exported symbols. Populated by the comment scan during
/// resolver insertion (src/ecosystems/node/discovery/resolver.rs).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseTag {
    Internal,
    Alpha,
    Beta,
    Public,
    Deprecated,
}

/// CLS-10: a single decorator on a class, method, field, or parameter.
/// `name` is the decorator's primary identifier (for `@ns.foo` takes the last
/// member). `args` is the source-text bytes of the `arguments` field of the
/// decorator's call_expression child, or None for bare decorators like `@Component`.
/// Populated by `syntax::annotations::extract_decorators`.
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
    pub constraint_shape: Option<type_shapes::TypeShape>,
    pub is_const: bool,             // const type parameter modifier (TS 4.9+)
    pub variance: Option<Variance>, // TLF-05
}

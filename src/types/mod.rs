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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub overloads: Vec<shape::TypeShape>,       // replaces type_shape per D-06
    pub decl_kind: Option<DeclarationKind>,      // per D-03
    pub type_guard_type: Option<String>,          // for REF-03 — guarded type name
    pub is_async: bool,                           // for BEH-01/BEH-02 — async detection
    pub is_generator: bool,                       // for BEH-03 — generator detection
    pub is_reexport: bool,                        // for GAP-15-05 — tracks external re-export entries
}

#[allow(dead_code)]
pub type ExportMap = HashMap<String, ExportSymbol>;

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
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ClassShape {
    pub members: Vec<ClassMember>,
    pub is_abstract: bool,
    pub extends_text: Option<String>,
    pub implements_list: Vec<String>,
    pub type_params: Vec<TypeParam>,
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

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub constraint_text: Option<String>,
    pub default_text: Option<String>,
    pub constraint_shape: Option<shape::TypeShape>,
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
        };
        let shape = ClassShape {
            members: vec![member],
            is_abstract: false,
            extends_text: Some("Base".to_string()),
            implements_list: vec!["IFoo".to_string()],
            type_params: vec![],
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
            decl_kind: None,
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
        };
        assert_eq!(sym.name, "foo");
    }

    #[test]
    fn test_severity_warning_sorts_last() {
        let mut severities = vec![Severity::Warning, Severity::Major, Severity::Patch, Severity::Minor];
        severities.sort();
        assert_eq!(severities, vec![Severity::Major, Severity::Minor, Severity::Patch, Severity::Warning]);
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
            decl_kind: Some(DeclarationKind::Function),
            type_guard_type: None,
            is_async: true,
            is_generator: true,
            is_reexport: false,
        };
        assert!(sym.is_async);
        assert!(sym.is_generator);
    }
}

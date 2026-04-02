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

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ExportSymbol {
    pub name: String,
    pub kind: ExportKind,
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub type_signature: Option<String>,
    pub type_shape: Option<shape::TypeShape>,
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
            type_shape: None,
        };
        assert_eq!(sym.name, "foo");
    }
}

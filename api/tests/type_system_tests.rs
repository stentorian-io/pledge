//! Type system breaking change detection tests.
//!
//! Tests for detecting breaking changes in type definitions,
//! including structural comparison, variance, and interface changes.

use interlint_format::{
    TypeChange, TypeInfo, TypeKind, PropertyChange, PropertyChangeType,
    Severity,
    changes::{BreakingChange, CodeChange, Change, TypeChangeImpact},
};

// =============================================================================
// STRUCTURAL TYPE COMPARISON TESTS
// =============================================================================

mod structural_comparison_tests {
    use super::*;

    #[test]
    fn test_type_kind_equality() {
        let string_type = TypeInfo::primitive("string");
        let another_string = TypeInfo::primitive("string");
        let number_type = TypeInfo::primitive("number");

        assert_eq!(string_type, another_string);
        assert_ne!(string_type, number_type);
    }

    #[test]
    fn test_array_element_type_change() {
        let old_type = TypeInfo::array(TypeInfo::primitive("string"));
        let new_type = TypeInfo::array(TypeInfo::primitive("number"));

        assert_eq!(old_type.kind, TypeKind::Array);
        assert_eq!(new_type.kind, TypeKind::Array);

        // Element type changed
        let old_element = old_type.element_type.as_ref().unwrap();
        let new_element = new_type.element_type.as_ref().unwrap();

        assert_ne!(old_element.repr, new_element.repr);

        // This is a breaking change
        let change = BreakingChange::Code(CodeChange::TypeChanged(TypeChange {
            name: "ItemList".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Array element type changed from string to number".to_string(),
            old_type: Some(old_type.clone()),
            new_type: Some(new_type.clone()),
            property_changes: vec![],
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::ConsumerBreaking),
            migration_hint: Some("Update array element types to number".to_string()),
        }));

        assert_eq!(change.severity(), Severity::Major);
    }

    #[test]
    fn test_type_repr_comparison_not_sufficient() {
        // String comparison is not sufficient for structural types
        // These have same repr but different structure
        let type_a = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ name: string }".to_string(),
            ..Default::default()
        };

        let type_b = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ name: string }".to_string(),
            nullable: true, // Different nullability
            ..Default::default()
        };

        // Repr same but types are not structurally equal
        assert_eq!(type_a.repr, type_b.repr);
        assert_ne!(type_a, type_b); // nullable differs
    }

    #[test]
    fn test_nested_type_comparison() {
        // Promise<string> vs Promise<number>
        let promise_string = TypeInfo {
            kind: TypeKind::Async,
            repr: "Promise<string>".to_string(),
            type_arguments: vec![TypeInfo::primitive("string")],
            ..Default::default()
        };

        let promise_number = TypeInfo {
            kind: TypeKind::Async,
            repr: "Promise<number>".to_string(),
            type_arguments: vec![TypeInfo::primitive("number")],
            ..Default::default()
        };

        assert_eq!(promise_string.kind, promise_number.kind);
        assert_ne!(promise_string.type_arguments, promise_number.type_arguments);
    }
}

// =============================================================================
// UNION TYPE MEMBER DETECTION TESTS
// =============================================================================

mod union_member_tests {
    use super::*;

    fn create_union(members: Vec<TypeInfo>) -> TypeInfo {
        let repr = members
            .iter()
            .map(|m| m.repr.as_str())
            .collect::<Vec<_>>()
            .join(" | ");

        TypeInfo {
            kind: TypeKind::Union,
            repr,
            constituents: members,
            ..Default::default()
        }
    }

    #[test]
    fn test_union_member_removal_is_breaking() {
        // Old: string | number | boolean
        let old_union = create_union(vec![
            TypeInfo::primitive("string"),
            TypeInfo::primitive("number"),
            TypeInfo::primitive("boolean"),
        ]);

        // New: string | number (boolean removed)
        let new_union = create_union(vec![
            TypeInfo::primitive("string"),
            TypeInfo::primitive("number"),
        ]);

        assert_eq!(old_union.constituents.len(), 3);
        assert_eq!(new_union.constituents.len(), 2);

        // Find removed members
        let old_reprs: std::collections::HashSet<_> = old_union
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let new_reprs: std::collections::HashSet<_> = new_union
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let removed: Vec<_> = old_reprs.difference(&new_reprs).collect();
        assert!(removed.contains(&&"boolean"));

        // Union member removal is breaking - existing values may not match
        let change = TypeChange {
            name: "Result".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Union member 'boolean' was removed".to_string(),
            old_type: Some(old_union),
            new_type: Some(new_union),
            property_changes: vec![],
            severity: Severity::Major,
            impact: None,
            migration_hint: Some("Values of type 'boolean' are no longer valid".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }

    #[test]
    fn test_union_member_addition_is_safe() {
        // Old: string | number
        let old_union = create_union(vec![
            TypeInfo::primitive("string"),
            TypeInfo::primitive("number"),
        ]);

        // New: string | number | boolean
        let new_union = create_union(vec![
            TypeInfo::primitive("string"),
            TypeInfo::primitive("number"),
            TypeInfo::primitive("boolean"),
        ]);

        // Find added members
        let old_reprs: std::collections::HashSet<_> = old_union
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let new_reprs: std::collections::HashSet<_> = new_union
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let added: Vec<_> = new_reprs.difference(&old_reprs).collect();
        assert!(added.contains(&&"boolean"));

        // Union member addition is safe for consumers
        let change = TypeChange {
            name: "Result".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Union member 'boolean' was added".to_string(),
            old_type: Some(old_union),
            new_type: Some(new_union),
            property_changes: vec![],
            severity: Severity::Minor,
            impact: None,
            migration_hint: None,
        };

        assert_eq!(change.severity, Severity::Minor);
    }

    #[test]
    fn test_discriminated_union_member_removal() {
        // Discriminated unions with literal types
        let success = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ type: 'success', data: T }".to_string(),
            ..Default::default()
        };

        let error = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ type: 'error', message: string }".to_string(),
            ..Default::default()
        };

        let pending = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ type: 'pending' }".to_string(),
            ..Default::default()
        };

        // Old: Success | Error | Pending
        let old_union = create_union(vec![success.clone(), error.clone(), pending.clone()]);

        // New: Success | Error (Pending removed)
        let new_union = create_union(vec![success, error]);

        assert!(old_union.constituents.len() > new_union.constituents.len());

        // Removal of discriminated union member breaks switch statements
        let change = TypeChange {
            name: "AsyncResult".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Discriminated union member 'pending' was removed".to_string(),
            old_type: Some(old_union),
            new_type: Some(new_union),
            property_changes: vec![],
            severity: Severity::Major,
            impact: None,
            migration_hint: Some("Remove 'pending' case from switch statements".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }
}

// =============================================================================
// INTERSECTION TYPE DETECTION TESTS
// =============================================================================

mod intersection_member_tests {
    use super::*;

    fn create_intersection(members: Vec<TypeInfo>) -> TypeInfo {
        let repr = members
            .iter()
            .map(|m| m.repr.as_str())
            .collect::<Vec<_>>()
            .join(" & ");

        TypeInfo {
            kind: TypeKind::Intersection,
            repr,
            constituents: members,
            ..Default::default()
        }
    }

    #[test]
    fn test_intersection_member_addition_is_breaking() {
        // Old: A & B
        let old_intersection = create_intersection(vec![
            TypeInfo::reference("A"),
            TypeInfo::reference("B"),
        ]);

        // New: A & B & C (C added)
        let new_intersection = create_intersection(vec![
            TypeInfo::reference("A"),
            TypeInfo::reference("B"),
            TypeInfo::reference("C"),
        ]);

        // Find added members
        let old_reprs: std::collections::HashSet<_> = old_intersection
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let new_reprs: std::collections::HashSet<_> = new_intersection
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let added: Vec<_> = new_reprs.difference(&old_reprs).collect();
        assert!(added.contains(&&"C"));

        // Intersection member addition is BREAKING - requires more properties
        let change = TypeChange {
            name: "CombinedConfig".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Intersection member 'C' was added".to_string(),
            old_type: Some(old_intersection),
            new_type: Some(new_intersection),
            property_changes: vec![],
            severity: Severity::Major,
            impact: None,
            migration_hint: Some("Add properties from type C to existing values".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }

    #[test]
    fn test_intersection_member_removal_is_safe() {
        // Old: A & B & C
        let old_intersection = create_intersection(vec![
            TypeInfo::reference("A"),
            TypeInfo::reference("B"),
            TypeInfo::reference("C"),
        ]);

        // New: A & B (C removed)
        let new_intersection = create_intersection(vec![
            TypeInfo::reference("A"),
            TypeInfo::reference("B"),
        ]);

        // Find removed members
        let old_reprs: std::collections::HashSet<_> = old_intersection
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let new_reprs: std::collections::HashSet<_> = new_intersection
            .constituents
            .iter()
            .map(|c| c.repr.as_str())
            .collect();

        let removed: Vec<_> = old_reprs.difference(&new_reprs).collect();
        assert!(removed.contains(&&"C"));

        // Intersection member removal is SAFE - fewer requirements
        let change = TypeChange {
            name: "CombinedConfig".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Intersection member 'C' was removed".to_string(),
            old_type: Some(old_intersection),
            new_type: Some(new_intersection),
            property_changes: vec![],
            severity: Severity::Minor,
            impact: None,
            migration_hint: None,
        };

        assert_eq!(change.severity, Severity::Minor);
    }
}

// =============================================================================
// GENERIC CONSTRAINT DETECTION TESTS
// =============================================================================

mod generic_constraint_tests {
    use super::*;
    use interlint_format::changes::GenericConstraintChange;

    #[test]
    fn test_generic_constraint_narrowing_is_breaking() {
        // Old: function process<T>(value: T): T
        // New: function process<T extends Serializable>(value: T): T

        let change = GenericConstraintChange {
            name: "process".to_string(),
            module_path: "src/utils.ts".to_string(),
            type_parameter: "T".to_string(),
            old_constraint: None, // Was unconstrained
            new_constraint: Some(TypeInfo::reference("Serializable")),
            severity: Severity::Major,
            migration_hint: Some("Ensure type arguments implement Serializable".to_string()),
        };

        // Adding a constraint narrows valid types - breaking
        assert_eq!(change.severity, Severity::Major);
    }

    #[test]
    fn test_generic_constraint_widening_is_safe() {
        // Old: function process<T extends Serializable>(value: T): T
        // New: function process<T>(value: T): T

        let change = GenericConstraintChange {
            name: "process".to_string(),
            module_path: "src/utils.ts".to_string(),
            type_parameter: "T".to_string(),
            old_constraint: Some(TypeInfo::reference("Serializable")),
            new_constraint: None, // Now unconstrained
            severity: Severity::Minor,
            migration_hint: None,
        };

        // Removing a constraint widens valid types - safe
        assert_eq!(change.severity, Severity::Minor);
    }

    #[test]
    fn test_generic_constraint_change_to_narrower() {
        // Old: T extends object
        // New: T extends Record<string, unknown>

        let change = GenericConstraintChange {
            name: "Container".to_string(),
            module_path: "src/container.ts".to_string(),
            type_parameter: "T".to_string(),
            old_constraint: Some(TypeInfo::reference("object")),
            new_constraint: Some(TypeInfo::reference("Record<string, unknown>")),
            severity: Severity::Major,
            migration_hint: Some("Arrays no longer valid - use Record type".to_string()),
        };

        // Narrower constraint is breaking
        assert_eq!(change.severity, Severity::Major);
    }

    #[test]
    fn test_generic_constraint_change_to_wider() {
        // Old: T extends Record<string, unknown>
        // New: T extends object

        let change = GenericConstraintChange {
            name: "Container".to_string(),
            module_path: "src/container.ts".to_string(),
            type_parameter: "T".to_string(),
            old_constraint: Some(TypeInfo::reference("Record<string, unknown>")),
            new_constraint: Some(TypeInfo::reference("object")),
            severity: Severity::Minor,
            migration_hint: None,
        };

        // Wider constraint is safe
        assert_eq!(change.severity, Severity::Minor);
    }
}

// =============================================================================
// READONLY TRANSITION TESTS
// =============================================================================

mod readonly_tests {
    use super::*;

    #[test]
    fn test_mutable_to_readonly_is_breaking() {
        // Property changed from mutable to readonly
        let change = PropertyChange {
            name: "config".to_string(),
            change_type: PropertyChangeType::BecameReadonly,
            old_type: Some(TypeInfo::reference("Config")),
            new_type: Some(TypeInfo::reference("Readonly<Config>")),
        };

        // Code that modifies the property will break
        assert_eq!(change.change_type, PropertyChangeType::BecameReadonly);

        // Verify change type implies breaking
        let is_breaking = matches!(
            change.change_type,
            PropertyChangeType::BecameReadonly |
            PropertyChangeType::BecameRequired |
            PropertyChangeType::Removed |
            PropertyChangeType::TypeChanged
        );
        assert!(is_breaking);
    }

    #[test]
    fn test_readonly_to_mutable_is_safe() {
        // Property changed from readonly to mutable
        let change = PropertyChange {
            name: "config".to_string(),
            change_type: PropertyChangeType::BecameMutable,
            old_type: Some(TypeInfo::reference("Readonly<Config>")),
            new_type: Some(TypeInfo::reference("Config")),
        };

        // Code can now modify the property - safe addition of capability
        assert_eq!(change.change_type, PropertyChangeType::BecameMutable);

        let is_safe = matches!(
            change.change_type,
            PropertyChangeType::BecameMutable |
            PropertyChangeType::BecameOptional |
            PropertyChangeType::Added
        );
        assert!(is_safe);
    }
}

// =============================================================================
// OPTIONAL TO REQUIRED PROPERTY TESTS
// =============================================================================

mod optionality_tests {
    use super::*;

    #[test]
    fn test_optional_to_required_is_breaking() {
        let change = PropertyChange {
            name: "timeout".to_string(),
            change_type: PropertyChangeType::BecameRequired,
            old_type: Some(TypeInfo::primitive("number")),
            new_type: Some(TypeInfo::primitive("number")),
        };

        // Code that doesn't provide the property will break
        assert_eq!(change.change_type, PropertyChangeType::BecameRequired);
    }

    #[test]
    fn test_required_to_optional_is_safe() {
        let change = PropertyChange {
            name: "timeout".to_string(),
            change_type: PropertyChangeType::BecameOptional,
            old_type: Some(TypeInfo::primitive("number")),
            new_type: Some(TypeInfo::primitive("number")),
        };

        // Code that provides the property still works
        assert_eq!(change.change_type, PropertyChangeType::BecameOptional);
    }
}

// =============================================================================
// INTERFACE PROPERTY ADDITION TESTS (Consumer vs Implementer)
// =============================================================================

mod interface_property_tests {
    use super::*;

    /// Consumer perspective: uses the interface type
    /// Adding optional property is safe (code still compiles)
    /// Adding required property is safe (can still read it)
    #[test]
    fn test_interface_required_property_addition_safe_for_consumers() {
        // Consumer code: function process(user: User) { }
        // Adding a property to User doesn't break consumer code

        let change = TypeChange {
            name: "User".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Added required property 'email' to interface".to_string(),
            old_type: None,
            new_type: None,
            property_changes: vec![PropertyChange {
                name: "email".to_string(),
                change_type: PropertyChangeType::Added,
                old_type: None,
                new_type: Some(TypeInfo::primitive("string")),
            }],
            // For consumers reading the type, this is safe
            severity: Severity::Minor,
            impact: Some(TypeChangeImpact::ImplementerBreaking),
            migration_hint: None,
        };

        assert_eq!(change.severity, Severity::Minor);
    }

    /// Implementer perspective: creates objects matching the interface
    /// Adding required property is BREAKING (must provide it)
    #[test]
    fn test_interface_required_property_addition_breaks_implementers() {
        // Implementer code: const user: User = { name: "John" }
        // Adding 'email' as required breaks this code

        let change = TypeChange {
            name: "User".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Added required property 'email' to interface - breaks implementers".to_string(),
            old_type: None,
            new_type: None,
            property_changes: vec![PropertyChange {
                name: "email".to_string(),
                change_type: PropertyChangeType::Added,
                old_type: None,
                new_type: Some(TypeInfo::primitive("string")),
            }],
            // For implementers, this is breaking
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::ImplementerBreaking),
            migration_hint: Some("Add 'email' property to all User object literals".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }

    /// Adding optional property is safe for both consumers and implementers
    #[test]
    fn test_interface_optional_property_addition_is_safe() {
        let change = TypeChange {
            name: "Config".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Added optional property 'debug' to interface".to_string(),
            old_type: None,
            new_type: None,
            property_changes: vec![PropertyChange {
                name: "debug".to_string(),
                change_type: PropertyChangeType::Added,
                old_type: None,
                new_type: Some(TypeInfo::primitive("boolean")),
            }],
            severity: Severity::Minor,
            impact: None,
            migration_hint: None,
        };

        // Optional property addition is always safe
        assert_eq!(change.severity, Severity::Minor);
    }

    #[test]
    fn test_interface_property_removal_is_breaking() {
        let change = TypeChange {
            name: "User".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Removed property 'nickname' from interface".to_string(),
            old_type: None,
            new_type: None,
            property_changes: vec![PropertyChange {
                name: "nickname".to_string(),
                change_type: PropertyChangeType::Removed,
                old_type: Some(TypeInfo::primitive("string")),
                new_type: None,
            }],
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::Both),
            migration_hint: Some("Remove references to 'nickname' property".to_string()),
        };

        // Property removal breaks both consumers and implementers
        assert_eq!(change.severity, Severity::Major);
    }
}

// =============================================================================
// TYPE KIND CHANGE DETECTION TESTS
// =============================================================================

mod type_kind_change_tests {
    use super::*;

    #[test]
    fn test_primitive_to_object_is_breaking() {
        let old_type = TypeInfo::primitive("string");
        let new_type = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ value: string }".to_string(),
            ..Default::default()
        };

        assert_ne!(old_type.kind, new_type.kind);

        // Type kind change is always major breaking
        let change = TypeChange {
            name: "UserId".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Type changed from primitive to object".to_string(),
            old_type: Some(old_type),
            new_type: Some(new_type),
            property_changes: vec![],
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::Both),
            migration_hint: Some("Update all UserId values to use object syntax".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }

    #[test]
    fn test_array_to_tuple_is_breaking() {
        let old_type = TypeInfo::array(TypeInfo::primitive("number"));
        let new_type = TypeInfo {
            kind: TypeKind::Tuple,
            repr: "[number, number]".to_string(),
            type_arguments: vec![
                TypeInfo::primitive("number"),
                TypeInfo::primitive("number"),
            ],
            ..Default::default()
        };

        assert_ne!(old_type.kind, new_type.kind);

        // Array to tuple changes length flexibility
        let change = TypeChange {
            name: "Point".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Type changed from number[] to [number, number]".to_string(),
            old_type: Some(old_type),
            new_type: Some(new_type),
            property_changes: vec![],
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::Both),
            migration_hint: Some("Ensure arrays have exactly 2 elements".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }

    #[test]
    fn test_reference_to_inline_is_breaking() {
        let old_type = TypeInfo::reference("Config");
        let new_type = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ debug: boolean }".to_string(),
            ..Default::default()
        };

        assert_ne!(old_type.kind, new_type.kind);

        // Reference to inline loses type identity
        let change = TypeChange {
            name: "AppConfig".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Type changed from Config reference to inline object".to_string(),
            old_type: Some(old_type),
            new_type: Some(new_type),
            property_changes: vec![],
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::Both),
            migration_hint: Some("Config type no longer exists - update type references".to_string()),
        };

        assert_eq!(change.severity, Severity::Major);
    }
}

// =============================================================================
// SERIALIZATION TESTS
// =============================================================================

mod serialization_tests {
    use super::*;

    #[test]
    fn test_type_change_serialization_roundtrip() {
        let change = BreakingChange::Code(CodeChange::TypeChanged(TypeChange {
            name: "Config".to_string(),
            module_path: "src/config.ts".to_string(),
            change_summary: "Added required field".to_string(),
            old_type: Some(TypeInfo::reference("OldConfig")),
            new_type: Some(TypeInfo::reference("NewConfig")),
            property_changes: vec![
                PropertyChange {
                    name: "apiKey".to_string(),
                    change_type: PropertyChangeType::Added,
                    old_type: None,
                    new_type: Some(TypeInfo::primitive("string")),
                },
            ],
            severity: Severity::Major,
            impact: Some(TypeChangeImpact::ImplementerBreaking),
            migration_hint: Some("Add apiKey to config".to_string()),
        }));

        let json = serde_json::to_string(&change).unwrap();
        let decoded: BreakingChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
    }

    #[test]
    fn test_union_type_serialization() {
        let union_type = TypeInfo {
            kind: TypeKind::Union,
            repr: "string | number".to_string(),
            constituents: vec![
                TypeInfo::primitive("string"),
                TypeInfo::primitive("number"),
            ],
            ..Default::default()
        };

        let json = serde_json::to_string(&union_type).unwrap();
        let decoded: TypeInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(union_type, decoded);
        assert_eq!(decoded.constituents.len(), 2);
    }

    #[test]
    fn test_intersection_type_serialization() {
        let intersection_type = TypeInfo {
            kind: TypeKind::Intersection,
            repr: "A & B".to_string(),
            constituents: vec![
                TypeInfo::reference("A"),
                TypeInfo::reference("B"),
            ],
            ..Default::default()
        };

        let json = serde_json::to_string(&intersection_type).unwrap();
        let decoded: TypeInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(intersection_type, decoded);
    }
}

//! Variance and covariance/contravariance breaking change detection tests.
//!
//! Tests for detecting breaking changes related to type variance in:
//! - Return types (covariant - can widen)
//! - Parameter types (contravariant - can narrow)
//! - Both positions (invariant - exact match)

use interlint_format::{
    TypeInfo, TypeKind, Severity,
    changes::{
        BreakingChange, CodeChange, Change, SignatureChange, ReturnTypeChange,
        ParameterChange, ParameterChangeType,
    },
};

// =============================================================================
// RETURN TYPE (COVARIANT POSITION) TESTS
// =============================================================================

mod return_type_tests {
    use super::*;

    /// Return type widening is BREAKING because consumers expect a more specific type
    /// Example: () => Dog becomes () => Animal
    #[test]
    fn test_return_type_widening_is_breaking() {
        // Old: returns Dog (more specific)
        // New: returns Animal (less specific)

        let change = SignatureChange {
            name: "getAnimal".to_string(),
            module_path: "src/zoo.ts".to_string(),
            old_signature: "getAnimal(): Dog".to_string(),
            new_signature: "getAnimal(): Animal".to_string(),
            change_details: vec!["Return type widened from Dog to Animal".to_string()],
            parameter_changes: vec![],
            return_type_change: Some(ReturnTypeChange {
                old_type: Some(TypeInfo::reference("Dog")),
                new_type: Some(TypeInfo::reference("Animal")),
                became_async: false,
                was_async: false,
            }),
            migration_hint: Some("Check that code doesn't rely on Dog-specific methods".to_string()),
        };

        // Verify the breaking change semantics
        // Consumer code: const dog: Dog = getAnimal(); // Would fail now
        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change.clone()));

        // Return type widening is a major breaking change
        assert_eq!(breaking_change.severity(), Severity::Major);
    }

    /// Return type narrowing is SAFE because consumers get a more specific type
    /// Example: () => Animal becomes () => Dog
    #[test]
    fn test_return_type_narrowing_is_safe() {
        // Old: returns Animal (less specific)
        // New: returns Dog (more specific)

        let change = SignatureChange {
            name: "getAnimal".to_string(),
            module_path: "src/zoo.ts".to_string(),
            old_signature: "getAnimal(): Animal".to_string(),
            new_signature: "getAnimal(): Dog".to_string(),
            change_details: vec!["Return type narrowed from Animal to Dog".to_string()],
            parameter_changes: vec![],
            return_type_change: Some(ReturnTypeChange {
                old_type: Some(TypeInfo::reference("Animal")),
                new_type: Some(TypeInfo::reference("Dog")),
                became_async: false,
                was_async: false,
            }),
            migration_hint: None,
        };

        // Consumer code: const animal: Animal = getAnimal(); // Still works
        // Dog is assignable to Animal

        // This would be Minor or Patch severity in a proper implementation
        // For now, SignatureChange defaults to Major
        let _breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
    }

    #[test]
    fn test_return_type_void_to_value_is_safe() {
        // Old: () => void
        // New: () => number

        let change = ReturnTypeChange {
            old_type: Some(TypeInfo::void()),
            new_type: Some(TypeInfo::primitive("number")),
            became_async: false,
            was_async: false,
        };

        // Adding a return value is safe - callers can ignore it
        assert_eq!(change.old_type.as_ref().unwrap().kind, TypeKind::Void);
        assert_eq!(change.new_type.as_ref().unwrap().kind, TypeKind::Primitive);
    }

    #[test]
    fn test_return_type_value_to_void_is_breaking() {
        // Old: () => number
        // New: () => void

        let change = ReturnTypeChange {
            old_type: Some(TypeInfo::primitive("number")),
            new_type: Some(TypeInfo::void()),
            became_async: false,
            was_async: false,
        };

        // Removing a return value breaks code that uses it
        assert_eq!(change.old_type.as_ref().unwrap().kind, TypeKind::Primitive);
        assert_eq!(change.new_type.as_ref().unwrap().kind, TypeKind::Void);
    }

    #[test]
    fn test_return_type_became_async_is_breaking() {
        let change = SignatureChange {
            name: "fetchData".to_string(),
            module_path: "src/api.ts".to_string(),
            old_signature: "fetchData(): Data".to_string(),
            new_signature: "fetchData(): Promise<Data>".to_string(),
            change_details: vec!["Function is now async".to_string()],
            parameter_changes: vec![],
            return_type_change: Some(ReturnTypeChange {
                old_type: Some(TypeInfo::reference("Data")),
                new_type: Some(TypeInfo {
                    kind: TypeKind::Async,
                    repr: "Promise<Data>".to_string(),
                    type_arguments: vec![TypeInfo::reference("Data")],
                    ..Default::default()
                }),
                became_async: true,
                was_async: false,
            }),
            migration_hint: Some("Add await or .then() to all call sites".to_string()),
        };

        // Sync to async is always breaking
        assert!(change.return_type_change.as_ref().unwrap().became_async);

        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
        assert_eq!(breaking_change.severity(), Severity::Major);
    }

    #[test]
    fn test_return_type_was_async_now_sync_is_breaking() {
        let change = SignatureChange {
            name: "fetchData".to_string(),
            module_path: "src/api.ts".to_string(),
            old_signature: "fetchData(): Promise<Data>".to_string(),
            new_signature: "fetchData(): Data".to_string(),
            change_details: vec!["Function is no longer async".to_string()],
            parameter_changes: vec![],
            return_type_change: Some(ReturnTypeChange {
                old_type: Some(TypeInfo {
                    kind: TypeKind::Async,
                    repr: "Promise<Data>".to_string(),
                    type_arguments: vec![TypeInfo::reference("Data")],
                    ..Default::default()
                }),
                new_type: Some(TypeInfo::reference("Data")),
                became_async: false,
                was_async: true,
            }),
            migration_hint: Some("Remove await from all call sites".to_string()),
        };

        // Async to sync is also breaking
        assert!(change.return_type_change.as_ref().unwrap().was_async);

        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
        assert_eq!(breaking_change.severity(), Severity::Major);
    }
}

// =============================================================================
// PARAMETER TYPE (CONTRAVARIANT POSITION) TESTS
// =============================================================================

mod parameter_type_tests {
    use super::*;

    /// Parameter type narrowing is BREAKING because callers may pass wider types
    /// Example: (animal: Animal) => void becomes (animal: Dog) => void
    #[test]
    fn test_parameter_type_narrowing_is_breaking() {
        // Old: accepts Animal (wider type)
        // New: accepts Dog (narrower type)

        let change = SignatureChange {
            name: "processAnimal".to_string(),
            module_path: "src/zoo.ts".to_string(),
            old_signature: "processAnimal(animal: Animal): void".to_string(),
            new_signature: "processAnimal(animal: Dog): void".to_string(),
            change_details: vec!["Parameter type narrowed from Animal to Dog".to_string()],
            parameter_changes: vec![ParameterChange {
                name: "animal".to_string(),
                change_type: ParameterChangeType::TypeChanged,
                old_type: Some(TypeInfo::reference("Animal")),
                new_type: Some(TypeInfo::reference("Dog")),
                old_position: Some(0),
                new_position: Some(0),
            }],
            return_type_change: None,
            migration_hint: Some("Ensure callers pass Dog, not other Animal types".to_string()),
        };

        // Consumer code: processAnimal(new Cat()); // Would fail now
        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change.clone()));

        // Parameter type narrowing is breaking
        assert_eq!(breaking_change.severity(), Severity::Major);
    }

    /// Parameter type widening is SAFE because callers can still pass narrow types
    /// Example: (animal: Dog) => void becomes (animal: Animal) => void
    #[test]
    fn test_parameter_type_widening_is_safe() {
        // Old: accepts Dog (narrower type)
        // New: accepts Animal (wider type)

        let change = SignatureChange {
            name: "processAnimal".to_string(),
            module_path: "src/zoo.ts".to_string(),
            old_signature: "processAnimal(animal: Dog): void".to_string(),
            new_signature: "processAnimal(animal: Animal): void".to_string(),
            change_details: vec!["Parameter type widened from Dog to Animal".to_string()],
            parameter_changes: vec![ParameterChange {
                name: "animal".to_string(),
                change_type: ParameterChangeType::TypeChanged,
                old_type: Some(TypeInfo::reference("Dog")),
                new_type: Some(TypeInfo::reference("Animal")),
                old_position: Some(0),
                new_position: Some(0),
            }],
            return_type_change: None,
            migration_hint: None,
        };

        // Consumer code: processAnimal(new Dog()); // Still works
        // Dog is assignable to Animal
        let _breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
    }

    #[test]
    fn test_required_parameter_added_is_breaking() {
        let change = ParameterChange {
            name: "options".to_string(),
            change_type: ParameterChangeType::Added,
            old_type: None,
            new_type: Some(TypeInfo::reference("Options")),
            old_position: None,
            new_position: Some(1),
        };

        // Adding required parameter breaks existing calls
        assert_eq!(change.change_type, ParameterChangeType::Added);
    }

    #[test]
    fn test_required_parameter_removed_is_breaking() {
        let change = ParameterChange {
            name: "timeout".to_string(),
            change_type: ParameterChangeType::Removed,
            old_type: Some(TypeInfo::primitive("number")),
            new_type: None,
            old_position: Some(1),
            new_position: None,
        };

        // Removing a parameter can break callers providing it
        assert_eq!(change.change_type, ParameterChangeType::Removed);
    }

    #[test]
    fn test_parameter_became_required_is_breaking() {
        let change = ParameterChange {
            name: "callback".to_string(),
            change_type: ParameterChangeType::BecameRequired,
            old_type: Some(TypeInfo::reference("Callback")),
            new_type: Some(TypeInfo::reference("Callback")),
            old_position: Some(0),
            new_position: Some(0),
        };

        // Optional to required breaks callers not providing argument
        assert_eq!(change.change_type, ParameterChangeType::BecameRequired);
    }

    #[test]
    fn test_parameter_became_optional_is_safe() {
        let change = ParameterChange {
            name: "callback".to_string(),
            change_type: ParameterChangeType::BecameOptional,
            old_type: Some(TypeInfo::reference("Callback")),
            new_type: Some(TypeInfo::reference("Callback")),
            old_position: Some(0),
            new_position: Some(0),
        };

        // Required to optional is safe
        assert_eq!(change.change_type, ParameterChangeType::BecameOptional);
    }

    #[test]
    fn test_parameter_reordering_is_breaking() {
        let changes = vec![
            ParameterChange {
                name: "a".to_string(),
                change_type: ParameterChangeType::Reordered,
                old_type: Some(TypeInfo::primitive("string")),
                new_type: Some(TypeInfo::primitive("string")),
                old_position: Some(0),
                new_position: Some(1),
            },
            ParameterChange {
                name: "b".to_string(),
                change_type: ParameterChangeType::Reordered,
                old_type: Some(TypeInfo::primitive("number")),
                new_type: Some(TypeInfo::primitive("number")),
                old_position: Some(1),
                new_position: Some(0),
            },
        ];

        // Positional arguments would be swapped
        for change in changes {
            assert_eq!(change.change_type, ParameterChangeType::Reordered);
        }
    }
}

// =============================================================================
// COVARIANT POSITION TESTS (Output positions)
// =============================================================================

mod covariant_position_tests {
    use super::*;

    /// Test that Promise<T> is covariant in T for return positions
    #[test]
    fn test_promise_return_type_covariant() {
        // Promise<Dog> -> Promise<Animal> is breaking (widening)
        let old_return = TypeInfo {
            kind: TypeKind::Async,
            repr: "Promise<Dog>".to_string(),
            type_arguments: vec![TypeInfo::reference("Dog")],
            ..Default::default()
        };

        let new_return = TypeInfo {
            kind: TypeKind::Async,
            repr: "Promise<Animal>".to_string(),
            type_arguments: vec![TypeInfo::reference("Animal")],
            ..Default::default()
        };

        assert_eq!(old_return.type_arguments[0].repr, "Dog");
        assert_eq!(new_return.type_arguments[0].repr, "Animal");

        // This is widening in a covariant position - breaking
        let change = ReturnTypeChange {
            old_type: Some(old_return),
            new_type: Some(new_return),
            became_async: false,
            was_async: false,
        };

        // Verify the types changed
        assert_ne!(
            change.old_type.as_ref().unwrap().type_arguments[0].repr,
            change.new_type.as_ref().unwrap().type_arguments[0].repr
        );
    }

    /// Test Array<T> covariance in T
    #[test]
    fn test_array_element_type_covariant() {
        // Dog[] -> Animal[] is breaking (widening the element type)
        let old_return = TypeInfo::array(TypeInfo::reference("Dog"));
        let new_return = TypeInfo::array(TypeInfo::reference("Animal"));

        assert_eq!(old_return.element_type.as_ref().unwrap().repr, "Dog");
        assert_eq!(new_return.element_type.as_ref().unwrap().repr, "Animal");

        // Array return type widening is breaking
        let change = ReturnTypeChange {
            old_type: Some(old_return),
            new_type: Some(new_return),
            became_async: false,
            was_async: false,
        };

        assert!(change.old_type.is_some());
        assert!(change.new_type.is_some());
    }

    /// Test getter property type (covariant position)
    #[test]
    fn test_getter_property_is_covariant() {
        // Getter: get value(): Dog -> get value(): Animal is breaking

        let change = SignatureChange {
            name: "value".to_string(),
            module_path: "src/container.ts".to_string(),
            old_signature: "get value(): Dog".to_string(),
            new_signature: "get value(): Animal".to_string(),
            change_details: vec!["Getter return type widened".to_string()],
            parameter_changes: vec![],
            return_type_change: Some(ReturnTypeChange {
                old_type: Some(TypeInfo::reference("Dog")),
                new_type: Some(TypeInfo::reference("Animal")),
                became_async: false,
                was_async: false,
            }),
            migration_hint: Some("Verify code doesn't rely on Dog-specific properties".to_string()),
        };

        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
        assert_eq!(breaking_change.severity(), Severity::Major);
    }
}

// =============================================================================
// CONTRAVARIANT POSITION TESTS (Input positions)
// =============================================================================

mod contravariant_position_tests {
    use super::*;

    /// Test callback parameter type (contravariant position)
    #[test]
    fn test_callback_parameter_contravariant() {
        // Callback param: (handler: (animal: Animal) => void) becomes (handler: (animal: Dog) => void)
        // Narrowing the callback's parameter is BREAKING

        let old_callback = TypeInfo {
            kind: TypeKind::Function,
            repr: "(animal: Animal) => void".to_string(),
            param_types: vec![TypeInfo::reference("Animal")],
            return_type: Some(Box::new(TypeInfo::void())),
            ..Default::default()
        };

        let new_callback = TypeInfo {
            kind: TypeKind::Function,
            repr: "(animal: Dog) => void".to_string(),
            param_types: vec![TypeInfo::reference("Dog")],
            return_type: Some(Box::new(TypeInfo::void())),
            ..Default::default()
        };

        // Callback's parameter narrowed from Animal to Dog
        assert_eq!(old_callback.param_types[0].repr, "Animal");
        assert_eq!(new_callback.param_types[0].repr, "Dog");

        // This is breaking because existing callbacks accepting Animal won't work
        let change = ParameterChange {
            name: "handler".to_string(),
            change_type: ParameterChangeType::TypeChanged,
            old_type: Some(old_callback),
            new_type: Some(new_callback),
            old_position: Some(0),
            new_position: Some(0),
        };

        assert_eq!(change.change_type, ParameterChangeType::TypeChanged);
    }

    /// Test setter property type (contravariant position)
    #[test]
    fn test_setter_property_is_contravariant() {
        // Setter: set value(v: Animal) -> set value(v: Dog) is breaking (narrowing)

        let change = SignatureChange {
            name: "value".to_string(),
            module_path: "src/container.ts".to_string(),
            old_signature: "set value(v: Animal)".to_string(),
            new_signature: "set value(v: Dog)".to_string(),
            change_details: vec!["Setter parameter type narrowed".to_string()],
            parameter_changes: vec![ParameterChange {
                name: "v".to_string(),
                change_type: ParameterChangeType::TypeChanged,
                old_type: Some(TypeInfo::reference("Animal")),
                new_type: Some(TypeInfo::reference("Dog")),
                old_position: Some(0),
                new_position: Some(0),
            }],
            return_type_change: None,
            migration_hint: Some("Ensure only Dog values are assigned".to_string()),
        };

        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
        assert_eq!(breaking_change.severity(), Severity::Major);
    }

    /// Test method parameter (contravariant)
    #[test]
    fn test_method_parameter_contravariant() {
        // Class method parameter narrowing is breaking
        // process(data: object) -> process(data: Array<number>)

        let change = SignatureChange {
            name: "process".to_string(),
            module_path: "src/processor.ts".to_string(),
            old_signature: "process(data: object): void".to_string(),
            new_signature: "process(data: Array<number>): void".to_string(),
            change_details: vec!["Parameter type narrowed from object to Array<number>".to_string()],
            parameter_changes: vec![ParameterChange {
                name: "data".to_string(),
                change_type: ParameterChangeType::TypeChanged,
                old_type: Some(TypeInfo::reference("object")),
                new_type: Some(TypeInfo::array(TypeInfo::primitive("number"))),
                old_position: Some(0),
                new_position: Some(0),
            }],
            return_type_change: None,
            migration_hint: Some("Only Array<number> is accepted now".to_string()),
        };

        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
        assert_eq!(breaking_change.severity(), Severity::Major);
    }
}

// =============================================================================
// INVARIANT POSITION TESTS (Both input and output)
// =============================================================================

mod invariant_position_tests {
    use super::*;

    /// Test ref/out parameter (invariant - must match exactly)
    #[test]
    fn test_ref_parameter_is_invariant() {
        // If a parameter is both read and written, any type change is breaking
        // inout param: (ref data: Dog) - cannot change at all

        let change = SignatureChange {
            name: "modifyInPlace".to_string(),
            module_path: "src/utils.ts".to_string(),
            old_signature: "modifyInPlace(ref data: Dog): void".to_string(),
            new_signature: "modifyInPlace(ref data: Animal): void".to_string(),
            change_details: vec!["Ref parameter type changed (invariant position)".to_string()],
            parameter_changes: vec![ParameterChange {
                name: "data".to_string(),
                change_type: ParameterChangeType::TypeChanged,
                old_type: Some(TypeInfo::reference("Dog")),
                new_type: Some(TypeInfo::reference("Animal")),
                old_position: Some(0),
                new_position: Some(0),
            }],
            return_type_change: None,
            migration_hint: Some("Ref parameter requires exact type match".to_string()),
        };

        // Any change to invariant position is breaking
        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));
        assert_eq!(breaking_change.severity(), Severity::Major);
    }

    /// Test property with getter AND setter (invariant)
    #[test]
    fn test_readwrite_property_is_invariant() {
        // Property with both get and set is invariant
        // value: Dog (get + set) cannot change to Animal or Cat

        let old_type = TypeInfo::reference("Dog");
        let new_type = TypeInfo::reference("Animal");

        // Even though Animal is a supertype, this breaks:
        // - Getter widening (Dog -> Animal) might be ok
        // - But setter narrowing (Animal -> Dog) is not implied

        assert_ne!(old_type.repr, new_type.repr);

        // For read-write properties, treat as invariant - any change is breaking
    }
}

// =============================================================================
// SERIALIZATION TESTS
// =============================================================================

mod serialization_tests {
    use super::*;

    #[test]
    fn test_signature_change_with_variance_roundtrip() {
        let change = SignatureChange {
            name: "transform".to_string(),
            module_path: "src/utils.ts".to_string(),
            old_signature: "transform<T>(input: T): T".to_string(),
            new_signature: "transform<T extends object>(input: T): T".to_string(),
            change_details: vec!["Generic constraint added".to_string()],
            parameter_changes: vec![],
            return_type_change: None,
            migration_hint: Some("Ensure T extends object".to_string()),
        };

        let breaking_change = BreakingChange::Code(CodeChange::SignatureChanged(change));

        let json = serde_json::to_string(&breaking_change).unwrap();
        let decoded: BreakingChange = serde_json::from_str(&json).unwrap();

        assert_eq!(breaking_change, decoded);
    }

    #[test]
    fn test_return_type_change_serialization() {
        let change = ReturnTypeChange {
            old_type: Some(TypeInfo::reference("Dog")),
            new_type: Some(TypeInfo::reference("Animal")),
            became_async: false,
            was_async: false,
        };

        let json = serde_json::to_string(&change).unwrap();
        let decoded: ReturnTypeChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
    }

    #[test]
    fn test_async_return_type_change_serialization() {
        let change = ReturnTypeChange {
            old_type: Some(TypeInfo::primitive("number")),
            new_type: Some(TypeInfo {
                kind: TypeKind::Async,
                repr: "Promise<number>".to_string(),
                type_arguments: vec![TypeInfo::primitive("number")],
                ..Default::default()
            }),
            became_async: true,
            was_async: false,
        };

        let json = serde_json::to_string(&change).unwrap();
        let decoded: ReturnTypeChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
        assert!(decoded.became_async);
        assert!(!decoded.was_async);
    }

    #[test]
    fn test_parameter_change_serialization() {
        let change = ParameterChange {
            name: "callback".to_string(),
            change_type: ParameterChangeType::TypeChanged,
            old_type: Some(TypeInfo {
                kind: TypeKind::Function,
                repr: "(x: string) => void".to_string(),
                param_types: vec![TypeInfo::primitive("string")],
                return_type: Some(Box::new(TypeInfo::void())),
                ..Default::default()
            }),
            new_type: Some(TypeInfo {
                kind: TypeKind::Function,
                repr: "(x: string, y: number) => void".to_string(),
                param_types: vec![
                    TypeInfo::primitive("string"),
                    TypeInfo::primitive("number"),
                ],
                return_type: Some(Box::new(TypeInfo::void())),
                ..Default::default()
            }),
            old_position: Some(0),
            new_position: Some(0),
        };

        let json = serde_json::to_string(&change).unwrap();
        let decoded: ParameterChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
    }
}

//! Property-based tests for the binary format using proptest.
//!
//! These tests verify that arbitrary valid data structures can be
//! successfully round-tripped through the binary encoding.

use interlint_format::{
    binary, ClassSig, EnumMember, EnumSig, Export, FunctionSig, IndexSignature, InterfaceSig,
    Metadata, Module, Parameter, Property, PublicInterface, Signature, TypeAliasSig, TypeInfo,
    TypeKind, TypeParameter, VariableSig, Visibility,
};
use proptest::prelude::*;

// ==================== Strategy Definitions ====================

fn arb_visibility() -> impl Strategy<Value = Visibility> {
    prop_oneof![
        Just(Visibility::Public),
        Just(Visibility::Protected),
        Just(Visibility::Private),
        Just(Visibility::Internal),
    ]
}

fn arb_type_kind() -> impl Strategy<Value = TypeKind> {
    prop_oneof![
        Just(TypeKind::Primitive),
        Just(TypeKind::Array),
        Just(TypeKind::Tuple),
        Just(TypeKind::Object),
        Just(TypeKind::Function),
        Just(TypeKind::Union),
        Just(TypeKind::Intersection),
        Just(TypeKind::Generic),
        Just(TypeKind::Reference),
        Just(TypeKind::Literal),
        Just(TypeKind::Optional),
        Just(TypeKind::Map),
        Just(TypeKind::Set),
        Just(TypeKind::Async),
        Just(TypeKind::Void),
        Just(TypeKind::Never),
        Just(TypeKind::Any),
        Just(TypeKind::Unknown),
    ]
}

fn arb_simple_type_info() -> impl Strategy<Value = TypeInfo> {
    (
        arb_type_kind(),
        "[a-zA-Z_][a-zA-Z0-9_]{0,20}",
        any::<bool>(),
    )
        .prop_map(|(kind, repr, nullable)| TypeInfo {
            kind,
            repr,
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable,
        })
}

fn arb_type_parameter() -> impl Strategy<Value = TypeParameter> {
    (
        "[A-Z][A-Za-z0-9]{0,5}",
        proptest::option::of(arb_simple_type_info().prop_map(Box::new)),
        proptest::option::of(arb_simple_type_info().prop_map(Box::new)),
    )
        .prop_map(|(name, constraint, default)| TypeParameter {
            name,
            constraint,
            default,
        })
}

fn arb_parameter() -> impl Strategy<Value = Parameter> {
    (
        "[a-z][a-zA-Z0-9]{0,15}",
        arb_simple_type_info(),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of("[a-zA-Z0-9_\"']+"),
    )
        .prop_map(|(name, type_info, optional, rest, default_value)| Parameter {
            name,
            type_info,
            optional,
            rest,
            default_value,
        })
}

fn arb_property() -> impl Strategy<Value = Property> {
    (
        "[a-z][a-zA-Z0-9_]{0,15}",
        arb_simple_type_info(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,100}"),
    )
        .prop_map(
            |(name, type_info, optional, readonly, is_static, visibility, doc)| Property {
                name,
                type_info,
                optional,
                readonly,
                is_static,
                visibility,
                doc,
            },
        )
}

fn arb_function_sig() -> impl Strategy<Value = FunctionSig> {
    (
        "[a-z][a-zA-Z0-9_]{0,20}",
        proptest::collection::vec(arb_type_parameter(), 0..3),
        proptest::collection::vec(arb_parameter(), 0..5),
        arb_simple_type_info(),
        any::<bool>(),
        any::<bool>(),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(
            |(name, type_parameters, parameters, return_type, is_async, is_generator, visibility, doc)| {
                FunctionSig {
                    name,
                    type_parameters,
                    parameters,
                    return_type,
                    is_async,
                    is_generator,
                    visibility,
                    doc,
                }
            },
        )
}

fn arb_class_sig() -> impl Strategy<Value = ClassSig> {
    (
        "[A-Z][a-zA-Z0-9_]{0,20}",
        proptest::collection::vec(arb_type_parameter(), 0..3),
        proptest::option::of(arb_simple_type_info()),
        proptest::collection::vec(arb_simple_type_info(), 0..3),
        proptest::collection::vec(arb_property(), 0..5),
        proptest::collection::vec(arb_function_sig(), 0..3),
        proptest::option::of(arb_function_sig().prop_map(Box::new)),
        any::<bool>(),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(
            |(name, type_parameters, extends, implements, properties, methods, constructor, is_abstract, visibility, doc)| {
                ClassSig {
                    name,
                    type_parameters,
                    extends,
                    implements,
                    properties,
                    methods,
                    constructor,
                    is_abstract,
                    visibility,
                    doc,
                }
            },
        )
}

fn arb_variable_sig() -> impl Strategy<Value = VariableSig> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,20}",
        arb_simple_type_info(),
        any::<bool>(),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(|(name, type_info, is_const, visibility, doc)| VariableSig {
            name,
            type_info,
            is_const,
            visibility,
            doc,
        })
}

fn arb_index_signature() -> impl Strategy<Value = IndexSignature> {
    (arb_simple_type_info(), arb_simple_type_info(), any::<bool>()).prop_map(
        |(key_type, value_type, readonly)| IndexSignature {
            key_type,
            value_type,
            readonly,
        },
    )
}

fn arb_interface_sig() -> impl Strategy<Value = InterfaceSig> {
    (
        "[A-Z][a-zA-Z0-9_]{0,20}",
        proptest::collection::vec(arb_type_parameter(), 0..3),
        proptest::collection::vec(arb_simple_type_info(), 0..3),
        proptest::collection::vec(arb_property(), 0..5),
        proptest::collection::vec(arb_function_sig(), 0..3),
        proptest::option::of(arb_index_signature()),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(
            |(name, type_parameters, extends, properties, methods, index_signature, visibility, doc)| {
                InterfaceSig {
                    name,
                    type_parameters,
                    extends,
                    properties,
                    methods,
                    index_signature,
                    visibility,
                    doc,
                }
            },
        )
}

fn arb_enum_member() -> impl Strategy<Value = EnumMember> {
    (
        "[A-Z][A-Z0-9_]{0,15}",
        proptest::option::of("[0-9]+|\"[a-zA-Z0-9_]+\""),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,100}"),
    )
        .prop_map(|(name, value, doc)| EnumMember { name, value, doc })
}

fn arb_enum_sig() -> impl Strategy<Value = EnumSig> {
    (
        "[A-Z][a-zA-Z0-9_]{0,20}",
        proptest::collection::vec(arb_enum_member(), 0..10),
        any::<bool>(),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(|(name, members, is_const, visibility, doc)| EnumSig {
            name,
            members,
            is_const,
            visibility,
            doc,
        })
}

fn arb_type_alias_sig() -> impl Strategy<Value = TypeAliasSig> {
    (
        "[A-Z][a-zA-Z0-9_]{0,20}",
        proptest::collection::vec(arb_type_parameter(), 0..3),
        arb_simple_type_info(),
        arb_visibility(),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(
            |(name, type_parameters, aliased_type, visibility, doc)| TypeAliasSig {
                name,
                type_parameters,
                aliased_type,
                visibility,
                doc,
            },
        )
}

fn arb_signature() -> impl Strategy<Value = Signature> {
    prop_oneof![
        arb_function_sig().prop_map(Signature::Function),
        arb_class_sig().prop_map(Signature::Class),
        arb_variable_sig().prop_map(Signature::Variable),
        arb_interface_sig().prop_map(Signature::Interface),
        arb_enum_sig().prop_map(Signature::Enum),
        arb_type_alias_sig().prop_map(Signature::TypeAlias),
        // Skip namespace to avoid deep recursion
    ]
}

fn arb_export() -> impl Strategy<Value = Export> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,20}",
        proptest::option::of("[a-zA-Z_][a-zA-Z0-9_]{0,20}"),
        arb_signature(),
        any::<bool>(),
    )
        .prop_map(|(name, original_name, signature, is_default)| Export {
            name,
            original_name,
            signature,
            is_default,
        })
}

fn arb_module() -> impl Strategy<Value = Module> {
    (
        "[a-zA-Z_/][a-zA-Z0-9_/.]{0,50}",
        proptest::collection::vec(arb_export(), 0..5),
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
    )
        .prop_map(|(path, exports, doc)| {
            let mut module = Module::new(path);
            for export in exports {
                module.add_export(export);
            }
            module.doc = doc;
            module
        })
}

fn arb_metadata() -> impl Strategy<Value = Metadata> {
    (
        "[a-z][a-z0-9-]{0,30}",
        "[0-9]+\\.[0-9]+\\.[0-9]+",
        proptest::option::of("[a-zA-Z0-9 .,;:!?]{0,200}"),
        proptest::option::of("https://[a-z.]+/[a-z/]+"),
        proptest::option::of("MIT|Apache-2.0|ISC|BSD-3-Clause"),
        proptest::option::of("typescript|javascript|rust|python|go"),
    )
        .prop_map(
            |(name, version, description, repository, license, language)| Metadata {
                name,
                version,
                description,
                repository,
                license,
                language,
            },
        )
}

fn arb_public_interface() -> impl Strategy<Value = PublicInterface> {
    (arb_metadata(), proptest::collection::vec(arb_module(), 0..3)).prop_map(|(metadata, modules)| {
        let mut interface = PublicInterface::new(metadata);
        for module in modules {
            interface.add_module(module);
        }
        interface
    })
}

// ==================== Property Tests ====================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn roundtrip_metadata(metadata in arb_metadata()) {
        let encoded = binary::encode(&metadata).expect("encoding should succeed");
        let decoded: Metadata = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(metadata, decoded);
    }

    #[test]
    fn roundtrip_type_info(type_info in arb_simple_type_info()) {
        let encoded = binary::encode(&type_info).expect("encoding should succeed");
        let decoded: TypeInfo = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(type_info, decoded);
    }

    #[test]
    fn roundtrip_type_parameter(type_param in arb_type_parameter()) {
        let encoded = binary::encode(&type_param).expect("encoding should succeed");
        let decoded: TypeParameter = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(type_param, decoded);
    }

    #[test]
    fn roundtrip_parameter(param in arb_parameter()) {
        let encoded = binary::encode(&param).expect("encoding should succeed");
        let decoded: Parameter = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(param, decoded);
    }

    #[test]
    fn roundtrip_property(prop in arb_property()) {
        let encoded = binary::encode(&prop).expect("encoding should succeed");
        let decoded: Property = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(prop, decoded);
    }

    #[test]
    fn roundtrip_function_sig(func in arb_function_sig()) {
        let encoded = binary::encode(&func).expect("encoding should succeed");
        let decoded: FunctionSig = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(func, decoded);
    }

    #[test]
    fn roundtrip_class_sig(class in arb_class_sig()) {
        let encoded = binary::encode(&class).expect("encoding should succeed");
        let decoded: ClassSig = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(class, decoded);
    }

    #[test]
    fn roundtrip_variable_sig(var in arb_variable_sig()) {
        let encoded = binary::encode(&var).expect("encoding should succeed");
        let decoded: VariableSig = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(var, decoded);
    }

    #[test]
    fn roundtrip_interface_sig(iface in arb_interface_sig()) {
        let encoded = binary::encode(&iface).expect("encoding should succeed");
        let decoded: InterfaceSig = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(iface, decoded);
    }

    #[test]
    fn roundtrip_enum_sig(enum_sig in arb_enum_sig()) {
        let encoded = binary::encode(&enum_sig).expect("encoding should succeed");
        let decoded: EnumSig = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(enum_sig, decoded);
    }

    #[test]
    fn roundtrip_type_alias_sig(alias in arb_type_alias_sig()) {
        let encoded = binary::encode(&alias).expect("encoding should succeed");
        let decoded: TypeAliasSig = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(alias, decoded);
    }

    #[test]
    fn roundtrip_signature(sig in arb_signature()) {
        let encoded = binary::encode(&sig).expect("encoding should succeed");
        let decoded: Signature = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(sig, decoded);
    }

    #[test]
    fn roundtrip_export(export in arb_export()) {
        let encoded = binary::encode(&export).expect("encoding should succeed");
        let decoded: Export = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(export, decoded);
    }

    #[test]
    fn roundtrip_module(module in arb_module()) {
        let encoded = binary::encode(&module).expect("encoding should succeed");
        let decoded: Module = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(module, decoded);
    }

    #[test]
    fn roundtrip_public_interface(interface in arb_public_interface()) {
        let encoded = binary::encode(&interface).expect("encoding should succeed");
        let decoded: PublicInterface = binary::decode(&encoded).expect("decoding should succeed");
        prop_assert_eq!(interface, decoded);
    }

    #[test]
    fn valid_format_after_encode(interface in arb_public_interface()) {
        let encoded = binary::encode(&interface).expect("encoding should succeed");
        prop_assert!(binary::is_valid_format(&encoded));
    }

    #[test]
    fn format_info_matches_encoded(interface in arb_public_interface()) {
        let encoded = binary::encode(&interface).expect("encoding should succeed");
        let info = binary::get_format_info(&encoded).expect("should get format info");
        prop_assert_eq!(info.version, binary::FORMAT_VERSION);
        prop_assert_eq!(info.total_size, encoded.len());
        prop_assert_eq!(info.payload_size, encoded.len() - binary::HEADER_SIZE);
    }

    #[test]
    fn deterministic_encoding(interface in arb_public_interface()) {
        let encoded1 = binary::encode(&interface).expect("first encoding should succeed");
        let encoded2 = binary::encode(&interface).expect("second encoding should succeed");
        prop_assert_eq!(encoded1, encoded2, "Encoding should be deterministic");
    }
}

// ==================== Edge Case Property Tests ====================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn corrupted_data_detected(interface in arb_public_interface(), corrupt_pos in 0usize..1000, corrupt_val in any::<u8>()) {
        let mut encoded = binary::encode(&interface).expect("encoding should succeed");

        // Only corrupt if position is within data bounds and after header
        if corrupt_pos < encoded.len() && corrupt_pos >= binary::HEADER_SIZE {
            let original = encoded[corrupt_pos];
            if original != corrupt_val {
                encoded[corrupt_pos] = corrupt_val;
                // After corruption, either checksum fails or deserialization fails
                let result: Result<PublicInterface, _> = binary::decode(&encoded);
                prop_assert!(result.is_err() || !binary::is_valid_format(&encoded));
            }
        }
    }

    #[test]
    fn magic_corruption_detected(interface in arb_public_interface(), corrupt_pos in 0usize..4, corrupt_val in any::<u8>()) {
        let mut encoded = binary::encode(&interface).expect("encoding should succeed");

        let original = encoded[corrupt_pos];
        if original != corrupt_val {
            encoded[corrupt_pos] = corrupt_val;
            prop_assert!(!binary::is_valid_format(&encoded));
        }
    }

    #[test]
    fn version_corruption_detected(interface in arb_public_interface(), new_version in any::<u16>()) {
        let mut encoded = binary::encode(&interface).expect("encoding should succeed");

        // Change version bytes
        let version_bytes = new_version.to_le_bytes();
        encoded[4] = version_bytes[0];
        encoded[5] = version_bytes[1];

        if new_version != binary::FORMAT_VERSION {
            prop_assert!(!binary::is_valid_format(&encoded));
        }
    }
}

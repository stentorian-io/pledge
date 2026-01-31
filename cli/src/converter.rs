//! Converter module for transforming SWC AST types to interlint-format types.
//!
//! This module provides conversion functions to translate the internal SWC-based
//! EntityDeclaration types to the language-agnostic format types defined in
//! the interlint-format crate.

use crate::ecma::entity::EntityDeclaration;
use crate::ecma::parser::parse_pkg_entry;
use crate::ecma::walker::{get_exports_in_module, Declarations};
use crate::pkg::entries::{PkgEntries, PkgEntry, PkgEntryType};
use crate::pkg::Pkg;
use anyhow::{Context, Result};
use interlint_format::{
    ClassSig, Entrypoint, EnumMember, EnumSig, Export, FunctionSig, InterfaceSig, Metadata,
    Module, Parameter, Property, PublicInterface, Signature, TypeAliasSig, TypeInfo, TypeKind,
    TypeParameter, VariableSig, Visibility,
};
use std::collections::HashMap;
use swc_ecma_ast::{
    Accessibility, Class, Function, Param, Pat, TsEntityName, TsEnumDecl, TsFnParam,
    TsInterfaceDecl, TsType, TsTypeAliasDecl, TsTypeAnn, TsTypeParamDecl, VarDeclarator,
};

/// Convert a Pkg to a PublicInterface.
///
/// This function extracts all public exports from a package and converts them
/// to the language-agnostic format.
pub fn pkg_to_public_interface(pkg: &Pkg) -> Result<PublicInterface> {
    let metadata = Metadata {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        description: None,
        repository: None,
        license: None,
        language: Some("typescript".to_string()),
    };

    let mut interface = PublicInterface::new(metadata);

    // Add entrypoints from package entries
    add_entrypoints_from_entries(&mut interface, &pkg.entries)?;

    // Add modules from each entry type
    add_modules_from_entries(&mut interface, &pkg.entries)?;

    Ok(interface)
}

/// Add entrypoints from PkgEntries to the PublicInterface.
fn add_entrypoints_from_entries(interface: &mut PublicInterface, entries: &PkgEntries) -> Result<()> {
    // Main entry
    for (name, entry) in &entries.main {
        interface.add_entrypoint(Entrypoint {
            name: name.clone(),
            module_path: entry.path.to_string_lossy().to_string(),
            conditions: vec!["require".to_string(), "default".to_string()],
        });
    }

    // Exports entries
    for (name, entry) in &entries.exports {
        interface.add_entrypoint(Entrypoint {
            name: name.clone(),
            module_path: entry.path.to_string_lossy().to_string(),
            conditions: vec!["import".to_string(), "require".to_string()],
        });
    }

    // Browser entries
    for (name, entry) in &entries.browser {
        interface.add_entrypoint(Entrypoint {
            name: name.clone(),
            module_path: entry.path.to_string_lossy().to_string(),
            conditions: vec!["browser".to_string()],
        });
    }

    // Bin entries
    for (name, entry) in &entries.bin {
        interface.add_entrypoint(Entrypoint {
            name: name.clone(),
            module_path: entry.path.to_string_lossy().to_string(),
            conditions: vec!["bin".to_string()],
        });
    }

    Ok(())
}

/// Add modules from PkgEntries to the PublicInterface.
fn add_modules_from_entries(interface: &mut PublicInterface, entries: &PkgEntries) -> Result<()> {
    let mut processed_paths: HashMap<String, bool> = HashMap::new();

    // Process all entry types
    let all_entries = [
        (&entries.main, PkgEntryType::Main),
        (&entries.exports, PkgEntryType::Exports),
        (&entries.browser, PkgEntryType::Browser),
        (&entries.bin, PkgEntryType::Bin),
    ];

    for (entry_map, _entry_type) in all_entries {
        for (_name, entry) in entry_map {
            let path_str = entry.path.to_string_lossy().to_string();

            // Skip already processed modules
            if processed_paths.contains_key(&path_str) {
                continue;
            }
            processed_paths.insert(path_str.clone(), true);

            // Parse and convert the module
            if let Ok(module) = entry_to_module(entry) {
                interface.add_module(module);
            }
        }
    }

    Ok(())
}

/// Convert a PkgEntry to a Module.
fn entry_to_module(entry: &PkgEntry) -> Result<Module> {
    let path = entry.path.to_string_lossy().to_string();
    let mut module = Module::new(&path);

    // Parse the entry file
    let parsed = parse_pkg_entry(entry)
        .with_context(|| format!("Failed to parse entry: {}", path))?;

    // Get exports from the parsed module
    let base_path = entry.dir_path();
    let (default_export, named_exports) = get_exports_in_module(base_path, parsed)
        .with_context(|| format!("Failed to get exports from module: {}", path))?;

    // Add default export if present
    if let Some(default_decl) = default_export {
        let signature = entity_to_signature(&default_decl)?;
        module.add_export(Export {
            name: "default".to_string(),
            original_name: Some(signature.name().to_string()),
            signature,
            is_default: true,
        });
    }

    // Add named exports
    for (name, decl) in named_exports {
        let signature = entity_to_signature(&decl)?;
        module.add_export(Export {
            name: name.clone(),
            original_name: None,
            signature,
            is_default: false,
        });
    }

    Ok(module)
}

/// Convert an EntityDeclaration to a Signature.
pub fn entity_to_signature(entity: &EntityDeclaration) -> Result<Signature> {
    match entity {
        EntityDeclaration::Func(name, function) => {
            Ok(Signature::Function(function_to_sig(name, function)))
        }
        EntityDeclaration::Class(name, class) => {
            Ok(Signature::Class(class_to_sig(name, class)))
        }
        EntityDeclaration::Var(name, var) => {
            Ok(Signature::Variable(var_to_sig(name, var)))
        }
        EntityDeclaration::Interface(name, interface) => {
            Ok(Signature::Interface(interface_to_sig(name, interface)))
        }
        EntityDeclaration::Enum(name, ts_enum) => {
            Ok(Signature::Enum(enum_to_sig(name, ts_enum)))
        }
        EntityDeclaration::TypeAlias(name, type_alias) => {
            Ok(Signature::TypeAlias(type_alias_to_sig(name, type_alias)))
        }
    }
}

/// Convert a SWC Function to FunctionSig.
fn function_to_sig(name: &str, function: &Function) -> FunctionSig {
    let type_parameters = function
        .type_params
        .as_ref()
        .map(|tp| convert_type_params(tp))
        .unwrap_or_default();

    let parameters: Vec<Parameter> = function
        .params
        .iter()
        .map(|param| param_to_parameter(param))
        .collect();

    let return_type = function
        .return_type
        .as_ref()
        .map(|rt| ts_type_ann_to_type_info(rt))
        .unwrap_or_else(TypeInfo::void);

    FunctionSig {
        name: name.to_string(),
        type_parameters,
        parameters,
        return_type,
        is_async: function.is_async,
        is_generator: function.is_generator,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert a SWC Class to ClassSig.
fn class_to_sig(name: &str, class: &Class) -> ClassSig {
    let type_parameters = class
        .type_params
        .as_ref()
        .map(|tp| convert_type_params(tp))
        .unwrap_or_default();

    let extends = class.super_class.as_ref().and_then(|sc| {
        sc.as_ident().map(|ident| TypeInfo::reference(ident.sym.to_string()))
    });

    let implements: Vec<TypeInfo> = class
        .implements
        .iter()
        .filter_map(|impl_| {
            // TsExprWithTypeArgs.expr is a Box<Expr>, we need to extract identifier
            impl_.expr.as_ident().map(|ident| TypeInfo::reference(ident.sym.to_string()))
        })
        .collect();

    let mut properties: Vec<Property> = Vec::new();
    let mut methods: Vec<FunctionSig> = Vec::new();
    let mut constructor: Option<Box<FunctionSig>> = None;

    for member in &class.body {
        if let Some(prop) = member.as_class_prop() {
            if let Some(key) = prop.key.as_ident() {
                let type_info = prop
                    .type_ann
                    .as_ref()
                    .map(|ta| ts_type_ann_to_type_info(ta))
                    .unwrap_or_else(TypeInfo::any);

                properties.push(Property {
                    name: key.sym.to_string(),
                    type_info,
                    optional: prop.is_optional,
                    readonly: prop.readonly,
                    is_static: prop.is_static,
                    visibility: accessibility_to_visibility(prop.accessibility),
                    doc: None,
                });
            }
        } else if let Some(method) = member.as_method() {
            if let Some(key) = method.key.as_ident() {
                let method_name = key.sym.to_string();
                let fn_sig = function_to_sig(&method_name, &method.function);
                methods.push(FunctionSig {
                    visibility: accessibility_to_visibility(method.accessibility),
                    ..fn_sig
                });
            }
        } else if let Some(ctor) = member.as_constructor() {
            let params: Vec<Parameter> = ctor
                .params
                .iter()
                .filter_map(|p| {
                    match p {
                        swc_ecma_ast::ParamOrTsParamProp::Param(param) => {
                            Some(param_to_parameter(param))
                        }
                        swc_ecma_ast::ParamOrTsParamProp::TsParamProp(ts_param) => {
                            ts_param_prop_to_parameter(ts_param)
                        }
                    }
                })
                .collect();

            constructor = Some(Box::new(FunctionSig {
                name: "constructor".to_string(),
                type_parameters: vec![],
                parameters: params,
                return_type: TypeInfo::reference(name),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }));
        }
    }

    ClassSig {
        name: name.to_string(),
        type_parameters,
        extends,
        implements,
        properties,
        methods,
        constructor,
        is_abstract: class.is_abstract,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert a VarDeclarator to VariableSig.
fn var_to_sig(name: &str, var: &VarDeclarator) -> VariableSig {
    let type_info = var
        .name
        .as_ident()
        .and_then(|ident| ident.type_ann.as_ref())
        .map(|ta| ts_type_ann_to_type_info(ta))
        .unwrap_or_else(|| infer_type_from_init(var));

    VariableSig {
        name: name.to_string(),
        type_info,
        is_const: true, // Exported variables are typically const
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert a Param to Parameter.
fn param_to_parameter(param: &Param) -> Parameter {
    let (name, type_info, optional) = extract_param_info(&param.pat);

    Parameter {
        name,
        type_info,
        optional,
        rest: matches!(param.pat, Pat::Rest(_)),
        default_value: None,
    }
}

/// Extract parameter info from a pattern.
fn extract_param_info(pat: &Pat) -> (String, TypeInfo, bool) {
    match pat {
        Pat::Ident(ident) => {
            let type_info = ident
                .type_ann
                .as_ref()
                .map(|ta| ts_type_ann_to_type_info(ta))
                .unwrap_or_else(TypeInfo::any);
            (ident.sym.to_string(), type_info, ident.optional)
        }
        Pat::Rest(rest) => {
            let (name, inner_type, _) = extract_param_info(&rest.arg);
            let type_info = TypeInfo::array(inner_type);
            (name, type_info, false)
        }
        Pat::Assign(assign) => {
            let (name, type_info, _) = extract_param_info(&assign.left);
            (name, type_info, true) // Has default, so optional
        }
        Pat::Array(_) => ("_".to_string(), TypeInfo::any(), false),
        Pat::Object(_) => ("_".to_string(), TypeInfo::any(), false),
        Pat::Expr(_) => ("_".to_string(), TypeInfo::any(), false),
        Pat::Invalid(_) => ("_".to_string(), TypeInfo::any(), false),
    }
}

/// Convert a TsParamProp to Parameter.
fn ts_param_prop_to_parameter(ts_param: &swc_ecma_ast::TsParamProp) -> Option<Parameter> {
    match &ts_param.param {
        swc_ecma_ast::TsParamPropParam::Ident(ident) => {
            let type_info = ident
                .type_ann
                .as_ref()
                .map(|ta| ts_type_ann_to_type_info(ta))
                .unwrap_or_else(TypeInfo::any);
            Some(Parameter {
                name: ident.sym.to_string(),
                type_info,
                optional: ident.optional,
                rest: false,
                default_value: None,
            })
        }
        swc_ecma_ast::TsParamPropParam::Assign(assign) => {
            let (name, type_info, _) = extract_param_info(&Pat::Assign(assign.clone()));
            Some(Parameter {
                name,
                type_info,
                optional: true,
                rest: false,
                default_value: None,
            })
        }
    }
}

/// Convert type parameters.
fn convert_type_params(type_params: &TsTypeParamDecl) -> Vec<TypeParameter> {
    type_params
        .params
        .iter()
        .map(|p| TypeParameter {
            name: p.name.sym.to_string(),
            constraint: p.constraint.as_ref().map(|c| Box::new(ts_type_to_type_info(c))),
            default: p.default.as_ref().map(|d| Box::new(ts_type_to_type_info(d))),
        })
        .collect()
}

/// Convert TsTypeAnn to TypeInfo.
fn ts_type_ann_to_type_info(type_ann: &TsTypeAnn) -> TypeInfo {
    ts_type_to_type_info(&type_ann.type_ann)
}

/// Convert TsType to TypeInfo.
fn ts_type_to_type_info(ts_type: &TsType) -> TypeInfo {
    match ts_type {
        TsType::TsKeywordType(kw) => {
            use swc_ecma_ast::TsKeywordTypeKind::*;
            match kw.kind {
                TsStringKeyword => TypeInfo::primitive("string"),
                TsNumberKeyword => TypeInfo::primitive("number"),
                TsBooleanKeyword => TypeInfo::primitive("boolean"),
                TsBigIntKeyword => TypeInfo::primitive("bigint"),
                TsSymbolKeyword => TypeInfo::primitive("symbol"),
                TsNullKeyword => TypeInfo::primitive("null"),
                TsUndefinedKeyword => TypeInfo::primitive("undefined"),
                TsVoidKeyword => TypeInfo::void(),
                TsNeverKeyword => TypeInfo {
                    kind: TypeKind::Never,
                    repr: "never".to_string(),
                    type_arguments: vec![],
                    constituents: vec![],
                    element_type: None,
                    key_type: None,
                    value_type: None,
                    param_types: vec![],
                    return_type: None,
                    nullable: false,
                },
                TsAnyKeyword => TypeInfo::any(),
                TsUnknownKeyword => TypeInfo {
                    kind: TypeKind::Unknown,
                    repr: "unknown".to_string(),
                    type_arguments: vec![],
                    constituents: vec![],
                    element_type: None,
                    key_type: None,
                    value_type: None,
                    param_types: vec![],
                    return_type: None,
                    nullable: false,
                },
                TsObjectKeyword => TypeInfo {
                    kind: TypeKind::Object,
                    repr: "object".to_string(),
                    type_arguments: vec![],
                    constituents: vec![],
                    element_type: None,
                    key_type: None,
                    value_type: None,
                    param_types: vec![],
                    return_type: None,
                    nullable: false,
                },
                TsIntrinsicKeyword => TypeInfo::primitive("intrinsic"),
            }
        }
        TsType::TsTypeRef(type_ref) => {
            let name = ts_entity_name_to_string(&type_ref.type_name);
            let type_arguments: Vec<TypeInfo> = type_ref
                .type_params
                .as_ref()
                .map(|params| {
                    params.params.iter().map(|p| ts_type_to_type_info(p)).collect()
                })
                .unwrap_or_default();

            let repr = if type_arguments.is_empty() {
                name.clone()
            } else {
                let args: Vec<String> = type_arguments.iter().map(|t| t.repr.clone()).collect();
                format!("{}<{}>", name, args.join(", "))
            };

            TypeInfo {
                kind: if type_arguments.is_empty() {
                    TypeKind::Reference
                } else {
                    TypeKind::Generic
                },
                repr,
                type_arguments,
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            }
        }
        TsType::TsArrayType(arr) => {
            let elem_type = ts_type_to_type_info(&arr.elem_type);
            TypeInfo::array(elem_type)
        }
        TsType::TsUnionOrIntersectionType(union_or_intersection) => {
            match union_or_intersection {
                swc_ecma_ast::TsUnionOrIntersectionType::TsUnionType(union) => {
                    let constituents: Vec<TypeInfo> = union
                        .types
                        .iter()
                        .map(|t| ts_type_to_type_info(t))
                        .collect();
                    let repr = constituents
                        .iter()
                        .map(|t| t.repr.clone())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    TypeInfo {
                        kind: TypeKind::Union,
                        repr,
                        type_arguments: vec![],
                        constituents,
                        element_type: None,
                        key_type: None,
                        value_type: None,
                        param_types: vec![],
                        return_type: None,
                        nullable: false,
                    }
                }
                swc_ecma_ast::TsUnionOrIntersectionType::TsIntersectionType(intersection) => {
                    let constituents: Vec<TypeInfo> = intersection
                        .types
                        .iter()
                        .map(|t| ts_type_to_type_info(t))
                        .collect();
                    let repr = constituents
                        .iter()
                        .map(|t| t.repr.clone())
                        .collect::<Vec<_>>()
                        .join(" & ");
                    TypeInfo {
                        kind: TypeKind::Intersection,
                        repr,
                        type_arguments: vec![],
                        constituents,
                        element_type: None,
                        key_type: None,
                        value_type: None,
                        param_types: vec![],
                        return_type: None,
                        nullable: false,
                    }
                }
            }
        }
        TsType::TsFnOrConstructorType(fn_type) => {
            match fn_type {
                swc_ecma_ast::TsFnOrConstructorType::TsFnType(fn_t) => {
                    let param_types: Vec<TypeInfo> = fn_t
                        .params
                        .iter()
                        .map(|p| ts_fn_param_to_type_info(p))
                        .collect();
                    let return_type = Box::new(ts_type_to_type_info(&fn_t.type_ann.type_ann));

                    let param_repr: Vec<String> = fn_t
                        .params
                        .iter()
                        .map(|p| {
                            let (name, ti) = ts_fn_param_name_and_type(p);
                            format!("{}: {}", name, ti.repr)
                        })
                        .collect();
                    let repr = format!(
                        "({}) => {}",
                        param_repr.join(", "),
                        return_type.repr
                    );

                    TypeInfo {
                        kind: TypeKind::Function,
                        repr,
                        type_arguments: vec![],
                        constituents: vec![],
                        element_type: None,
                        key_type: None,
                        value_type: None,
                        param_types,
                        return_type: Some(return_type),
                        nullable: false,
                    }
                }
                swc_ecma_ast::TsFnOrConstructorType::TsConstructorType(ctor) => {
                    let param_types: Vec<TypeInfo> = ctor
                        .params
                        .iter()
                        .map(|p| ts_fn_param_to_type_info(p))
                        .collect();
                    let return_type = Box::new(ts_type_to_type_info(&ctor.type_ann.type_ann));

                    TypeInfo {
                        kind: TypeKind::Function,
                        repr: format!("new (...) => {}", return_type.repr),
                        type_arguments: vec![],
                        constituents: vec![],
                        element_type: None,
                        key_type: None,
                        value_type: None,
                        param_types,
                        return_type: Some(return_type),
                        nullable: false,
                    }
                }
            }
        }
        TsType::TsTypeLit(_lit) => {
            TypeInfo {
                kind: TypeKind::Object,
                repr: "{ ... }".to_string(),
                type_arguments: vec![],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            }
        }
        TsType::TsTupleType(tuple) => {
            let constituents: Vec<TypeInfo> = tuple
                .elem_types
                .iter()
                .map(|t| ts_type_to_type_info(&t.ty))
                .collect();
            let repr = format!(
                "[{}]",
                constituents
                    .iter()
                    .map(|t| t.repr.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            TypeInfo {
                kind: TypeKind::Tuple,
                repr,
                type_arguments: vec![],
                constituents,
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            }
        }
        TsType::TsOptionalType(opt) => {
            let inner = ts_type_to_type_info(&opt.type_ann);
            TypeInfo {
                kind: TypeKind::Optional,
                repr: format!("{}?", inner.repr),
                type_arguments: vec![],
                constituents: vec![],
                element_type: Some(Box::new(inner)),
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: true,
            }
        }
        TsType::TsLitType(lit) => {
            let repr = match &lit.lit {
                swc_ecma_ast::TsLit::Number(n) => n.value.to_string(),
                swc_ecma_ast::TsLit::Str(s) => format!("\"{}\"", s.value),
                swc_ecma_ast::TsLit::Bool(b) => b.value.to_string(),
                swc_ecma_ast::TsLit::BigInt(b) => format!("{}n", b.value),
                swc_ecma_ast::TsLit::Tpl(_) => "template literal".to_string(),
            };
            TypeInfo {
                kind: TypeKind::Literal,
                repr,
                type_arguments: vec![],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            }
        }
        _ => TypeInfo::any(),
    }
}

/// Convert TsFnParam to TypeInfo.
fn ts_fn_param_to_type_info(param: &TsFnParam) -> TypeInfo {
    let (_, type_info) = ts_fn_param_name_and_type(param);
    type_info
}

/// Get name and type from TsFnParam.
fn ts_fn_param_name_and_type(param: &TsFnParam) -> (String, TypeInfo) {
    match param {
        TsFnParam::Ident(ident) => {
            let type_info = ident
                .type_ann
                .as_ref()
                .map(|ta| ts_type_ann_to_type_info(ta))
                .unwrap_or_else(TypeInfo::any);
            (ident.sym.to_string(), type_info)
        }
        TsFnParam::Array(arr) => {
            let type_info = arr
                .type_ann
                .as_ref()
                .map(|ta| ts_type_ann_to_type_info(ta))
                .unwrap_or_else(TypeInfo::any);
            ("_".to_string(), type_info)
        }
        TsFnParam::Rest(rest) => {
            let type_info = rest
                .type_ann
                .as_ref()
                .map(|ta| ts_type_ann_to_type_info(ta))
                .unwrap_or_else(TypeInfo::any);
            ("...rest".to_string(), type_info)
        }
        TsFnParam::Object(obj) => {
            let type_info = obj
                .type_ann
                .as_ref()
                .map(|ta| ts_type_ann_to_type_info(ta))
                .unwrap_or_else(TypeInfo::any);
            ("_".to_string(), type_info)
        }
    }
}

/// Convert TsFnParam to Parameter.
fn ts_fn_param_to_parameter(param: &TsFnParam) -> Parameter {
    let (name, type_info) = ts_fn_param_name_and_type(param);
    let (optional, rest) = match param {
        TsFnParam::Ident(ident) => (ident.optional, false),
        TsFnParam::Rest(_) => (false, true),
        _ => (false, false),
    };

    Parameter {
        name,
        type_info,
        optional,
        rest,
        default_value: None,
    }
}

/// Convert TsEntityName to string.
fn ts_entity_name_to_string(name: &TsEntityName) -> String {
    match name {
        TsEntityName::Ident(ident) => ident.sym.to_string(),
        TsEntityName::TsQualifiedName(qn) => {
            format!(
                "{}.{}",
                ts_entity_name_to_string(&qn.left),
                qn.right.sym
            )
        }
    }
}

/// Convert TsEntityName to TypeInfo.
#[allow(dead_code)]
fn ts_entity_name_to_type_info(name: &TsEntityName) -> TypeInfo {
    TypeInfo::reference(ts_entity_name_to_string(name))
}

/// Convert accessibility to visibility.
fn accessibility_to_visibility(accessibility: Option<Accessibility>) -> Visibility {
    match accessibility {
        Some(Accessibility::Public) | None => Visibility::Public,
        Some(Accessibility::Protected) => Visibility::Protected,
        Some(Accessibility::Private) => Visibility::Private,
    }
}

/// Infer type from initializer.
fn infer_type_from_init(var: &VarDeclarator) -> TypeInfo {
    if let Some(init) = &var.init {
        if init.is_fn_expr() || init.is_arrow() {
            return TypeInfo {
                kind: TypeKind::Function,
                repr: "(...) => any".to_string(),
                type_arguments: vec![],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: Some(Box::new(TypeInfo::any())),
                nullable: false,
            };
        } else if init.is_object() {
            return TypeInfo {
                kind: TypeKind::Object,
                repr: "object".to_string(),
                type_arguments: vec![],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            };
        } else if init.is_array() {
            return TypeInfo::array(TypeInfo::any());
        } else if init.is_lit() {
            if let Some(lit) = init.as_lit() {
                return match lit {
                    swc_ecma_ast::Lit::Str(_) => TypeInfo::primitive("string"),
                    swc_ecma_ast::Lit::Bool(_) => TypeInfo::primitive("boolean"),
                    swc_ecma_ast::Lit::Num(_) => TypeInfo::primitive("number"),
                    swc_ecma_ast::Lit::BigInt(_) => TypeInfo::primitive("bigint"),
                    swc_ecma_ast::Lit::Null(_) => TypeInfo::primitive("null"),
                    swc_ecma_ast::Lit::Regex(_) => TypeInfo::reference("RegExp"),
                    swc_ecma_ast::Lit::JSXText(_) => TypeInfo::primitive("string"),
                };
            }
        }
    }
    TypeInfo::any()
}

/// Convert declarations map to format exports.
pub fn declarations_to_exports(declarations: &Declarations) -> Result<Vec<Export>> {
    let mut exports = Vec::new();

    for (name, decl) in declarations {
        let signature = entity_to_signature(decl)?;
        exports.push(Export {
            name: name.clone(),
            original_name: None,
            signature,
            is_default: false,
        });
    }

    Ok(exports)
}

/// Convert a TsInterfaceDecl to InterfaceSig.
fn interface_to_sig(name: &str, interface: &TsInterfaceDecl) -> InterfaceSig {
    let type_parameters = interface
        .type_params
        .as_ref()
        .map(|tp| convert_type_params(tp))
        .unwrap_or_default();

    let extends: Vec<TypeInfo> = interface
        .extends
        .iter()
        .map(|ext| {
            // Extract name from the expression
            let name = match ext.expr.as_ref() {
                swc_ecma_ast::Expr::Ident(ident) => ident.sym.to_string(),
                swc_ecma_ast::Expr::Member(member) => {
                    if let Some(ident) = member.prop.as_ident() {
                        ident.sym.to_string()
                    } else {
                        "unknown".to_string()
                    }
                }
                _ => "unknown".to_string(),
            };
            TypeInfo::reference(&name)
        })
        .collect();

    let mut properties = Vec::new();
    let mut methods = Vec::new();

    for member in &interface.body.body {
        match member {
            swc_ecma_ast::TsTypeElement::TsPropertySignature(prop) => {
                if let Some(key) = prop.key.as_ident() {
                    let type_info = prop
                        .type_ann
                        .as_ref()
                        .map(|ta| ts_type_ann_to_type_info(ta))
                        .unwrap_or_else(TypeInfo::any);

                    properties.push(Property {
                        name: key.sym.to_string(),
                        type_info,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_static: false,
                        visibility: Visibility::Public,
                        doc: None,
                    });
                }
            }
            swc_ecma_ast::TsTypeElement::TsMethodSignature(method) => {
                if let Some(key) = method.key.as_ident() {
                    let params: Vec<Parameter> = method
                        .params
                        .iter()
                        .map(|p| ts_fn_param_to_parameter(p))
                        .collect();

                    let return_type = method
                        .type_ann
                        .as_ref()
                        .map(|ta| ts_type_ann_to_type_info(ta))
                        .unwrap_or_else(TypeInfo::void);

                    methods.push(FunctionSig {
                        name: key.sym.to_string(),
                        type_parameters: vec![],
                        parameters: params,
                        return_type,
                        is_async: false,
                        is_generator: false,
                        visibility: Visibility::Public,
                        doc: None,
                    });
                }
            }
            _ => {}
        }
    }

    InterfaceSig {
        name: name.to_string(),
        type_parameters,
        extends,
        properties,
        methods,
        index_signature: None,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert a TsEnumDecl to EnumSig.
fn enum_to_sig(name: &str, ts_enum: &TsEnumDecl) -> EnumSig {
    let members: Vec<EnumMember> = ts_enum
        .members
        .iter()
        .map(|member| {
            let member_name = match &member.id {
                swc_ecma_ast::TsEnumMemberId::Ident(ident) => ident.sym.to_string(),
                swc_ecma_ast::TsEnumMemberId::Str(s) => s.value.to_string(),
            };

            let value = member.init.as_ref().map(|init| {
                match init.as_ref() {
                    swc_ecma_ast::Expr::Lit(lit) => match lit {
                        swc_ecma_ast::Lit::Str(s) => format!("\"{}\"", s.value),
                        swc_ecma_ast::Lit::Num(n) => n.value.to_string(),
                        _ => "unknown".to_string(),
                    },
                    _ => "unknown".to_string(),
                }
            });

            EnumMember {
                name: member_name,
                value,
                doc: None,
            }
        })
        .collect();

    EnumSig {
        name: name.to_string(),
        members,
        is_const: ts_enum.is_const,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert a TsTypeAliasDecl to TypeAliasSig.
fn type_alias_to_sig(name: &str, type_alias: &TsTypeAliasDecl) -> TypeAliasSig {
    let type_parameters = type_alias
        .type_params
        .as_ref()
        .map(|tp| convert_type_params(tp))
        .unwrap_or_default();

    let aliased_type = ts_type_to_type_info(&type_alias.type_ann);

    TypeAliasSig {
        name: name.to_string(),
        type_parameters,
        aliased_type,
        visibility: Visibility::Public,
        doc: None,
    }
}

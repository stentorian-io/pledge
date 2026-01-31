//! Converter module for transforming tree-sitter parsed types to interlint-format types.
//!
//! This module provides conversion functions to translate the internal tree-sitter-based
//! EntityDeclaration types to the language-agnostic format types defined in
//! the interlint-format crate.

use crate::ecma::entity::{
    ClassInfo, EntityDeclaration, EnumInfo, FuncInfo, InterfaceInfo, TypeAliasInfo, VarInfo,
    VarKind,
};
use crate::ecma::parser::parse_pkg_entry;
use crate::ecma::walker::{get_exports_in_module, Declarations};
use crate::pkg::entries::{PkgEntries, PkgEntry, PkgEntryType};
use crate::pkg::Pkg;
use anyhow::{Context, Result};
use interlint_format::{
    Asset, ClassSig, Entrypoint, EnumMember, EnumSig, Export, FunctionSig, InterfaceSig, Metadata,
    Module, Parameter, Property, PublicInterface, Signature, TypeAliasSig, TypeInfo, TypeKind,
    VariableSig, Visibility,
};
use std::collections::HashMap;

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

    // Add assets from package contents
    add_assets_from_contents(&mut interface, pkg)?;

    Ok(interface)
}

/// Add entrypoints from PkgEntries to the PublicInterface.
fn add_entrypoints_from_entries(
    interface: &mut PublicInterface,
    entries: &PkgEntries,
) -> Result<()> {
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

    for (entry_map, entry_type) in all_entries {
        println!("Processing {} entries: {} found", entry_type, entry_map.len());
        for (name, entry) in entry_map {
            let path_str = entry.path.to_string_lossy().to_string();
            println!("  - Entry '{}': {}", name, path_str);

            // Skip already processed modules
            if processed_paths.contains_key(&path_str) {
                continue;
            }
            processed_paths.insert(path_str.clone(), true);

            // Parse and convert the module
            match entry_to_module(entry) {
                Ok(module) => {
                    println!("    -> Successfully parsed, {} exports", module.exports.len());
                    interface.add_module(module);
                }
                Err(e) => {
                    eprintln!("    -> Failed to parse: {}", e);
                }
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
    println!("    -> File parsed successfully");

    // Get exports from the parsed module
    let base_path = entry.dir_path();
    let base_path_display = base_path.display().to_string();
    println!("    -> Extracting exports (base_path: {})", base_path_display);
    let (default_export, named_exports) = get_exports_in_module(base_path, parsed)
        .with_context(|| {
            format!("Failed to get exports from module: {} (base_path: {})", path, base_path_display)
        })?;
    println!("    -> Extracted {} named exports + {} default", named_exports.len(), if default_export.is_some() { 1 } else { 0 });

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
        EntityDeclaration::Func(name, info) => {
            Ok(Signature::Function(function_to_sig(name, info)))
        }
        EntityDeclaration::Class(name, info) => {
            Ok(Signature::Class(class_to_sig(name, info)))
        }
        EntityDeclaration::Var(name, info) => {
            Ok(Signature::Variable(var_to_sig(name, info)))
        }
        EntityDeclaration::Interface(name, info) => {
            Ok(Signature::Interface(interface_to_sig(name, info)))
        }
        EntityDeclaration::Enum(name, info) => {
            Ok(Signature::Enum(enum_to_sig(name, info)))
        }
        EntityDeclaration::TypeAlias(name, info) => {
            Ok(Signature::TypeAlias(type_alias_to_sig(name, info)))
        }
    }
}

/// Convert FuncInfo to FunctionSig.
fn function_to_sig(name: &str, info: &FuncInfo) -> FunctionSig {
    let parameters: Vec<Parameter> = info
        .params
        .iter()
        .map(|param_name| Parameter {
            name: param_name.clone(),
            type_info: TypeInfo::any(),
            optional: false,
            rest: false,
            default_value: None,
        })
        .collect();

    // Try to extract return type from source if available
    let return_type = extract_return_type_from_source(&info.source);

    FunctionSig {
        name: name.to_string(),
        type_parameters: vec![],
        parameters,
        return_type,
        is_async: info.is_async,
        is_generator: info.is_generator,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert ClassInfo to ClassSig.
fn class_to_sig(name: &str, info: &ClassInfo) -> ClassSig {
    let methods: Vec<FunctionSig> = info
        .methods
        .iter()
        .map(|method_name| FunctionSig {
            name: method_name.clone(),
            type_parameters: vec![],
            parameters: vec![],
            return_type: TypeInfo::any(),
            is_async: false,
            is_generator: false,
            visibility: Visibility::Public,
            doc: None,
        })
        .collect();

    let properties: Vec<Property> = info
        .properties
        .iter()
        .map(|prop_name| Property {
            name: prop_name.clone(),
            type_info: TypeInfo::any(),
            optional: false,
            readonly: false,
            is_static: false,
            visibility: Visibility::Public,
            doc: None,
        })
        .collect();

    ClassSig {
        name: name.to_string(),
        type_parameters: vec![],
        extends: None,
        implements: vec![],
        properties,
        methods,
        constructor: None,
        is_abstract: info.is_abstract,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert VarInfo to VariableSig.
fn var_to_sig(name: &str, info: &VarInfo) -> VariableSig {
    // Try to extract type from source if available
    let type_info = extract_type_from_source(&info.source);

    VariableSig {
        name: name.to_string(),
        type_info,
        is_const: info.kind == VarKind::Const,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert InterfaceInfo to InterfaceSig.
fn interface_to_sig(name: &str, info: &InterfaceInfo) -> InterfaceSig {
    let properties: Vec<Property> = info
        .properties
        .iter()
        .map(|prop_name| Property {
            name: prop_name.clone(),
            type_info: TypeInfo::any(),
            optional: false,
            readonly: false,
            is_static: false,
            visibility: Visibility::Public,
            doc: None,
        })
        .collect();

    let methods: Vec<FunctionSig> = info
        .methods
        .iter()
        .map(|method_name| FunctionSig {
            name: method_name.clone(),
            type_parameters: vec![],
            parameters: vec![],
            return_type: TypeInfo::any(),
            is_async: false,
            is_generator: false,
            visibility: Visibility::Public,
            doc: None,
        })
        .collect();

    InterfaceSig {
        name: name.to_string(),
        type_parameters: vec![],
        extends: vec![],
        properties,
        methods,
        index_signature: None,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert EnumInfo to EnumSig.
fn enum_to_sig(name: &str, info: &EnumInfo) -> EnumSig {
    let members: Vec<EnumMember> = info
        .members
        .iter()
        .map(|member_name| EnumMember {
            name: member_name.clone(),
            value: None,
            doc: None,
        })
        .collect();

    EnumSig {
        name: name.to_string(),
        members,
        is_const: info.source.contains("const enum"),
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Convert TypeAliasInfo to TypeAliasSig.
fn type_alias_to_sig(name: &str, info: &TypeAliasInfo) -> TypeAliasSig {
    // Try to extract the aliased type from source
    let aliased_type = extract_aliased_type_from_source(&info.source);

    TypeAliasSig {
        name: name.to_string(),
        type_parameters: vec![],
        aliased_type,
        visibility: Visibility::Public,
        doc: None,
    }
}

/// Extract return type from function source.
fn extract_return_type_from_source(source: &str) -> TypeInfo {
    // Look for ): Type pattern
    if let Some(idx) = source.find("): ") {
        let rest = &source[idx + 3..];
        if let Some(end) = rest.find(|c: char| c == '{' || c == ';' || c == '\n') {
            let type_str = rest[..end].trim();
            return parse_type_string(type_str);
        }
    }
    TypeInfo::any()
}

/// Extract type annotation from source.
fn extract_type_from_source(source: &str) -> TypeInfo {
    // Look for : Type = or : Type; pattern
    if let Some(idx) = source.find(": ") {
        let rest = &source[idx + 2..];
        if let Some(end) = rest.find(|c: char| c == '=' || c == ';' || c == '\n') {
            let type_str = rest[..end].trim();
            return parse_type_string(type_str);
        }
    }
    TypeInfo::any()
}

/// Extract aliased type from type alias source.
fn extract_aliased_type_from_source(source: &str) -> TypeInfo {
    // Look for = Type pattern
    if let Some(idx) = source.find(" = ") {
        let rest = &source[idx + 3..];
        if let Some(end) = rest.find(';') {
            let type_str = rest[..end].trim();
            return parse_type_string(type_str);
        }
        // No semicolon, take the rest
        return parse_type_string(rest.trim());
    }
    TypeInfo::any()
}

/// Parse a type string into TypeInfo.
fn parse_type_string(type_str: &str) -> TypeInfo {
    let type_str = type_str.trim();

    // Primitive types
    match type_str {
        "string" => return TypeInfo::primitive("string"),
        "number" => return TypeInfo::primitive("number"),
        "boolean" => return TypeInfo::primitive("boolean"),
        "bigint" => return TypeInfo::primitive("bigint"),
        "symbol" => return TypeInfo::primitive("symbol"),
        "null" => return TypeInfo::primitive("null"),
        "undefined" => return TypeInfo::primitive("undefined"),
        "void" => return TypeInfo::void(),
        "any" => return TypeInfo::any(),
        "never" => {
            return TypeInfo {
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
                properties: vec![],
            }
        }
        "unknown" => {
            return TypeInfo {
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
                properties: vec![],
            }
        }
        "object" => {
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
                properties: vec![],
            }
        }
        _ => {}
    }

    // Array type
    if type_str.ends_with("[]") {
        let elem_type_str = &type_str[..type_str.len() - 2];
        return TypeInfo::array(parse_type_string(elem_type_str));
    }

    // Union type
    if type_str.contains(" | ") {
        let constituents: Vec<TypeInfo> = type_str
            .split(" | ")
            .map(|s| parse_type_string(s.trim()))
            .collect();
        let repr = constituents
            .iter()
            .map(|t| t.repr.clone())
            .collect::<Vec<_>>()
            .join(" | ");
        return TypeInfo {
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
            properties: vec![],
        };
    }

    // Intersection type
    if type_str.contains(" & ") {
        let constituents: Vec<TypeInfo> = type_str
            .split(" & ")
            .map(|s| parse_type_string(s.trim()))
            .collect();
        let repr = constituents
            .iter()
            .map(|t| t.repr.clone())
            .collect::<Vec<_>>()
            .join(" & ");
        return TypeInfo {
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
            properties: vec![],
        };
    }

    // Generic type (e.g., Array<T>, Promise<T>)
    if type_str.contains('<') && type_str.ends_with('>') {
        if let Some(idx) = type_str.find('<') {
            let _base_name = &type_str[..idx];
            let args_str = &type_str[idx + 1..type_str.len() - 1];
            let type_arguments: Vec<TypeInfo> = args_str
                .split(',')
                .map(|s| parse_type_string(s.trim()))
                .collect();

            return TypeInfo {
                kind: TypeKind::Generic,
                repr: type_str.to_string(),
                type_arguments,
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
                properties: vec![],
            };
        }
    }

    // Reference type (any other identifier)
    TypeInfo::reference(type_str)
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

/// Add assets from package contents to the PublicInterface.
fn add_assets_from_contents(interface: &mut PublicInterface, pkg: &Pkg) -> Result<()> {
    let assets = pkg
        .contents
        .asset_list()
        .with_context(|| "Failed to get asset list from package contents")?;

    for asset_path in assets {
        let path_str = asset_path.to_string_lossy().to_string();
        let extension = asset_path
            .extension()
            .map(|ext| ext.to_string_lossy().to_string());

        interface.add_asset(Asset {
            path: path_str,
            extension,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_primitive_type() {
        assert_eq!(parse_type_string("string").repr, "string");
        assert_eq!(parse_type_string("number").repr, "number");
        assert_eq!(parse_type_string("boolean").repr, "boolean");
    }

    #[test]
    fn test_parse_array_type() {
        let type_info = parse_type_string("string[]");
        assert!(matches!(type_info.kind, TypeKind::Array));
    }

    #[test]
    fn test_parse_union_type() {
        let type_info = parse_type_string("string | number");
        assert!(matches!(type_info.kind, TypeKind::Union));
        assert_eq!(type_info.constituents.len(), 2);
    }

    #[test]
    fn test_parse_generic_type() {
        let type_info = parse_type_string("Promise<string>");
        assert!(matches!(type_info.kind, TypeKind::Generic));
        assert_eq!(type_info.type_arguments.len(), 1);
    }

    #[test]
    fn test_parse_reference_type() {
        let type_info = parse_type_string("MyCustomType");
        assert!(matches!(type_info.kind, TypeKind::Reference));
        assert_eq!(type_info.repr, "MyCustomType");
    }
}

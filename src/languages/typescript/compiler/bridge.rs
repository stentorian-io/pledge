//! Evaluate public TypeScript contracts with the compiler paired with the LSP.

use crate::languages::javascript::model::{ExportKind, ExportMap, SymbolSpace};
use crate::languages::typescript::compiler::toolchain::CompilerBinary;
#[cfg(all(test, feature = "lsp-tests"))]
use crate::languages::typescript::compiler::toolchain::LspBinary;
use crate::languages::typescript::types::shape::{FieldShape, PrimitiveKind, TypeShape};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct CompilerResponse {
    results: Vec<ResolvedSymbol>,
    issues: Vec<String>,
}

#[derive(Deserialize)]
struct ResolvedSymbol {
    id: usize,
    overloads: Vec<TypeShape>,
    signature: Option<String>,
    typeof_targets: Vec<Option<async_lsp::lsp_types::Location>>,
    issues: Vec<String>,
    #[serde(default)]
    commonjs_members: Option<Vec<CommonJsMember>>,
    #[serde(default)]
    commonjs_property: Option<crate::languages::typescript::types::shape::FieldShape>,
}

#[derive(Deserialize)]
struct CommonJsMember {
    name: String,
    symbol: bool,
    field: FieldShape,
}

/// `None` is reserved for test servers without a paired compiler.
#[cfg(all(test, feature = "lsp-tests"))]
pub(super) async fn resolve_public_types(
    exports: &mut ExportMap,
    root: &Path,
    binary: &LspBinary,
) -> Option<Vec<String>> {
    let server = binary.tsserver_path(root)?;
    let toolchain = CompilerBinary {
        library: server.parent()?.join("typescript.js"),
        node: match binary {
            LspBinary::Managed { node, .. } => node.clone(),
            LspBinary::Direct(_) => super::toolchain::find_in_path("node")?,
        },
    };
    Some(
        evaluate_public_types(
            exports,
            root,
            &toolchain,
            &mut super::worker::WorkerSession::default(),
        )
        .await,
    )
}

pub(super) async fn evaluate_public_types(
    exports: &mut ExportMap,
    root: &Path,
    toolchain: &CompilerBinary,
    worker: &mut super::worker::WorkerSession,
) -> Vec<String> {
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => return vec![format!("compiler package directory unavailable: {error}")],
    };
    let library = match toolchain.library.canonicalize() {
        Ok(library) => library,
        Err(error) => return vec![format!("compiler library unavailable: {error}")],
    };
    let mut keys: Vec<_> = exports.keys().cloned().collect();
    keys.sort_by_key(|key| {
        format!(
            "{}|{:?}|{:?}|{}",
            key.subpath, key.condition, key.space, key.name
        )
    });
    let symbols: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(id, key)| {
            let symbol = &exports[key];
            let refs: Vec<_> = symbol.typeof_import_refs.iter().map(|reference| serde_json::json!({
                "name":reference.property, "line":reference.property_line, "col":reference.property_col
            })).collect();
            serde_json::json!({"id":id, "file":root.join(&symbol.file), "name":symbol.name,
            "local_name":symbol.local_name, "line":symbol.line, "col":symbol.col, "refs":refs,
            "space": if key.space == SymbolSpace::Type { "type" } else { "value" },
            "export_kind":match symbol.kind { ExportKind::Default => "default", ExportKind::Named => "named" }})
        })
        .collect();
    let input = serde_json::json!({"root":root, "compiler":library, "symbols":symbols});
    let response: CompilerResponse = match worker.request(toolchain, &input).await {
        Ok(response) => response,
        Err(issue) => return vec![issue],
    };
    let compiler_complete = response.issues.is_empty();
    let mut issues = response.issues;
    if response.results.len() != keys.len() {
        issues.push("compiler did not resolve every public declaration".into());
    }
    for result in response.results {
        let Some(key) = keys.get(result.id) else {
            issues.push("compiler returned an unknown public declaration".into());
            continue;
        };
        let symbol_complete = compiler_complete && result.issues.is_empty();
        issues.extend(
            result
                .issues
                .into_iter()
                .map(|issue| format!("{}: {issue}", key.name)),
        );
        if !result.overloads.is_empty() {
            if let Some(symbol) = exports.get_mut(key) {
                symbol.overloads = result
                    .overloads
                    .into_iter()
                    .map(TypeShape::normalize)
                    .collect();
                symbol.type_signature = result.signature;
                if let Some(TypeShape::Satisfies { narrowed, .. }) = symbol.satisfies_type.as_mut()
                {
                    *narrowed = if symbol_complete {
                        symbol.overloads.first().cloned().map(Box::new)
                    } else {
                        None
                    };
                }
            }
        }
        for (index, location) in result.typeof_targets.into_iter().enumerate() {
            exports.record_typeof_target(key, index, location);
        }
        if symbol_complete {
            if let Some(contract) = result.commonjs_property {
                exports.record_property_contract(key.clone(), contract);
            }
        }
        if let Some(members) = result.commonjs_members.filter(|_| {
            symbol_complete
                && exports.get(key).is_some_and(|symbol| {
                    !symbol.is_reexport
                        || (key.name == "default"
                            && symbol.kind == ExportKind::Default
                            && symbol.local_name.is_none())
                })
        }) {
            if let Some(aggregate) = exports.get(key).cloned() {
                // A factory object has the same public require members as an
                // object literal. Keep the assignment anchor even for types
                // whose property declarations live in an external dependency.
                exports.remove(key);
                for CommonJsMember {
                    name,
                    symbol,
                    mut field,
                } in members
                {
                    if !symbol && name == "__esModule" {
                        continue;
                    }
                    field.shape = field.shape.normalize();
                    field.write_type = field.write_type.map(TypeShape::normalize);
                    let mut member_key = key.clone();
                    member_key.name = name.clone();
                    member_key.space = if symbol {
                        SymbolSpace::Symbol
                    } else {
                        SymbolSpace::Value
                    };
                    let mut member = aggregate.clone();
                    member.name = name;
                    member.kind = ExportKind::Named;
                    member.local_name = None;
                    member.decl_kinds =
                        vec![crate::languages::javascript::model::DeclarationKind::Variable];
                    member.type_signature = None;
                    let read_shape = if field.optional {
                        TypeShape::Union(vec![
                            field.shape.clone(),
                            TypeShape::Primitive(PrimitiveKind::Undefined),
                        ])
                        .normalize()
                    } else {
                        field.shape.clone()
                    };
                    member.overloads = vec![read_shape];
                    if exports
                        .get(&member_key)
                        .is_none_or(|existing| existing.is_reexport)
                    {
                        exports.insert(member_key.clone(), member);
                        exports.record_property_contract(member_key, field);
                    }
                }
            }
        }
    }
    issues
}

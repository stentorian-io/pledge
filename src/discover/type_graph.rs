//! Resolve source declarations reachable through the public type surface.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use tree_sitter::Node;

use super::{resolver, tree_query};
use crate::types::parser::parse_type_string;
use crate::types::shape::{FieldShape, PrimitiveKind, TypeShape};
use crate::types::ExportMap;

#[derive(Clone)]
struct Definition {
    shapes: Vec<TypeShape>,
    bases: Vec<TypeShape>,
    parameters: Vec<String>,
}

#[derive(Default)]
struct SourceTypes {
    definitions: HashMap<String, Definition>,
    imports: HashMap<String, (String, String)>,
    aliases: HashMap<String, String>,
    stars: Vec<String>,
}

#[derive(Default)]
struct TypeGraph {
    files: HashMap<PathBuf, SourceTypes>,
    active: HashSet<(PathBuf, String)>,
    remaining: usize,
    truncated: bool,
    missing_imports: HashSet<String>,
}

pub(crate) fn resolve_public_types(
    exports: &mut ExportMap,
    root: &Path,
    report_missing_imports: bool,
) -> Vec<String> {
    let mut graph = TypeGraph::default();
    let mut issues = Vec::new();
    for symbol in exports.values_mut() {
        let file = root.join(&symbol.file);
        let file = file.canonicalize().unwrap_or(file);
        let name = symbol.local_name.as_deref().unwrap_or(&symbol.name);
        graph.remaining = 4096;
        graph.truncated = false;
        let source_shapes = graph.named(&file, name, &[], 0);
        let needs_source = symbol.overloads.is_empty()
            || symbol
                .overloads
                .iter()
                .all(|s| matches!(s, TypeShape::Opaque(_)));
        if needs_source {
            if let Some(shapes) = source_shapes {
                symbol.overloads = shapes;
            }
        } else {
            let bindings: HashMap<_, _> = graph
                .files
                .get(&file)
                .and_then(|types| types.definitions.get(name))
                .map(|definition| {
                    definition
                        .parameters
                        .iter()
                        .map(|name| (name.clone(), TypeShape::TypeParam(name.clone())))
                        .collect()
                })
                .unwrap_or_default();
            symbol.overloads = symbol
                .overloads
                .iter()
                .map(|shape| graph.expand(shape.clone(), &file, &bindings, 0))
                .collect();
        }
        if graph.truncated {
            issues.push(format!(
                "public type expansion limit reached for `{}`",
                symbol.name
            ));
        }
    }
    if report_missing_imports {
        issues.extend(graph.missing_imports);
    }
    issues.sort();
    issues.dedup();
    issues
}

impl TypeGraph {
    fn load(&mut self, file: &Path) {
        if self.files.contains_key(file) {
            return;
        }
        let parsed = std::fs::read(file)
            .ok()
            .and_then(|source| {
                if crate::source_filter::classify(&source).is_some() {
                    return None;
                }
                let language = tree_query::language_for_file(file)?;
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_query::get_language(&language))
                    .ok()?;
                let tree = parser.parse(&source, None)?;
                let mut types = SourceTypes::default();
                collect_declarations(tree.root_node(), &source, &mut types);
                // Reuse export parsing for aliases and re-export chains.
                if let Ok(exports) = tree_query::parse_exports(&source, language) {
                    for export in exports {
                        let local = export.local_name.unwrap_or_else(|| export.name.clone());
                        if let Some(specifier) = export.source {
                            if export.kind == tree_query::RawExportKind::StarReexport {
                                types.stars.push(specifier);
                            } else {
                                types.imports.insert(export.name, (specifier, local));
                            }
                        } else if local != export.name {
                            types.aliases.insert(export.name, local);
                        }
                    }
                }
                Some(types)
            })
            .unwrap_or_default();
        self.files.insert(file.to_path_buf(), parsed);
    }

    fn target(&self, file: &Path, specifier: &str) -> Option<PathBuf> {
        let mut paths = resolver::resolve_module_path(file, specifier);
        if !specifier.starts_with('.') && !specifier.starts_with('/') && !specifier.starts_with('#')
        {
            let parts: Vec<_> = specifier.split('/').collect();
            let package_len = if specifier.starts_with('@') { 2 } else { 1 };
            if parts.len() >= package_len {
                let package = parts[..package_len].join("/");
                let subpath = if parts.len() == package_len {
                    ".".to_string()
                } else {
                    format!("./{}", parts[package_len..].join("/"))
                };
                for parent in file.ancestors().skip(1) {
                    let root = parent.join("node_modules").join(&package);
                    if root.join("package.json").is_file() {
                        if let Ok(entries) = super::entry_points::resolve_entry_points(&root) {
                            paths.extend(
                                entries
                                    .into_iter()
                                    .filter(|e| e.subpath == subpath)
                                    .map(|e| e.path),
                            );
                        }
                        if subpath != "." {
                            paths.extend(resolver::resolve_module_path(
                                &root.join("index.js"),
                                &subpath,
                            ));
                        }
                        break;
                    }
                }
            }
        }
        paths
            .iter()
            .find(|p| {
                p.is_file()
                    && matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("ts" | "tsx" | "mts" | "cts")
                    )
            })
            .or_else(|| paths.iter().find(|p| p.is_file()))
            .and_then(|path| path.canonicalize().ok())
    }

    fn named(
        &mut self,
        file: &Path,
        name: &str,
        args: &[TypeShape],
        depth: usize,
    ) -> Option<Vec<TypeShape>> {
        if depth >= 32 || self.remaining == 0 {
            self.truncated = true;
            return None;
        }
        self.remaining -= 1;
        let identity = (file.to_path_buf(), name.to_string());
        if !self.active.insert(identity.clone()) {
            return Some(vec![TypeShape::TypeParam("<recursive>".into())]);
        }
        let result = self.named_inner(file, name, args, depth);
        self.active.remove(&identity);
        result
    }

    fn named_inner(
        &mut self,
        file: &Path,
        name: &str,
        args: &[TypeShape],
        depth: usize,
    ) -> Option<Vec<TypeShape>> {
        self.load(file);
        let types = self.files.get(file)?;
        if let Some(local) = types.aliases.get(name).cloned() {
            return self.named(file, &local, args, depth + 1);
        }
        if let Some((specifier, imported)) = types.imports.get(name).cloned() {
            let Some(target) = self.target(file, &specifier) else {
                self.missing_imports.insert(format!(
                    "cannot resolve public type `{imported}` from `{specifier}`"
                ));
                return None;
            };
            return self.named(&target, &imported, args, depth + 1);
        }
        if let Some((namespace, member)) = name.split_once('.') {
            if let Some((specifier, imported)) = types.imports.get(namespace).cloned() {
                if imported == "*" {
                    let target = self.target(file, &specifier)?;
                    return self.named(&target, member, args, depth + 1);
                }
            }
        }
        let definition = types.definitions.get(name).cloned();
        let stars = types.stars.clone();
        if let Some(definition) = definition {
            let bindings: HashMap<_, _> = definition
                .parameters
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    (
                        name.clone(),
                        args.get(i)
                            .cloned()
                            .unwrap_or_else(|| TypeShape::TypeParam(name.clone())),
                    )
                })
                .collect();
            let mut inherited = BTreeMap::new();
            for base in definition.bases {
                if let TypeShape::Object(fields) = self.expand(base, file, &bindings, depth + 1) {
                    inherited.extend(fields);
                }
            }
            return Some(
                definition
                    .shapes
                    .into_iter()
                    .map(|shape| {
                        let mut shape = self.expand(shape, file, &bindings, depth + 1);
                        if let TypeShape::Object(fields) = &mut shape {
                            let own = std::mem::take(fields);
                            fields.extend(inherited.clone());
                            fields.extend(own);
                        }
                        shape
                    })
                    .collect(),
            );
        }
        for specifier in stars {
            if let Some(target) = self.target(file, &specifier) {
                if let Some(shapes) = self.named(&target, name, args, depth + 1) {
                    return Some(shapes);
                }
            }
        }
        None
    }

    fn expand(
        &mut self,
        shape: TypeShape,
        file: &Path,
        bindings: &HashMap<String, TypeShape>,
        depth: usize,
    ) -> TypeShape {
        if depth >= 32 || self.remaining == 0 {
            self.truncated = true;
            return shape;
        }
        self.remaining -= 1;
        let mut shape = match shape {
            TypeShape::Opaque(ref text) => {
                let parsed = parse_type_string(text);
                if parsed == shape {
                    return shape;
                }
                return self.expand(parsed, file, bindings, depth + 1);
            }
            TypeShape::TypeParam(ref name) => {
                if let Some(bound) = bindings.get(name) {
                    return bound.clone();
                }
                return self
                    .named(file, name, &[], depth + 1)
                    .and_then(|v| v.into_iter().next())
                    .unwrap_or(shape);
            }
            TypeShape::Generic { name, args } => {
                let args: Vec<_> = args
                    .into_iter()
                    .map(|arg| self.expand(arg, file, bindings, depth + 1))
                    .collect();
                if let Some(resolved) = self
                    .named(file, &name, &args, depth + 1)
                    .and_then(|v| v.into_iter().next())
                {
                    return resolved;
                }
                return TypeShape::Generic { name, args };
            }
            other => other,
        };
        shape.for_each_child_mut(&mut |child| {
            *child = self.expand(child.clone(), file, bindings, depth + 1);
        });
        shape
    }
}

fn text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn annotation(node: Node, source: &[u8], field: &str) -> Option<TypeShape> {
    node.child_by_field_name(field)
        .map(|n| parse_type_string(text(n, source).trim_start_matches(':').trim()))
}

fn collect_declarations(node: Node, source: &[u8], types: &mut SourceTypes) {
    if node.kind() == "import_statement" {
        if let Some(specifier) = node.child_by_field_name("source") {
            let specifier = text(specifier, source).trim_matches(['\'', '"']);
            collect_imports(node, source, specifier, types);
        }
        return;
    }
    let name = node.child_by_field_name("name").map(|n| text(n, source));
    let mut bases = Vec::new();
    let shape = match node.kind() {
        "type_alias_declaration" => node
            .child_by_field_name("value")
            .map(|n| parse_type_string(text(n, source))),
        "interface_declaration" | "class_declaration" | "abstract_class_declaration" => {
            collect_bases(node, source, &mut bases);
            node.child_by_field_name("body")
                .map(|body| object_shape(body, source))
        }
        "function_declaration" | "function_signature" => {
            // Inferred return types belong to the language server.
            node.child_by_field_name("return_type")
                .and_then(|_| tree_query::build_function_shape_from_node(node, source))
        }
        "variable_declarator" => annotation(node, source, "type"),
        _ => None,
    };
    if let (Some(name), Some(shape)) = (name, shape) {
        let parameters = tree_query::parse_type_params(source, node)
            .into_iter()
            .map(|p| p.name)
            .collect();
        let entry = types
            .definitions
            .entry(name.to_string())
            .or_insert_with(|| Definition {
                shapes: Vec::new(),
                bases: Vec::new(),
                parameters,
            });
        if let (Some(TypeShape::Object(existing)), TypeShape::Object(fields)) =
            (entry.shapes.first_mut(), &shape)
        {
            existing.extend(fields.clone());
        } else {
            entry.shapes.push(shape);
        }
        entry.bases.extend(bases);
        return;
    }
    if matches!(
        node.kind(),
        "program"
            | "export_statement"
            | "ambient_declaration"
            | "lexical_declaration"
            | "variable_declaration"
    ) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect_declarations(child, source, types);
        }
    }
}

fn collect_imports(node: Node, source: &[u8], specifier: &str, types: &mut SourceTypes) {
    if node.kind() == "import_specifier" {
        if let Some(name) = node.child_by_field_name("name") {
            let local = node.child_by_field_name("alias").unwrap_or(name);
            types.imports.insert(
                text(local, source).to_string(),
                (specifier.to_string(), text(name, source).to_string()),
            );
        }
        return;
    }
    if node.kind() == "namespace_import" {
        if let Some(name) = node.named_child(0) {
            types.imports.insert(
                text(name, source).to_string(),
                (specifier.to_string(), "*".into()),
            );
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if node.kind() == "import_clause" && child.kind() == "identifier" {
            types.imports.insert(
                text(child, source).to_string(),
                (specifier.to_string(), "default".into()),
            );
        } else {
            collect_imports(child, source, specifier, types);
        }
    }
}

fn collect_bases(node: Node, source: &[u8], bases: &mut Vec<TypeShape>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "extends_type_clause" => {
                let mut cursor = child.walk();
                for base in child.named_children(&mut cursor) {
                    bases.push(parse_type_string(text(base, source)));
                }
            }
            "extends_clause" => {
                bases.push(parse_type_string(
                    text(child, source).trim_start_matches("extends").trim(),
                ));
            }
            "class_heritage" => collect_bases(child, source, bases),
            _ => {}
        }
    }
}

fn object_shape(body: Node, source: &[u8]) -> TypeShape {
    let mut fields = BTreeMap::new();
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        let Some(name) = member.child_by_field_name("name") else {
            continue;
        };
        let name = text(name, source).trim_matches(['\'', '"']);
        let mut cursor = member.walk();
        let modifiers: Vec<_> = member
            .children(&mut cursor)
            .map(|n| text(n, source))
            .collect();
        if name.starts_with('#')
            || name == "constructor"
            || modifiers
                .iter()
                .any(|s| matches!(*s, "private" | "protected" | "static"))
        {
            continue;
        }
        let shape = if matches!(
            member.kind(),
            "method_signature" | "method_definition" | "abstract_method_signature"
        ) {
            tree_query::build_function_shape_from_node(member, source)
        } else {
            annotation(member, source, "type")
        };
        fields.insert(
            name.to_string(),
            FieldShape {
                optional: modifiers.contains(&"?"),
                readonly: modifiers.contains(&"readonly"),
                shape: shape.unwrap_or(TypeShape::Primitive(PrimitiveKind::Unknown)),
            },
        );
    }
    TypeShape::Object(fields)
}

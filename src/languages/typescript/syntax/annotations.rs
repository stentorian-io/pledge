use crate::languages::typescript::model as ts_model;

/// META-04: Extract TSDoc release tag from the block comment immediately
/// preceding a declaration node.
///
/// The block comment preceding an exported declaration lives as a sibling of
/// the `export_statement` wrapper at the program-level, not as a sibling of the
/// inner declaration node. This function walks up to the export_statement (or
/// ambient_declaration) level and checks its preceding named sibling for a
/// `/**... */` doc comment.
///
/// Returns the first recognized release tag found: @internal, @alpha,
/// @beta, @deprecated, or @public. Order defines priority.
pub fn extract_release_tag(
    decl_node: tree_sitter::Node,
    source: &[u8],
) -> Option<ts_model::ReleaseTag> {
    // The comment is a sibling of the wrapping export_statement/ambient_declaration,
    // not of the inner declaration node. Walk up to find the correct ancestor.
    let search_node = find_statement_ancestor(decl_node);

    // Now look at the preceding named sibling for a doc comment.
    let prev = search_node.prev_named_sibling()?;
    if prev.kind() != "comment" {
        return None;
    }
    let text = prev.utf8_text(source).ok()?;
    if !text.starts_with("/**") {
        return None;
    }
    // Simple tag scan — match on word-boundary @tag patterns.
    if text.contains("@internal") {
        return Some(ts_model::ReleaseTag::Internal);
    }
    if text.contains("@alpha") {
        return Some(ts_model::ReleaseTag::Alpha);
    }
    if text.contains("@beta") {
        return Some(ts_model::ReleaseTag::Beta);
    }
    if text.contains("@deprecated") {
        return Some(ts_model::ReleaseTag::Deprecated);
    }
    if text.contains("@public") {
        return Some(ts_model::ReleaseTag::Public);
    }
    None
}

/// CLS-10: Extract `DecoratorSpec` values from the `decorator`
/// children of a given AST node, and from preceding-sibling `decorator` nodes.
///
/// decorators are first-class
/// AST nodes in both tree-sitter-typescript 0.23.2 and tree-sitter-javascript
/// 0.25.0.
///
/// The caller passes the node that MIGHT carry decorators — this is
/// grammar-and-position-specific:
///  - For class-level decorators (TypeScript): pass the `class_declaration`,
///    `abstract_class_declaration`, or the `export_statement` that wraps the
///    class (decorators can land on either the export_statement or the inner
///    class node in TS).
///  - For class-level decorators (JavaScript): pass the `class_declaration`
///    or the `export_statement`.
///  - For method decorators: pass the `method_definition` node (JS grammar
///    stores decorators directly on it; TS grammar sometimes puts them on
///    the preceding `class_body` child — handle both by walking preceding
///    siblings as a fallback).
///  - For field decorators: pass the `public_field_definition` (TS) or
///    `field_definition` (JS).
///  - For parameter decorators: pass the `required_parameter` or
///    `optional_parameter` node.
///
/// The implementation scans BOTH direct children (JS pattern) AND preceding
/// siblings that are `decorator` nodes (TS pattern). This is robust across
/// both grammars.
pub fn extract_decorators(
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Vec<ts_model::DecoratorSpec> {
    let mut out: Vec<ts_model::DecoratorSpec> = Vec::new();

    // Pattern 1: direct children — e.g. class_declaration has `decorator`
    // as a direct child in both TS and JS grammars.
    for i in 0..node.child_count() as u32 {
        let child = match node.child(i) {
            Some(c) => c,
            None => continue,
        };
        if child.kind() == "decorator" {
            if let Some(spec) = decorator_node_to_spec(child, source) {
                out.push(spec);
            }
        }
    }

    // Pattern 2: preceding siblings — TS grammar sometimes emits
    // `decorator` as a sibling directly before a `method_definition` or
    // `public_field_definition` inside `class_body`.
    let mut cursor = node;
    while let Some(prev) = cursor.prev_named_sibling() {
        if prev.kind() != "decorator" {
            break;
        }
        if let Some(spec) = decorator_node_to_spec(prev, source) {
            // Prepend to preserve source order (we're walking backwards).
            out.insert(0, spec);
        }
        cursor = prev;
    }

    out
}

/// Convert a single `decorator` AST node to a DecoratorSpec.
pub(crate) fn decorator_node_to_spec(
    decorator_node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<ts_model::DecoratorSpec> {
    // decorator's meaningful child is the first named child (identifier /
    // call_expression / member_expression / parenthesized_expression).
    // Node-types.json confirms no `field` name; we walk children.
    let inner = (0..decorator_node.child_count() as u32)
        .filter_map(|i| decorator_node.child(i))
        .find(|c| c.is_named())?;

    match inner.kind() {
        "call_expression" => {
            // @Route('/users') — name from `function` field, args from `arguments` field.
            let fn_node = inner.child_by_field_name("function")?;
            let name = fn_node.utf8_text(source).ok()?.to_string();
            let args = inner
                .child_by_field_name("arguments")
                .and_then(|a| a.utf8_text(source).ok().map(|s| s.to_string()));
            Some(ts_model::DecoratorSpec { name, args })
        }
        "identifier" | "member_expression" => {
            // @Component or @ns.foo — name is the source text, no args.
            let name = inner.utf8_text(source).ok()?.to_string();
            Some(ts_model::DecoratorSpec { name, args: None })
        }
        "parenthesized_expression" => {
            // @(expr) — rare; best effort.
            let name = inner.utf8_text(source).ok()?.to_string();
            Some(ts_model::DecoratorSpec { name, args: None })
        }
        _ => None,
    }
}

/// Walk up the tree from a declaration node to the nearest program-level
/// statement wrapper (`export_statement` or `ambient_declaration`), returning
/// that node. If no such ancestor is found (the node itself is at program level),
/// return the original node unchanged.
pub(crate) fn find_statement_ancestor(mut node: tree_sitter::Node) -> tree_sitter::Node {
    loop {
        let parent = match node.parent() {
            Some(p) => p,
            None => return node, // reached root
        };
        let parent_kind = parent.kind();
        if parent_kind == "program" {
            // `node` is directly under the root — it is the statement we want
            return node;
        }
        if parent_kind == "export_statement" || parent_kind == "ambient_declaration" {
            // `export declare` nests ambient_declaration inside export_statement.
            // Its documentation precedes the outer wrapper, including inline docs.
            let mut statement = parent;
            while let Some(wrapper) = statement.parent().filter(|parent| {
                matches!(parent.kind(), "export_statement" | "ambient_declaration")
            }) {
                statement = wrapper;
            }
            return statement;
        }
        node = parent;
    }
}

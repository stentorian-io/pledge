use crate::languages::javascript::syntax::grammar;
use crate::languages::typescript::model as ts_model;
use tree_sitter::{QueryCursor, StreamingIterator};

/// TYPE-01: Extract `satisfies T` constraints from exported
/// const/let/var declarations at the top level of a TS/TSX source file.
///
/// Returns a map from the exported binding name to the RHS type-expression
/// source text. Resolver insertion consumes this map to post-populate
/// `ExportSymbol.satisfies_type` after parsing the RHS through
/// `parse_type_string`.
///
/// Tree-sitter AST path: `variable_declarator (name: identifier)
/// (value: satisfies_expression (_) @value (_) @satisfies_type)`.
///
/// Grammar gate: `satisfies_expression` does NOT exist in
/// tree-sitter-javascript 0.25.0. This function MUST be a no-op on JS
/// inputs — call sites guard via `SourceLanguage::TypeScript` /
/// `SourceLanguage::TypeScriptTsx`.
pub fn extract_satisfies_types(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    lang: grammar::SourceLanguage,
) -> std::collections::HashMap<String, String> {
    let mut out: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Grammar gate: only TS/TSX have `satisfies_expression`. Silently no-op on JS.
    if !matches!(
        lang,
        grammar::SourceLanguage::TypeScript | grammar::SourceLanguage::TypeScriptTsx
    ) {
        return out;
    }

    let language = grammar::get_language(&lang);
    let query_src = r#"
        (variable_declarator
          name: (identifier) @name
          value: (satisfies_expression
            (_) @value
            (_) @satisfies_type))
    "#;

    let query = match grammar::cached_query(&language, query_src) {
        Ok(q) => q,
        Err(_) => return out,
    };
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source);
    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut satisfies_text: Option<String> = None;
        for capture in m.captures {
            let capture_name = capture_names[capture.index as usize];
            let text = match capture.node.utf8_text(source) {
                Ok(t) => t.to_string(),
                Err(_) => continue,
            };
            match capture_name {
                "name" => name = Some(text),
                "satisfies_type" => satisfies_text = Some(text),
                _ => {}
            }
        }
        if let (Some(n), Some(t)) = (name, satisfies_text) {
            out.insert(n, t);
        }
    }
    out
}

/// INF-04: walk a subtree for `typeof import('./x').Y` and
/// `import('./x').Y` member-expression shapes. Returns all refs found in
/// the subtree — a single type position can contain multiple via unions,
/// intersections, generic args, etc.
///
/// AST shape (both TS `type_query > member_expression` and bare
/// `member_expression` — the `type_query` wrapper is transparent here):
/// ```text
/// member_expression
/// object: call_expression
/// function: import (anonymous keyword)
/// arguments: (string "./x")
/// property: property_identifier "Y"
/// ```
///
/// Paired with the resolver's post-parse populate pass (resolver.rs ~line
/// 485) to attach refs to the owning RawExport. The diff pipeline then
/// walks `ExportSymbol.typeof_import_refs` and resolves each via LSP.
pub fn extract_typeof_import_refs(
    root: tree_sitter::Node,
    source: &[u8],
) -> Vec<ts_model::TypeofImportRef> {
    use crate::languages::typescript::model::TypeofImportRef;
    let mut refs = Vec::new();
    let mut cursor = root.walk();
    walk_typeof_import_subtree(&mut cursor, &mut |node: tree_sitter::Node| {
        if node.kind() != "member_expression" {
            return;
        }
        let Some(object) = node.child_by_field_name("object") else {
            return;
        };
        if object.kind() != "call_expression" {
            return;
        }
        let Some(fn_node) = object.child_by_field_name("function") else {
            return;
        };
        if fn_node.kind() != "import" {
            return;
        }
        let Some(args) = object.child_by_field_name("arguments") else {
            return;
        };
        let Some(specifier) = first_string_arg_text(args, source) else {
            return;
        };
        let Some(property) = node.child_by_field_name("property") else {
            return;
        };
        if property.kind() != "property_identifier" {
            return;
        }
        let Ok(prop_text) = property.utf8_text(source) else {
            return;
        };
        let line = property.start_position().row as u32 + 1;
        let col = property.start_position().column as u32 + 1;
        refs.push(TypeofImportRef {
            specifier,
            property: prop_text.to_string(),
            property_line: line,
            property_col: col,
        });
    });
    refs
}

/// INF-04 helper: read the first `string` argument from a `call_expression.arguments`
/// node and return its unescaped text (stripping surrounding quotes).
pub(crate) fn first_string_arg_text(args: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut c = args.walk();
    for child in args.named_children(&mut c) {
        if child.kind() == "string" {
            // The string's child `string_fragment` carries the unquoted content.
            let mut sc = child.walk();
            for grand in child.named_children(&mut sc) {
                if grand.kind() == "string_fragment" {
                    return grand.utf8_text(source).ok().map(|s| s.to_string());
                }
            }
            // Fallback: return the raw literal with quotes stripped.
            if let Ok(raw) = child.utf8_text(source) {
                return Some(
                    raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
                        .to_string(),
                );
            }
        }
    }
    None
}

/// Visit every node under `cursor.node()` in pre-order.
pub(crate) fn walk_typeof_import_subtree(
    cursor: &mut tree_sitter::TreeCursor,
    visit: &mut dyn FnMut(tree_sitter::Node),
) {
    let node = cursor.node();
    visit(node);
    if cursor.goto_first_child() {
        loop {
            walk_typeof_import_subtree(cursor, visit);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

//! Select JS/TS grammars and cache compiled queries.

use std::path::Path;
use tree_sitter::Query;

pub(crate) fn cached_query(
    language: &tree_sitter::Language,
    source: &'static str,
) -> anyhow::Result<std::sync::Arc<Query>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    type Cache = HashMap<(tree_sitter::Language, &'static str), Result<Arc<Query>, String>>;
    static QUERIES: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut queries = QUERIES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    queries
        .entry((language.clone(), source))
        .or_insert_with(|| {
            Query::new(language, source)
                .map(Arc::new)
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

/// Which grammar should be used to parse a source file.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    TypeScript,
    /// INF-01: JSX-bearing TypeScript files (`.tsx`) — routed to
    /// `tree_sitter_typescript::LANGUAGE_TSX`. TSX is a strict superset of
    /// LANGUAGE_TYPESCRIPT so every TypeScript query compiles and matches
    /// identically on TSX source. `.tsx` was routed to
    /// LANGUAGE_TYPESCRIPT and produced ERROR nodes on any JSX body,
    /// suppressing export discovery.
    TypeScriptTsx,
    JavaScript,
}

/// Determine which grammar to use based on the file extension.
///
/// Returns `None` for unsupported extensions so callers can emit a warning and
/// skip the file rather than hard-failing.
#[allow(dead_code)]
pub fn language_for_file(path: &Path) -> Option<SourceLanguage> {
    let ext = path.extension()?.to_str()?;
    // Check for compound extensions like `.d.ts`, `.d.mts`, `.d.cts`.
    // Declaration files are pure type surface — no JSX — routed to standard
    // TypeScript grammar (NOT TypeScriptTsx).
    let file_name = path.file_name()?.to_str().unwrap_or("");
    if file_name.ends_with(".d.ts")
        || file_name.ends_with(".d.mts")
        || file_name.ends_with(".d.cts")
    {
        return Some(SourceLanguage::TypeScript);
    }
    match ext {
        // INF-01: `.tsx` routes to LANGUAGE_TSX — a strict superset
        // of LANGUAGE_TYPESCRIPT that handles JSX expression syntax.
        "tsx" => Some(SourceLanguage::TypeScriptTsx),
        "ts" | "mts" | "cts" => Some(SourceLanguage::TypeScript),
        "js" | "mjs" | "cjs" | "jsx" => Some(SourceLanguage::JavaScript),
        _ => None,
    }
}

/// Return the tree-sitter `Language` handle for the given `SourceLanguage`.
///
/// Uses the non-deprecated `LANGUAGE_TYPESCRIPT` / `LANGUAGE_TSX` / `LANGUAGE` constants.
///
/// made `pub(super)` so `comment_scan.rs` can call it.
pub(crate) fn get_language(lang: &SourceLanguage) -> tree_sitter::Language {
    match lang {
        SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    }
}

/// Parse supported module syntax and reject unaccounted-for recovery nodes.
/// Attribute clauses on re-exports are valid ECMAScript but absent from the
/// bundled grammars. Validate them through the equivalent import production,
/// then mask only the clause, preserving every source byte coordinate.
pub(crate) fn parse_source(
    source: &[u8],
    lang: &SourceLanguage,
) -> anyhow::Result<tree_sitter::Tree> {
    use anyhow::Context;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&get_language(lang))
        .context("failed to set tree-sitter language")?;
    let mut tree = parser
        .parse(source, None)
        .context("tree-sitter returned no source tree")?;
    for statement in tree
        .root_node()
        .named_children(&mut tree.root_node().walk())
    {
        // The bundled lexer can treat `export` before a line terminator as a
        // bare identifier without a recovery node. Such an expression is never
        // valid ECMAScript, so it cannot establish an empty public surface.
        if !tree.root_node().has_error()
            && statement.kind() == "expression_statement"
            && statement.named_child(0).is_some_and(|expression| {
                expression.kind() == "identifier"
                    && expression.utf8_text(source).ok() == Some("export")
            })
        {
            anyhow::bail!(
                "export keyword parsed as an identifier by the bundled grammar at {}:{}; public surface is incomplete",
                statement.start_position().row + 1,
                statement.start_position().column + 1
            );
        }
        if statement.kind() == "export_statement"
            && statement
                .child_by_field_name("declaration")
                .is_some_and(|declaration| declaration.kind() == "using_declaration")
        {
            anyhow::bail!(
                "using declarations cannot be exported at {}:{}; public surface is incomplete",
                statement.start_position().row + 1,
                statement.start_position().column + 1
            );
        }
    }
    if !tree.root_node().has_error() {
        return Ok(tree);
    }

    fn collect_module_sources<'tree>(
        node: tree_sitter::Node<'tree>,
        source: &[u8],
        sources: &mut Vec<tree_sitter::Node<'tree>>,
    ) {
        let mut statement = node;
        while statement
            .parent()
            .is_some_and(|parent| parent.kind() != "program")
        {
            statement = statement.parent().unwrap_or(statement);
        }
        let export_statement = statement
            .utf8_text(source)
            .unwrap_or("")
            .trim_start()
            .starts_with("export");
        let module_clause = node
            .children(&mut node.walk())
            .any(|child| matches!(child.kind(), "export_clause" | "namespace_export" | "from"));
        if matches!(node.kind(), "ERROR" | "export_statement") && export_statement && module_clause
        {
            if let Some(specifier) = node.child_by_field_name("source").or_else(|| {
                node.named_children(&mut node.walk())
                    .filter(|child| child.kind() == "string")
                    .last()
            }) {
                sources.push(specifier);
            }
        }
        for child in node.named_children(&mut node.walk()) {
            collect_module_sources(child, source, sources);
        }
    }
    let mut specifiers = Vec::new();
    collect_module_sources(tree.root_node(), source, &mut specifiers);
    let mut normalized = source.to_vec();
    let (mut changed, insertions) =
        normalize_recovered_syntax(tree.root_node(), source, lang, &mut normalized);

    for specifier in specifiers {
        let mut tail = specifier.end_byte();
        loop {
            while source.get(tail).is_some_and(u8::is_ascii_whitespace) {
                tail += 1;
            }
            if source.get(tail..tail + 2) == Some(b"/*") {
                let Some(end) = source[tail + 2..].windows(2).position(|pair| pair == b"*/") else {
                    break;
                };
                tail += end + 4;
            } else if source.get(tail..tail + 2) == Some(b"//") {
                tail = source[tail..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(source.len(), |end| tail + end);
            } else {
                break;
            }
        }
        if !source
            .get(tail..)
            .is_some_and(|tail| tail.starts_with(b"with"))
        {
            continue;
        }
        let mut import = b"import ".to_vec();
        import.extend_from_slice(&source[specifier.byte_range()]);
        import.push(b' ');
        let prefix_len = import.len();
        import.extend_from_slice(&source[tail..]);
        let Some(import_tree) = parser.parse(&import, None) else {
            continue;
        };
        let Some(statement) = import_tree.root_node().named_child(0) else {
            continue;
        };
        if statement.kind() != "import_statement" || statement.has_error() {
            continue;
        }
        let attribute = statement
            .named_children(&mut statement.walk())
            .find(|node| node.kind() == "import_attribute");
        let Some(attribute) = attribute else { continue };
        let Some(attributes) = attribute
            .named_children(&mut attribute.walk())
            .find(|node| node.kind() == "object")
        else {
            continue;
        };
        let mut keys = std::collections::HashSet::new();
        let valid_attributes = attributes
            .named_children(&mut attributes.walk())
            .all(|entry| {
                if entry.kind() == "comment" {
                    return true;
                }
                if entry.kind() != "pair" {
                    return false;
                }
                let Some(key) = entry
                    .child_by_field_name("key")
                    .filter(|key| matches!(key.kind(), "property_identifier" | "string"))
                else {
                    return false;
                };
                let Some(name) = super::property_key::source_name(key, &import) else {
                    return false;
                };
                keys.insert(name)
                    && entry
                        .child_by_field_name("value")
                        .is_some_and(|value| value.kind() == "string")
            });
        if !valid_attributes {
            continue;
        }

        let start = tail + attribute.start_byte() - prefix_len;
        let end = tail + attribute.end_byte() - prefix_len;
        for byte in &mut normalized[start..end] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        changed = true;
    }
    let mut inserted = Vec::new();
    for (count, position) in insertions.into_iter().enumerate() {
        let offset = position + count * 2;
        let row = normalized[..offset]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        let column = normalized[..offset]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(offset, |line| offset - line - 1);
        normalized.splice(offset..offset, b" _".iter().copied());
        inserted.push((offset, tree_sitter::Point::new(row, column)));
        changed = true;
    }
    if changed {
        tree = parser
            .parse(&normalized, None)
            .context("tree-sitter returned no normalized module tree")?;
        // Anonymous declaration signatures need a synthetic name and separator. Remove
        // them from the tree's coordinates so every real token still points into
        // the original source. The empty name belongs only to a default export.
        for (offset, point) in inserted.into_iter().rev() {
            tree.edit(&tree_sitter::InputEdit {
                start_byte: offset,
                old_end_byte: offset + 2,
                new_end_byte: offset,
                start_position: point,
                old_end_position: tree_sitter::Point::new(point.row, point.column + 2),
                new_end_position: point,
            });
        }
    }

    fn unexpected_error<'tree>(
        node: tree_sitter::Node<'tree>,
        source: &[u8],
        lang: &SourceLanguage,
    ) -> Option<tree_sitter::Node<'tree>> {
        if node.is_error() || node.is_missing() {
            // TypeScript 0.23 represents the valid `export type *` modifier as
            // ERROR(type). Export extraction deliberately retains that token.
            let type_star = !matches!(lang, SourceLanguage::JavaScript)
                && node.is_error()
                && node.utf8_text(source).ok() == Some("type")
                && node.parent().is_some_and(|parent| {
                    parent.kind() == "export_statement"
                        && parent
                            .utf8_text(source)
                            .unwrap_or("")
                            .trim_start()
                            .starts_with("export type *")
                });
            if !type_star {
                return Some(node);
            }
        }
        for child in node.children(&mut node.walk()) {
            if let Some(error) = unexpected_error(child, source, lang) {
                return Some(error);
            }
        }
        None
    }
    if let Some(error) = unexpected_error(tree.root_node(), source, lang) {
        anyhow::bail!(
            "unsupported or invalid syntax at {}:{}; public surface is incomplete",
            error.start_position().row + 1,
            error.start_position().column + 1
        );
    }
    Ok(tree)
}

fn normalize_recovered_syntax(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    language: &SourceLanguage,
    normalized: &mut [u8],
) -> (bool, Vec<usize>) {
    let mut changed = false;
    let mut insertions = Vec::new();
    let mut nodes = vec![root];
    while let Some(node) = nodes.pop() {
        nodes.extend(node.named_children(&mut node.walk()));
        if matches!(language, SourceLanguage::JavaScript) {
            if node.is_error()
                && node.parent().is_some_and(|parent| {
                    parent.kind() == "export_clause"
                        || (parent.kind() == "export_statement"
                            && parent
                                .children(&mut parent.walk())
                                .any(|token| token.kind() == "*"))
                })
            {
                let tokens: Vec<_> = node
                    .children(&mut node.walk())
                    .filter(|token| token.kind() != "comment")
                    .collect();
                if let [as_token, alias] = tokens.as_slice() {
                    if as_token.utf8_text(source).ok() == Some("as")
                        && matches!(
                            alias.utf8_text(source).unwrap_or(""),
                            "await"
                                | "break"
                                | "case"
                                | "catch"
                                | "class"
                                | "const"
                                | "continue"
                                | "debugger"
                                | "default"
                                | "delete"
                                | "do"
                                | "else"
                                | "enum"
                                | "export"
                                | "extends"
                                | "false"
                                | "finally"
                                | "for"
                                | "function"
                                | "if"
                                | "import"
                                | "in"
                                | "instanceof"
                                | "let"
                                | "new"
                                | "null"
                                | "return"
                                | "super"
                                | "switch"
                                | "this"
                                | "throw"
                                | "true"
                                | "try"
                                | "typeof"
                                | "var"
                                | "void"
                                | "while"
                                | "with"
                                | "yield"
                                | "implements"
                                | "interface"
                                | "package"
                                | "private"
                                | "protected"
                                | "public"
                                | "static"
                        )
                    {
                        normalized[alias.start_byte()..alias.end_byte()].fill(b'_');
                        changed = true;
                    }
                }
            }
            continue;
        }
        if node.kind() == "type_parameters"
            && node.has_error()
            && node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "interface_declaration"
                        | "type_alias_declaration"
                        | "class_declaration"
                        | "abstract_class_declaration"
                )
            })
        {
            for parameter in node.named_children(&mut node.walk()) {
                if !matches!(parameter.kind(), "type_parameter" | "ERROR") {
                    continue;
                }
                let mut start = parameter
                    .child_by_field_name("name")
                    .unwrap_or(parameter)
                    .start_byte();
                let mut modifiers = Vec::new();
                loop {
                    let tail = &source[start..node.end_byte()];
                    let width = if tail.starts_with(b"out") {
                        3
                    } else if tail.starts_with(b"in") {
                        2
                    } else {
                        break;
                    };
                    let mut next = start + width;
                    loop {
                        while source.get(next).is_some_and(u8::is_ascii_whitespace) {
                            next += 1;
                        }
                        if source.get(next..next + 2) == Some(b"/*") {
                            let Some(end) = source[next + 2..node.end_byte()]
                                .windows(2)
                                .position(|pair| pair == b"*/")
                            else {
                                break;
                            };
                            next += end + 4;
                        } else if source.get(next..next + 2) == Some(b"//") {
                            next = source[next..node.end_byte()]
                                .iter()
                                .position(|byte| *byte == b'\n')
                                .map_or(node.end_byte(), |end| next + end);
                        } else {
                            break;
                        }
                    }
                    if next == start + width || next >= node.end_byte() {
                        break;
                    }
                    let next_word = source[next..node.end_byte()]
                        .split(|byte| {
                            !byte.is_ascii_alphanumeric() && !matches!(*byte, b'_' | b'$')
                        })
                        .next()
                        .unwrap_or_default();
                    if next_word.is_empty() || next_word == b"extends" {
                        break;
                    }
                    modifiers.push((start, width));
                    start = next;
                }
                let words: Vec<_> = modifiers
                    .iter()
                    .map(|(start, width)| &source[*start..*start + *width])
                    .collect();
                if matches!(words.as_slice(), [b"in"] | [b"out"] | [b"in", b"out"]) {
                    for (start, width) in modifiers {
                        normalized[start..start + width].fill(b' ');
                    }
                    changed = true;
                }
            }
        }
        if node.is_error()
            && node.parent().is_some_and(|parent| {
                parent.kind() == "expression_statement"
                    && parent
                        .utf8_text(source)
                        .unwrap_or("")
                        .trim_start()
                        .starts_with("export default function")
            })
            && node
                .utf8_text(source)
                .unwrap_or("")
                .trim_start()
                .starts_with("default function")
        {
            if let Some(parameters) = node
                .child_by_field_name("type_parameters")
                .or_else(|| {
                    node.named_children(&mut node.walk())
                        .find(|child| child.kind() == "type_parameters")
                })
                .or_else(|| node.child_by_field_name("parameters"))
                .or_else(|| {
                    node.named_children(&mut node.walk())
                        .find(|child| child.kind() == "formal_parameters")
                })
            {
                let prefix = &source[node.start_byte()..parameters.start_byte()];
                if prefix
                    .split(|byte| byte.is_ascii_whitespace())
                    .filter(|word| !word.is_empty())
                    .eq([b"default".as_slice(), b"function".as_slice()])
                {
                    insertions.push(parameters.start_byte());
                }
            }
        }
    }
    insertions.sort_unstable();
    insertions.dedup();
    (changed, insertions)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::languages::typescript::model::Variance;
    use crate::languages::typescript::syntax::generics::parse_type_params;

    #[test]
    fn line_terminators_cannot_hide_export_declarations() {
        for separator in ["\n", "\r\n", "\u{2028}", "\u{2029}"] {
            let source = format!("export{separator}const api=1;");
            let error = parse_source(source.as_bytes(), &SourceLanguage::JavaScript).unwrap_err();
            assert!(error.to_string().contains("public surface is incomplete"));
        }
        for source in [
            "export const api='\u{2028}\u{2029}';",
            "export const api=`\u{2028}\u{2029}`;",
            "const api={export:1};export {api};",
        ] {
            let tree = parse_source(source.as_bytes(), &SourceLanguage::JavaScript).unwrap();
            assert!(!tree.root_node().has_error(), "{source}");
        }
    }

    #[test]
    fn anonymous_default_recovery_preserves_multiline_import_context() {
        let source = b"import type * as errors from '../core/errors.js';\nexport default function (): {localeError: errors.$ZodErrorMap;};";
        let tree = parse_source(source, &SourceLanguage::TypeScript).unwrap();
        assert!(!tree.root_node().has_error());
        assert!(tree
            .root_node()
            .named_children(&mut tree.root_node().walk())
            .any(|statement| {
                statement.kind() == "export_statement"
                    && statement.child_by_field_name("declaration").is_some()
            }));
    }

    #[test]
    fn variance_recovery_preserves_parameter_identity_constraints_defaults_and_positions() {
        for source in [
            "export interface Box<out T extends string = 'a', in U = never> {}",
            "export interface Box<out /* marker */ T extends string = 'a', in /* marker */ out U = never> {}",
        ] {
            let tree = parse_source(source.as_bytes(), &SourceLanguage::TypeScript).unwrap();
            let declaration = tree.root_node().named_child(0).unwrap().child_by_field_name("declaration").unwrap();
            let params = parse_type_params(source.as_bytes(), declaration);
            assert_eq!(params.len(), 2, "{params:?}");
            assert_eq!(params[0].name, "T");
            assert_eq!(params[0].constraint_text.as_deref(), Some("string"));
            assert_eq!(params[0].default_text.as_deref(), Some("'a'"));
            assert_eq!(params[0].variance, Some(Variance::Out));
            assert_eq!(params[1].name, "U");
            assert_eq!(params[1].default_text.as_deref(), Some("never"));
            assert_eq!(params[1].variance, Some(if source.contains("out U") {Variance::InOut} else {Variance::In}));
            let name = declaration.child_by_field_name("name").unwrap();
            assert_eq!(name.utf8_text(source.as_bytes()).unwrap(), "Box");
        }
    }

    #[test]
    fn anonymous_signature_recovery_keeps_parameter_and_return_types_at_original_offsets() {
        use crate::languages::javascript::syntax::functions::build_function_shape_from_node;
        use crate::languages::typescript::types::shape::TypeShape;
        let source =
            b"export default function (value: string): number;\nexport declare const next:boolean;";
        let tree = parse_source(source, &SourceLanguage::TypeScript).unwrap();
        let declaration = tree
            .root_node()
            .named_child(0)
            .unwrap()
            .child_by_field_name("declaration")
            .unwrap();
        assert_eq!(
            declaration
                .child_by_field_name("parameters")
                .unwrap()
                .utf8_text(source)
                .unwrap(),
            "(value: string)"
        );
        assert_eq!(
            declaration
                .child_by_field_name("return_type")
                .unwrap()
                .utf8_text(source)
                .unwrap(),
            ": number"
        );
        let TypeShape::Function {
            params,
            return_type,
            ..
        } = build_function_shape_from_node(declaration, source).unwrap()
        else {
            panic!("expected signature")
        };
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "value");
        assert_eq!(params[0].shape, TypeShape::Opaque("string".into()));
        assert_eq!(*return_type, TypeShape::Opaque("number".into()));
    }
}

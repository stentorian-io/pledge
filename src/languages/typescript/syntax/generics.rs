use crate::languages::typescript::model as ts_model;
use crate::languages::typescript::model::TypeParam;

/// Extract generic type parameters from ANY declaration node that has a
/// `type_parameters` field (class, function, interface, type_alias).
///
/// applies to ALL exported constructs.
/// Returns empty Vec if no `type_parameters` field.
pub fn parse_type_params(source: &[u8], decl_node: tree_sitter::Node) -> Vec<TypeParam> {
    let mut params = Vec::new();

    let tp_node = match decl_node.child_by_field_name("type_parameters") {
        Some(n) => n,
        None => return params,
    };

    // TLF-05: tree-sitter-typescript 0.23.2 does not fully support variance modifiers
    // (`in T`, `out T`, `in out T`). When encountered, it parses them as:
    // type_parameter { name: "in" | "out" } followed by ERROR { identifier: "T" }
    // We collect all children first to allow lookahead for this pattern.
    let all_children: Vec<tree_sitter::Node> = {
        let mut wc = tp_node.walk();
        tp_node.children(&mut wc).collect()
    };

    let mut i = 0;
    while i < all_children.len() {
        let child = all_children[i];
        if child.kind() == "type_parameter" {
            let raw_name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string();

            // TLF-05: Detect variance modifiers by checking if the type_parameter's
            // apparent "name" is actually an `in`/`out` keyword, indicating that
            // tree-sitter mis-parsed a variance-annotated type parameter.
            let (name, variance) = {
                let name_is_variance_kw = matches!(raw_name.as_str(), "in" | "out");
                let recovered_name = all_children
                    .get(i + 1)
                    .is_some_and(|node| node.kind() == "ERROR");
                if name_is_variance_kw && recovered_name {
                    // Look for the actual name in the next ERROR sibling
                    let saw_in = raw_name == "in";
                    let mut saw_out = raw_name == "out";
                    let mut real_name = String::new();

                    // The ERROR node immediately following this type_parameter contains
                    // the real type parameter name (and possibly "out" for `in out T`).
                    let mut j = i + 1;
                    while j < all_children.len() && all_children[j].kind() == "ERROR" {
                        let err = all_children[j];
                        let mut ec = err.walk();
                        for err_child in err.children(&mut ec) {
                            let t = err_child.utf8_text(source).unwrap_or("");
                            match t {
                                "out" => saw_out = true,
                                s if !s.is_empty() && s != "in" && real_name.is_empty() => {
                                    real_name = s.to_string();
                                }
                                _ => {}
                            }
                        }
                        j += 1;
                    }
                    // Advance i past the consumed ERROR nodes
                    i = j - 1; // will be incremented to j at end of loop body

                    let variance = match (saw_in, saw_out) {
                        (true, true) => Some(ts_model::Variance::InOut),
                        (true, false) => Some(ts_model::Variance::In),
                        (false, true) => Some(ts_model::Variance::Out),
                        _ => None,
                    };
                    (real_name, variance)
                } else {
                    let start = all_children[..i]
                        .iter()
                        .rev()
                        .find(|node| matches!(node.kind(), "<" | ","))
                        .map_or(tp_node.start_byte() + 1, |node| node.end_byte());
                    let end = child
                        .child_by_field_name("name")
                        .map_or(child.start_byte(), |node| node.start_byte());
                    let prefix = source.get(start..end).unwrap_or_default();
                    (raw_name, variance_prefix(prefix))
                }
            };

            let constraint_text = child
                .child_by_field_name("constraint")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| {
                    // Strip the leading "extends " keyword if present
                    let s = s.trim();
                    s.strip_prefix("extends ").unwrap_or(s).to_string()
                });

            let default_text = child
                .child_by_field_name("value")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| {
                    let s = s.trim();
                    // Strip leading "= " prefix if present (tree-sitter may include it)
                    s.strip_prefix("= ").unwrap_or(s).trim().to_string()
                });

            // Check for `const` modifier on type parameter.
            // In tree-sitter-typescript, `<const T>` has the "const" keyword as an
            // anonymous child before the name node in the type_parameter node.
            // Walk children to detect it — `const` appears as an unnamed node before "name".
            let is_const = {
                let mut found_const = false;
                let mut tp_cursor = child.walk();
                for tp_child in child.children(&mut tp_cursor) {
                    // Stop once we hit the name node (const can only precede the name)
                    if tp_child.is_named() {
                        break;
                    }
                    if tp_child.utf8_text(source).ok() == Some("const") {
                        found_const = true;
                        break;
                    }
                }
                found_const
            };

            if !name.is_empty() {
                params.push(TypeParam {
                    name,
                    constraint_text,
                    default_text,
                    constraint_shape: None, // LSP fills in later for GEN-04
                    is_const,
                    variance,
                });
            }
        }
        i += 1;
    }

    params
}

/// Strict grammar recovery masks only validated modifiers. Read their original
/// bytes so the normalized tree keeps both correct positions and variance.
fn variance_prefix(prefix: &[u8]) -> Option<ts_model::Variance> {
    let mut remaining = std::str::from_utf8(prefix).ok()?.trim_start();
    let mut modifiers = Vec::new();
    while !remaining.is_empty() {
        if let Some(comment) = remaining.strip_prefix("/*") {
            remaining = comment.get(comment.find("*/")? + 2..)?.trim_start();
        } else if let Some(comment) = remaining.strip_prefix("//") {
            remaining = comment.get(comment.find(['\n', '\r'])?..)?.trim_start();
        } else {
            let end = remaining
                .find(|character: char| !character.is_ascii_alphabetic())
                .unwrap_or(remaining.len());
            if end == 0 {
                return None;
            }
            let token = &remaining[..end];
            if token != "const" {
                modifiers.push(token);
            }
            remaining = remaining[end..].trim_start();
        }
    }
    match modifiers.as_slice() {
        ["in"] => Some(ts_model::Variance::In),
        ["out"] => Some(ts_model::Variance::Out),
        ["in", "out"] => Some(ts_model::Variance::InOut),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::javascript::syntax::grammar::{parse_source, SourceLanguage};

    #[test]
    fn normalized_variance_keeps_const_constraint_default_and_original_names() {
        let source =
            b"export class Box<const out T extends string = 'a', in /* marker */ U = never> {}";
        let tree = parse_source(source, &SourceLanguage::TypeScript).unwrap();
        let declaration = tree
            .root_node()
            .named_child(0)
            .unwrap()
            .child_by_field_name("declaration")
            .unwrap();
        let parameters = parse_type_params(source, declaration);
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name, "T");
        assert!(parameters[0].is_const);
        assert_eq!(parameters[0].variance, Some(ts_model::Variance::Out));
        assert_eq!(parameters[0].constraint_text.as_deref(), Some("string"));
        assert_eq!(parameters[0].default_text.as_deref(), Some("'a'"));
        assert_eq!(parameters[1].name, "U");
        assert!(!parameters[1].is_const);
        assert_eq!(parameters[1].variance, Some(ts_model::Variance::In));
        assert_eq!(parameters[1].default_text.as_deref(), Some("never"));
    }

    #[test]
    fn contextual_out_parameter_name_is_not_removed_as_a_modifier() {
        let source = b"export type Box<out> = {value: out};";
        let tree = parse_source(source, &SourceLanguage::TypeScript).unwrap();
        let declaration = tree
            .root_node()
            .named_child(0)
            .unwrap()
            .child_by_field_name("declaration")
            .unwrap();
        let parameters = parse_type_params(source, declaration);
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "out");
        assert_eq!(parameters[0].variance, None);
    }
}

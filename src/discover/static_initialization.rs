//! Walk static initialization in execution order without evaluating package code.
use tree_sitter::Node;

use super::property_key;

fn truthiness(node: Node<'_>, source: &[u8], depth: usize) -> Option<bool> {
    if depth > 32 {
        return None;
    }
    match node.kind() {
        "parenthesized_expression" | "as_expression" | "satisfies_expression" => {
            truthiness(node.named_child(0)?, source, depth + 1)
        }
        "identifier" => truthiness(property_key::initializer(node, source)?, source, depth + 1),
        "true" | "object" | "array" => Some(true),
        "false" | "null" => Some(false),
        "number" => node
            .utf8_text(source)
            .ok()?
            .parse::<f64>()
            .ok()
            .map(|number| number != 0.0 && !number.is_nan()),
        "string" => match property_key::resolve(node, source)? {
            property_key::PropertyKey::String(value) => Some(!value.is_empty()),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn assignments<'tree>(body: Node<'tree>, source: &[u8]) -> Vec<(Node<'tree>, bool)> {
    let mut result = Vec::new();
    let mut pending = vec![(body, false)];
    while let Some((node, conditional)) = pending.pop() {
        match node.kind() {
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "generator_function_declaration"
            | "generator_function"
            | "class_declaration"
            | "class" => continue,
            "assignment_expression" => {
                result.push((node, conditional));
                continue;
            }
            "if_statement" => {
                let known = node
                    .child_by_field_name("condition")
                    .and_then(|condition| truthiness(condition, source, 0));
                if known != Some(true) {
                    if let Some(branch) = node.child_by_field_name("alternative") {
                        pending.push((branch, conditional || known.is_none()));
                    }
                }
                if known != Some(false) {
                    if let Some(branch) = node.child_by_field_name("consequence") {
                        pending.push((branch, conditional || known.is_none()));
                    }
                }
                continue;
            }
            _ => {}
        }
        let conditional = conditional
            || !matches!(
                node.kind(),
                "statement_block"
                    | "expression_statement"
                    | "else_clause"
                    | "parenthesized_expression"
            );
        let children: Vec<_> = node.named_children(&mut node.walk()).collect();
        pending.extend(children.into_iter().rev().map(|child| (child, conditional)));
    }
    result
}

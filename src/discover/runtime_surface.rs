//! Distinguish ordinary reflective construction from runtime-dispatched surfaces.

use tree_sitter::{Node, Point};

use super::property_key;

fn text<'a>(node: Node<'_>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn unwrap(mut node: Node<'_>) -> Node<'_> {
    while matches!(
        node.kind(),
        "parenthesized_expression" | "as_expression" | "satisfies_expression"
    ) {
        let Some(inner) = node.named_child(0) else {
            break;
        };
        node = inner;
    }
    node
}

pub(super) fn dispatch_at(root: Node<'_>, source: &[u8], line: u32, col: u32) -> Option<bool> {
    let position = Point::new(
        line.saturating_sub(1) as usize,
        col.saturating_sub(1) as usize,
    );
    let mut node = root;
    let mut expression = None;
    loop {
        match node.kind() {
            "variable_declarator" => expression = node.child_by_field_name("value"),
            "assignment_expression" => expression = node.child_by_field_name("right"),
            "export_statement" => {
                if let Some(value) = node.child_by_field_name("value") {
                    expression = Some(value);
                }
            }
            _ => {}
        }
        let next = node
            .named_children(&mut node.walk())
            .find(|child| child.start_position() <= position && position < child.end_position());
        let Some(next) = next else { break };
        node = next;
    }
    classify(expression?, source, 0)
}

fn classify(expression: Node<'_>, source: &[u8], depth: usize) -> Option<bool> {
    if depth >= 32 {
        return Some(true);
    }
    let expression = unwrap(expression);
    if expression.kind() == "identifier" {
        return classify(
            property_key::initializer(expression, source)?,
            source,
            depth + 1,
        );
    }
    let callable = expression
        .child_by_field_name("constructor")
        .or_else(|| expression.child_by_field_name("function"))?;
    let name: String = text(callable, source)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let arguments = expression.child_by_field_name("arguments")?;
    let arguments: Vec<_> = arguments
        .named_children(&mut arguments.walk())
        .filter(|node| node.kind() != "comment")
        .collect();
    if name == "Proxy" && expression.kind() == "new_expression" {
        if !property_key::is_unbound(callable, "Proxy", source) {
            return Some(false);
        }
        let handler = unwrap(*arguments.get(1)?);
        // A fresh empty handler has no traps and cannot subsequently be mutated
        // through an alias. Direct Reflect trap functions also forward unchanged.
        let transparent = handler.kind() == "object"
            && handler.named_children(&mut handler.walk()).all(|member| {
                if member.kind() == "comment" {
                    return true;
                }
                let Some(key) = member.child_by_field_name("key") else {
                    return false;
                };
                let Some(value) = member.child_by_field_name("value") else {
                    return false;
                };
                let trap = text(key, source).trim_matches(['\'', '"']);
                let forwards = matches!(
                    trap,
                    "apply"
                        | "construct"
                        | "defineProperty"
                        | "deleteProperty"
                        | "get"
                        | "getOwnPropertyDescriptor"
                        | "getPrototypeOf"
                        | "has"
                        | "isExtensible"
                        | "ownKeys"
                        | "preventExtensions"
                        | "set"
                        | "setPrototypeOf"
                );
                forwards
                    && text(value, source) == format!("Reflect.{trap}")
                    && property_key::is_unbound(value, "Reflect", source)
            });
        return Some(!transparent);
    }
    if name == "Reflect.construct" {
        if !property_key::is_unbound(callable, "Reflect", source) {
            return Some(false);
        }
        let ordinary_prototype = arguments.len() == 2
            || arguments.len() == 3 && text(arguments[0], source) == text(arguments[2], source);
        return Some(!ordinary_prototype);
    }
    if name == "Reflect.get" {
        return Some(true);
    }
    None
}

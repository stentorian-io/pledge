//! Literal CommonJS forwarding, retaining the environment that selects a module.

use std::collections::HashMap;
use tree_sitter::Node;

#[derive(Debug, Clone)]
pub(super) struct Forward {
    pub specifier: String,
    pub condition: Option<String>,
    pub line: u32,
    pub uncertain: bool,
    pub shadowed: Vec<String>,
}

pub(super) fn forwards(root: Node<'_>, source: &[u8]) -> Vec<Forward> {
    let mut forwards = Vec::new();
    walk(root, source, &mut HashMap::new(), &[], false, &mut forwards);
    forwards
}

fn text<'a>(node: Node<'_>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn unwrap(mut node: Node<'_>) -> Node<'_> {
    while node.kind() == "parenthesized_expression" {
        let Some(inner) = node.named_child(0) else {
            break;
        };
        node = inner;
    }
    node
}

fn require_target(
    node: Node<'_>,
    source: &[u8],
    bindings: &HashMap<String, String>,
) -> Option<String> {
    let node = unwrap(node);
    if node.kind() == "identifier" {
        return bindings.get(text(node, source)).cloned();
    }
    if node.kind() != "call_expression"
        || text(node.child_by_field_name("function")?, source) != "require"
    {
        return None;
    }
    let argument = node.child_by_field_name("arguments")?.named_child(0)?;
    (argument.kind() == "string")
        .then(|| text(argument, source).trim_matches(['\'', '"']).to_string())
}

fn environment_test(node: Node<'_>, source: &[u8]) -> Option<(String, String)> {
    let node = unwrap(node);
    if node.kind() != "binary_expression" {
        return None;
    }
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;
    let operator = text(node.child_by_field_name("operator")?, source);
    if !matches!(operator, "==" | "===" | "!=" | "!==") {
        return None;
    }
    let (environment, literal) = if right.kind() == "string" {
        (left, right)
    } else if left.kind() == "string" {
        (right, left)
    } else {
        return None;
    };
    let compact: String = text(environment, source)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let name = compact.strip_prefix("process.env.")?;
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    let value = text(literal, source).trim_matches(['\'', '"']);
    let equal = format!("{name}={value}");
    let unequal = format!("{name}!={value}");
    Some(if operator.starts_with('!') {
        (unequal, equal)
    } else {
        (equal, unequal)
    })
}

fn walk(
    node: Node<'_>,
    source: &[u8],
    bindings: &mut HashMap<String, String>,
    conditions: &[String],
    uncertain: bool,
    forwards: &mut Vec<Forward>,
) {
    if matches!(
        node.kind(),
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
    ) {
        return;
    }
    if node.kind() == "if_statement" {
        let Some(test) = node.child_by_field_name("condition") else {
            return;
        };
        let recognized = environment_test(test, source);
        let unsupported = recognized.is_none();
        let (positive, negative) = recognized.unwrap_or_else(|| {
            (
                format!("if({})", text(test, source)),
                format!("unless({})", text(test, source)),
            )
        });
        for (field, predicate) in [("consequence", positive), ("alternative", negative)] {
            if let Some(branch) = node.child_by_field_name(field) {
                let mut branch_conditions = conditions.to_vec();
                branch_conditions.push(predicate);
                walk(
                    branch,
                    source,
                    &mut bindings.clone(),
                    &branch_conditions,
                    uncertain || unsupported,
                    forwards,
                );
            }
        }
        return;
    }
    if node.kind() == "variable_declarator" {
        if let (Some(name), Some(value)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        ) {
            if name.kind() == "identifier" {
                if let Some(target) = require_target(value, source, bindings) {
                    bindings.insert(text(name, source).into(), target);
                }
            }
        }
    }
    if node.kind() == "assignment_expression" {
        if let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) {
            let target: String = text(left, source)
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            if target == "module.exports" {
                let mut targets = Vec::new();
                if let Some(target) = require_target(right, source, bindings) {
                    targets.push((target, Vec::new()));
                }
                if right.kind() == "object" {
                    let children: Vec<_> = right.named_children(&mut right.walk()).collect();
                    for (index, child) in children
                        .iter()
                        .enumerate()
                        .filter(|(_, child)| child.kind() == "spread_element")
                    {
                        if let Some(target) = child
                            .named_child(0)
                            .and_then(|child| require_target(child, source, bindings))
                        {
                            let shadowed = children[index + 1..]
                                .iter()
                                .filter_map(|node| {
                                    if node.kind() == "pair" {
                                        node.child_by_field_name("key")
                                    } else if node.kind() == "shorthand_property_identifier" {
                                        Some(*node)
                                    } else {
                                        None
                                    }
                                })
                                .map(|node| {
                                    text(node, source).trim_matches(['\'', '"']).to_string()
                                })
                                .collect();
                            targets.push((target, shadowed));
                        }
                    }
                }
                for (specifier, shadowed) in targets {
                    forwards.push(Forward {
                        specifier,
                        condition: (!conditions.is_empty()).then(|| conditions.join("&")),
                        line: node.start_position().row as u32 + 1,
                        uncertain,
                        shadowed,
                    });
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk(child, source, bindings, conditions, uncertain, forwards);
    }
}

/// Computed writes on the module's own export object, excluding deferred bodies.
pub(super) fn computed_exports<'tree>(
    root: Node<'tree>,
    source: &[u8],
) -> Vec<(Node<'tree>, Option<super::property_key::PropertyKey>)> {
    fn visit<'tree>(
        node: Node<'tree>,
        source: &[u8],
        result: &mut Vec<(Node<'tree>, Option<super::property_key::PropertyKey>)>,
    ) {
        if matches!(
            node.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "class_declaration"
        ) {
            return;
        }
        if node.kind() == "assignment_expression" {
            if let Some(left) = node.child_by_field_name("left") {
                if left.kind() == "subscript_expression" {
                    if let (Some(object), Some(index)) = (
                        left.child_by_field_name("object"),
                        left.child_by_field_name("index"),
                    ) {
                        let target: String = text(object, source)
                            .chars()
                            .filter(|ch| !ch.is_whitespace())
                            .collect();
                        if (target == "exports"
                            && super::property_key::is_unbound(object, "exports", source))
                            || (target == "module.exports"
                                && super::property_key::is_unbound(object, "module", source))
                        {
                            result.push((index, super::property_key::resolve(index, source)));
                        }
                    }
                } else if text(left, source) == "module.exports" {
                    if let Some(object) = node
                        .child_by_field_name("right")
                        .filter(|node| node.kind() == "object")
                    {
                        for member in object.named_children(&mut object.walk()) {
                            if let Some(key) = member
                                .child_by_field_name("key")
                                .or_else(|| member.child_by_field_name("name"))
                            {
                                if key.kind() == "computed_property_name" {
                                    result.push((key, super::property_key::resolve(key, source)));
                                }
                            }
                        }
                    }
                }
            }
        }
        for child in node.named_children(&mut node.walk()) {
            visit(child, source, result);
        }
    }
    let mut result = Vec::new();
    visit(root, source, &mut result);
    result
}

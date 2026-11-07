//! Retain evidence limits for reflective effects on the public CommonJS object.

use super::{commonjs, grammar, property_key};
use tree_sitter::Node;

fn public_target(node: Node<'_>, source: &[u8], boundary: Option<Node<'_>>) -> bool {
    commonjs::is_export_object(node, source, boundary)
        || node
            .child_by_field_name("object")
            .is_some_and(|object| commonjs::is_export_object(object, source, boundary))
}

fn public_result(mut node: Node<'_>, source: &[u8], boundary: Option<Node<'_>>) -> bool {
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), "parenthesized_expression" | "as_expression") {
            node = parent;
            continue;
        }
        return parent.kind() == "assignment_expression"
            && parent.child_by_field_name("right") == Some(node)
            && parent
                .child_by_field_name("left")
                .is_some_and(|left| public_target(left, source, boundary));
    }
    false
}

fn opaque_initialization(mut node: Node<'_>, source: &[u8]) -> bool {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement"
            | "switch_statement" | "labeled_statement" | "labelled_statement"
            | "with_statement" => return true,
            "if_statement"
                if parent
                    .child_by_field_name("condition")
                    .and_then(|condition| commonjs::environment_test(condition, source))
                    .is_none() =>
            {
                return true
            }
            _ => {}
        }
        node = parent;
    }
    false
}

fn literal_eval_writes_public(call: Node<'_>, source: &[u8]) -> bool {
    let Some(literal) = call
        .child_by_field_name("arguments")
        .and_then(|arguments| arguments.named_child(0))
        .filter(|argument| argument.kind() == "string")
    else {
        return false;
    };
    let Some(decoded) = literal
        .utf8_text(source)
        .ok()
        .and_then(property_key::decode_string)
    else {
        return false;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&grammar::get_language(&grammar::SourceLanguage::JavaScript))
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(&decoded, None) else {
        return false;
    };
    let root = tree.root_node();
    let language = grammar::get_language(&grammar::SourceLanguage::JavaScript);
    let bodies = commonjs::initializer_bodies(root, &language, decoded.as_bytes());
    fn writes(
        node: Node<'_>,
        decoded: &[u8],
        bodies: &[commonjs::Initializer<'_>],
        caller: Node<'_>,
        source: &[u8],
    ) -> bool {
        if let Some(boundary) = commonjs::initialization_scope(node, bodies, decoded) {
            if node.kind() == "assignment_expression" {
                if let Some(left) = node
                    .child_by_field_name("left")
                    .filter(|left| public_target(*left, decoded, boundary))
                {
                    let mut target = left;
                    while !commonjs::is_export_object(target, decoded, boundary) {
                        let Some(object) = target.child_by_field_name("object") else {
                            break;
                        };
                        target = object;
                    }
                    let binding = if target.kind() == "identifier" {
                        "exports"
                    } else {
                        "module"
                    };
                    if property_key::is_unbound(caller, binding, source) {
                        return true;
                    }
                }
            }
        }
        node.named_children(&mut node.walk())
            .any(|child| writes(child, decoded, bodies, caller, source))
    }
    writes(root, decoded.as_bytes(), &bodies, call, source)
}

fn unresolved_call(node: Node<'_>, source: &[u8], boundary: Option<Node<'_>>) -> bool {
    let Some(function) = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("constructor"))
    else {
        return false;
    };
    if function.kind() == "identifier" {
        let name = property_key::source_name(function, source);
        return match name.as_deref() {
            Some("eval") if property_key::is_unbound(function, "eval", source) => {
                literal_eval_writes_public(node, source)
            }
            Some("Function") if property_key::is_unbound(function, "Function", source) => {
                public_result(node, source, boundary)
            }
            _ => false,
        };
    }
    let (Some(object), Some(method)) = (
        function.child_by_field_name("object"),
        function
            .child_by_field_name("property")
            .and_then(|method| property_key::source_name(method, source)),
    ) else {
        return false;
    };
    if method == "bind" && public_result(node, source, boundary) {
        let mut receiver = property_key::initializer(object, source).unwrap_or(object);
        while receiver.kind() == "parenthesized_expression" {
            let Some(inner) = receiver.named_child(0) else {
                break;
            };
            receiver = inner;
        }
        if matches!(
            receiver.kind(),
            "function_expression" | "arrow_function" | "generator_function"
        ) {
            return true;
        }
    }
    if matches!(method.as_str(), "__defineGetter__" | "__defineSetter__")
        && commonjs::is_export_object(object, source, boundary)
    {
        return true;
    }
    let Some(builtin) = property_key::source_name(object, source)
        .filter(|name| matches!(name.as_str(), "Object" | "Reflect"))
        .filter(|name| property_key::is_unbound(object, name, source))
    else {
        return false;
    };
    let first = node.child_by_field_name("arguments").and_then(|arguments| {
        arguments
            .named_children(&mut arguments.walk())
            .find(|argument| argument.kind() != "comment")
    });
    let public_argument =
        first.is_some_and(|argument| commonjs::is_export_object(argument, source, boundary));
    match builtin.as_str() {
        "Object" => {
            (public_argument
                && matches!(
                    method.as_str(),
                    "freeze" | "seal" | "preventExtensions" | "setPrototypeOf"
                ))
                || (public_result(node, source, boundary)
                    && matches!(
                        method.as_str(),
                        "create" | "fromEntries" | "seal" | "preventExtensions" | "setPrototypeOf"
                    ))
                || (public_argument
                    && opaque_initialization(node, source)
                    && matches!(
                        method.as_str(),
                        "assign" | "defineProperty" | "defineProperties"
                    ))
        }
        "Reflect" => {
            public_argument
                && matches!(
                    method.as_str(),
                    "defineProperty"
                        | "set"
                        | "deleteProperty"
                        | "preventExtensions"
                        | "setPrototypeOf"
                )
        }
        _ => false,
    }
}

pub(crate) fn unresolved_lines(root: Node<'_>, source: &[u8]) -> Vec<u32> {
    let bodies = commonjs::initializer_bodies(root, &root.language(), source);
    fn collect(
        node: Node<'_>,
        source: &[u8],
        bodies: &[commonjs::Initializer<'_>],
        lines: &mut Vec<u32>,
    ) {
        if node.kind() == "assignment_expression" {
            if let Some(boundary) = commonjs::initialization_scope(node, bodies, source) {
                if node.child_by_field_name("left").is_some_and(|left| {
                    (public_target(left, source, boundary) && opaque_initialization(node, source))
                        || (left.kind() == "member_expression"
                            && left
                                .child_by_field_name("property")
                                .and_then(|property| property_key::source_name(property, source))
                                .as_deref()
                                == Some("__proto__")
                            && left.child_by_field_name("object").is_some_and(|object| {
                                commonjs::is_export_object(object, source, boundary)
                            }))
                }) {
                    lines.push(node.start_position().row as u32 + 1);
                }
            }
        }
        if matches!(node.kind(), "call_expression" | "new_expression") {
            if let Some(boundary) = commonjs::initialization_scope(node, bodies, source) {
                if unresolved_call(node, source, boundary) {
                    lines.push(node.start_position().row as u32 + 1);
                }
            }
        }
        for child in node.named_children(&mut node.walk()) {
            collect(child, source, bodies, lines);
        }
    }
    let mut lines = Vec::new();
    collect(root, source, &bodies, &mut lines);
    lines
}

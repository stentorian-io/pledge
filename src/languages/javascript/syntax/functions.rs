use crate::languages::typescript::types::shape as type_shapes;

/// IFT-03: Extract per-position optionality of parameters in a function/method node.
///
/// Finds the `formal_parameters` child and returns a `Vec<bool>` where `true`
/// means the parameter at that position is optional. Skips non-parameter children
/// (commas, parentheses, etc.).
pub fn extract_param_optionality(fn_or_method_node: tree_sitter::Node) -> Vec<bool> {
    for i in 0..fn_or_method_node.child_count() as u32 {
        if let Some(params_node) = fn_or_method_node.child(i) {
            if params_node.kind() == "formal_parameters" {
                let mut result = Vec::new();
                let mut cursor = params_node.walk();
                for child in params_node.children(&mut cursor) {
                    match child.kind() {
                        "required_parameter" => result.push(parameter_can_be_omitted(child)),
                        "optional_parameter" => result.push(true),
                        _ => {} // skip commas, parens, etc.
                    }
                }
                return result;
            }
        }
    }
    Vec::new()
}

fn parameter_can_be_omitted(parameter: tree_sitter::Node) -> bool {
    parameter.kind() == "optional_parameter"
        || parameter.child_by_field_name("value").is_some()
        || is_rest_param(parameter) && rest_type_allows_empty(parameter.child_by_field_name("type"))
}

fn rest_type_allows_empty(annotation: Option<tree_sitter::Node>) -> bool {
    let Some(annotation) = annotation else {
        return true;
    };
    let shape = if annotation.kind() == "type_annotation" {
        annotation.named_child(0).unwrap_or(annotation)
    } else {
        annotation
    };
    if shape.kind() != "tuple_type" {
        // Array rests accept no arguments. Tuple aliases and generic rests need
        // compiler resolution before their required arity can be established.
        return true;
    }
    shape
        .named_children(&mut shape.walk())
        .all(|element| match element.kind() {
            "optional_type" | "optional_parameter" => true,
            "required_parameter" => parameter_can_be_omitted(element),
            "rest_type" => rest_type_allows_empty(element.named_child(0)),
            _ => false,
        })
}

/// Check if a function_declaration node has the `async` modifier.
///
/// The `async` keyword is an anonymous token that appears BEFORE the `function`
/// keyword in the AST child order. We stop iterating at the `function` keyword
/// to avoid false positives.
pub fn is_async_function(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "function" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("async") {
            return true;
        }
    }
    false
}

/// Check if a function_declaration or generator_function_declaration node has
/// the `*` generator marker.
///
/// The `*` token appears between the `function` keyword and the
/// `formal_parameters` node. We stop at `formal_parameters` to avoid
/// matching `*` in other contexts.
///
/// Note: tree-sitter-typescript parses `function* foo() {}` as
/// `generator_function_declaration`, not `function_declaration`.
pub fn is_generator_function(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "formal_parameters" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("*") {
            return true;
        }
    }
    false
}

/// Check if a required_parameter node represents a rest parameter.
/// rest params are `required_parameter` with a `rest_pattern` child.
/// NOT `rest_parameter` — that node kind does not exist in tree-sitter-typescript 0.23.2.
pub fn is_rest_param(param_node: tree_sitter::Node) -> bool {
    let mut cursor = param_node.walk();
    for child in param_node.children(&mut cursor) {
        if child.kind() == "rest_pattern" {
            return true;
        }
    }
    false
}

/// Extract the text of the first named child of a type_annotation node.
/// Returns None if no named child is found.
pub(crate) fn first_named_child_text<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Find the `rest_pattern` child of a `required_parameter` node, then return the
/// `identifier` text inside that rest_pattern (the param name).
pub(crate) fn rest_param_name<'a>(
    param: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut pc = param.walk();
    for child in param.children(&mut pc) {
        if child.kind() == "rest_pattern" {
            let mut rc = child.walk();
            for inner in child.children(&mut rc) {
                if inner.kind() == "identifier" {
                    return inner.utf8_text(source).ok();
                }
            }
        }
    }
    None
}

/// Build a TypeShape::Function from a function_signature or function_declaration node.
/// Extracts parameter names, types, optionality, and rest flags.
/// Used for overload signature extraction.
pub fn build_function_shape_from_node(
    fn_node: tree_sitter::Node,
    source: &[u8],
) -> Option<type_shapes::TypeShape> {
    use crate::languages::typescript::types::shape::{ParamShape, PrimitiveKind, TypeShape};

    // Find formal_parameters child
    let params_node = fn_node.child_by_field_name("parameters")?;
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for param in params_node.children(&mut cursor) {
        match param.kind() {
            "required_parameter" => {
                let rest = is_rest_param(param);
                let name = if rest {
                    // For rest params, the identifier is inside rest_pattern
                    rest_param_name(param, source).unwrap_or("_").to_string()
                } else {
                    param
                        .child_by_field_name("pattern")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("_")
                        .to_string()
                };
                let type_text = param
                    .child_by_field_name("type")
                    .and_then(|ta| first_named_child_text(ta, source))
                    .unwrap_or("unknown");
                params.push(ParamShape {
                    name,
                    optional: false,
                    rest,
                    shape: TypeShape::Opaque(type_text.to_string()),
                });
            }
            "optional_parameter" => {
                let name = param
                    .child_by_field_name("pattern")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("_")
                    .to_string();
                let type_text = param
                    .child_by_field_name("type")
                    .and_then(|ta| first_named_child_text(ta, source))
                    .unwrap_or("unknown");
                params.push(ParamShape {
                    name,
                    optional: true,
                    rest: false,
                    shape: TypeShape::Opaque(type_text.to_string()),
                });
            }
            _ => {} // skip commas, other tokens
        }
    }

    // Extract return type
    let return_type = fn_node
        .child_by_field_name("return_type")
        .and_then(|ta| first_named_child_text(ta, source))
        .map(|s| TypeShape::Opaque(s.to_string()))
        .unwrap_or(TypeShape::Primitive(PrimitiveKind::Void));

    Some(TypeShape::function(params, return_type))
}

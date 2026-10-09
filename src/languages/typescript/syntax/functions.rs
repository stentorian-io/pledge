/// Extract the type guard type from a function declaration/signature node.
/// Returns `Some("Foo")` for `function isStr(x: unknown): x is Foo`.
/// Returns `None` if the function does not have a type predicate return.
///
/// type guard returns use `type_predicate_annotation` (not `type_annotation`).
/// The `type_predicate` child contains the parameter name and guarded type.
pub fn extract_type_guard(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "type_predicate_annotation" {
            // Find the type_predicate child
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if inner_child.kind() == "type_predicate" {
                    // The guarded type is the last named child (type node)
                    // Structure: identifier "is" type
                    let type_node = inner_child.child_by_field_name("type").or_else(|| {
                        // Fallback: last named child that is a type (not the identifier)
                        let mut tc = inner_child.walk();
                        inner_child
                            .children(&mut tc)
                            .filter(|c| c.is_named() && c.kind() != "identifier")
                            .last()
                    });
                    if let Some(tn) = type_node {
                        return tn.utf8_text(source).ok().map(|s| s.to_string());
                    }
                }
            }
        }
    }
    None
}

/// TLF-04: Extract the asserted type from an `asserts x is T` signature.
///
/// In tree-sitter-typescript 0.23.2 the asserts annotation appears inside a
/// `type_predicate_annotation` node (the same container as `x is T` type guards).
/// Within it, when the `asserts` keyword is present, there may be a `type_predicate`
/// child or the asserted type may be a direct named child after `is`.
/// Returns `Some("T")` for `function foo(): asserts x is T`.
/// Returns `None` for non-assertion functions.
pub fn extract_assertion_target(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "type_predicate_annotation" {
            // Look for `asserts` keyword — if present, this is an assertion function
            let mut saw_asserts = false;
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if !inner_child.is_named() && inner_child.utf8_text(source).ok() == Some("asserts")
                {
                    saw_asserts = true;
                }
            }
            if !saw_asserts {
                continue;
            }
            // Walk type_predicate child or direct type child after `asserts`
            let mut inner_cursor2 = child.walk();
            for inner_child in child.children(&mut inner_cursor2) {
                if inner_child.kind() == "type_predicate" {
                    // asserts x is T — find the type after `is`
                    let type_node = inner_child.child_by_field_name("type").or_else(|| {
                        let mut tc = inner_child.walk();
                        inner_child
                            .children(&mut tc)
                            .filter(|c| c.is_named() && c.kind() != "identifier")
                            .last()
                    });
                    if let Some(tn) = type_node {
                        return tn.utf8_text(source).ok().map(String::from);
                    }
                } else if inner_child.is_named() && inner_child.kind() != "identifier" {
                    // asserts x (without `is T`) — asserts a boolean condition
                    // Return None to distinguish from typed asserts
                }
            }
        }
    }
    None
}

/// TLF-03: Extract the `this` parameter type annotation from a function node.
///
/// In TypeScript, `this` is the first parameter with name `this`. It constrains
/// the call receiver. Example: `function onClick(this: HTMLElement): void {}`.
/// Returns `Some("HTMLElement")` for that example, `None` otherwise.
pub fn extract_this_param_type(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Look for "parameters" field child (works for function_declaration, arrow_function,
    // function_signature, method_definition, etc.)
    let params_node = fn_node.child_by_field_name("parameters")?;
    let mut cursor = params_node.walk();
    for param in params_node.children(&mut cursor) {
        // `this` parameter parses as required_parameter with identifier child named "this"
        if param.kind() == "required_parameter" {
            // Check if the pattern (first named child) is the identifier `this`
            let mut pc = param.walk();
            let mut name_text: Option<&str> = None;
            for child in param.children(&mut pc) {
                if child.kind() == "identifier" || child.kind() == "this" {
                    name_text = child.utf8_text(source).ok();
                    break;
                }
            }
            if name_text != Some("this") {
                continue;
            }
            // Found the `this` parameter — extract its type annotation
            let mut tc = param.walk();
            for child in param.children(&mut tc) {
                if child.kind() == "type_annotation" {
                    let mut inner_cursor = child.walk();
                    let type_node = child.children(&mut inner_cursor).find(|c| c.is_named());
                    return type_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(String::from);
                }
            }
        }
        // Only check the first parameter — `this` must be first
        break;
    }
    None
}

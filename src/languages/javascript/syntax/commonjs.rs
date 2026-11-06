//! CommonJS initialization, property identity, and literal forwarding.

use crate::languages::javascript::syntax::{grammar, property_key, queries};
use std::collections::HashMap;
use tree_sitter::{Node, Point, QueryCursor, StreamingIterator};

#[derive(Debug, Clone)]
pub(crate) struct Forward {
    pub specifier: String,
    pub condition: Option<String>,
    pub line: u32,
    pub uncertain: bool,
    pub shadowed: Vec<String>,
    pub kind: ForwardKind,
    pub value_position: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwardKind {
    WholeModule,
    Properties,
    StarProperties,
}

#[derive(Clone, PartialEq)]
struct RequireBinding {
    specifier: String,
    uncertain_line: Option<u32>,
}

#[derive(Default)]
pub(crate) struct CommonJsAnalysis {
    pub forwards: Vec<Forward>,
    pub mutation_lines: Vec<u32>,
    last_replacement: Option<Point>,
    has_unconditional_write: bool,
    has_named_export_write: bool,
    exports_may_be_detached: bool,
    mutated_targets: Vec<(String, u32)>,
}

impl CommonJsAnalysis {
    pub(crate) fn retains_export_at(
        &self,
        root: Node<'_>,
        source: &[u8],
        line: u32,
        col: u32,
    ) -> bool {
        let position = Point::new(
            line.saturating_sub(1) as usize,
            col.saturating_sub(1) as usize,
        );
        if self
            .last_replacement
            .is_none_or(|replacement| position >= replacement)
        {
            return true;
        }
        let mut ancestor = root.descendant_for_point_range(position, position);
        while let Some(node) = ancestor {
            if node.kind() == "assignment_expression" && is_export_annotation(node, source) {
                return true;
            }
            if node.kind() == "assignment_expression"
                && node
                    .child_by_field_name("left")
                    .is_some_and(|left| is_export_target(left, source))
            {
                return false;
            }
            ancestor = node.parent();
        }
        true
    }
}

pub(crate) fn analyze(root: Node<'_>, source: &[u8]) -> CommonJsAnalysis {
    let mut analysis = CommonJsAnalysis::default();
    walk(root, source, &mut HashMap::new(), &[], false, &mut analysis);
    for (specifier, line) in &analysis.mutated_targets {
        if analysis
            .forwards
            .iter()
            .any(|forward| forward.specifier == *specifier)
        {
            analysis.mutation_lines.push(*line);
        }
    }
    analysis.mutation_lines.sort_unstable();
    analysis.mutation_lines.dedup();
    analysis
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

fn compact(node: Node<'_>, source: &[u8]) -> String {
    if let Some(name) = property_key::source_name(node, source)
        .filter(|_| matches!(node.kind(), "identifier" | "property_identifier"))
    {
        return name;
    }
    if node.kind() == "member_expression" {
        if let (Some(object), Some(property)) = (
            node.child_by_field_name("object"),
            node.child_by_field_name("property"),
        ) {
            return format!("{}.{}", compact(object, source), compact(property, source));
        }
    }
    text(node, source)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

pub(crate) struct Initializer<'tree> {
    pub body: Node<'tree>,
    pub returns_exports: bool,
    pub returned_calls: Vec<Node<'tree>>,
}

pub(crate) fn initializer_bodies<'tree>(
    root: Node<'tree>,
    language: &tree_sitter::Language,
    source: &[u8],
) -> Vec<Initializer<'tree>> {
    fn invocation<'tree>(
        node: Node<'tree>,
        wrapper: Node<'tree>,
        factory: &str,
        exports_parameter: Option<usize>,
        source: &[u8],
    ) -> (bool, Vec<Node<'tree>>) {
        let mut initialized = false;
        let mut returned_calls = Vec::new();
        if node.kind() == "call_expression" {
            if let Some(function) = node.child_by_field_name("function") {
                let public_argument = exports_parameter.is_none_or(|index| {
                    node.child_by_field_name("arguments")
                        .and_then(|arguments| {
                            arguments
                                .named_children(&mut arguments.walk())
                                .filter(|argument| argument.kind() != "comment")
                                .nth(index)
                        })
                        .is_some_and(|argument| is_export_object(argument, source, None))
                });
                if text(function, source) == factory
                    && !property_key::is_shadowed_within(function, factory, wrapper, source)
                    && initialization_scope(node, &[], source).is_some()
                    && public_argument
                {
                    initialized = true;
                    let mut result = node;
                    while result
                        .parent()
                        .is_some_and(|parent| parent.kind() == "parenthesized_expression")
                    {
                        result = result.parent().unwrap();
                    }
                    if result.parent().is_some_and(|assignment| {
                        assignment.kind() == "assignment_expression"
                            && assignment.child_by_field_name("right") == Some(result)
                            && assignment.child_by_field_name("left").is_some_and(|left| {
                                left.kind() != "identifier" && is_export_object(left, source, None)
                            })
                    }) {
                        returned_calls.push(node);
                    }
                }
            }
        }
        for child in node.named_children(&mut node.walk()) {
            let (child_initialized, child_returns) =
                invocation(child, wrapper, factory, exports_parameter, source);
            initialized |= child_initialized;
            returned_calls.extend(child_returns);
        }
        (initialized, returned_calls)
    }

    let mut bodies = Vec::new();
    for query in [
        queries::UMD_FACTORY_WRAPPER_QUERY,
        queries::UMD_FACTORY_WRAPPER_QUERY_FORM2,
    ] {
        let Ok(query) = grammar::cached_query(language, query) else {
            continue;
        };
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(found) = matches.next() {
            let capture = |index| {
                found
                    .captures
                    .iter()
                    .find(|capture| capture.index == index)
                    .map(|capture| capture.node)
            };
            let (Some(factory), Some(parameters), Some(body)) =
                (capture(1), capture(3), capture(4))
            else {
                continue;
            };
            let Some(wrapper) = factory
                .parent()
                .and_then(|parameters| parameters.parent())
                .and_then(|function| function.child_by_field_name("body"))
            else {
                continue;
            };
            let exports_parameter = parameters
                .named_children(&mut parameters.walk())
                .filter(|parameter| parameter.kind() != "comment")
                .position(|parameter| {
                    property_key::source_name(parameter, source).as_deref() == Some("exports")
                });
            let (initialized, returned_calls) = invocation(
                wrapper,
                wrapper,
                text(factory, source),
                exports_parameter,
                source,
            );
            if initialized {
                bodies.push(Initializer {
                    body,
                    returns_exports: !returned_calls.is_empty(),
                    returned_calls,
                });
            }
        }
    }
    bodies
}

pub(crate) fn initialization_scope<'tree>(
    node: Node<'tree>,
    bodies: &[Initializer<'tree>],
    source: &[u8],
) -> Option<Option<Node<'tree>>> {
    let mut child = node;
    let mut ancestor = node.parent();
    let mut boundary = None;
    while let Some(parent) = ancestor {
        if matches!(
            parent.kind(),
            "generator_function" | "generator_function_declaration"
        ) {
            return None;
        }
        if parent.kind() == "method_definition" && parent.child_by_field_name("name") != Some(child)
        {
            return None;
        }
        if matches!(
            parent.kind(),
            "field_definition" | "public_field_definition"
        ) && parent.child_by_field_name("property") != Some(child)
            && parent.child_by_field_name("name") != Some(child)
            && !parent
                .children(&mut parent.walk())
                .any(|token| token.kind() == "static")
        {
            return None;
        }
        if matches!(
            parent.kind(),
            "function_expression" | "function_declaration" | "arrow_function"
        ) {
            if let Some(body) = parent
                .child_by_field_name("body")
                .filter(|body| bodies.iter().any(|initializer| initializer.body == *body))
            {
                boundary.get_or_insert(body);
                // A recognized UMD factory is a module initialization boundary.
                break;
            }
            let mut callable = parent;
            while callable
                .parent()
                .is_some_and(|outer| outer.kind() == "parenthesized_expression")
            {
                callable = callable.parent()?;
            }
            if let Some(member) = callable.parent().filter(|member| {
                member.kind() == "member_expression"
                    && member.child_by_field_name("object") == Some(callable)
                    && member
                        .child_by_field_name("property")
                        .is_some_and(|property| {
                            matches!(
                                property_key::source_name(property, source).as_deref(),
                                Some("call" | "apply")
                            )
                        })
            }) {
                callable = member;
            }
            let call = callable.parent().filter(|call| {
                call.kind() == "call_expression"
                    && call.child_by_field_name("function") == Some(callable)
            })?;
            let parameters: Vec<_> = parent
                .child_by_field_name("parameters")
                .map(|parameters| {
                    parameters
                        .named_children(&mut parameters.walk())
                        .filter(|parameter| parameter.kind() != "comment")
                        .collect()
                })
                .unwrap_or_else(|| {
                    parent
                        .child_by_field_name("parameter")
                        .into_iter()
                        .collect()
                });
            if let Some(index) = parameters.iter().position(|parameter| {
                property_key::source_name(*parameter, source).as_deref() == Some("exports")
            }) {
                let arguments = call.child_by_field_name("arguments")?;
                let args: Vec<_> = arguments
                    .named_children(&mut arguments.walk())
                    .filter(|argument| argument.kind() != "comment")
                    .collect();
                let method = callable
                    .child_by_field_name("property")
                    .and_then(|property| property_key::source_name(property, source));
                let argument = match method.as_deref() {
                    Some("call") => args.get(index + 1).copied(),
                    Some("apply") => args
                        .get(1)
                        .filter(|array| array.kind() == "array")
                        .and_then(|array| {
                            let mut position = 0;
                            for element in array.children(&mut array.walk()) {
                                if element.kind() == "," {
                                    position += 1;
                                }
                                if element.is_named()
                                    && element.kind() != "comment"
                                    && position == index
                                {
                                    return Some(element);
                                }
                            }
                            None
                        }),
                    _ => args.get(index).copied(),
                };
                if argument.is_some_and(|argument| {
                    let argument_scope = initialization_scope(argument, bodies, source).flatten();
                    is_export_object(argument, source, argument_scope)
                }) {
                    if let Some(body) = parent.child_by_field_name("body") {
                        boundary.get_or_insert(body);
                    }
                }
            }
        }
        child = parent;
        ancestor = parent.parent();
    }
    Some(boundary)
}

pub(crate) fn is_export_object(node: Node<'_>, source: &[u8], boundary: Option<Node<'_>>) -> bool {
    let node = unwrap(node);
    if node.kind() == "identifier"
        && property_key::source_name(node, source).as_deref() == Some("exports")
    {
        return boundary.map_or_else(
            || property_key::is_unbound(node, "exports", source),
            |body| !property_key::is_shadowed_within(node, "exports", body, source),
        );
    }
    node.kind() == "member_expression"
        && node.child_by_field_name("object").is_some_and(|object| {
            property_key::source_name(object, source).as_deref() == Some("module")
                && property_key::is_unbound(object, "module", source)
        })
        && node
            .child_by_field_name("property")
            .is_some_and(|property| {
                property_key::source_name(property, source).as_deref() == Some("exports")
            })
}

/// Await suspension makes a CommonJS write's availability depend on load timing.
pub(crate) fn has_suspended_export(node: Node<'_>) -> bool {
    fn has_await_before(node: Node<'_>, end: usize) -> bool {
        if node.start_byte() >= end
            || matches!(
                node.kind(),
                "function_expression"
                    | "function_declaration"
                    | "arrow_function"
                    | "method_definition"
            )
        {
            return false;
        }
        if node.kind() == "await_expression"
            || (node.kind() == "for_in_statement"
                && node
                    .children(&mut node.walk())
                    .any(|token| token.kind() == "await"))
        {
            return true;
        }
        node.named_children(&mut node.walk())
            .any(|child| has_await_before(child, end))
    }
    let mut end = node.end_byte();
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if matches!(parent.kind(), "assignment_expression" | "call_expression") {
            end = parent.end_byte();
        }
        if matches!(
            parent.kind(),
            "function_expression" | "function_declaration" | "arrow_function"
        ) && parent
            .children(&mut parent.walk())
            .any(|token| token.kind() == "async")
            && parent
                .child_by_field_name("body")
                .is_some_and(|body| has_await_before(body, end))
        {
            return true;
        }
        ancestor = parent.parent();
    }
    false
}

/// Whether a captured assignment writes the public CommonJS object at initialization.
pub(crate) fn is_public_assignment(
    node: Node<'_>,
    source: &[u8],
    bodies: &[Initializer<'_>],
) -> bool {
    let Some(boundary) = initialization_scope(node, bodies, source) else {
        return false;
    };
    let mut ancestor = Some(node);
    while let Some(assignment) = ancestor {
        if assignment.kind() == "assignment_expression" {
            let Some(left) = assignment.child_by_field_name("left") else {
                return false;
            };
            return (left.kind() != "identifier" && is_export_object(left, source, boundary))
                || left
                    .child_by_field_name("object")
                    .is_some_and(|object| is_export_object(object, source, boundary));
        }
        ancestor = assignment.parent();
    }
    false
}

fn is_export_target(node: Node<'_>, source: &[u8]) -> bool {
    let target = compact(node, source);
    ((target == "module.exports"
        || target.starts_with("module.exports.")
        || target.starts_with("module.exports["))
        && property_key::is_unbound(node, "module", source))
        || ((target == "exports"
            || target.starts_with("exports.")
            || target.starts_with("exports["))
            && property_key::is_unbound(node, "exports", source))
}

fn is_definite_replacement(
    node: Node<'_>,
    source: &[u8],
    bindings: &HashMap<String, RequireBinding>,
) -> bool {
    let node = unwrap(node);
    if require_target(node, source, bindings).is_some() {
        return true;
    }
    if node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .is_some_and(|function| {
                function.kind() == "member_expression"
                    && function
                        .child_by_field_name("object")
                        .is_some_and(|object| {
                            property_key::source_name(object, source).as_deref() == Some("Object")
                                && property_key::is_unbound(object, "Object", source)
                        })
                    && function
                        .child_by_field_name("property")
                        .is_some_and(|property| {
                            property_key::source_name(property, source).as_deref() == Some("assign")
                        })
            })
        && node
            .child_by_field_name("arguments")
            .is_some_and(|arguments| {
                arguments
                    .named_children(&mut arguments.walk())
                    .any(|argument| is_export_object(argument, source, None))
            })
    {
        return false;
    }
    match node.kind() {
        "object" => node.named_children(&mut node.walk()).all(|child| {
            child.kind() != "spread_element"
                || child
                    .named_child(0)
                    .and_then(|spread| require_target(spread, source, bindings))
                    .is_some()
        }),
        "array"
        | "call_expression"
        | "new_expression"
        | "class"
        | "function_expression"
        | "generator_function"
        | "arrow_function"
        | "string"
        | "number"
        | "true"
        | "false"
        | "null" => true,
        _ => false,
    }
}

pub(crate) fn is_export_annotation(mut node: Node<'_>, source: &[u8]) -> bool {
    loop {
        if node.kind() == "binary_expression"
            && node
                .child_by_field_name("operator")
                .is_some_and(|operator| text(operator, source) == "&&")
            && node
                .child_by_field_name("left")
                .is_some_and(|left| matches!(text(unwrap(left), source), "0" | "false" | "null"))
        {
            return true;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

fn require_target(
    node: Node<'_>,
    source: &[u8],
    bindings: &HashMap<String, RequireBinding>,
) -> Option<RequireBinding> {
    let node = unwrap(node);
    if node.kind() == "identifier" {
        return bindings.get(text(node, source)).cloned();
    }
    if node.kind() != "call_expression"
        || !node
            .child_by_field_name("function")
            .is_some_and(|function| {
                property_key::source_name(function, source).as_deref() == Some("require")
                    && property_key::is_unbound(function, "require", source)
            })
    {
        return None;
    }
    let argument = node.child_by_field_name("arguments")?.named_child(0)?;
    (argument.kind() == "string").then(|| RequireBinding {
        specifier: text(argument, source).trim_matches(['\'', '"']).to_string(),
        uncertain_line: None,
    })
}

fn assign_require_binding(
    bindings: &mut HashMap<String, RequireBinding>,
    name: &str,
    replacement: Option<RequireBinding>,
    uncertain: bool,
    line: u32,
) {
    if uncertain {
        // A possible rebind changes future reads, not the object already forwarded.
        if let Some(replacement) = replacement {
            bindings.entry(name.to_string()).or_insert(replacement);
        }
        if let Some(binding) = bindings.get_mut(name) {
            binding.uncertain_line = Some(line);
        }
    } else if let Some(replacement) = replacement {
        bindings.insert(name.to_string(), replacement);
    } else {
        bindings.remove(name);
    }
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
    bindings: &mut HashMap<String, RequireBinding>,
    conditions: &[String],
    uncertain: bool,
    analysis: &mut CommonJsAnalysis,
) {
    if matches!(
        node.kind(),
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration"
            | "generator_function"
            | "class_declaration"
            | "class"
    ) {
        return;
    }
    // Bundler annotations can describe exports without replacing the live object.
    let annotation = matches!(
        node.kind(),
        "binary_expression" | "assignment_expression" | "augmented_assignment_expression"
    ) && is_export_annotation(node, source);
    let uncertain = uncertain
        || matches!(
            node.kind(),
            "for_statement"
                | "for_in_statement"
                | "while_statement"
                | "do_statement"
                | "switch_statement"
                | "try_statement"
                | "ternary_expression"
                | "augmented_assignment_expression"
        )
        || (node.kind() == "binary_expression" && !annotation);
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
        let original_bindings = bindings.clone();
        for (field, predicate) in [("consequence", positive), ("alternative", negative)] {
            if let Some(branch) = node.child_by_field_name(field) {
                let mut branch_conditions = conditions.to_vec();
                branch_conditions.push(predicate);
                let mut branch_bindings = original_bindings.clone();
                walk(
                    branch,
                    source,
                    &mut branch_bindings,
                    &branch_conditions,
                    uncertain || unsupported,
                    analysis,
                );
                for (name, target) in &branch_bindings {
                    if original_bindings.get(name) != Some(target) {
                        bindings
                            .entry(name.clone())
                            .or_insert_with(|| target.clone())
                            .uncertain_line = Some(node.start_position().row as u32 + 1);
                    }
                }
                for (name, target) in &original_bindings {
                    if !branch_bindings.contains_key(name) {
                        let mut target = target.clone();
                        target.uncertain_line = Some(node.start_position().row as u32 + 1);
                        bindings.insert(name.clone(), target);
                    }
                }
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
                let replacement = require_target(value, source, bindings);
                assign_require_binding(
                    bindings,
                    text(name, source),
                    replacement,
                    uncertain,
                    node.start_position().row as u32 + 1,
                );
            }
        }
    }
    if node.kind() == "unary_expression"
        && node
            .child_by_field_name("operator")
            .is_some_and(|operator| text(operator, source) == "delete")
    {
        if let Some(argument) = node.child_by_field_name("argument") {
            let line = node.start_position().row as u32 + 1;
            if is_export_target(argument, source) {
                analysis.mutation_lines.push(line);
            } else if let Some(object) = argument.child_by_field_name("object") {
                if let Some(target) = bindings.get(text(object, source)) {
                    analysis
                        .mutated_targets
                        .push((target.specifier.clone(), line));
                }
            }
        }
    }
    if matches!(
        node.kind(),
        "assignment_expression" | "augmented_assignment_expression"
    ) {
        if let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) {
            if left.kind() == "identifier" && !annotation {
                let replacement = (node.kind() == "assignment_expression")
                    .then(|| require_target(right, source, bindings))
                    .flatten();
                assign_require_binding(
                    bindings,
                    text(left, source),
                    replacement,
                    uncertain,
                    node.start_position().row as u32 + 1,
                );
            }
            let target = compact(left, source);
            if !annotation && is_export_target(left, source) {
                let right_target = compact(right, source);
                if target == "module.exports" && right_target != "module.exports" {
                    analysis.exports_may_be_detached = right_target != "exports"
                        || !is_export_target(right, source)
                        || uncertain
                        || !conditions.is_empty();
                } else if target == "exports" && right_target != "exports" {
                    analysis.exports_may_be_detached = right_target != "module.exports"
                        || !is_export_target(right, source)
                        || uncertain
                        || !conditions.is_empty();
                } else if analysis.exports_may_be_detached
                    && (target.starts_with("exports.") || target.starts_with("exports["))
                {
                    analysis
                        .mutation_lines
                        .push(node.start_position().row as u32 + 1);
                }
            }
            if target == "module.exports" && is_export_target(left, source) {
                let context = (!conditions.is_empty()).then(|| conditions.join("&"));
                if !annotation
                    && node.kind() == "assignment_expression"
                    && is_definite_replacement(right, source, bindings)
                {
                    if uncertain {
                        analysis
                            .mutation_lines
                            .push(node.start_position().row as u32 + 1);
                    } else if conditions.is_empty() {
                        analysis.forwards.clear();
                        analysis.last_replacement = Some(node.start_position());
                    } else {
                        analysis
                            .forwards
                            .retain(|forward| forward.condition != context);
                        if analysis.has_unconditional_write {
                            analysis
                                .mutation_lines
                                .push(node.start_position().row as u32 + 1);
                        }
                    }
                } else if !annotation
                    && (analysis.has_unconditional_write || !analysis.forwards.is_empty())
                    && compact(right, source) != "module.exports"
                    && !(compact(right, source) == "exports.default"
                        && !analysis.has_named_export_write
                        && analysis.last_replacement.is_none()
                        && analysis.forwards.is_empty())
                {
                    analysis
                        .mutation_lines
                        .push(node.start_position().row as u32 + 1);
                }
                let mut targets = Vec::new();
                if let Some(target) = require_target(right, source, bindings) {
                    targets.push((target, Vec::new(), ForwardKind::WholeModule));
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
                            let kind = if annotation {
                                ForwardKind::StarProperties
                            } else {
                                ForwardKind::Properties
                            };
                            targets.push((target, shadowed, kind));
                        }
                    }
                }
                for (target, shadowed, kind) in targets {
                    if let Some(line) = target.uncertain_line {
                        analysis.mutation_lines.push(line);
                    }
                    analysis.forwards.push(Forward {
                        specifier: target.specifier,
                        condition: (!conditions.is_empty()).then(|| conditions.join("&")),
                        line: node.start_position().row as u32 + 1,
                        uncertain,
                        shadowed,
                        kind,
                        value_position: right.start_position(),
                    });
                }
            }
            if !annotation && !uncertain && conditions.is_empty() && is_export_target(left, source)
            {
                analysis.has_unconditional_write = true;
                if !matches!(
                    target.as_str(),
                    "module.exports" | "exports.default" | "exports.__esModule"
                ) {
                    analysis.has_named_export_write = true;
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk(child, source, bindings, conditions, uncertain, analysis);
    }
}

/// Property writes on the module export object during module initialization.
/// Kept shared with discovery so unresolved keys degrade the same surface we enumerate.
pub(crate) fn computed_exports<'tree>(
    root: Node<'tree>,
    source: &[u8],
) -> Vec<(Node<'tree>, Option<property_key::PropertyKey>)> {
    fn property(node: Node<'_>, source: &[u8]) -> Option<property_key::PropertyKey> {
        if matches!(
            node.kind(),
            "property_identifier" | "shorthand_property_identifier"
        ) {
            property_key::source_name(node, source).map(property_key::PropertyKey::String)
        } else {
            property_key::resolve(node, source)
        }
    }
    fn visit<'tree>(
        node: Node<'tree>,
        source: &[u8],
        bodies: &[Initializer<'tree>],
        result: &mut Vec<(Node<'tree>, Option<property_key::PropertyKey>)>,
    ) {
        if node.kind() == "assignment_expression" && is_public_assignment(node, source, bodies) {
            let Some(left) = node.child_by_field_name("left") else {
                return;
            };
            let boundary = initialization_scope(node, bodies, source).flatten();
            if left
                .child_by_field_name("object")
                .is_some_and(|object| is_export_object(object, source, boundary))
            {
                if let Some(key) = left
                    .child_by_field_name("index")
                    .or_else(|| left.child_by_field_name("property"))
                {
                    result.push((key, property(key, source)));
                }
            } else if is_export_object(left, source, boundary) {
                if let Some(object) = node
                    .child_by_field_name("right")
                    .filter(|node| node.kind() == "object")
                {
                    for member in object.named_children(&mut object.walk()) {
                        let key = if member.kind() == "shorthand_property_identifier" {
                            Some(member)
                        } else {
                            member
                                .child_by_field_name("key")
                                .or_else(|| member.child_by_field_name("name"))
                        };
                        if let Some(key) = key {
                            result.push((key, property(key, source)));
                        }
                    }
                }
            }
        }
        for child in node.named_children(&mut node.walk()) {
            visit(child, source, bodies, result);
        }
    }
    let language = grammar::get_language(&grammar::SourceLanguage::JavaScript);
    let bodies = initializer_bodies(root, &language, source);
    let mut result = Vec::new();
    visit(root, source, &bodies, &mut result);
    result
}

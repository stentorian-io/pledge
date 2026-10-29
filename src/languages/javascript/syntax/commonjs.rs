//! Literal CommonJS forwarding, retaining the environment that selects a module.

use crate::languages::javascript::syntax::property_key;
use std::collections::HashMap;
use tree_sitter::{Node, Point};

#[derive(Debug, Clone)]
pub(crate) struct Forward {
    pub specifier: String,
    pub condition: Option<String>,
    pub line: u32,
    pub uncertain: bool,
    pub shadowed: Vec<String>,
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
    text(node, source)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
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
    match node.kind() {
        "object" => node.named_children(&mut node.walk()).all(|child| {
            child.kind() != "spread_element"
                || child
                    .named_child(0)
                    .and_then(|spread| require_target(spread, source, bindings))
                    .is_some()
        }),
        "array"
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

fn is_export_annotation(mut node: Node<'_>, source: &[u8]) -> bool {
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
        || text(node.child_by_field_name("function")?, source) != "require"
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
                for (target, shadowed) in targets {
                    if let Some(line) = target.uncertain_line {
                        analysis.mutation_lines.push(line);
                    }
                    analysis.forwards.push(Forward {
                        specifier: target.specifier,
                        condition: (!conditions.is_empty()).then(|| conditions.join("&")),
                        line: node.start_position().row as u32 + 1,
                        uncertain,
                        shadowed,
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

/// Computed writes on the module's own export object, excluding deferred bodies.
pub(crate) fn computed_exports<'tree>(
    root: Node<'tree>,
    source: &[u8],
) -> Vec<(Node<'tree>, Option<property_key::PropertyKey>)> {
    fn visit<'tree>(
        node: Node<'tree>,
        source: &[u8],
        result: &mut Vec<(Node<'tree>, Option<property_key::PropertyKey>)>,
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
                            && property_key::is_unbound(object, "exports", source))
                            || (target == "module.exports"
                                && property_key::is_unbound(object, "module", source))
                        {
                            result.push((index, property_key::resolve(index, source)));
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
                                    result.push((key, property_key::resolve(key, source)));
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

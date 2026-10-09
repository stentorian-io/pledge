use crate::languages::typescript::model::{EnumMember, EnumShape, EnumValueKind};

/// Returns true if the given `enum_declaration` node has a `const` anonymous
/// keyword child (i.e., it is a `const enum`).
///
/// `const` is an anonymous grammar token,
/// not a named field. Must iterate children and check text.
pub fn is_const_enum(node: tree_sitter::Node, source: &[u8]) -> bool {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "enum" {
                break; // Past modifiers
            }
            if !child.is_named() {
                if let Ok(text) = child.utf8_text(source) {
                    if text == "const" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Compute auto-incremented numeric values for enum members.
///
/// maintains running counter starting at 0.
/// - No explicit value: assign computed_value = Some(counter), increment.
/// - Numeric explicit value: assign computed_value = Some(n), set counter = n + 1.
/// - Non-numeric initializer: set computed_value = None, still increment counter.
pub fn compute_enum_values(members: &mut [EnumMember]) {
    let mut next_value: i64 = 0;
    for member in members.iter_mut() {
        match &member.raw_value {
            None => {
                member.computed_value = Some(next_value);
                next_value += 1;
            }
            Some(text) => {
                if let Ok(n) = text.trim().parse::<i64>() {
                    member.computed_value = Some(n);
                    next_value = n + 1;
                } else {
                    member.computed_value = None;
                    next_value += 1; // TypeScript behavior: still increment
                }
            }
        }
    }
}

/// Extract all members from an `enum_declaration` node.
///
/// Handles both explicit (`enum_assignment`) and implicit (bare identifier) members.
/// Calls `compute_enum_values` to fill in computed_value fields.
pub fn parse_enum_members(source: &[u8], enum_node: tree_sitter::Node) -> Vec<EnumMember> {
    let mut members = Vec::new();

    // Find enum_body child
    let enum_body = {
        let mut body = None;
        for i in 0..enum_node.child_count() as u32 {
            if let Some(child) = enum_node.child(i) {
                if child.kind() == "enum_body" {
                    body = Some(child);
                    break;
                }
            }
        }
        match body {
            Some(b) => b,
            None => return members,
        }
    };

    let mut cursor = enum_body.walk();
    for child in enum_body.children(&mut cursor) {
        match child.kind() {
            "enum_assignment" => {
                // Has explicit value: name: (_) @name value: (_) @value
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                let value_text = child
                    .child_by_field_name("value")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.to_string());

                let value_kind = if let Some(ref text) = value_text {
                    let trimmed = text.trim();
                    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                        EnumValueKind::StringLiteral
                    } else if trimmed.parse::<i64>().is_ok() {
                        EnumValueKind::Numeric
                    } else {
                        EnumValueKind::OtherExpr
                    }
                } else {
                    EnumValueKind::Numeric // implicit
                };

                members.push(EnumMember {
                    name,
                    raw_value: value_text,
                    computed_value: None, // filled in by compute_enum_values
                    value_kind,
                });
            }
            "property_identifier" | "identifier" => {
                // Bare member with no explicit value (implicit numeric)
                let name = child.utf8_text(source).unwrap_or("").to_string();
                members.push(EnumMember {
                    name,
                    raw_value: None,
                    computed_value: None,
                    value_kind: EnumValueKind::Numeric, // implicit numeric
                });
            }
            _ => {}
        }
    }

    compute_enum_values(&mut members);
    members
}

/// Extract a full `EnumShape` from an `enum_declaration` node.
pub fn parse_enum_shape(source: &[u8], enum_node: tree_sitter::Node) -> EnumShape {
    let is_const = is_const_enum(enum_node, source);
    let members = parse_enum_members(source, enum_node);
    EnumShape { members, is_const }
}

use crate::languages::javascript::model::{ClassMember, ClassMemberKind, Visibility};
use crate::languages::javascript::syntax::classes as class_syntax;
use crate::languages::typescript::model as ts_model;

/// An index signature extracted from an interface or object type.
/// E.g., `[key: string]: number` has key_type_text="string", value_type_text="number".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSignature {
    pub key_type_text: String,
    pub value_type_text: String,
}

/// Extract all index signatures from an interface_body or object_type node.
/// Returns empty Vec if node has no index_signature children.
pub fn extract_index_signatures(
    body_node: tree_sitter::Node,
    source: &[u8],
) -> Vec<IndexSignature> {
    let mut sigs = Vec::new();
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "index_signature" {
            let mut key_type = None;
            let mut value_type = None;
            let mut in_brackets = false;
            let mut ic = child.walk();
            for c in child.children(&mut ic) {
                match c.kind() {
                    "[" => in_brackets = true,
                    "]" => in_brackets = false,
                    "type_annotation" if !in_brackets => {
                        // Value type is the type_annotation after the brackets
                        // Iterate children with for loop to avoid cursor borrow issues
                        let mut tc = c.walk();
                        for type_child in c.children(&mut tc) {
                            if type_child.is_named() {
                                value_type =
                                    type_child.utf8_text(source).ok().map(|s| s.to_string());
                                break;
                            }
                        }
                    }
                    _ if in_brackets
                        && c.is_named()
                        && c.kind() != "identifier"
                        && key_type.is_none() =>
                    {
                        // Key type is the named non-identifier child inside brackets
                        // (predefined_type like "string" or "number")
                        key_type = c.utf8_text(source).ok().map(|s| s.to_string());
                    }
                    _ => {}
                }
            }
            if let (Some(kt), Some(vt)) = (key_type, value_type) {
                sigs.push(IndexSignature {
                    key_type_text: kt,
                    value_type_text: vt,
                });
            }
        }
    }
    sigs
}

/// Find the interface_body child of an interface_declaration node.
/// Returns None if the declaration has no body (forward declaration).
pub fn find_interface_body(decl_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = decl_node.walk();
    // Use a local result variable so the cursor is dropped before function return
    let result = decl_node
        .children(&mut cursor)
        .find(|child| child.kind() == "interface_body" || child.kind() == "object_type");
    result
}

/// Extract members from an `interface_body` node.
///
/// Interface members are `method_signature` and `property_signature` nodes
/// (same node kinds as `.d.ts` ambient class members).
/// All interface members are public (no visibility modifiers in interfaces).
/// Reuses ClassMember type for consistency with class_diff pipeline.
pub fn parse_interface_members(
    source: &[u8],
    interface_body: tree_sitter::Node,
) -> Vec<ClassMember> {
    let mut members = Vec::new();
    let mut cursor = interface_body.walk();
    for child in interface_body.children(&mut cursor) {
        match child.kind() {
            "method_signature" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let param_count = class_syntax::count_required_params(child);
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    decorators: Vec::new(),
                    is_private: false,
                });
            }
            "property_signature" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                // Detect readonly modifier
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.kind() == "readonly" {
                            is_readonly = true;
                        }
                    }
                }
                // Detect optional (?) — property_signature with "?" anonymous child
                // Optional properties are non-breaking to add; required are breaking
                let mut is_optional = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if !kw.is_named() && kw.utf8_text(source).ok() == Some("?") {
                            is_optional = true;
                            break;
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    // use proper is_optional field (replaces type_signature stopgap)
                    type_signature: None,
                    is_optional,
                    is_accessor: false,
                    this_param_type: None,
                    decorators: Vec::new(),
                    is_private: false,
                });
            }
            "call_signature" => {
                // Call signatures: interface with () callable syntax
                // Treat as a method with a synthetic name "(call)"
                let param_count = class_syntax::count_required_params(child);
                members.push(ClassMember {
                    name: "(call)".to_string(),
                    kind: ClassMemberKind::Method,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    decorators: Vec::new(),
                    is_private: false,
                });
            }
            _ => {}
        }
    }
    members
}

/// TLF-01: Extract interface construct signatures (`new (x: T): U` and `abstract new(): T`).
///
/// In tree-sitter-typescript 0.23.2 the node kind is `construct_signature`.
/// It appears as a direct child of `interface_body` / `object_type`.
pub fn extract_construct_signatures(
    body_node: tree_sitter::Node,
    source: &[u8],
) -> Vec<ts_model::ConstructSignature> {
    let mut sigs = Vec::new();
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "construct_signature" {
            let param_count = class_syntax::count_required_params(child);
            // In tree-sitter-typescript 0.23.2 the return type of a construct_signature
            // appears as a `type_annotation` child (`: T`), not a named field `return_type`.
            // Walk children to find it and unwrap the inner type node.
            let return_text = {
                let mut cs_cursor = child.walk();
                let ta_node = child
                    .children(&mut cs_cursor)
                    .find(|c| c.kind() == "type_annotation");
                if let Some(ta) = ta_node {
                    let mut ta_cursor = ta.walk();
                    let inner_node = ta.children(&mut ta_cursor).find(|c| c.is_named());
                    inner_node
                        .and_then(|inner| inner.utf8_text(source).ok())
                        .map(String::from)
                } else {
                    None
                }
            };
            // Check for `abstract` token (unnamed child before the `new` keyword)
            let mut is_abstract = false;
            let mut abs_cursor = child.walk();
            for ch in child.children(&mut abs_cursor) {
                if !ch.is_named() && ch.utf8_text(source).ok() == Some("abstract") {
                    is_abstract = true;
                    break;
                }
            }
            sigs.push(ts_model::ConstructSignature {
                param_count,
                return_text,
                is_abstract,
            });
        }
    }
    sigs
}

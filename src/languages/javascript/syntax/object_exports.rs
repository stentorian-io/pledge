//! Public properties created by eager calls to the global Object helpers.

use super::{commonjs, grammar, property_key, queries};
use anyhow::Context;
use property_key::PropertyKey;
use tree_sitter::{Node, QueryCursor, StreamingIterator};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Descriptor {
    pub writable: Option<bool>,
    pub enumerable: Option<bool>,
}

pub(crate) struct ObjectExport<'tree> {
    pub key: PropertyKey,
    pub node: Node<'tree>,
    pub descriptor: Descriptor,
}

fn property_name(node: Node<'_>, source: &[u8]) -> Option<PropertyKey> {
    if matches!(
        node.kind(),
        "property_identifier" | "shorthand_property_identifier"
    ) {
        property_key::source_name(node, source).map(PropertyKey::String)
    } else {
        property_key::resolve(node, source)
    }
}

fn descriptor(node: Node<'_>, source: &[u8]) -> Descriptor {
    if node.kind() != "object" {
        return Descriptor::default();
    }
    let mut descriptor = Descriptor {
        writable: Some(false),
        enumerable: Some(false),
    };
    for member in node.named_children(&mut node.walk()) {
        if member.kind() == "comment" {
            continue;
        }
        let Some(key) = member
            .child_by_field_name("key")
            .or_else(|| member.child_by_field_name("name"))
            .and_then(|key| property_name(key, source))
        else {
            return Descriptor::default();
        };
        let PropertyKey::String(key) = key else {
            continue;
        };
        let value = member.child_by_field_name("value");
        let boolean = value.and_then(|value| match value.kind() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        });
        match key.as_str() {
            "writable" => descriptor.writable = boolean,
            // An accessor with a setter accepts writes even without a writable flag.
            "set" => {
                descriptor.writable = if member.kind() == "method_definition" {
                    (!member
                        .children(&mut member.walk())
                        .any(|token| matches!(token.kind(), "get" | "set")))
                    .then_some(true)
                } else {
                    value.and_then(|value| match value.kind() {
                        "function_expression" | "arrow_function" => Some(true),
                        "undefined" => Some(false),
                        _ => None,
                    })
                };
            }
            "enumerable" => descriptor.enumerable = boolean,
            _ => {}
        }
    }
    descriptor
}

pub(crate) fn collect<'tree>(
    root: Node<'tree>,
    language: &tree_sitter::Language,
    source: &[u8],
) -> anyhow::Result<Vec<ObjectExport<'tree>>> {
    let bodies = commonjs::initializer_bodies(root, language, source);
    let mut exports = Vec::new();
    let query = grammar::cached_query(language, queries::OBJECT_DEFINE_EXPORT_QUERY)?;
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
        let (Some(object), Some(method), Some(arguments)) = (capture(0), capture(1), capture(2))
        else {
            continue;
        };
        if property_key::source_name(object, source).as_deref() != Some("Object")
            || !property_key::is_unbound(object, "Object", source)
        {
            continue;
        }
        let Some(boundary) = commonjs::initialization_scope(object, &bodies, source) else {
            continue;
        };
        let args: Vec<_> = arguments
            .named_children(&mut arguments.walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        let Some(first) = args.first().copied() else {
            continue;
        };
        let mut emit = |node, key, descriptor| {
            if key != PropertyKey::String("__esModule".into()) {
                exports.push(ObjectExport {
                    node,
                    key,
                    descriptor,
                });
            }
        };
        match property_key::source_name(method, source)
            .as_deref()
            .unwrap_or("")
        {
            "defineProperty" if commonjs::is_export_object(first, source, boundary) => {
                if let (Some(key_node), Some(description)) = (args.get(1), args.get(2)) {
                    let key = property_key::resolve(*key_node, source).context(
                        "unresolved CommonJS descriptor property key; public surface is incomplete",
                    )?;
                    emit(*key_node, key, descriptor(*description, source));
                }
            }
            "defineProperties" if commonjs::is_export_object(first, source, boundary) => {
                if let Some(descriptions) = args.get(1).filter(|node| node.kind() == "object") {
                    for member in descriptions.named_children(&mut descriptions.walk()) {
                        if let (Some(key_node), Some(description)) = (
                            member.child_by_field_name("key"),
                            member.child_by_field_name("value"),
                        ) {
                            let key = property_name(key_node, source)
                                .context("unresolved CommonJS descriptor property key; public surface is incomplete")?;
                            emit(key_node, key, descriptor(description, source));
                        }
                    }
                }
            }
            "assign" => {
                let returned_to_exports = arguments
                    .parent()
                    .and_then(|call| call.parent())
                    .is_some_and(|assignment| {
                        assignment.kind() == "assignment_expression"
                            && assignment.child_by_field_name("left").is_some_and(|left| {
                                left.kind() == "member_expression"
                                    && commonjs::is_export_object(left, source, boundary)
                            })
                    });
                let writes_exports = commonjs::is_export_object(first, source, boundary)
                    || (returned_to_exports
                        && args
                            .iter()
                            .any(|arg| commonjs::is_export_object(*arg, source, boundary)));
                if !writes_exports {
                    continue;
                }
                for object in args.iter().filter(|node| node.kind() == "object") {
                    for member in object.named_children(&mut object.walk()) {
                        let key_node = member
                            .child_by_field_name("key")
                            .or_else(|| member.child_by_field_name("name"))
                            .or_else(|| {
                                (member.kind() == "shorthand_property_identifier").then_some(member)
                            });
                        if let Some(key_node) = key_node {
                            let key = property_name(key_node, source)
                                .context("unresolved CommonJS assigned property key; public surface is incomplete")?;
                            emit(
                                key_node,
                                key,
                                Descriptor {
                                    writable: Some(true),
                                    enumerable: Some(true),
                                },
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(exports)
}

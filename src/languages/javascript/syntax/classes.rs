use crate::languages::javascript::model as js_model;
use crate::languages::javascript::model::{ClassMember, ClassMemberKind, ClassShape, Visibility};
use crate::languages::javascript::syntax::{
    functions as function_syntax, property_key, static_initialization,
};
use crate::languages::typescript::model as ts_model;
use crate::languages::typescript::syntax::{annotations, generics as generic_syntax};

/// Extract the `Visibility` for a method or field node by iterating its children
/// looking for an `accessibility_modifier` child.
///
/// no modifier = `Visibility::Public`.
pub(crate) fn extract_visibility(node: tree_sitter::Node, source: &[u8]) -> Visibility {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "accessibility_modifier" {
                if let Ok(text) = child.utf8_text(source) {
                    return match text {
                        "protected" => Visibility::Protected,
                        "private" => Visibility::Private,
                        _ => Visibility::Public, // "public" explicit
                    };
                }
            }
        }
    }
    Visibility::Public // No modifier = public (TypeScript default)
}

/// Minimum argument count implied by the declared parameter positions.
/// Defaults and array rests may be omitted, but a required argument following
/// a default still requires callers to supply that earlier position.
pub(crate) fn count_required_params(method_node: tree_sitter::Node) -> usize {
    function_syntax::extract_param_optionality(method_node)
        .iter()
        .rposition(|optional| !optional)
        .map_or(0, |last_required| last_required + 1)
}

/// Extract all class members (methods, constructors, properties, abstract methods)
/// from a `class_declaration` or `abstract_class_declaration` node.
///
/// Line and column are one-based; tree-sitter positions are zero-based.
///
/// CLS-12: returns `(members, has_dynamic_static_key)`. The bool is
/// `true` if any `class_static_block` body contains a subscript-expression LHS
/// (`Foo[computed()] = x`) — consumed by `parse_class_shape` to set
/// `ClassShape.has_dynamic_static_key`, which the diff layer reads to emit the
/// `limitation: dynamic-static-member-key` annotation.
pub fn parse_class_members(
    source: &[u8],
    class_node: tree_sitter::Node,
) -> (Vec<ClassMember>, bool) {
    let mut members = Vec::new();
    // CLS-12: extract the enclosing class name once — used by the
    // class_static_block arm to gate `Foo.bar =...` assignments. Returns an
    // empty string for anonymous `export default class {... }`, in which case
    // only the `this.bar =...` path matches inside the static-block walker.
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
        .to_string();
    let mut has_dynamic_static_key = false;

    // Find class_body child
    let class_body = {
        let mut body = None;
        for i in 0..class_node.child_count() as u32 {
            if let Some(child) = class_node.child(i) {
                if child.kind() == "class_body" {
                    body = Some(child);
                    break;
                }
            }
        }
        match body {
            Some(b) => b,
            None => return (members, has_dynamic_static_key),
        }
    };

    let declared_static_members: std::collections::HashSet<String> = class_body
        .named_children(&mut class_body.walk())
        .filter(|member| {
            member
                .children(&mut member.walk())
                .any(|child| child.kind() == "static")
        })
        .filter_map(|member| member.child_by_field_name("name"))
        .map(|name| {
            if name.kind() == "computed_property_name" {
                if let Some(key) = property_key::resolve(name, source) {
                    return key.member_name();
                }
            }
            name.utf8_text(source).unwrap_or("").to_string()
        })
        .collect();

    let mut cursor = class_body.walk();
    for child in class_body.children(&mut cursor) {
        match child.kind() {
            "method_definition" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .map(|node| {
                        if node.kind() == "computed_property_name" {
                            if let Some(key) = property_key::resolve(node, source) {
                                return key.member_name();
                            }
                            has_dynamic_static_key = true;
                        }
                        node.utf8_text(source).unwrap_or("").to_string()
                    })
                    .unwrap_or_default();
                // CLS-11: ES2022 private methods (`#compute`) keep
                // the `#` in `name` so diff messages stay accurate; the flag
                // lets class_diff.rs route them out of the public-surface
                // HashMaps (Risk 6 Option A — prevents false-positive CLS-03
                // `class-method-removed` on `#foo` → `#bar` renames).
                let is_private = name.starts_with('#');

                // Detect getter/setter by scanning anonymous children for "get"/"set" keywords.
                // Modifiers appear before the name field; stop at named nodes other than
                // accessibility_modifier to avoid false positives.
                let mut is_getter = false;
                let mut is_setter = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.is_named() && kw.kind() != "accessibility_modifier" {
                            break;
                        }
                        match kw.kind() {
                            "get" => is_getter = true,
                            "set" => is_setter = true,
                            _ => {}
                        }
                    }
                }

                let visibility = extract_visibility(child, source);

                // Check for static, readonly, abstract, accessor keyword children
                let mut is_static = false;
                let mut is_abstract = false;
                let mut is_readonly = false;
                let mut is_accessor = false; // TLF-06
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "abstract" => is_abstract = true,
                            "readonly" => is_readonly = true,
                            "accessor" => is_accessor = true, // TLF-06
                            _ => {}
                        }
                    }
                }

                let kind = if name == "constructor" && !is_static {
                    ClassMemberKind::Constructor
                } else if is_getter {
                    ClassMemberKind::Getter
                } else if is_setter {
                    ClassMemberKind::Setter
                } else {
                    ClassMemberKind::Method
                };

                let param_count = count_required_params(child);

                // TLF-02: Synthesize Property members from constructor parameter properties.
                // A parameter property is a constructor parameter with an accessibility modifier
                // (public/protected/private) or readonly keyword. Bare parameters are NOT
                // synthesized.
                if name == "constructor" {
                    // Walk formal_parameters to find parameter properties
                    let fp_node = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "formal_parameters");
                    if let Some(fp) = fp_node {
                        let mut fp_cursor = fp.walk();
                        for param in fp.children(&mut fp_cursor) {
                            if param.kind() == "required_parameter"
                                || param.kind() == "optional_parameter"
                            {
                                let is_opt_param = param.kind() == "optional_parameter";
                                // Check for accessibility_modifier or readonly child
                                let mut param_vis: Option<js_model::Visibility> = None;
                                let mut param_readonly = false;
                                let mut pp_cursor = param.walk();
                                for pp_child in param.children(&mut pp_cursor) {
                                    if pp_child.kind() == "accessibility_modifier" {
                                        let text = pp_child.utf8_text(source).unwrap_or("");
                                        param_vis = Some(match text {
                                            "protected" => js_model::Visibility::Protected,
                                            "private" => js_model::Visibility::Private,
                                            _ => js_model::Visibility::Public,
                                        });
                                    } else if pp_child.kind() == "readonly" {
                                        param_readonly = true;
                                    }
                                }
                                // Only synthesize when an accessibility modifier or readonly is present
                                if param_vis.is_some() || param_readonly {
                                    // Get the parameter name: prefer `name` field, fallback to
                                    // first identifier child (tree-sitter 0.23.2 may not always
                                    // use the field name for parameter identifier nodes).
                                    let param_name_node =
                                        param.child_by_field_name("name").or_else(|| {
                                            let mut wc = param.walk();
                                            let found = param
                                                .children(&mut wc)
                                                .find(|n| n.kind() == "identifier");
                                            found
                                        });
                                    let param_name = param_name_node
                                        .and_then(|n| n.utf8_text(source).ok())
                                        .unwrap_or("")
                                        .to_string();
                                    if !param_name.is_empty() {
                                        members.push(ClassMember {
                                            name: param_name,
                                            kind: ClassMemberKind::Property,
                                            visibility: param_vis
                                                .unwrap_or(js_model::Visibility::Public),
                                            is_static: false,
                                            is_abstract: false,
                                            is_readonly: param_readonly,
                                            param_count: 0,
                                            type_signature: None,
                                            is_optional: is_opt_param,
                                            is_accessor: false,
                                            this_param_type: None,
                                            // CLS-10: parameter-property decorators
                                            // live on the required_parameter/optional_parameter node.
                                            decorators: annotations::extract_decorators(
                                                param, source,
                                            ),
                                            is_private: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                members.push(ClassMember {
                    name,
                    kind,
                    visibility,
                    is_static,
                    is_abstract,
                    is_readonly,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor,
                    this_param_type: None,
                    // CLS-10: method decorators attach to the
                    // method_definition node in JS, or appear as preceding
                    // decorator siblings inside class_body in TS. extract_decorators
                    // handles both patterns.
                    decorators: annotations::extract_decorators(child, source),
                    // CLS-11: propagate `#`-prefix private method detection.
                    is_private,
                });
            }
            "public_field_definition" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .map(|node| {
                        if node.kind() == "computed_property_name" {
                            if let Some(key) = property_key::resolve(node, source) {
                                return key.member_name();
                            }
                            has_dynamic_static_key = true;
                        }
                        node.utf8_text(source).unwrap_or("").to_string()
                    })
                    .unwrap_or_default();

                // CLS-11: keep `#`-prefix private fields; the `is_private`
                // flag lets the diff layer pair public `foo` ↔ private `#foo` across
                // versions. `#` is retained in `name` so diff messages can show the
                // accurate source-level identifier on either side. The identity-match
                // helper in class_diff.rs strips the prefix on the fly.
                if name.is_empty() {
                    continue;
                }
                let is_private = name.starts_with('#');

                let visibility = extract_visibility(child, source);

                let mut is_static = false;
                let mut is_readonly = false;
                let mut is_accessor = false; // TLF-06: accessor keyword on public_field_definition
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "readonly" => is_readonly = true,
                            "accessor" => is_accessor = true, // TLF-06
                            _ => {}
                        }
                    }
                }

                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    type_signature: None,
                    is_optional: false,
                    is_accessor,
                    this_param_type: None,
                    // CLS-10: field decorators attach to the
                    // public_field_definition node in TS, or appear as
                    // preceding decorator siblings in class_body.
                    decorators: annotations::extract_decorators(child, source),
                    // CLS-11: propagate `#`-prefix private field detection.
                    is_private,
                });
            }
            "abstract_method_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .map(|node| {
                        if node.kind() == "computed_property_name" {
                            if let Some(key) = property_key::resolve(node, source) {
                                return key.member_name();
                            }
                            has_dynamic_static_key = true;
                        }
                        node.utf8_text(source).unwrap_or("").to_string()
                    })
                    .unwrap_or_default();

                let visibility = extract_visibility(child, source);
                let param_count = count_required_params(child);

                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility,
                    is_static: false,
                    is_abstract: true,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    // CLS-10: abstract-method decorators.
                    decorators: annotations::extract_decorators(child, source),
                    is_private: false,
                });
            }
            // .d.ts ambient class members use method_signature /
            // property_signature instead of method_definition / public_field_definition.
            "method_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .map(|node| {
                        if node.kind() == "computed_property_name" {
                            if let Some(key) = property_key::resolve(node, source) {
                                return key.member_name();
                            }
                            has_dynamic_static_key = true;
                        }
                        node.utf8_text(source).unwrap_or("").to_string()
                    })
                    .unwrap_or_default();
                if name.is_empty() {
                    continue;
                }
                let visibility = extract_visibility(child, source);
                let param_count = count_required_params(child);
                let mut is_static = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.kind() == "static" {
                            is_static = true;
                        }
                    }
                }
                let kind = if name == "constructor" && !is_static {
                    ClassMemberKind::Constructor
                } else {
                    ClassMemberKind::Method
                };
                members.push(ClassMember {
                    name,
                    kind,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    // CLS-10: ambient method_signature decorators
                    // (rare in.d.ts but harmless to extract).
                    decorators: annotations::extract_decorators(child, source),
                    is_private: false,
                });
            }
            "property_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .map(|node| {
                        if node.kind() == "computed_property_name" {
                            if let Some(key) = property_key::resolve(node, source) {
                                return key.member_name();
                            }
                            has_dynamic_static_key = true;
                        }
                        node.utf8_text(source).unwrap_or("").to_string()
                    })
                    .unwrap_or_default();
                // CLS-11: keep `#`-prefix ambient private members;
                // same rationale as public_field_definition arm.
                if name.is_empty() {
                    continue;
                }
                let is_private = name.starts_with('#');
                let visibility = extract_visibility(child, source);
                let mut is_static = false;
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "readonly" => is_readonly = true,
                            _ => {}
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    type_signature: None,
                    is_optional: false,
                    is_accessor: false,
                    this_param_type: None,
                    // CLS-10: ambient property_signature decorators.
                    decorators: annotations::extract_decorators(child, source),
                    // CLS-11: propagate `#`-prefix private ambient-member detection.
                    is_private,
                });
            }
            "class_static_block" => {
                let Some(sb_body) = child.child_by_field_name("body") else {
                    continue;
                };
                for (assign, conditional) in static_initialization::assignments(sb_body, source) {
                    let Some(lhs) = assign.child_by_field_name("left") else {
                        continue;
                    };
                    match lhs.kind() {
                        "member_expression" | "subscript_expression" => {
                            let obj = lhs.child_by_field_name("object").map(|mut node| {
                                while matches!(
                                    node.kind(),
                                    "parenthesized_expression"
                                        | "as_expression"
                                        | "satisfies_expression"
                                ) {
                                    let Some(inner) = node.named_child(0) else {
                                        break;
                                    };
                                    node = inner;
                                }
                                node
                            });
                            let prop = lhs
                                .child_by_field_name("property")
                                .or_else(|| lhs.child_by_field_name("index"));
                            let obj_text = obj.and_then(|n| n.utf8_text(source).ok()).unwrap_or("");
                            // Gate: object must be the class name (by text) OR
                            // `this` (handles anonymous default-exported
                            // classes). If class_name is empty (anonymous),
                            // only `this` matches.
                            let is_class_ref = (!class_name.is_empty()
                                && obj_text == class_name
                                && obj.is_some_and(|node| {
                                    !property_key::is_shadowed_within(
                                        node,
                                        &class_name,
                                        class_node,
                                        source,
                                    )
                                }))
                                || obj.is_some_and(|n| n.kind() == "this");
                            if !is_class_ref {
                                continue;
                            }
                            if conditional {
                                has_dynamic_static_key = true;
                                continue;
                            }
                            let prop_name = if lhs.kind() == "subscript_expression" {
                                let Some(key) =
                                    prop.and_then(|node| property_key::resolve(node, source))
                                else {
                                    has_dynamic_static_key = true;
                                    continue;
                                };
                                key.member_name()
                            } else {
                                prop.and_then(|n| n.utf8_text(source).ok())
                                    .unwrap_or("")
                                    .to_string()
                            };
                            if prop_name.is_empty() {
                                continue;
                            }
                            // Preserve declared visibility, decorators, and modifiers.
                            // Compiler enrichment separately tracks initialization types.
                            if prop_name.starts_with('#')
                                || declared_static_members.contains(&prop_name)
                            {
                                continue;
                            }
                            members.retain(|member| {
                                !(member.name == prop_name
                                    && member.is_static
                                    && matches!(member.kind, ClassMemberKind::Property))
                            });
                            members.push(ClassMember {
                                name: prop_name,
                                kind: ClassMemberKind::Property,
                                visibility: Visibility::Public,
                                is_static: true,
                                is_abstract: false,
                                is_readonly: false,
                                param_count: 0,
                                // No LSP enrichment on synthesized members.
                                type_signature: None,
                                is_optional: false,
                                is_accessor: false,
                                this_param_type: None,
                                decorators: Vec::new(),
                                // Static-block synthesized members are public surface.
                                is_private: false,
                            });
                        }
                        _ => {
                            // Other LHS shapes (identifier, etc.) are ignored
                            // not static-member exports.
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (members, has_dynamic_static_key)
}

/// Extract a full `ClassShape` from a `class_declaration` or
/// `abstract_class_declaration` node.
pub fn parse_class_shape(source: &[u8], decl_node: tree_sitter::Node) -> ClassShape {
    let is_abstract = decl_node.kind() == "abstract_class_declaration";

    // Find class_heritage child (contains extends_clause and implements_clause)
    let mut extends_text: Option<String> = None;
    let mut implements_list: Vec<String> = Vec::new();

    for i in 0..decl_node.child_count() as u32 {
        if let Some(child) = decl_node.child(i) {
            if child.kind() == "class_heritage" {
                // Look for extends_clause and implements_clause
                for j in 0..child.child_count() as u32 {
                    if let Some(heritage_child) = child.child(j) {
                        match heritage_child.kind() {
                            "extends_clause" => {
                                if let Ok(text) = heritage_child.utf8_text(source) {
                                    // Strip the leading "extends " keyword
                                    let text = text.trim();
                                    let text = text.strip_prefix("extends ").unwrap_or(text);
                                    extends_text = Some(text.to_string());
                                }
                            }
                            "implements_clause" => {
                                // Iterate named children (type references)
                                let mut cursor = heritage_child.walk();
                                for impl_child in heritage_child.children(&mut cursor) {
                                    // Skip the "implements" keyword itself and commas
                                    if impl_child.is_named() {
                                        if let Ok(text) = impl_child.utf8_text(source) {
                                            implements_list.push(text.to_string());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
        }
    }

    let type_params = generic_syntax::parse_type_params(source, decl_node);
    let (members, has_dynamic_static_key) = parse_class_members(source, decl_node);

    // CLS-10: Extract class-level decorators. In TS, decorators
    // appear as direct children on the class_declaration / abstract_class_declaration
    // AND ALSO re-surface as children of the parent export_statement wrapper
    // (verified empirically against tree-sitter-typescript 0.23.2 —
    // `@Component\nexport class Foo {}` produces the decorator as a child of
    // BOTH the export_statement and the inner class_declaration).
    //
    // To avoid double-counting we prefer the class_declaration's own children
    // and only fall back to the parent export_statement if the direct children
    // yielded nothing. For the JS grammar, decorators only appear on the
    // export_statement, so the fallback path is the primary one there.
    let class_decorators: Vec<ts_model::DecoratorSpec> = {
        let direct = annotations::extract_decorators(decl_node, source);
        if !direct.is_empty() {
            direct
        } else if let Some(parent) = decl_node.parent() {
            if parent.kind() == "export_statement" {
                annotations::extract_decorators(parent, source)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };

    // IFT-03: Extract per-position constructor param optionality.
    // Find the class_body, scan for the constructor method_definition, then
    // call extract_param_optionality on it.
    let constructor_param_optionality = {
        let class_body = {
            let mut found = None;
            for i in 0..decl_node.child_count() as u32 {
                if let Some(child) = decl_node.child(i) {
                    if child.kind() == "class_body" {
                        found = Some(child);
                        break;
                    }
                }
            }
            found
        };
        if let Some(body) = class_body {
            let mut optionality = Vec::new();
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if matches!(child.kind(), "method_definition" | "method_signature") {
                    let is_ctor = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        == Some("constructor");
                    let is_static = child
                        .children(&mut child.walk())
                        .any(|modifier| modifier.kind() == "static");
                    if is_ctor && !is_static {
                        optionality = function_syntax::extract_param_optionality(child);
                        break;
                    }
                }
            }
            optionality
        } else {
            Vec::new()
        }
    };

    ClassShape {
        members,
        is_abstract,
        extends_text,
        implements_list,
        type_params,
        constructor_param_optionality,
        decorators: class_decorators,
        // CLS-12: propagated from `parse_class_members` tuple return.
        has_dynamic_static_key,
    }
}

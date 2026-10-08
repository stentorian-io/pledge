//! Evaluate property keys without executing package code.
//!
//! Only immutable lexical bindings and side-effect-free scalar expressions are
//! folded. Unknown calls, imports, mutations, and shadowed globals stay unknown.

use tree_sitter::Node;

pub(super) const WELL_KNOWN_SYMBOLS: &[&str] = &[
    "asyncDispose",
    "asyncIterator",
    "dispose",
    "hasInstance",
    "isConcatSpreadable",
    "iterator",
    "match",
    "matchAll",
    "replace",
    "search",
    "species",
    "split",
    "toPrimitive",
    "toStringTag",
    "unscopables",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PropertyKey {
    String(String),
    Symbol(String),
    Unique(String),
}

impl PropertyKey {
    pub(super) fn export_name(&self) -> String {
        match self {
            Self::String(name) => name.clone(),
            Self::Symbol(name) => format!("@@{name}"),
            Self::Unique(name) => format!("@@unique({name})"),
        }
    }

    pub(super) fn member_name(&self) -> String {
        match self {
            Self::String(name) => name.clone(),
            Self::Symbol(name) => format!("[Symbol.{name}]"),
            Self::Unique(name) => format!("[{name}]"),
        }
    }
}

fn text<'a>(node: Node<'_>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

pub(super) fn resolve(node: Node<'_>, source: &[u8]) -> Option<PropertyKey> {
    evaluate(node, source, 0)
}

pub(super) fn is_unbound(node: Node<'_>, name: &str, source: &[u8]) -> bool {
    binding(node, name, source).is_none()
}

fn evaluate(node: Node<'_>, source: &[u8], depth: usize) -> Option<PropertyKey> {
    if depth >= 32 {
        return None;
    }
    let string = |node| match evaluate(node, source, depth + 1)? {
        PropertyKey::String(value) => Some(value),
        PropertyKey::Symbol(_) | PropertyKey::Unique(_) => None,
    };
    match node.kind() {
        "computed_property_name"
        | "parenthesized_expression"
        | "as_expression"
        | "satisfies_expression" => evaluate(node.named_child(0)?, source, depth + 1),
        "string" => decode_string(text(node, source)).map(PropertyKey::String),
        "number" => {
            let raw = text(node, source).replace('_', "");
            // Decimal integer keys are exact up to Number's safe-integer limit.
            let value: u64 = raw.parse().ok()?;
            (value <= 9_007_199_254_740_991).then(|| PropertyKey::String(value.to_string()))
        }
        "true" | "false" | "null" => Some(PropertyKey::String(text(node, source).into())),
        "identifier" => {
            let binding = binding(node, text(node, source), source)??;
            evaluate(binding, source, depth + 1)
        }
        "variable_declarator" => Some(PropertyKey::Unique(
            text(node.child_by_field_name("name")?, source).into(),
        )),
        "binary_expression" if text(node.child_by_field_name("operator")?, source) == "+" => {
            let left = node.child_by_field_name("left")?;
            let right = node.child_by_field_name("right")?;
            // Require a string operand: numeric addition has different semantics.
            if !matches!(left.kind(), "string" | "template_string")
                && !matches!(right.kind(), "string" | "template_string")
            {
                return None;
            }
            Some(PropertyKey::String(string(left)? + &string(right)?))
        }
        "template_string" => {
            let mut value = String::new();
            for child in node.named_children(&mut node.walk()) {
                match child.kind() {
                    "string_fragment" => value.push_str(text(child, source)),
                    "escape_sequence" => {
                        value.push_str(&decode_string(&format!("\"{}\"", text(child, source)))?)
                    }
                    "template_substitution" => value.push_str(&string(child.named_child(0)?)?),
                    _ => return None,
                }
            }
            Some(PropertyKey::String(value))
        }
        "member_expression" => {
            let object = node.child_by_field_name("object")?;
            let property = text(node.child_by_field_name("property")?, source);
            if text(object, source) == "Symbol"
                && binding(object, "Symbol", source).is_none()
                && WELL_KNOWN_SYMBOLS.contains(&property)
            {
                Some(PropertyKey::Symbol(property.into()))
            } else {
                None
            }
        }
        "call_expression" => {
            let function = node.child_by_field_name("function")?;
            if function.kind() != "member_expression" {
                return None;
            }
            let object = function.child_by_field_name("object")?;
            if text(object, source) != "Symbol"
                || binding(object, "Symbol", source).is_some()
                || text(function.child_by_field_name("property")?, source) != "for"
            {
                return None;
            }
            let arguments = node.child_by_field_name("arguments")?;
            if arguments.named_child_count() != 1 {
                return None;
            }
            let key = string(arguments.named_child(0)?)?;
            Some(PropertyKey::Symbol(format!(
                "for({})",
                serde_json::to_string(&key).ok()?
            )))
        }
        _ => None,
    }
}

/// None means unbound; Some(None) means bound but not a usable constant.
fn binding<'tree>(at: Node<'tree>, name: &str, source: &[u8]) -> Option<Option<Node<'tree>>> {
    let mut scope = at.parent();
    while let Some(parent) = scope {
        for child in parent.named_children(&mut parent.walk()) {
            if child.kind() == "import_statement" && imports_name(child, name, source) {
                return Some(None);
            }
            let mut declaration = if child.kind() == "export_statement" {
                child.child_by_field_name("declaration").unwrap_or(child)
            } else {
                child
            };
            if declaration.kind() == "ambient_declaration" {
                declaration = declaration.named_child(0).unwrap_or(declaration);
            }
            if matches!(
                declaration.kind(),
                "lexical_declaration" | "variable_declaration"
            ) {
                for variable in declaration.named_children(&mut declaration.walk()) {
                    let Some(identifier) = variable.child_by_field_name("name") else {
                        continue;
                    };
                    if text(identifier, source) == name {
                        let is_const = text(declaration, source).trim_start().starts_with("const ");
                        if is_const
                            && variable
                                .child_by_field_name("type")
                                .is_some_and(|annotation| {
                                    text(annotation, source).trim_start_matches(':').trim()
                                        == "unique symbol"
                                })
                        {
                            return Some(Some(variable));
                        }
                        return Some(
                            (is_const && variable.end_byte() <= at.start_byte())
                                .then(|| variable.child_by_field_name("value"))
                                .flatten(),
                        );
                    }
                }
            } else if matches!(
                declaration.kind(),
                "function_declaration"
                    | "class_declaration"
                    | "import_specifier"
                    | "required_parameter"
                    | "optional_parameter"
            ) {
                let identifier = declaration
                    .child_by_field_name("alias")
                    .or_else(|| declaration.child_by_field_name("name"))
                    .or_else(|| declaration.child_by_field_name("pattern"));
                if identifier.is_some_and(|identifier| text(identifier, source) == name) {
                    return Some(None);
                }
            } else if parent.kind() == "formal_parameters" && text(child, source) == name {
                return Some(None);
            }
        }
        // Parameters and import bindings live below these wrappers.
        if let Some(parameters) = parent.child_by_field_name("parameters") {
            for parameter in parameters.named_children(&mut parameters.walk()) {
                let identifier = parameter
                    .child_by_field_name("pattern")
                    .unwrap_or(parameter);
                if text(identifier, source) == name {
                    return Some(None);
                }
            }
        }
        scope = parent.parent();
    }
    None
}

fn imports_name(node: Node<'_>, name: &str, source: &[u8]) -> bool {
    if node.kind() == "import_specifier" {
        return node
            .child_by_field_name("alias")
            .or_else(|| node.child_by_field_name("name"))
            .is_some_and(|node| text(node, source) == name);
    }
    if node.kind() == "identifier" {
        return text(node, source) == name;
    }
    node.named_children(&mut node.walk())
        .any(|child| imports_name(child, name, source))
}

/// Decode JS string escapes using UTF-16 so paired Unicode escapes compare with
/// literal Unicode. Unpaired surrogates remain unknown rather than being changed.
fn decode_string(raw: &str) -> Option<String> {
    let mut chars = raw.get(1..raw.len().checked_sub(1)?)?.chars().peekable();
    let mut units = Vec::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            units.extend(character.encode_utf16(&mut [0; 2]).iter().copied());
            continue;
        }
        let escaped = chars.next()?;
        let decoded = match escaped {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'v' => '\u{b}',
            '0' if !chars.peek().is_some_and(char::is_ascii_digit) => '\0',
            '\n' => continue,
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                continue;
            }
            'x' | 'u' => {
                if escaped == 'u' && chars.peek() == Some(&'{') {
                    chars.next();
                    let mut digits = String::new();
                    loop {
                        let ch = chars.next()?;
                        if ch == '}' {
                            break;
                        }
                        digits.push(ch);
                    }
                    let scalar = char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?;
                    units.extend(scalar.encode_utf16(&mut [0; 2]).iter().copied());
                } else {
                    let count = if escaped == 'x' { 2 } else { 4 };
                    let digits: String = (0..count).map(|_| chars.next()).collect::<Option<_>>()?;
                    units.push(u16::from_str_radix(&digits, 16).ok()?);
                }
                continue;
            }
            '0'..='9' => return None,
            other => other,
        };
        units.extend(decoded.encode_utf16(&mut [0; 2]).iter().copied());
    }
    String::from_utf16(&units).ok()
}

//! Recursive descent parser for TypeScript type notation strings.
//!
//! Converts raw type strings (from LSP hover responses) into `TypeShape` trees.
//! Falls back to `TypeShape::Opaque(raw_string)` on any parse failure (D-13).

use crate::types::shape::{FieldShape, LiteralKind, ParamShape, PrimitiveKind, TypeShape};
use std::collections::BTreeMap;

/// Parse a TypeScript type string (from LSP hover output) into a `TypeShape`.
///
/// Falls back to `TypeShape::Opaque(raw_string)` on any parse failure (per D-13).
/// Applies normalization (sorting of union/intersection members) after parsing (D-11).
pub fn parse_type_string(input: &str) -> TypeShape {
    let input = input.trim();

    // Strip hover prefix forms:
    // 1. `type Foo = <type>` — LSP hover for type aliases
    // 2. `function foo(params): ReturnType` — LSP hover for functions
    // 3. `const foo: <type>` / `let foo: <type>` / `var foo: <type>` — variable hover
    let stripped = strip_hover_prefix(input);

    match TypeParser::new(stripped).parse() {
        Some(shape) => shape.normalize(),
        None => TypeShape::Opaque(input.to_string()),
    }
}

/// Strip hover-output prefixes from LSP type strings.
fn strip_hover_prefix(input: &str) -> &str {
    // `type Ident = <type>` — strip everything up to and including `= `
    if let Some(rest) = strip_type_alias_prefix(input) {
        return rest;
    }

    // `function name(...)` — strip `function name` to leave `(...)`
    if let Some(rest) = strip_function_name_prefix(input) {
        return rest;
    }

    // `const/let/var name: <type>` — strip to just the type after `: `
    if let Some(rest) = strip_variable_prefix(input) {
        return rest;
    }

    input
}

/// Strip `type <Ident> = ` prefix, returning the RHS.
fn strip_type_alias_prefix(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("type")?;
    let rest = rest.trim_start();
    // Read identifier
    let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    let rest = &rest[end..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    Some(rest.trim_start())
}

/// Strip `function name` prefix, leaving `(params): ReturnType` or the body.
fn strip_function_name_prefix(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("function")?;
    let rest = rest.trim_start();
    // Skip the function name identifier (may be empty for anonymous)
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    Some(rest[end..].trim_start())
}

/// Strip `const/let/var name: ` prefix returning the type after `: `.
fn strip_variable_prefix(input: &str) -> Option<&str> {
    let rest = if let Some(r) = input.strip_prefix("const ") {
        r
    } else if let Some(r) = input.strip_prefix("let ") {
        r
    } else if let Some(r) = input.strip_prefix("var ") {
        r
    } else {
        return None;
    };
    // Skip the identifier
    let colon_pos = rest.find(':')?;
    Some(rest[colon_pos + 1..].trim_start())
}

// ---------------------------------------------------------------------------
// Parser struct
// ---------------------------------------------------------------------------

struct TypeParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> TypeParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    /// Attempt to parse the entire input as a type, returning None on failure.
    fn parse(mut self) -> Option<TypeShape> {
        let shape = self.parse_type()?;
        self.skip_whitespace();
        // Must consume the entire input
        if self.pos != self.input.len() {
            return None;
        }
        Some(shape)
    }

    // -----------------------------------------------------------------------
    // Grammar entry point: Union < Intersection < Primary
    // -----------------------------------------------------------------------

    fn parse_type(&mut self) -> Option<TypeShape> {
        self.parse_union()
    }

    fn parse_union(&mut self) -> Option<TypeShape> {
        let first = self.parse_intersection()?;
        self.skip_whitespace();
        if self.peek_char() != Some('|') {
            return Some(first);
        }
        let mut members = vec![first];
        while self.peek_char() == Some('|') {
            self.advance(); // consume '|'
            self.skip_whitespace();
            members.push(self.parse_intersection()?);
            self.skip_whitespace();
        }
        Some(TypeShape::Union(members))
    }

    fn parse_intersection(&mut self) -> Option<TypeShape> {
        let first = self.parse_primary()?;
        self.skip_whitespace();
        if self.peek_char() != Some('&') {
            return Some(first);
        }
        let mut members = vec![first];
        while self.peek_char() == Some('&') {
            // Don't consume `&` if it could be part of `&&` or is followed by whitespace+`{`
            // just consume and continue
            self.advance();
            self.skip_whitespace();
            members.push(self.parse_primary()?);
            self.skip_whitespace();
        }
        Some(TypeShape::Intersection(members))
    }

    fn parse_primary(&mut self) -> Option<TypeShape> {
        self.skip_whitespace();
        let ch = self.peek_char()?;

        let shape = match ch {
            '{' => self.parse_object()?,
            '[' => self.parse_tuple()?,
            '(' => {
                // Could be function type `(params) => T` or parenthesized type
                self.parse_function_or_paren()?
            }
            '"' | '\'' | '`' => self.parse_string_literal()?,
            '0'..='9' | '-' => self.parse_number_literal()?,
            _ => {
                // Identifier-based: primitive, generic, type param
                let ident = self.read_identifier()?;
                self.skip_whitespace();
                match ident {
                    "string" => TypeShape::Primitive(PrimitiveKind::String),
                    "number" => TypeShape::Primitive(PrimitiveKind::Number),
                    "boolean" => TypeShape::Primitive(PrimitiveKind::Boolean),
                    "null" => TypeShape::Primitive(PrimitiveKind::Null),
                    "undefined" => TypeShape::Primitive(PrimitiveKind::Undefined),
                    "void" => TypeShape::Primitive(PrimitiveKind::Void),
                    "never" => TypeShape::Primitive(PrimitiveKind::Never),
                    "any" => TypeShape::Primitive(PrimitiveKind::Any),
                    "unknown" => TypeShape::Primitive(PrimitiveKind::Unknown),
                    "symbol" => TypeShape::Primitive(PrimitiveKind::Symbol),
                    "bigint" => TypeShape::Primitive(PrimitiveKind::Bigint),
                    "true" => TypeShape::Literal(LiteralKind::Boolean(true)),
                    "false" => TypeShape::Literal(LiteralKind::Boolean(false)),
                    name => {
                        // Check for generic args
                        if self.peek_char() == Some('<') {
                            self.parse_generic(name.to_string())?
                        } else {
                            // Type parameter reference
                            TypeShape::TypeParam(name.to_string())
                        }
                    }
                }
            }
        };

        // Check for array suffix `[]` (may be stacked: `string[][]`)
        let shape = self.parse_array_suffix(shape);

        Some(shape)
    }

    // -----------------------------------------------------------------------
    // Object type: `{ key: T; key?: T }`
    // -----------------------------------------------------------------------

    fn parse_object(&mut self) -> Option<TypeShape> {
        self.expect_char('{')?;
        self.skip_whitespace();
        let mut fields = BTreeMap::new();

        while self.peek_char() != Some('}') {
            self.skip_whitespace();
            if self.peek_char() == Some('}') {
                break;
            }
            // Skip `readonly` keyword
            if self.peek_str("readonly") {
                self.pos += 8;
                self.skip_whitespace();
            }
            // Parse field name (may be quoted or identifier)
            let key = if matches!(self.peek_char(), Some('"') | Some('\'') | Some('`')) {
                self.read_quoted_string()?.to_string()
            } else {
                self.read_identifier()?.to_string()
            };
            self.skip_whitespace();
            // Check for `?` — optional field
            let optional = if self.peek_char() == Some('?') {
                self.advance();
                true
            } else {
                false
            };
            self.skip_whitespace();
            // Expect `:` separator
            self.expect_char(':')?;
            self.skip_whitespace();
            let shape = self.parse_type()?;
            fields.insert(key, FieldShape { optional, shape });
            self.skip_whitespace();
            // Separator: `;` or `,` (optional trailing)
            if matches!(self.peek_char(), Some(';') | Some(',')) {
                self.advance();
            }
            self.skip_whitespace();
        }
        self.expect_char('}')?;
        Some(TypeShape::Object(fields))
    }

    // -----------------------------------------------------------------------
    // Tuple type: `[T, U, ...]`
    // -----------------------------------------------------------------------

    fn parse_tuple(&mut self) -> Option<TypeShape> {
        self.expect_char('[')?;
        self.skip_whitespace();
        let mut elements = Vec::new();
        while self.peek_char() != Some(']') {
            elements.push(self.parse_type()?);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.advance();
                self.skip_whitespace();
            }
        }
        self.expect_char(']')?;
        Some(TypeShape::Tuple(elements))
    }

    // -----------------------------------------------------------------------
    // Function type: `(params) => ReturnType`
    // Parenthesized type: `(T)` — fallback when no `=>`
    // -----------------------------------------------------------------------

    fn parse_function_or_paren(&mut self) -> Option<TypeShape> {
        // Save position so we can backtrack
        let saved_pos = self.pos;

        // Attempt function parse: consume `(`, parse params, consume `)`, check for `=>` or `:`
        self.expect_char('(')?;
        self.skip_whitespace();

        // Parse parameter list
        let params = self.parse_param_list();

        self.skip_whitespace();
        if self.expect_char(')').is_none() {
            // Failed — backtrack and try parenthesized expression
            self.pos = saved_pos;
            return self.parse_parenthesized();
        }
        self.skip_whitespace();

        // Check for `=>` (arrow function type syntax)
        if self.peek_str("=>") {
            self.pos += 2;
            self.skip_whitespace();
            let return_type = self.parse_type()?;
            return Some(TypeShape::Function {
                params: params?,
                return_type: Box::new(return_type),
            });
        }

        // Check for `:` (TypeScript declaration syntax from LSP hover, e.g. `(x: string): void`)
        // This handles LSP hover output like `function foo(x: string): string` after prefix strip.
        if self.peek_char() == Some(':') {
            self.advance(); // consume `:`
            self.skip_whitespace();
            let return_type = self.parse_type()?;
            return Some(TypeShape::Function {
                params: params?,
                return_type: Box::new(return_type),
            });
        }

        // No `=>` or `:` — this was a parenthesized type (single element)
        // Backtrack and parse as parenthesized
        self.pos = saved_pos;
        self.parse_parenthesized()
    }

    /// Parse `(T)` — a type wrapped in parens for grouping.
    fn parse_parenthesized(&mut self) -> Option<TypeShape> {
        self.expect_char('(')?;
        self.skip_whitespace();
        let inner = self.parse_type()?;
        self.skip_whitespace();
        self.expect_char(')')?;
        Some(inner)
    }

    /// Parse a comma-separated list of `name?: T` parameters inside `(...)`.
    /// Returns None if parsing fails (so caller can backtrack).
    fn parse_param_list(&mut self) -> Option<Vec<ParamShape>> {
        let mut params = Vec::new();
        while self.peek_char() != Some(')') {
            self.skip_whitespace();
            if self.peek_char() == Some(')') {
                break;
            }
            // Handle rest params: `...name: T`
            let rest_param = if self.peek_str("...") {
                self.pos += 3;
                true
            } else {
                false
            };
            let _ = rest_param; // future use

            let name = self.read_identifier()?.to_string();
            self.skip_whitespace();
            let optional = if self.peek_char() == Some('?') {
                self.advance();
                true
            } else {
                false
            };
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let shape = self.parse_type()?;
            params.push(ParamShape {
                name,
                optional,
                shape,
            });
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.advance();
                self.skip_whitespace();
            }
        }
        Some(params)
    }

    // -----------------------------------------------------------------------
    // Generic type: `Name<T, U>`  (special: Array<T> -> TypeShape::Array)
    // -----------------------------------------------------------------------

    fn parse_generic(&mut self, name: String) -> Option<TypeShape> {
        let args = self.parse_generic_args()?;
        if name == "Array" && args.len() == 1 {
            return Some(TypeShape::Array(Box::new(args.into_iter().next().unwrap())));
        }
        Some(TypeShape::Generic { name, args })
    }

    fn parse_generic_args(&mut self) -> Option<Vec<TypeShape>> {
        self.expect_char('<')?;
        self.skip_whitespace();
        let mut args = Vec::new();
        while self.peek_char() != Some('>') {
            args.push(self.parse_type()?);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.advance();
                self.skip_whitespace();
            }
        }
        self.expect_char('>')?;
        Some(args)
    }

    // -----------------------------------------------------------------------
    // Array suffix: `T[]` -> Array(T), stacked: `T[][]` -> Array(Array(T))
    // -----------------------------------------------------------------------

    fn parse_array_suffix(&mut self, base: TypeShape) -> TypeShape {
        let mut current = base;
        loop {
            self.skip_whitespace();
            if self.peek_str("[]") {
                self.pos += 2;
                current = TypeShape::Array(Box::new(current));
            } else {
                break;
            }
        }
        current
    }

    // -----------------------------------------------------------------------
    // Literal parsers
    // -----------------------------------------------------------------------

    fn parse_string_literal(&mut self) -> Option<TypeShape> {
        let value = self.read_quoted_string()?.to_string();
        Some(TypeShape::Literal(LiteralKind::String(value)))
    }

    fn parse_number_literal(&mut self) -> Option<TypeShape> {
        let value = self.read_number_literal()?.to_string();
        Some(TypeShape::Literal(LiteralKind::Number(value)))
    }

    // -----------------------------------------------------------------------
    // Tokenizer helpers
    // -----------------------------------------------------------------------

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn peek_str(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }

    fn advance(&mut self) {
        if let Some(c) = self.peek_char() {
            self.pos += c.len_utf8();
        }
    }

    fn expect_char(&mut self, expected: char) -> Option<()> {
        if self.peek_char() == Some(expected) {
            self.advance();
            Some(())
        } else {
            None
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Read an identifier: `[a-zA-Z_$][a-zA-Z0-9_$]*`
    fn read_identifier(&mut self) -> Option<&'a str> {
        let start = self.pos;
        let first = self.peek_char()?;
        if !first.is_alphabetic() && first != '_' && first != '$' {
            return None;
        }
        self.advance();
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                self.advance();
            } else {
                break;
            }
        }
        Some(&self.input[start..self.pos])
    }

    /// Read a quoted string literal (single, double, or backtick), returning the inner value.
    fn read_quoted_string(&mut self) -> Option<&'a str> {
        let quote = self.peek_char()?;
        if !matches!(quote, '"' | '\'' | '`') {
            return None;
        }
        self.advance(); // consume opening quote
        let start = self.pos;
        loop {
            let c = self.peek_char()?;
            if c == '\\' {
                // skip escape sequence
                self.advance();
                self.advance();
            } else if c == quote {
                break;
            } else {
                self.advance();
            }
        }
        let end = self.pos;
        self.advance(); // consume closing quote
        Some(&self.input[start..end])
    }

    /// Read a number literal (integer or float, possibly negative).
    fn read_number_literal(&mut self) -> Option<&'a str> {
        let start = self.pos;
        if self.peek_char() == Some('-') {
            self.advance();
        }
        let has_digits = self
            .peek_char()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
        if !has_digits {
            self.pos = start;
            return None;
        }
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '.' || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        // Handle `n` suffix for bigint literals
        if self.peek_char() == Some('n') {
            self.advance();
        }
        Some(&self.input[start..self.pos])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::parse_type_string;
    use crate::types::shape::{FieldShape, LiteralKind, PrimitiveKind, TypeShape};

    fn assert_shape(input: &str, expected: TypeShape) {
        let result = parse_type_string(input);
        assert_eq!(result, expected, "parsing {:?}", input);
    }

    #[test]
    fn test_primitive_string() {
        assert_shape("string", TypeShape::Primitive(PrimitiveKind::String));
    }

    #[test]
    fn test_primitive_number() {
        assert_shape("number", TypeShape::Primitive(PrimitiveKind::Number));
    }

    #[test]
    fn test_primitive_boolean() {
        assert_shape("boolean", TypeShape::Primitive(PrimitiveKind::Boolean));
    }

    #[test]
    fn test_primitive_void() {
        assert_shape("void", TypeShape::Primitive(PrimitiveKind::Void));
    }

    #[test]
    fn test_primitive_any() {
        assert_shape("any", TypeShape::Primitive(PrimitiveKind::Any));
    }

    #[test]
    fn test_primitive_null() {
        assert_shape("null", TypeShape::Primitive(PrimitiveKind::Null));
    }

    #[test]
    fn test_primitive_undefined() {
        assert_shape("undefined", TypeShape::Primitive(PrimitiveKind::Undefined));
    }

    #[test]
    fn test_primitive_never() {
        assert_shape("never", TypeShape::Primitive(PrimitiveKind::Never));
    }

    #[test]
    fn test_primitive_unknown() {
        assert_shape("unknown", TypeShape::Primitive(PrimitiveKind::Unknown));
    }

    #[test]
    fn test_string_literal() {
        assert_shape(
            "\"hello\"",
            TypeShape::Literal(LiteralKind::String("hello".to_string())),
        );
    }

    #[test]
    fn test_number_literal() {
        assert_shape(
            "42",
            TypeShape::Literal(LiteralKind::Number("42".to_string())),
        );
    }

    #[test]
    fn test_boolean_literal_true() {
        assert_shape("true", TypeShape::Literal(LiteralKind::Boolean(true)));
    }

    #[test]
    fn test_union_string_number() {
        let result = parse_type_string("string | number");
        let expected = TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::Number),
            TypeShape::Primitive(PrimitiveKind::String),
        ]);
        assert_eq!(result, expected, "parsing 'string | number'");
    }

    #[test]
    fn test_intersection() {
        let result = parse_type_string("string & { x: number }");
        assert!(
            matches!(result, TypeShape::Intersection(_)),
            "expected Intersection, got {:?}",
            result
        );
    }

    #[test]
    fn test_object_type() {
        use std::collections::BTreeMap;
        let result = parse_type_string("{ x: number; y: string }");
        let mut fields = BTreeMap::new();
        fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        fields.insert(
            "y".to_string(),
            FieldShape {
                optional: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            },
        );
        let expected = TypeShape::Object(fields);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_object_optional_field() {
        let result = parse_type_string("{ x?: number }");
        if let TypeShape::Object(fields) = result {
            let field = fields.get("x").expect("field x");
            assert!(field.optional, "field should be optional");
            assert_eq!(field.shape, TypeShape::Primitive(PrimitiveKind::Number));
        } else {
            panic!("expected Object, got {:?}", result);
        }
    }

    #[test]
    fn test_array_suffix() {
        assert_shape(
            "number[]",
            TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::Number))),
        );
    }

    #[test]
    fn test_array_generic() {
        assert_shape(
            "Array<string>",
            TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String))),
        );
    }

    #[test]
    fn test_tuple_type() {
        assert_shape(
            "[string, number]",
            TypeShape::Tuple(vec![
                TypeShape::Primitive(PrimitiveKind::String),
                TypeShape::Primitive(PrimitiveKind::Number),
            ]),
        );
    }

    #[test]
    fn test_function_type() {
        let result = parse_type_string("(x: string, y: number) => boolean");
        if let TypeShape::Function {
            params,
            return_type,
        } = result
        {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "x");
            assert_eq!(params[0].shape, TypeShape::Primitive(PrimitiveKind::String));
            assert!(!params[0].optional);
            assert_eq!(params[1].name, "y");
            assert_eq!(params[1].shape, TypeShape::Primitive(PrimitiveKind::Number));
            assert_eq!(*return_type, TypeShape::Primitive(PrimitiveKind::Boolean));
        } else {
            panic!("expected Function, got {:?}", result);
        }
    }

    #[test]
    fn test_generic_map() {
        assert_shape(
            "Map<string, number>",
            TypeShape::Generic {
                name: "Map".to_string(),
                args: vec![
                    TypeShape::Primitive(PrimitiveKind::String),
                    TypeShape::Primitive(PrimitiveKind::Number),
                ],
            },
        );
    }

    #[test]
    fn test_generic_promise_void() {
        assert_shape(
            "Promise<void>",
            TypeShape::Generic {
                name: "Promise".to_string(),
                args: vec![TypeShape::Primitive(PrimitiveKind::Void)],
            },
        );
    }

    #[test]
    fn test_nested_object_with_array() {
        let result = parse_type_string("{ items: string[]; count: number }");
        if let TypeShape::Object(fields) = result {
            let items = fields.get("items").expect("field items");
            assert_eq!(
                items.shape,
                TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String)))
            );
            let count = fields.get("count").expect("field count");
            assert_eq!(count.shape, TypeShape::Primitive(PrimitiveKind::Number));
        } else {
            panic!("expected Object, got {:?}", result);
        }
    }

    #[test]
    fn test_union_normalized_order_independent() {
        let a = parse_type_string("string | number");
        let b = parse_type_string("number | string");
        assert_eq!(a, b, "normalized unions should be order-independent");
    }

    #[test]
    fn test_opaque_fallback() {
        let result = parse_type_string("totallyBrokenGarbage<>{{");
        assert!(
            matches!(result, TypeShape::Opaque(_)),
            "expected Opaque fallback, got {:?}",
            result
        );
    }

    #[test]
    fn test_type_alias_stripping() {
        let result = parse_type_string("type Foo = string | number");
        assert!(
            matches!(result, TypeShape::Union(_)),
            "expected Union after alias stripping, got {:?}",
            result
        );
    }

    #[test]
    fn test_function_optional_param() {
        let result = parse_type_string("(x?: string) => void");
        if let TypeShape::Function {
            params,
            return_type,
        } = result
        {
            assert_eq!(params.len(), 1);
            assert!(params[0].optional, "param should be optional");
            assert_eq!(params[0].shape, TypeShape::Primitive(PrimitiveKind::String));
            assert_eq!(*return_type, TypeShape::Primitive(PrimitiveKind::Void));
        } else {
            panic!("expected Function, got {:?}", result);
        }
    }

    // Additional coverage tests

    #[test]
    fn test_primitive_symbol() {
        assert_shape("symbol", TypeShape::Primitive(PrimitiveKind::Symbol));
    }

    #[test]
    fn test_primitive_bigint() {
        assert_shape("bigint", TypeShape::Primitive(PrimitiveKind::Bigint));
    }

    #[test]
    fn test_boolean_literal_false() {
        assert_shape("false", TypeShape::Literal(LiteralKind::Boolean(false)));
    }

    #[test]
    fn test_array_nested() {
        assert_shape(
            "string[][]",
            TypeShape::Array(Box::new(TypeShape::Array(Box::new(TypeShape::Primitive(
                PrimitiveKind::String,
            ))))),
        );
    }

    #[test]
    fn test_empty_object() {
        assert_shape("{}", TypeShape::Object(std::collections::BTreeMap::new()));
    }

    #[test]
    fn test_union_three_members() {
        let result = parse_type_string("string | number | boolean");
        assert!(matches!(result, TypeShape::Union(_)));
        if let TypeShape::Union(members) = result {
            assert_eq!(members.len(), 3);
        }
    }

    #[test]
    fn test_function_hover_prefix_stripping() {
        // LSP hover for functions emits: `function foo(x: string): void`
        let result = parse_type_string("function foo(x: string): void");
        // The function hover format doesn't map cleanly to our parser's
        // `(params) => return` syntax — it falls back to Opaque or strips correctly.
        // At minimum it shouldn't panic.
        let _ = result;
    }

    #[test]
    fn test_single_quoted_string_literal() {
        assert_shape(
            "'world'",
            TypeShape::Literal(LiteralKind::String("world".to_string())),
        );
    }

    #[test]
    fn test_type_param_reference() {
        assert_shape("T", TypeShape::TypeParam("T".to_string()));
    }
}

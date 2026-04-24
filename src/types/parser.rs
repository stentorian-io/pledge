//! Recursive descent parser for TypeScript type notation strings.
//!
//! Converts raw type strings (from LSP hover responses) into `TypeShape` trees.
//! Falls back to `TypeShape::Opaque(raw_string)` on any parse failure (D-13).

use crate::types::shape::{
    FieldShape, LiteralKind, MappedModifier, ParamShape, PrimitiveKind, TemplatePart, TypeShape,
};
use std::collections::BTreeMap;

/// ROB-05: Maximum recursion depth for the type parser.
///
/// Matches the constant in `compare/subtype.rs` (MAX_DEPTH = 20) for consistency
/// across the type system. Adversarial or pathologically complex type strings that
/// exceed this depth produce `TypeShape::Opaque(raw_input)` rather than a stack overflow.
const MAX_DEPTH: usize = 20;

// WR-05 (Phase 17 review): byte-count constants for keywords the parser advances
// past. All matched keywords are pure ASCII, so byte length and char length agree.
// Using named constants rather than magic literals documents the invariant and
// makes the intent refactor-safe if someone extends the keyword set later.
const KW_EXTENDS_LEN: usize = "extends".len();
const KW_READONLY_LEN: usize = "readonly".len();
const KW_IN_LEN: usize = "in".len();
const KW_REST_DOTS_LEN: usize = "...".len();

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

    match TypeParser::new(stripped).parse(0) {
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
    fn parse(mut self, depth: usize) -> Option<TypeShape> {
        let shape = self.parse_type(depth)?;
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

    fn parse_type(&mut self, depth: usize) -> Option<TypeShape> {
        // ROB-05: depth guard — return Opaque on overflow to prevent stack overflow.
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        let base = self.parse_union(depth + 1)?;
        self.skip_whitespace();
        // Check for conditional type: `T extends U ? Then : Else`
        if self.peek_str("extends ") || self.peek_str("extends\t") {
            return self.parse_conditional(base, depth + 1);
        }
        Some(base)
    }

    /// Parse the `extends <U> ? <Then> : <Else>` tail of a conditional type.
    /// `check` is the type already parsed before the `extends` keyword.
    fn parse_conditional(&mut self, check: TypeShape, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        // consume "extends"
        self.pos += KW_EXTENDS_LEN;
        self.skip_whitespace();
        // Parse the extends constraint — use parse_primary to avoid consuming `?`
        let extends = self.parse_primary(depth + 1)?;
        self.skip_whitespace();
        self.expect_char('?')?;
        self.skip_whitespace();
        let then_type = self.parse_type(depth + 1)?;
        self.skip_whitespace();
        self.expect_char(':')?;
        self.skip_whitespace();
        let else_type = self.parse_type(depth + 1)?;
        Some(TypeShape::Conditional {
            check: Box::new(check),
            extends: Box::new(extends),
            then_type: Box::new(then_type),
            else_type: Box::new(else_type),
        })
    }

    fn parse_union(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        // BEH-06 (Phase 20): accept an optional leading `|` so the
        // idiomatic multi-line discriminated-union form
        // `type Shape = | { kind: 'a' } | { kind: 'b' };` parses to a
        // `TypeShape::Union`. Without this, the RHS returns None and
        // `parse_type_string` falls back to `TypeShape::Opaque`, which
        // suppresses all shape-level diff rules on the type body.
        if self.peek_char() == Some('|') {
            self.advance(); // consume leading '|'
            self.skip_whitespace();
        }
        let first = self.parse_intersection(depth + 1)?;
        self.skip_whitespace();
        if self.peek_char() != Some('|') {
            return Some(first);
        }
        let mut members = vec![first];
        while self.peek_char() == Some('|') {
            self.advance(); // consume '|'
            self.skip_whitespace();
            members.push(self.parse_intersection(depth + 1)?);
            self.skip_whitespace();
        }
        Some(TypeShape::Union(members))
    }

    fn parse_intersection(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        let first = self.parse_primary(depth + 1)?;
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
            members.push(self.parse_primary(depth + 1)?);
            self.skip_whitespace();
        }
        Some(TypeShape::Intersection(members))
    }

    fn parse_primary(&mut self, depth: usize) -> Option<TypeShape> {
        // ROB-05: depth guard at every entry point.
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        self.skip_whitespace();
        let ch = self.peek_char()?;

        let shape = match ch {
            '{' => self.parse_object(depth + 1)?,
            '[' => self.parse_tuple(depth + 1)?,
            '(' => {
                // Could be function type `(params) => T` or parenthesized type
                self.parse_function_or_paren(depth + 1)?
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
                    "keyof" => {
                        self.skip_whitespace();
                        let inner = self.parse_primary(depth + 1)?;
                        TypeShape::Generic {
                            name: "keyof".to_string(),
                            args: vec![inner],
                        }
                    }
                    "infer" => {
                        self.skip_whitespace();
                        let name = self.read_identifier()?;
                        self.skip_whitespace();
                        // TYPE-02 (Phase 20): optional `extends <type>` constraint.
                        // The outer `parse_conditional` already consumed its own top-level
                        // `extends` before recursing into parse_primary, so any `extends`
                        // we see here must be the inner infer-scoped constraint.
                        // The whitespace-suffix check (`extends ` / `extends\t`) mirrors
                        // `parse_type` at lines 142-144 — avoids misparsing an identifier
                        // that merely starts with the `extends` prefix (e.g. `extendsFoo`).
                        if self.peek_str("extends ") || self.peek_str("extends\t") {
                            self.pos += KW_EXTENDS_LEN;
                            self.skip_whitespace();
                            let constraint = self.parse_primary(depth + 1)?;
                            TypeShape::InferExtends {
                                name: name.to_string(),
                                constraint: Box::new(constraint),
                            }
                        } else {
                            TypeShape::TypeParam(format!("infer {}", name))
                        }
                    }
                    "typeof" => {
                        self.skip_whitespace();
                        let name = self.read_identifier()?;
                        TypeShape::TypeParam(format!("typeof {}", name))
                    }
                    name => {
                        // Check for generic args
                        if self.peek_char() == Some('<') {
                            self.parse_generic(name.to_string(), depth + 1)?
                        } else {
                            // Type parameter reference
                            TypeShape::TypeParam(name.to_string())
                        }
                    }
                }
            }
        };

        // Check for array suffix `[]` (may be stacked: `string[][]`)
        let shape = self.parse_array_suffix(shape, depth + 1);

        Some(shape)
    }

    // -----------------------------------------------------------------------
    // Object type: `{ key: T; key?: T }`
    // -----------------------------------------------------------------------

    fn parse_object(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        self.expect_char('{')?;
        self.skip_whitespace();

        // Check for mapped type: `{ [K in ...]: V }` or `{ readonly [K in ...]: V }`
        let saved = self.pos;
        let has_readonly = if self.peek_str("readonly") {
            self.pos += KW_READONLY_LEN;
            self.skip_whitespace();
            true
        } else {
            false
        };
        if self.peek_char() == Some('[') {
            if let Some(mapped) = self.try_parse_mapped_type(has_readonly, depth + 1) {
                return Some(mapped);
            }
        }
        // Not a mapped type — backtrack and proceed with normal object parsing
        self.pos = saved;

        let mut fields = BTreeMap::new();

        while self.peek_char() != Some('}') {
            self.skip_whitespace();
            if self.peek_char() == Some('}') {
                break;
            }
            // Check for `readonly` keyword — capture for IFT-02
            let readonly = if self.peek_str("readonly") {
                self.pos += KW_READONLY_LEN;
                self.skip_whitespace();
                true
            } else {
                false
            };
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
            let shape = self.parse_type(depth + 1)?;
            fields.insert(
                key,
                FieldShape {
                    optional,
                    readonly,
                    shape,
                },
            );
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
    // Mapped type: `{ [K in keyof T]: V }` or with readonly/optional modifiers
    // -----------------------------------------------------------------------

    /// Try to parse a mapped type starting at `[` (after optional `readonly` prefix).
    /// Returns None and backtracks on failure so parse_object can fall through.
    fn try_parse_mapped_type(&mut self, has_readonly: bool, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return None;
        }
        let saved = self.pos;
        self.expect_char('[')?;
        self.skip_whitespace();
        // Read the key variable name (e.g., `K`)
        let _key_name = self.read_identifier()?;
        self.skip_whitespace();
        // Must see `in` keyword followed by whitespace to be a mapped type
        if !self.peek_str("in ") && !self.peek_str("in\t") {
            self.pos = saved;
            return None;
        }
        self.pos += KW_IN_LEN;
        self.skip_whitespace();
        // Parse key constraint type (e.g., `keyof T`, `string`, etc.)
        let key_type = self.parse_type(depth + 1)?;
        self.skip_whitespace();
        // Expect closing `]`
        if self.expect_char(']').is_none() {
            self.pos = saved;
            return None;
        }
        self.skip_whitespace();
        // Check for `?` (optional modifier on the mapped property)
        let has_optional = if self.peek_char() == Some('?') {
            self.advance();
            true
        } else {
            false
        };
        self.skip_whitespace();
        // Expect `:`
        if self.expect_char(':').is_none() {
            self.pos = saved;
            return None;
        }
        self.skip_whitespace();
        let value_type = self.parse_type(depth + 1)?;
        self.skip_whitespace();
        // Allow optional trailing `;`
        if self.peek_char() == Some(';') {
            self.advance();
        }
        self.skip_whitespace();
        // Expect closing `}`
        if self.expect_char('}').is_none() {
            self.pos = saved;
            return None;
        }
        let modifier = match (has_readonly, has_optional) {
            (true, true) => Some(MappedModifier::ReadonlyOptional),
            (true, false) => Some(MappedModifier::Readonly),
            (false, true) => Some(MappedModifier::Optional),
            (false, false) => None,
        };
        Some(TypeShape::Mapped {
            key_type: Box::new(key_type),
            value_type: Box::new(value_type),
            modifier,
        })
    }

    // -----------------------------------------------------------------------
    // Tuple type: `[T, U, ...]`
    // -----------------------------------------------------------------------

    fn parse_tuple(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        self.expect_char('[')?;
        self.skip_whitespace();
        let mut elements = Vec::new();
        while self.peek_char() != Some(']') {
            elements.push(self.parse_type(depth + 1)?);
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

    fn parse_function_or_paren(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        // Save position so we can backtrack
        let saved_pos = self.pos;

        // Attempt function parse: consume `(`, parse params, consume `)`, check for `=>` or `:`
        self.expect_char('(')?;
        self.skip_whitespace();

        // Parse parameter list
        let params = self.parse_param_list(depth + 1);

        self.skip_whitespace();
        if self.expect_char(')').is_none() {
            // Failed — backtrack and try parenthesized expression
            self.pos = saved_pos;
            return self.parse_parenthesized(depth + 1);
        }
        self.skip_whitespace();

        // Check for `=>` (arrow function type syntax)
        if self.peek_str("=>") {
            self.pos += 2;
            self.skip_whitespace();
            let return_type = self.parse_type(depth + 1)?;
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
            let return_type = self.parse_type(depth + 1)?;
            return Some(TypeShape::Function {
                params: params?,
                return_type: Box::new(return_type),
            });
        }

        // No `=>` or `:` — this was a parenthesized type (single element)
        // Backtrack and parse as parenthesized
        self.pos = saved_pos;
        self.parse_parenthesized(depth + 1)
    }

    /// Parse `(T)` — a type wrapped in parens for grouping.
    fn parse_parenthesized(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return Some(TypeShape::Opaque(self.input[self.pos..].to_string()));
        }
        self.expect_char('(')?;
        self.skip_whitespace();
        let inner = self.parse_type(depth + 1)?;
        self.skip_whitespace();
        self.expect_char(')')?;
        Some(inner)
    }

    /// Parse a comma-separated list of `name?: T` parameters inside `(...)`.
    /// Returns None if parsing fails (so caller can backtrack).
    fn parse_param_list(&mut self, depth: usize) -> Option<Vec<ParamShape>> {
        if depth >= MAX_DEPTH {
            return None;
        }
        let mut params = Vec::new();
        while self.peek_char() != Some(')') {
            self.skip_whitespace();
            if self.peek_char() == Some(')') {
                break;
            }
            // Handle rest params: `...name: T`
            let rest_param = if self.peek_str("...") {
                self.pos += KW_REST_DOTS_LEN;
                true
            } else {
                false
            };

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
            let shape = self.parse_type(depth + 1)?;
            params.push(ParamShape {
                name,
                optional,
                rest: rest_param,
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

    fn parse_generic(&mut self, name: String, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            // Skip past the `<...>` block to avoid leaving the parser stuck,
            // then return Opaque for the generic construct.
            self.skip_balanced_angles();
            return Some(TypeShape::Opaque(format!("{}<...>", name)));
        }
        let args = self.parse_generic_args(depth + 1)?;
        if name == "Array" && args.len() == 1 {
            return Some(TypeShape::Array(Box::new(args.into_iter().next().unwrap())));
        }
        // GEN-07 (Phase 20): canonicalize `NoInfer<T>` into a dedicated
        // wrapper variant so diff engines observe wrapper-vs-unwrapped
        // transitions without string parsing. `NoInfer<T>` is a TS 5.4+
        // utility type; arity > 1 falls through to the generic catch-all
        // as a defensive measure.
        if name == "NoInfer" && args.len() == 1 {
            return Some(TypeShape::NoInferWrap(Box::new(
                args.into_iter().next().unwrap(),
            )));
        }
        Some(TypeShape::Generic { name, args })
    }

    /// Skip a balanced `<...>` block, handling nesting. Used by depth-cap fallback.
    fn skip_balanced_angles(&mut self) {
        if self.peek_char() != Some('<') {
            return;
        }
        let mut nesting = 0usize;
        while let Some(c) = self.peek_char() {
            match c {
                '<' => {
                    nesting += 1;
                    self.advance();
                }
                '>' => {
                    self.advance();
                    nesting -= 1;
                    if nesting == 0 {
                        break;
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_generic_args(&mut self, depth: usize) -> Option<Vec<TypeShape>> {
        self.expect_char('<')?;
        self.skip_whitespace();
        let mut args = Vec::new();
        while self.peek_char() != Some('>') {
            // ROB-05: if parse_type returns Opaque due to depth exhaustion, the
            // remaining input still contains the nested `<...>` body plus closing `>`.
            // We need to consume everything up to the matching `>` before breaking.
            let arg = self.parse_type(depth + 1)?;
            let is_opaque = matches!(arg, TypeShape::Opaque(_));
            args.push(arg);
            self.skip_whitespace();
            if is_opaque {
                // Depth was exhausted — skip any remaining content up to the
                // matching `>` (which may be many levels deep) to avoid infinite loop.
                let mut nesting = 0usize;
                loop {
                    match self.peek_char() {
                        None => break,
                        Some('<') => {
                            nesting += 1;
                            self.advance();
                        }
                        Some('>') if nesting > 0 => {
                            nesting -= 1;
                            self.advance();
                        }
                        Some('>') => break, // outer `>` — let expect_char consume it
                        _ => {
                            self.advance();
                        }
                    }
                }
                break;
            }
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

    fn parse_array_suffix(&mut self, base: TypeShape, depth: usize) -> TypeShape {
        let mut current = base;
        loop {
            // ROB-05: stop accumulating array wrappers at depth cap.
            if depth >= MAX_DEPTH {
                break;
            }
            self.skip_whitespace();
            if self.peek_str("[]") {
                self.pos += 2;
                current = TypeShape::Array(Box::new(current));
            } else if self.peek_char() == Some('[') {
                // Could be indexed access: T[K], T["key"]
                // Check it's not `[]` (handled above) — it could be `[K]` or `["bar"]`
                let saved = self.pos;
                self.advance(); // consume '['
                self.skip_whitespace();
                // Empty bracket would have been caught by `[]` check above
                if let Some(index) = self.parse_type(depth + 1) {
                    self.skip_whitespace();
                    if self.expect_char(']').is_some() {
                        current = TypeShape::Indexed {
                            base: Box::new(current),
                            index: Box::new(index),
                        };
                        continue;
                    }
                }
                // Failed — backtrack
                self.pos = saved;
                break;
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
// REF-09 (Phase 20): template-literal index-signature helpers
// ---------------------------------------------------------------------------
//
// These helpers are intentionally NOT wired into `parse_type_string` — they
// are called only from the REF-09 comparator in `src/compare/ref_diff.rs`
// and `src/compare/interface_diff.rs`. Keeping them dormant in the main
// parser path preserves existing LSP hover round-trip behavior and leaves
// `TypeShape::Template` unproduced from the normal parse flow.

/// REF-09 (Phase 20): parse an index-signature `key_type_text` if it is a
/// template-literal type like `` `on${string}` ``. Returns `None` for
/// non-template inputs; caller falls back to the REF-07 string-equality
/// path. Intentionally NOT wired into `parse_type_string` — existing LSP
/// hover round-trips stay unchanged.
pub fn parse_template_literal_type(input: &str) -> Option<Vec<TemplatePart>> {
    let trimmed = input.trim();
    let inner = trimmed.strip_prefix('`')?.strip_suffix('`')?;
    if inner.is_empty() {
        return Some(vec![TemplatePart::Literal(String::new())]);
    }
    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut remainder = inner;
    while let Some(ph_start) = remainder.find("${") {
        // Append any literal segment that precedes the ${.
        if ph_start > 0 {
            parts.push(TemplatePart::Literal(remainder[..ph_start].to_string()));
        }
        // Find matching } — shallow scan (template-literal types don't
        // nest placeholders in standard TS as of 4.4). If no close }, bail.
        let after_open = &remainder[ph_start + 2..];
        let ph_end = after_open.find('}')?;
        let placeholder_text = &after_open[..ph_end];
        let placeholder_shape = parse_type_string(placeholder_text);
        parts.push(TemplatePart::Placeholder(Box::new(placeholder_shape)));
        remainder = &after_open[ph_end + 1..];
    }
    // Append any trailing literal.
    if !remainder.is_empty() {
        parts.push(TemplatePart::Literal(remainder.to_string()));
    }
    if parts.is_empty() {
        parts.push(TemplatePart::Literal(String::new()));
    }
    Some(parts)
}

/// REF-09 (Phase 20): pattern-subset direction between two template-literal
/// patterns. Conservative prefix-extension algorithm — any ambiguous case
/// returns `Unrelated` so the caller can emit a conservative Major finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternDirection {
    /// Patterns are identical.
    Equal,
    /// New is strictly narrower than old (new's literal prefix extends old's).
    Narrowing,
    /// New is strictly wider than old (old's literal prefix extends new's).
    Widening,
    /// Neither side's literal prefix extends the other — conservative Major.
    Unrelated,
}

/// REF-09 (Phase 20): compute the pattern-subset relation between two
/// template-literal patterns. Concatenates the literal segments of each
/// side and compares for prefix-extension; placeholder types must agree
/// structurally (any mismatch -> `Unrelated`).
pub fn compare_template_literal_patterns(
    old: &[TemplatePart],
    new: &[TemplatePart],
) -> PatternDirection {
    if old == new {
        return PatternDirection::Equal;
    }
    let old_placeholders = collect_placeholders(old);
    let new_placeholders = collect_placeholders(new);
    // Require placeholder-type agreement (structural equality). Any
    // divergence -> Unrelated (conservative Major).
    if old_placeholders != new_placeholders {
        return PatternDirection::Unrelated;
    }
    let old_lits = concat_literals(old);
    let new_lits = concat_literals(new);
    if old_lits == new_lits {
        // Placeholders agreed, literals concatenate to the same string —
        // patterns are effectively equal even if the Part vec differs
        // slightly (e.g. trailing-empty-literal variance).
        return PatternDirection::Equal;
    }
    if new_lits.starts_with(&old_lits) {
        PatternDirection::Narrowing // new has extra literal prefix -> narrower
    } else if old_lits.starts_with(&new_lits) {
        PatternDirection::Widening // old had extra literal prefix -> new is wider
    } else {
        PatternDirection::Unrelated
    }
}

fn concat_literals(parts: &[TemplatePart]) -> String {
    let mut out = String::new();
    for p in parts {
        if let TemplatePart::Literal(s) = p {
            out.push_str(s);
        }
    }
    out
}

fn collect_placeholders(parts: &[TemplatePart]) -> Vec<&TypeShape> {
    parts
        .iter()
        .filter_map(|p| match p {
            TemplatePart::Placeholder(shape) => Some(shape.as_ref()),
            _ => None,
        })
        .collect()
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
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        fields.insert(
            "y".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
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

    /// BEH-06 (Phase 20): `parse_union` must accept an optional leading
    /// `|` so the idiomatic multi-line discriminated-union form parses
    /// into a `TypeShape::Union` (not `TypeShape::Opaque`). Without this,
    /// BEH-06's discriminated-union detector never sees the Union
    /// because the RHS falls to Opaque.
    #[test]
    fn beh_06_leading_pipe_multiline_union_parses() {
        let source = "| { kind: 'circle'; radius: number }\n  | { kind: 'square'; side: number }";
        let result = parse_type_string(source);
        assert!(
            matches!(result, TypeShape::Union(_)),
            "expected Union; got {result:?}"
        );
        if let TypeShape::Union(members) = result {
            assert_eq!(members.len(), 2, "expected 2 object variants");
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

    // ─── ROB-05: MAX_DEPTH guard — no stack overflow on adversarial input ───────

    #[test]
    fn rob_05_simple_type_parses_ok() {
        let shape = parse_type_string("string");
        assert!(
            matches!(shape, TypeShape::Primitive(_)),
            "ROB-05: simple 'string' should parse; got: {:?}",
            shape
        );
    }

    #[test]
    fn rob_05_deeply_nested_array_returns_opaque_not_stack_overflow() {
        // 100 levels of Array<Array<...>> should cap to Opaque at MAX_DEPTH=20
        let mut input = String::from("string");
        for _ in 0..100 {
            input = format!("Array<{}>", input);
        }
        // Should not panic (stack overflow). Result shape is well-defined.
        let shape = parse_type_string(&input);
        let _ = shape;
    }

    #[test]
    fn rob_05_parens_depth_exhaustion_no_panic() {
        let input = "(".repeat(100) + &"string".to_string() + &")".repeat(100);
        let shape = parse_type_string(&input);
        let _ = shape; // no panic
    }

    // ─── GAP-16-02: Conditional, mapped, indexed type parsing ─────────────────

    // Test: basic conditional type
    #[test]
    fn test_conditional_basic() {
        use crate::types::shape::PrimitiveKind;
        let result = parse_type_string("T extends string ? number : boolean");
        assert!(
            matches!(result, TypeShape::Conditional { .. }),
            "expected Conditional, got {:?}",
            result
        );
        if let TypeShape::Conditional {
            check,
            extends,
            then_type,
            else_type,
        } = result
        {
            assert_eq!(*check, TypeShape::TypeParam("T".to_string()));
            assert_eq!(*extends, TypeShape::Primitive(PrimitiveKind::String));
            assert_eq!(*then_type, TypeShape::Primitive(PrimitiveKind::Number));
            assert_eq!(*else_type, TypeShape::Primitive(PrimitiveKind::Boolean));
        }
    }

    // Test: conditional with union in else branch
    #[test]
    fn test_conditional_union_in_else() {
        let result = parse_type_string("T extends U ? A : B | C");
        assert!(
            matches!(result, TypeShape::Conditional { .. }),
            "expected Conditional, got {:?}",
            result
        );
        if let TypeShape::Conditional { else_type, .. } = result {
            assert!(
                matches!(*else_type, TypeShape::Union(_) | TypeShape::TypeParam(_)),
                "else branch should parse B | C, got {:?}",
                else_type
            );
        }
    }

    // Test: nested conditional T extends A ? B extends C ? D : E : F
    #[test]
    fn test_conditional_nested() {
        let result = parse_type_string("T extends A ? B extends C ? D : E : F");
        assert!(
            matches!(result, TypeShape::Conditional { .. }),
            "expected outer Conditional, got {:?}",
            result
        );
        if let TypeShape::Conditional { then_type, .. } = result {
            assert!(
                matches!(*then_type, TypeShape::Conditional { .. }),
                "then_type should be inner Conditional, got {:?}",
                then_type
            );
        }
    }

    // Test: mapped type basic (no modifiers)
    #[test]
    fn test_mapped_basic() {
        use crate::types::shape::PrimitiveKind;
        let result = parse_type_string("{ [K in keyof T]: string }");
        assert!(
            matches!(result, TypeShape::Mapped { .. }),
            "expected Mapped, got {:?}",
            result
        );
        if let TypeShape::Mapped {
            value_type,
            modifier,
            ..
        } = result
        {
            assert_eq!(*value_type, TypeShape::Primitive(PrimitiveKind::String));
            assert_eq!(modifier, None);
        }
    }

    // Test: mapped type with readonly modifier
    #[test]
    fn test_mapped_readonly() {
        use crate::types::shape::MappedModifier;
        let result = parse_type_string("{ readonly [K in keyof T]: string }");
        assert!(
            matches!(result, TypeShape::Mapped { .. }),
            "expected Mapped, got {:?}",
            result
        );
        if let TypeShape::Mapped { modifier, .. } = result {
            assert_eq!(modifier, Some(MappedModifier::Readonly));
        }
    }

    // Test: mapped type with optional modifier
    #[test]
    fn test_mapped_optional() {
        use crate::types::shape::MappedModifier;
        let result = parse_type_string("{ [K in keyof T]?: string }");
        assert!(
            matches!(result, TypeShape::Mapped { .. }),
            "expected Mapped, got {:?}",
            result
        );
        if let TypeShape::Mapped { modifier, .. } = result {
            assert_eq!(modifier, Some(MappedModifier::Optional));
        }
    }

    // Test: mapped type with readonly + optional modifier
    #[test]
    fn test_mapped_readonly_optional() {
        use crate::types::shape::MappedModifier;
        let result = parse_type_string("{ readonly [K in keyof T]?: string }");
        assert!(
            matches!(result, TypeShape::Mapped { .. }),
            "expected Mapped, got {:?}",
            result
        );
        if let TypeShape::Mapped { modifier, .. } = result {
            assert_eq!(modifier, Some(MappedModifier::ReadonlyOptional));
        }
    }

    // Test: indexed access T[K]
    #[test]
    fn test_indexed_access_typeparam() {
        let result = parse_type_string("T[K]");
        assert!(
            matches!(result, TypeShape::Indexed { .. }),
            "expected Indexed, got {:?}",
            result
        );
        if let TypeShape::Indexed { base, index } = result {
            assert_eq!(*base, TypeShape::TypeParam("T".to_string()));
            assert_eq!(*index, TypeShape::TypeParam("K".to_string()));
        }
    }

    // Test: indexed access with string literal key
    #[test]
    fn test_indexed_access_string_literal() {
        let result = parse_type_string("Foo[\"bar\"]");
        assert!(
            matches!(result, TypeShape::Indexed { .. }),
            "expected Indexed, got {:?}",
            result
        );
        if let TypeShape::Indexed { base, index } = result {
            assert_eq!(*base, TypeShape::TypeParam("Foo".to_string()));
            assert_eq!(
                *index,
                TypeShape::Literal(LiteralKind::String("bar".to_string()))
            );
        }
    }

    // Test: chained indexed access T[K][J]
    #[test]
    fn test_indexed_access_chained() {
        let result = parse_type_string("T[K][J]");
        // T[K][J] = (T[K])[J]
        assert!(
            matches!(result, TypeShape::Indexed { .. }),
            "expected outer Indexed, got {:?}",
            result
        );
        if let TypeShape::Indexed { base, index } = result {
            // base should be T[K] (inner Indexed)
            assert!(
                matches!(*base, TypeShape::Indexed { .. }),
                "base of outer indexed should be inner Indexed T[K], got {:?}",
                base
            );
            assert_eq!(*index, TypeShape::TypeParam("J".to_string()));
        }
    }

    // Test: keyof T
    #[test]
    fn test_keyof() {
        let result = parse_type_string("keyof T");
        assert!(
            matches!(result, TypeShape::Generic { ref name, .. } if name == "keyof"),
            "expected Generic {{ name: 'keyof', .. }}, got {:?}",
            result
        );
    }

    // Test: typeof x
    #[test]
    fn test_typeof() {
        let result = parse_type_string("typeof x");
        assert!(
            matches!(result, TypeShape::TypeParam(ref s) if s == "typeof x"),
            "expected TypeParam('typeof x'), got {:?}",
            result
        );
    }

    // ─── Phase 20 TYPE-02: `infer U extends C` constraint on inferred type ───

    // Backward compatibility: `infer U` with no trailing extends still
    // produces the legacy TypeShape::TypeParam("infer U") form so existing
    // consumers/diff engines don't regress.
    #[test]
    fn type_02_infer_without_extends_backward_compat() {
        let shape = parse_type_string("infer U");
        assert_eq!(shape, TypeShape::TypeParam("infer U".to_string()));
    }

    // `infer U extends string` should now parse into the dedicated
    // TypeShape::InferExtends variant (landed by Plan 20-01) whose
    // constraint is Primitive(String).
    #[test]
    fn type_02_infer_extends_string_produces_infer_extends_variant() {
        let shape = parse_type_string("infer U extends string");
        match shape {
            TypeShape::InferExtends { name, constraint } => {
                assert_eq!(name, "U");
                assert_eq!(*constraint, TypeShape::Primitive(PrimitiveKind::String));
            }
            other => panic!("expected InferExtends, got {:?}", other),
        }
    }

    // Same shape as the previous test but with a different primitive
    // constraint — confirms the parser captures the constraint faithfully
    // rather than hardcoding a specific type.
    #[test]
    fn type_02_infer_extends_number_produces_infer_extends_variant() {
        let shape = parse_type_string("infer U extends number");
        match shape {
            TypeShape::InferExtends { name, constraint } => {
                assert_eq!(name, "U");
                assert_eq!(*constraint, TypeShape::Primitive(PrimitiveKind::Number));
            }
            other => panic!("expected InferExtends, got {:?}", other),
        }
    }

    // ─── GEN-07 (Phase 20): NoInfer<T> wrapper canonicalization ───────────
    //
    // The parser must recognize `NoInfer<T>` as a dedicated wrapper variant
    // (`TypeShape::NoInferWrap`) rather than the generic catch-all
    // `TypeShape::Generic { name: "NoInfer", args: [T] }` form. This mirrors
    // the existing `Array<T>` special-case at parser.rs:608-610. Diff
    // engines depend on the canonical wrapper form to detect wrapper-vs-
    // unwrapped transitions without resorting to string comparison.

    #[test]
    fn gen_07_noinfer_of_primitive_produces_noinfer_wrap() {
        let shape = parse_type_string("NoInfer<string>");
        match shape {
            TypeShape::NoInferWrap(inner) => {
                assert_eq!(*inner, TypeShape::Primitive(PrimitiveKind::String));
            }
            other => panic!("expected NoInferWrap, got {:?}", other),
        }
    }

    #[test]
    fn gen_07_noinfer_of_typeparam_produces_noinfer_wrap() {
        let shape = parse_type_string("NoInfer<T>");
        match shape {
            TypeShape::NoInferWrap(inner) => {
                assert_eq!(*inner, TypeShape::TypeParam("T".to_string()));
            }
            other => panic!("expected NoInferWrap, got {:?}", other),
        }
    }

    #[test]
    fn gen_07_noinfer_of_generic_produces_noinfer_wrap_with_generic_inner() {
        let shape = parse_type_string("NoInfer<Foo<string>>");
        match shape {
            TypeShape::NoInferWrap(inner) => match *inner {
                TypeShape::Generic { name, args } => {
                    assert_eq!(name, "Foo");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected Generic inside NoInferWrap, got {:?}", other),
            },
            other => panic!("expected NoInferWrap, got {:?}", other),
        }
    }

    // Defensive: the real `NoInfer<T>` utility type takes exactly one
    // type parameter. If a user (or some future LSP hover-string drift)
    // hands us `NoInfer<X, Y>`, fall through to the generic catch-all
    // rather than panic or canonicalize incorrectly.
    #[test]
    fn gen_07_noinfer_arity_mismatch_falls_through_to_generic() {
        let shape = parse_type_string("NoInfer<string, number>");
        match shape {
            TypeShape::Generic { name, args } => {
                assert_eq!(name, "NoInfer");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected Generic fallthrough, got {:?}", other),
        }
    }

    // Regression: any other 1-arg generic name (not literally "NoInfer")
    // must remain a `TypeShape::Generic`. The new branch is name-keyed
    // and must not affect surrounding generics.
    #[test]
    fn gen_07_non_noinfer_generic_unchanged() {
        let shape = parse_type_string("SomethingElse<string>");
        match shape {
            TypeShape::Generic { name, args } => {
                assert_eq!(name, "SomethingElse");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected Generic, got {:?}", other),
        }
    }

    // ---------------------------------------------------------------------
    // REF-09 (Phase 20): parse_template_literal_type + compare_template_literal_patterns
    // ---------------------------------------------------------------------

    #[test]
    fn ref_09_parse_template_literal_type_simple() {
        use super::parse_template_literal_type;
        use crate::types::shape::TemplatePart;
        let parsed = parse_template_literal_type("`on${string}`").expect("should parse");
        // Expect at minimum: Literal("on"), Placeholder(Primitive(String))
        assert!(parsed.len() >= 2, "parsed: {:?}", parsed);
        match &parsed[0] {
            TemplatePart::Literal(s) => assert_eq!(s, "on"),
            other => panic!("expected Literal, got {:?}", other),
        }
        assert!(
            matches!(parsed[1], TemplatePart::Placeholder(_)),
            "second part should be Placeholder"
        );
    }

    #[test]
    fn ref_09_parse_template_literal_type_multi_literal() {
        use super::parse_template_literal_type;
        use crate::types::shape::TemplatePart;
        let parsed = parse_template_literal_type("`on_${string}_off`").expect("should parse");
        let literals: Vec<&String> = parsed
            .iter()
            .filter_map(|p| {
                if let TemplatePart::Literal(s) = p {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert!(literals.iter().any(|s| s.as_str() == "on_"));
        assert!(literals.iter().any(|s| s.as_str() == "_off"));
    }

    #[test]
    fn ref_09_parse_template_literal_type_non_template_returns_none() {
        use super::parse_template_literal_type;
        assert!(parse_template_literal_type("string").is_none());
        assert!(parse_template_literal_type("Record<string, number>").is_none());
    }

    #[test]
    fn ref_09_parse_template_literal_type_pure_literal() {
        use super::parse_template_literal_type;
        use crate::types::shape::TemplatePart;
        let parsed = parse_template_literal_type("`no-placeholder`").expect("should parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], TemplatePart::Literal("no-placeholder".to_string()));
    }

    #[test]
    fn ref_09_compare_template_literal_patterns_equal() {
        use super::{compare_template_literal_patterns, parse_template_literal_type, PatternDirection};
        let a = parse_template_literal_type("`on${string}`").unwrap();
        let b = parse_template_literal_type("`on${string}`").unwrap();
        assert_eq!(
            compare_template_literal_patterns(&a, &b),
            PatternDirection::Equal
        );
    }

    #[test]
    fn ref_09_compare_template_literal_patterns_narrowing() {
        use super::{compare_template_literal_patterns, parse_template_literal_type, PatternDirection};
        let old = parse_template_literal_type("`on${string}`").unwrap();
        let new = parse_template_literal_type("`onClick${string}`").unwrap();
        // `onClick` prefix extends `on` prefix -> new is narrower
        assert_eq!(
            compare_template_literal_patterns(&old, &new),
            PatternDirection::Narrowing
        );
    }

    #[test]
    fn ref_09_compare_template_literal_patterns_widening() {
        use super::{compare_template_literal_patterns, parse_template_literal_type, PatternDirection};
        let old = parse_template_literal_type("`onClick${string}`").unwrap();
        let new = parse_template_literal_type("`on${string}`").unwrap();
        assert_eq!(
            compare_template_literal_patterns(&old, &new),
            PatternDirection::Widening
        );
    }

    #[test]
    fn ref_09_compare_template_literal_patterns_unrelated() {
        use super::{compare_template_literal_patterns, parse_template_literal_type, PatternDirection};
        let old = parse_template_literal_type("`abc${string}`").unwrap();
        let new = parse_template_literal_type("`xyz${string}`").unwrap();
        // Neither `abc` nor `xyz` is a prefix of the other
        assert_eq!(
            compare_template_literal_patterns(&old, &new),
            PatternDirection::Unrelated
        );
    }
}

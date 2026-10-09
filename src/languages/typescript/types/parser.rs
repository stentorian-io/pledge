//! Recursive descent parser for TypeScript type notation strings.
//!
//! Converts raw type strings (from LSP hover responses) into `TypeShape` trees.
//! Falls back to `TypeShape::Opaque(raw_string)` on any parse failure.

use crate::languages::typescript::types::shape::{
    FieldShape, LiteralKind, ParamShape, PrimitiveKind, TemplatePart, TupleElement, TypeShape,
};
use std::collections::BTreeMap;

// Bound both recursive syntax and repeated/backtracking work. Grammar layers
// consume depth too, so the limit permits ordinary nested declarations.
const MAX_DEPTH: usize = 128;
const MAX_STEPS: usize = 8192;
const MAX_INPUT_BYTES: usize = 256 * 1024;

//: byte-count constants for keywords the parser advances
// past. All matched keywords are pure ASCII, so byte length and char length agree.
// Using named constants rather than magic literals documents the invariant and
// makes the intent refactor-safe if someone extends the keyword set later.
const KW_EXTENDS_LEN: usize = "extends".len();
const KW_READONLY_LEN: usize = "readonly".len();
const KW_IN_LEN: usize = "in".len();
const KW_REST_DOTS_LEN: usize = "...".len();

/// Parse a TypeScript type string (from LSP hover output) into a `TypeShape`.
///
/// Falls back to `TypeShape::Opaque(raw_string)` on any parse failure (per ).
/// Applies normalization (sorting of union/intersection members) after parsing.
///
pub fn parse_type_string(input: &str) -> TypeShape {
    let input = input.trim();
    if input.len() > MAX_INPUT_BYTES {
        return TypeShape::Opaque(input.to_owned());
    }

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
    } else {
        input.strip_prefix("var ")?
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
    remaining_steps: usize,
}

impl<'a> TypeParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            remaining_steps: MAX_STEPS,
        }
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
        // Abort the entire parse: a successful result must always consume input.
        if depth >= MAX_DEPTH {
            return None;
        }
        let base = self.parse_union(depth + 1)?;
        self.skip_whitespace();
        // Check for conditional type: `T extends U ? Then: Else`
        if self.peek_str("extends ") || self.peek_str("extends\t") {
            return self.parse_conditional(base, depth + 1);
        }
        Some(base)
    }

    /// Parse the `extends <U> ? <Then>: <Else>` tail of a conditional type.
    /// `check` is the type already parsed before the `extends` keyword.
    fn parse_conditional(&mut self, check: TypeShape, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return None;
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
            return None;
        }
        // BEH-06: accept an optional leading `|` so the
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
            return None;
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
        self.remaining_steps = self.remaining_steps.checked_sub(1)?;
        // ROB-05: depth guard at every entry point.
        if depth >= MAX_DEPTH {
            return None;
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
            '"' | '\'' => self.parse_string_literal()?,
            '`' => self.parse_template_literal(depth + 1)?,
            '0'..='9' | '-' => self.parse_number_literal()?,
            _ => {
                // Identifier-based: primitive, generic, type param
                let start = self.pos;
                self.read_identifier()?;
                while self.peek_char() == Some('.') && !self.peek_str("...") {
                    self.advance();
                    self.read_identifier()?;
                }
                let ident = &self.input[start..self.pos];
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
                        // TYPE-02: optional `extends <type>` constraint.
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
            return None;
        }
        self.expect_char('{')?;
        self.skip_whitespace();

        // Check for mapped type: `{ [K in...]: V }` or `{ readonly [K in...]: V }`
        let saved = self.pos;
        let polarity = if matches!(self.peek_char(), Some('-' | '+')) {
            let adds_modifier = self.peek_char() != Some('-');
            self.advance();
            adds_modifier
        } else {
            true
        };
        let readonly = if self.peek_str("readonly") {
            self.pos += KW_READONLY_LEN;
            self.skip_whitespace();
            Some(polarity)
        } else {
            None
        };
        if self.peek_char() == Some('[') {
            if let Some(mapped) = self.try_parse_mapped_type(readonly, depth + 1) {
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
                    write_type: None,
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
    fn try_parse_mapped_type(&mut self, readonly: Option<bool>, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return None;
        }
        let saved = self.pos;
        self.expect_char('[')?;
        self.skip_whitespace();
        // Read the key variable name (e.g., `K`)
        let key_name = self.read_identifier()?.to_string();
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
        let name_type = if self.peek_str("as ") || self.peek_str("as\t") {
            self.pos += 2;
            self.skip_whitespace();
            Some(Box::new(self.parse_type(depth + 1)?))
        } else {
            None
        };
        self.skip_whitespace();
        // Expect closing `]`
        if self.expect_char(']').is_none() {
            self.pos = saved;
            return None;
        }
        self.skip_whitespace();
        // Check for `?` (optional modifier on the mapped property)
        let polarity = if matches!(self.peek_char(), Some('-' | '+')) {
            let adds_modifier = self.peek_char() != Some('-');
            self.advance();
            adds_modifier
        } else {
            true
        };
        let optional = if self.peek_char() == Some('?') {
            self.advance();
            Some(polarity)
        } else {
            None
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
        Some(TypeShape::Mapped {
            key_name,
            key_type: Box::new(key_type),
            name_type,
            value_type: Box::new(value_type),
            readonly,
            optional,
        })
    }

    // -----------------------------------------------------------------------
    // Tuple type: `[T, U,...]`
    // -----------------------------------------------------------------------

    fn parse_tuple(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return None;
        }
        self.expect_char('[')?;
        self.skip_whitespace();
        let mut elements: Vec<TupleElement> = Vec::new();
        while self.peek_char() != Some(']') {
            // REF-10: recognize per-position label prefix
            //  `name:` → labeled element
            //  `name?:` → labeled optional element
            //  `...name:` → labeled rest element
            // Backtrack if the identifier is not followed by `:` (e.g.
            // `Map<string, number>` — `Map` is a type name, not a label).
            let outer_save = self.pos;
            let mut label: Option<String> = None;
            let mut optional = false;
            let mut rest = false;

            // Try `...name:` prefix for labeled rest element.
            if self.peek_str("...") {
                // Peek past the `...` to check for an identifier start.
                let after_dots = self.pos + KW_REST_DOTS_LEN;
                let starts_ident = self.input[after_dots..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$');
                if starts_ident {
                    self.pos += KW_REST_DOTS_LEN;
                    if let Some(name) = self.read_identifier() {
                        let name = name.to_string();
                        self.skip_whitespace();
                        if self.peek_char() == Some(':') {
                            self.advance();
                            label = Some(name);
                            rest = true;
                        } else {
                            // Not a labeled rest (e.g. `...T` spread in tuple
                            // without label). Rewind so the shape parser sees
                            // the full source and lets the existing generic /
                            // array machinery handle it.
                            self.pos = outer_save;
                        }
                    } else {
                        self.pos = outer_save;
                    }
                }
            }

            // Try `name:` or `name?:` prefix (only if we didn't consume a rest-label).
            if label.is_none() && !rest {
                let inner_save = self.pos;
                if let Some(name) = self.read_identifier() {
                    let name = name.to_string();
                    self.skip_whitespace();
                    let maybe_opt = self.peek_char() == Some('?');
                    if maybe_opt {
                        self.advance();
                        self.skip_whitespace();
                    }
                    if self.peek_char() == Some(':') {
                        self.advance();
                        label = Some(name);
                        optional = maybe_opt;
                    } else {
                        // Not a label (e.g. `Map<...>`, bare type reference
                        // `ID`). Rewind to let parse_type re-read the token
                        // as a type name.
                        self.pos = inner_save;
                    }
                }
            }

            self.skip_whitespace();
            if self.peek_str("...") {
                self.pos += KW_REST_DOTS_LEN;
                rest = true;
            }
            let start = self.pos;
            let shape = self.parse_type(depth + 1)?;
            if self.pos == start {
                return None;
            }
            self.skip_whitespace();
            if self.peek_char() == Some('?') {
                self.advance();
                optional = true;
            }
            elements.push(TupleElement {
                label,
                optional,
                rest,
                shape,
            });

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
            return None;
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
            return None;
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
    // Generic type: `Name<T, U>` (special: Array<T> -> TypeShape::Array)
    // -----------------------------------------------------------------------

    fn parse_generic(&mut self, name: String, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return None;
        }
        let args = self.parse_generic_args(depth + 1)?;
        if name == "Array" && args.len() == 1 {
            return Some(TypeShape::Array(Box::new(args.into_iter().next().unwrap())));
        }
        // GEN-07: canonicalize `NoInfer<T>` into a dedicated
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

    fn parse_generic_args(&mut self, depth: usize) -> Option<Vec<TypeShape>> {
        self.expect_char('<')?;
        self.skip_whitespace();
        let mut args = Vec::new();
        while self.peek_char() != Some('>') {
            let start = self.pos;
            args.push(self.parse_type(depth + 1)?);
            if self.pos == start {
                return None;
            }
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

    fn parse_array_suffix(&mut self, base: TypeShape, mut depth: usize) -> TypeShape {
        let mut current = base;
        loop {
            depth += 1;
            // Unconsumed suffixes make the enclosing parse fall back to Opaque.
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

    fn parse_template_literal(&mut self, depth: usize) -> Option<TypeShape> {
        if depth >= MAX_DEPTH {
            return None;
        }
        self.expect_char('`')?;
        let mut parts = Vec::new();
        let mut literal = String::new();
        loop {
            let ch = self.peek_char()?;
            if ch == '`' {
                self.advance();
                if parts.is_empty() {
                    return Some(TypeShape::Literal(LiteralKind::String(literal)));
                }
                parts.push(TemplatePart::Literal(literal));
                return Some(TypeShape::Template(parts));
            }
            if self.peek_str("${") {
                self.pos += 2;
                parts.push(TemplatePart::Literal(std::mem::take(&mut literal)));
                self.skip_whitespace();
                parts.push(TemplatePart::Placeholder(Box::new(
                    self.parse_type(depth + 1)?,
                )));
                self.skip_whitespace();
                self.expect_char('}')?;
            } else {
                literal.push(ch);
                self.advance();
                if ch == '\\' {
                    literal.push(self.peek_char()?);
                    self.advance();
                }
            }
        }
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
// REF-09: template-literal index-signature helpers
// ---------------------------------------------------------------------------
//
// These helpers are intentionally NOT wired into `parse_type_string` — they
// are called only from the REF-09 comparator in `src/languages/typescript/compare/refinements.rs`
// and `src/languages/typescript/compare/interfaces.rs`. Keeping them dormant in the main
// parser path preserves existing LSP hover round-trip behavior and leaves
// `TypeShape::Template` unproduced from the normal parse flow.

/// REF-09: parse an index-signature `key_type_text` if it is a
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

/// REF-09: pattern-subset direction between two template-literal
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

/// REF-09: compute the pattern-subset relation between two
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
        PatternDirection::Narrowing
    // new has extra literal prefix -> narrower
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
mod tests;

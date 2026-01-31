# Tree-sitter Query Patterns Reference

Quick reference for tree-sitter query patterns used in Interlint CLI. Each pattern includes the JavaScript/TypeScript syntax it matches and practical usage examples.

## Table of Contents

1. [Query Syntax Quick Reference](#query-syntax-quick-reference)
2. [Export Patterns](#export-patterns)
3. [Function Patterns](#function-patterns)
4. [Class Patterns](#class-patterns)
5. [Variable Patterns](#variable-patterns)
6. [TypeScript-Specific Patterns](#typescript-specific-patterns)
7. [JSX Patterns](#jsx-patterns)
8. [Practical Examples](#practical-examples)

---

## Query Syntax Quick Reference

```scheme
; Node type matching
(function_declaration)                    ; Match any function_declaration node

; Field matching
(function_declaration name: (identifier)) ; Match function with name field

; Captures (@ prefix)
(identifier) @name                        ; Capture node as "name"

; Predicates (# prefix)
(identifier) @const (#match? @const "^[A-Z_]+$")  ; Match uppercase identifiers

; Alternations ([...])
[(function_declaration) (arrow_function)] @func   ; Match either type

; Optional (?)
(type_annotation)? @type                  ; Optional type annotation

; Wildcards
(_) @any                                  ; Match any node type

; Negation (!)
(export_statement !declaration)           ; Export without declaration field

; Anchors
(export_statement . (export_clause))      ; export_clause must be first child
```

---

## Export Patterns

### Named Export - Function

**Matches:** `export function greet(name) { return name; }`

```scheme
(export_statement
  declaration: (function_declaration
    name: (identifier) @export.name
    parameters: (formal_parameters) @export.params)) @export.function
```

### Named Export - Class

**Matches:** `export class UserService { }`

```scheme
(export_statement
  declaration: (class_declaration
    name: (identifier) @export.name)) @export.class
```

### Named Export - Variable

**Matches:** `export const VERSION = '1.0.0'`

```scheme
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (identifier) @export.name
      value: (_)? @export.value))) @export.variable
```

### Default Export - Declaration

**Matches:** `export default function App() { }` or `export default class Service { }`

```scheme
(export_statement
  "default"
  declaration: [
    (function_declaration name: (identifier)? @export.default.name)
    (class_declaration name: (identifier)? @export.default.name)
  ]) @export.default
```

### Default Export - Expression

**Matches:** `export default { key: value }` or `export default someVariable`

```scheme
(export_statement
  "default"
  value: (_) @export.default.value) @export.default_expr
```

### Named Export List

**Matches:** `export { foo, bar as baz }`

```scheme
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @export.local
      alias: (identifier)? @export.exported))) @export.named_list
```

### Re-export From Module

**Matches:** `export { helper } from './utils'`

```scheme
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @export.name))
  source: (string) @export.source) @export.reexport
```

### Export All (Barrel)

**Matches:** `export * from './module'`

```scheme
(export_statement
  "*"
  source: (string) @export.source) @export.all
```

### Export Namespace

**Matches:** `export * as utils from './utils'`

```scheme
(export_statement
  (namespace_export
    (identifier) @export.namespace)
  source: (string) @export.source) @export.namespace
```

---

## Function Patterns

### Function Declaration

**Matches:** `function greet(name, age) { return name; }`

```scheme
(function_declaration
  name: (identifier) @func.name
  parameters: (formal_parameters
    (identifier)* @func.param)
  body: (statement_block) @func.body) @func.decl
```

### Async Function

**Matches:** `async function fetchData() { await api.get(); }`

```scheme
(function_declaration
  "async" @func.async
  name: (identifier) @func.name
  parameters: (formal_parameters) @func.params) @func.async_decl
```

### Generator Function

**Matches:** `function* range(start, end) { yield start; }`

```scheme
(generator_function_declaration
  name: (identifier) @func.name
  parameters: (formal_parameters) @func.params) @func.generator
```

### Arrow Function

**Matches:** `const add = (a, b) => a + b`

```scheme
(lexical_declaration
  (variable_declarator
    name: (identifier) @func.name
    value: (arrow_function
      parameters: [(formal_parameters) (identifier)]? @func.params
      body: (_) @func.body))) @func.arrow
```

### Async Arrow Function

**Matches:** `const fetch = async () => { await api.call(); }`

```scheme
(lexical_declaration
  (variable_declarator
    name: (identifier) @func.name
    value: (arrow_function
      "async" @func.async
      body: (_) @func.body))) @func.async_arrow
```

### Function Expression

**Matches:** `const greet = function(name) { return name; }`

```scheme
(lexical_declaration
  (variable_declarator
    name: (identifier) @func.name
    value: (function_expression
      parameters: (formal_parameters) @func.params))) @func.expression
```

### IIFE (Immediately Invoked)

**Matches:** `(function() { })()` or `(() => { })()`

```scheme
(call_expression
  function: (parenthesized_expression
    [(function_expression) (arrow_function)] @func.iife)) @func.iife_call
```

---

## Class Patterns

### Class Declaration

**Matches:** `class UserService { }`

```scheme
(class_declaration
  name: (identifier) @class.name
  body: (class_body) @class.body) @class.decl
```

### Class with Inheritance

**Matches:** `class Admin extends User { }`

```scheme
(class_declaration
  name: (identifier) @class.name
  (class_heritage
    (extends_clause
      value: (identifier) @class.extends))) @class.with_extends
```

### Class Method

**Matches:** `class { method() { } }`

```scheme
(method_definition
  name: (property_identifier) @method.name
  parameters: (formal_parameters) @method.params
  body: (statement_block) @method.body) @method.def
```

### Static Method

**Matches:** `class { static create() { } }`

```scheme
(method_definition
  "static"
  name: (property_identifier) @method.name) @method.static
```

### Getter/Setter

**Matches:** `class { get value() { } set value(v) { } }`

```scheme
; Getter
(method_definition
  "get"
  name: (property_identifier) @accessor.name) @accessor.getter

; Setter
(method_definition
  "set"
  name: (property_identifier) @accessor.name
  parameters: (formal_parameters) @accessor.params) @accessor.setter
```

### Constructor

**Matches:** `class { constructor(name) { this.name = name; } }`

```scheme
(method_definition
  name: (property_identifier) @method.name
  (#eq? @method.name "constructor")
  parameters: (formal_parameters) @ctor.params) @class.constructor
```

### Class Field

**Matches:** `class { count = 0; }`

```scheme
(field_definition
  property: (property_identifier) @field.name
  value: (_)? @field.value) @field.def
```

### Private Field (ES2022)

**Matches:** `class { #secret = 'hidden'; }`

```scheme
(field_definition
  property: (private_property_identifier) @field.private_name) @field.private
```

### Private Method

**Matches:** `class { #compute() { } }`

```scheme
(method_definition
  name: (private_property_identifier) @method.private_name) @method.private
```

---

## Variable Patterns

### Const Declaration

**Matches:** `const PI = 3.14159`

```scheme
(lexical_declaration
  kind: "const"
  (variable_declarator
    name: (identifier) @var.name
    value: (_)? @var.value)) @var.const
```

### Let Declaration

**Matches:** `let counter = 0`

```scheme
(lexical_declaration
  kind: "let"
  (variable_declarator
    name: (identifier) @var.name
    value: (_)? @var.value)) @var.let
```

### Var Declaration

**Matches:** `var legacy = 'old'`

```scheme
(variable_declaration
  (variable_declarator
    name: (identifier) @var.name
    value: (_)? @var.value)) @var.var
```

### Multiple Declarators

**Matches:** `const a = 1, b = 2, c = 3`

```scheme
(lexical_declaration
  (variable_declarator
    name: (identifier) @var.name)+) @var.multi
```

### Object Destructuring

**Matches:** `const { a, b: renamed, c = default } = obj`

```scheme
(lexical_declaration
  (variable_declarator
    name: (object_pattern
      [(shorthand_property_identifier_pattern) @var.destructured
       (pair_pattern
         key: (property_identifier) @var.key
         value: (identifier) @var.alias)]))) @var.destructure_obj
```

### Array Destructuring

**Matches:** `const [first, second, ...rest] = arr`

```scheme
(lexical_declaration
  (variable_declarator
    name: (array_pattern
      [(identifier) @var.element
       (rest_pattern (identifier) @var.rest)]))) @var.destructure_arr
```

---

## TypeScript-Specific Patterns

### Interface Declaration

**Matches:** `interface User { id: number; name: string; }`

```scheme
(interface_declaration
  name: (type_identifier) @interface.name
  body: (interface_body
    (property_signature
      name: (property_identifier) @prop.name
      type: (type_annotation (_) @prop.type))?)) @interface.decl
```

### Interface with Extends

**Matches:** `interface Admin extends User { role: string; }`

```scheme
(interface_declaration
  name: (type_identifier) @interface.name
  (extends_type_clause
    (type_identifier) @interface.extends)+) @interface.with_extends
```

### Interface Method Signature

**Matches:** `interface { greet(name: string): void; }`

```scheme
(method_signature
  name: (property_identifier) @method.name
  parameters: (formal_parameters
    (required_parameter
      pattern: (identifier) @param.name
      type: (type_annotation (_) @param.type))?)
  return_type: (type_annotation (_) @method.return)?) @interface.method
```

### Type Alias

**Matches:** `type Status = 'active' | 'inactive'`

```scheme
(type_alias_declaration
  name: (type_identifier) @type.name
  value: (_) @type.value) @type.alias
```

### Generic Type Alias

**Matches:** `type Result<T, E = Error> = Success<T> | Failure<E>`

```scheme
(type_alias_declaration
  name: (type_identifier) @type.name
  (type_parameters
    (type_parameter
      name: (type_identifier) @type.param
      constraint: (constraint (_) @type.constraint)?
      value: (default_type (_) @type.default)?)+)
  value: (_) @type.value) @type.generic_alias
```

### Enum Declaration

**Matches:** `enum HttpStatus { OK = 200, NotFound = 404 }`

```scheme
(enum_declaration
  name: (identifier) @enum.name
  body: (enum_body
    (enum_assignment
      name: (property_identifier) @member.name
      value: (_)? @member.value)*)) @enum.decl
```

### Const Enum

**Matches:** `const enum Direction { Up, Down, Left, Right }`

```scheme
(enum_declaration
  "const"
  name: (identifier) @enum.name) @enum.const
```

### Function with Types

**Matches:** `function greet(name: string): string { return name; }`

```scheme
(function_declaration
  name: (identifier) @func.name
  parameters: (formal_parameters
    (required_parameter
      pattern: (identifier) @param.name
      type: (type_annotation (_) @param.type))?)+
  return_type: (type_annotation (_) @func.return)?) @func.typed
```

### Class Implements

**Matches:** `class Service implements IService, IDisposable { }`

```scheme
(class_declaration
  name: (identifier) @class.name
  (class_heritage
    (implements_clause
      (type_identifier) @class.implements)+)) @class.with_implements
```

### Abstract Class

**Matches:** `abstract class Base { abstract method(): void; }`

```scheme
(abstract_class_declaration
  name: (identifier) @class.name
  body: (class_body
    (abstract_method_signature
      name: (property_identifier) @method.abstract)?)) @class.abstract
```

### Type Guard Function

**Matches:** `function isString(value: unknown): value is string { }`

```scheme
(function_declaration
  name: (identifier) @guard.name
  return_type: (type_annotation
    (type_predicate
      (identifier) @guard.param
      (type_identifier) @guard.type))) @func.type_guard
```

### Ambient Declaration

**Matches:** `declare function require(id: string): any;`

```scheme
(ambient_declaration
  "declare"
  [(function_signature name: (identifier) @ambient.func)
   (variable_declaration (variable_declarator name: (identifier) @ambient.var))
   (class_declaration name: (identifier) @ambient.class)]) @ambient.decl
```

---

## JSX Patterns

### JSX Self-Closing Element

**Matches:** `<Component prop="value" />`

```scheme
(jsx_self_closing_element
  name: [(identifier) (member_expression)] @jsx.component
  (jsx_attribute
    (property_identifier) @jsx.attr.name
    [(string) (jsx_expression)] @jsx.attr.value)?*) @jsx.self_closing
```

### JSX Element with Children

**Matches:** `<Parent><Child /></Parent>`

```scheme
(jsx_element
  open_tag: (jsx_opening_element
    name: (identifier) @jsx.tag)
  close_tag: (jsx_closing_element
    name: (identifier) @jsx.close_tag)) @jsx.element
```

### JSX Fragment

**Matches:** `<><Child /></>`

```scheme
(jsx_fragment) @jsx.fragment
```

### JSX Expression

**Matches:** `{condition && <Component />}`

```scheme
(jsx_expression
  (_) @jsx.expr) @jsx.expression
```

### JSX Spread Attributes

**Matches:** `<Component {...props} />`

```scheme
(jsx_self_closing_element
  (jsx_attribute
    (jsx_expression
      (spread_element (_) @jsx.spread)))) @jsx.with_spread
```

### Functional Component Pattern

**Matches:** `const Component = ({ prop }) => <div>{prop}</div>`

```scheme
(lexical_declaration
  (variable_declarator
    name: (identifier) @component.name
    value: (arrow_function
      parameters: (formal_parameters
        (object_pattern)? @component.props)
      body: [(jsx_element) (jsx_fragment) (parenthesized_expression (jsx_element))]))) @component.functional
```

---

## Practical Examples

### Extract All Exported Function Names

```rust
use tree_sitter::{Parser, Query, QueryCursor};

fn extract_exported_functions(source: &str) -> Vec<String> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_typescript::language_typescript()).unwrap();

    let tree = parser.parse(source, None).unwrap();

    let query = Query::new(
        &tree_sitter_typescript::language_typescript(),
        r#"
        (export_statement
          declaration: (function_declaration
            name: (identifier) @func_name))
        "#,
    ).unwrap();

    let mut cursor = QueryCursor::new();
    let mut functions = Vec::new();

    for match_ in cursor.matches(&query, tree.root_node(), source.as_bytes()) {
        for capture in match_.captures {
            if query.capture_names()[capture.index as usize] == "func_name" {
                functions.push(capture.node.utf8_text(source.as_bytes()).unwrap().to_string());
            }
        }
    }

    functions
}

#[test]
fn test_extract_functions() {
    let source = r#"
        export function greet(name: string): string { return name; }
        export function farewell(name: string): string { return `Bye ${name}`; }
        function internal() { }
    "#;

    let functions = extract_exported_functions(source);
    assert_eq!(functions, vec!["greet", "farewell"]);
}
```

### Extract Interface Properties

```rust
fn extract_interface_properties(source: &str, interface_name: &str) -> Vec<(String, String)> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_typescript::language_typescript()).unwrap();

    let tree = parser.parse(source, None).unwrap();

    let query = Query::new(
        &tree_sitter_typescript::language_typescript(),
        r#"
        (interface_declaration
          name: (type_identifier) @iface_name
          body: (interface_body
            (property_signature
              name: (property_identifier) @prop_name
              type: (type_annotation (_) @prop_type))))
        "#,
    ).unwrap();

    let mut cursor = QueryCursor::new();
    let mut properties = Vec::new();
    let mut current_interface = String::new();
    let mut current_prop = String::new();

    for match_ in cursor.matches(&query, tree.root_node(), source.as_bytes()) {
        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            let text = capture.node.utf8_text(source.as_bytes()).unwrap();

            match capture_name.as_str() {
                "iface_name" => current_interface = text.to_string(),
                "prop_name" => current_prop = text.to_string(),
                "prop_type" => {
                    if current_interface == interface_name {
                        properties.push((current_prop.clone(), text.to_string()));
                    }
                }
                _ => {}
            }
        }
    }

    properties
}

#[test]
fn test_extract_interface() {
    let source = r#"
        export interface User {
            id: number;
            name: string;
            email: string;
        }
    "#;

    let props = extract_interface_properties(source, "User");
    assert_eq!(props, vec![
        ("id".to_string(), "number".to_string()),
        ("name".to_string(), "string".to_string()),
        ("email".to_string(), "string".to_string()),
    ]);
}
```

### Detect Breaking Changes in Exports

```rust
struct ExportInfo {
    name: String,
    kind: String,  // "function", "class", "variable", etc.
    is_default: bool,
}

fn extract_exports(source: &str) -> Vec<ExportInfo> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_typescript::language_typescript()).unwrap();

    let tree = parser.parse(source, None).unwrap();

    let query = Query::new(
        &tree_sitter_typescript::language_typescript(),
        r#"
        ; Named function export
        (export_statement
          declaration: (function_declaration
            name: (identifier) @name)) @export_function

        ; Named class export
        (export_statement
          declaration: (class_declaration
            name: (identifier) @name)) @export_class

        ; Named variable export
        (export_statement
          declaration: (lexical_declaration
            (variable_declarator
              name: (identifier) @name))) @export_variable

        ; Default export
        (export_statement
          "default" @is_default
          [declaration: (_) value: (_)] @default_value)
        "#,
    ).unwrap();

    let mut cursor = QueryCursor::new();
    let mut exports = Vec::new();

    for match_ in cursor.matches(&query, tree.root_node(), source.as_bytes()) {
        let pattern_index = match_.pattern_index;
        let mut name = None;
        let mut is_default = false;

        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];

            match capture_name.as_str() {
                "name" => name = Some(capture.node.utf8_text(source.as_bytes()).unwrap().to_string()),
                "is_default" => is_default = true,
                _ => {}
            }
        }

        let kind = match pattern_index {
            0 => "function",
            1 => "class",
            2 => "variable",
            3 => "default",
            _ => "unknown",
        };

        if let Some(export_name) = name {
            exports.push(ExportInfo {
                name: export_name,
                kind: kind.to_string(),
                is_default,
            });
        } else if is_default {
            exports.push(ExportInfo {
                name: "default".to_string(),
                kind: "default".to_string(),
                is_default: true,
            });
        }
    }

    exports
}

fn find_removed_exports(old_source: &str, new_source: &str) -> Vec<String> {
    let old_exports: std::collections::HashSet<_> =
        extract_exports(old_source).into_iter().map(|e| e.name).collect();
    let new_exports: std::collections::HashSet<_> =
        extract_exports(new_source).into_iter().map(|e| e.name).collect();

    old_exports.difference(&new_exports).cloned().collect()
}
```

---

## Testing Queries

Use the tree-sitter CLI to test queries interactively:

```bash
# Install tree-sitter CLI
npm install -g tree-sitter-cli

# Parse a file and output the syntax tree
tree-sitter parse source.ts

# Test a query against a file
tree-sitter query queries/typescript.scm source.ts
```

Or use the online Tree-sitter Playground: https://tree-sitter.github.io/tree-sitter/playground

---

## Performance Tips

1. **Compile queries once**: Reuse `Query` objects across multiple parse operations
2. **Use specific patterns**: Avoid overly broad patterns like `(_) @any`
3. **Limit captures**: Only capture what you need
4. **Use predicates sparingly**: String matching predicates add overhead
5. **Batch operations**: Parse multiple related queries in one traversal when possible

```rust
// Good: Compile query once, reuse for multiple files
let query = Query::new(&language, EXPORT_QUERY).unwrap();
for source in sources {
    let tree = parser.parse(source, None).unwrap();
    // Use existing query
    cursor.matches(&query, tree.root_node(), source.as_bytes());
}

// Bad: Recompiling query for each file
for source in sources {
    let query = Query::new(&language, EXPORT_QUERY).unwrap(); // Wasteful!
    // ...
}
```

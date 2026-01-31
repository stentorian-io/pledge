# Interlint CLI Documentation

Documentation for the Interlint CLI parser and query system.

## Documents

| Document | Description |
|----------|-------------|
| [query-patterns-reference.md](./query-patterns-reference.md) | Tree-sitter query patterns for JS/TS |
| [parser-interface-design.md](./parser-interface-design.md) | Language-agnostic parser interface design |

## Parser Overview

The CLI uses tree-sitter for language-agnostic parsing, enabling support for multiple languages through a unified API.

### Architecture

```
src/parser/
├── mod.rs              # Language-agnostic Parser trait
├── types.rs            # Core types (Span, Visibility, etc.)
├── declarations.rs     # Declaration types (Function, Class, etc.)
├── exports.rs          # Export information
├── registry.rs         # Parser registry for multi-language
└── tree_sitter/
    ├── mod.rs          # TreeSitterParser implementation
    ├── languages.rs    # Language configuration
    └── queries/        # Query files per language
        ├── javascript.scm
        └── typescript.scm
```

### Key Concepts

**Tree-sitter Queries** - S-expression patterns to match AST nodes:

```scheme
; Match exported function declarations
(export_statement
  declaration: (function_declaration
    name: (identifier) @export.name)) @export.function
```

**Parser Trait** - Common interface for all language parsers:

```rust
pub trait Parser: Send + Sync {
    fn parse_file(&self, path: &Path) -> Result<ParseResult>;
    fn parse_source(&self, source: &str, filename: &str) -> Result<ParseResult>;
    fn language(&self) -> Language;
}
```

### Export Detection by Language

| Language | Export Mechanism |
|----------|------------------|
| JavaScript/TypeScript | `export` keyword |
| Python | `__all__` list or public names |
| Go | Capitalized identifiers |
| Rust | `pub` visibility |

## Resources

- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter Playground](https://tree-sitter.github.io/tree-sitter/playground)

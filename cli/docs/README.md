# Interlint CLI Documentation

Complete documentation for the Breakpoint CLI and Interlint system.

## Quick Start & Troubleshooting

| Document | Description |
|----------|-------------|
| [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) | Common issues, error messages, and solutions |
| [SERVER_REQUIREMENTS.md](./SERVER_REQUIREMENTS.md) | API server setup, deployment, and configuration |
| [BEST_PRACTICES.md](./BEST_PRACTICES.md) | Recommended workflows, CI/CD integration, and examples |

## Technical Documentation

| Document | Description |
|----------|-------------|
| [query-patterns-reference.md](./query-patterns-reference.md) | Tree-sitter query patterns for JS/TS |
| [parser-interface-design.md](./parser-interface-design.md) | Language-agnostic parser interface design |
| [package-manager-research.md](./package-manager-research.md) | Package manager integration details |

## Documentation Map

### For Users

**Starting Out?** Read these first:
1. [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues and solutions
2. [SERVER_REQUIREMENTS.md](./SERVER_REQUIREMENTS.md) - Getting the API server running
3. [BEST_PRACTICES.md](./BEST_PRACTICES.md) - Recommended workflows and CI/CD setup

### For Developers

**Technical Details:**
- [ERROR_HANDLING.md](./ERROR_HANDLING.md) - Comprehensive error reference
- [BINARY_FORMAT.md](./BINARY_FORMAT.md) - API communication format
- [query-patterns-reference.md](./query-patterns-reference.md) - Tree-sitter patterns
- [parser-interface-design.md](./parser-interface-design.md) - Parser architecture
- [package-manager-research.md](./package-manager-research.md) - Package manager details

## CLI Architecture

The CLI uses tree-sitter for language-agnostic parsing, enabling support for multiple languages through a unified API.

### Source Structure

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

## External Resources

- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter Playground](https://tree-sitter.github.io/tree-sitter/playground)
- [MessagePack Specification](https://github.com/msgpack/msgpack)

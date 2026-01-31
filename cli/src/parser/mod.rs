//! Language-agnostic parsing abstraction layer.
//!
//! This module provides traits and types for parsing source code in a
//! language-agnostic way, with concrete implementations using tree-sitter.

mod error;
mod location;
mod node;
mod query;
mod traits;
mod treesitter;

pub use error::{ParseError, ParseErrorKind};
pub use location::{SourceLocation, SourceRange};
pub use node::{NodeKind, ParsedNode};
pub use query::{NodeQuery, QueryMatch, QueryPattern};
pub use traits::{LanguageParser, ParseResult, ParserRegistry, SourceParser};
pub use treesitter::{create_js_ts_registry, JsParser, TsParser, TsxParser, TreeSitterParser};

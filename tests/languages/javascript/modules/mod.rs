//! Modules checks.

mod commonjs_aliases;
mod commonjs_declarations;
mod commonjs_exports;
mod default_aliases;
mod destructuring;
mod discovery;
#[cfg(feature = "lsp-tests")]
mod ecmascript_coverage;
mod export_bindings;
mod export_forms;
mod external_reexports;
mod forwarding;
#[cfg(feature = "lsp-tests")]
mod lexical_contracts;
mod nested_destructuring;
mod object_destructuring;
mod reexports;

mod object_helpers;
mod standard_exports;

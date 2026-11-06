//! Shared JavaScript and TypeScript grammar dispatch and JavaScript extraction.

pub mod classes;
pub(crate) mod commonjs;
pub mod exports;
pub mod functions;
pub mod grammar;
pub(crate) mod object_exports;
pub(crate) mod property_key;
pub(crate) mod queries;
pub(crate) mod runtime_surface;
pub(crate) mod static_initialization;

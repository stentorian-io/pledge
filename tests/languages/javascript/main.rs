//! javascript integration checks.

mod classes;
mod functions;
mod modules;
mod properties;
mod runtime;

#[cfg(feature = "lsp-tests")]
#[path = "../typescript/consumers/mod.rs"]
mod typescript_consumers;

#[cfg(feature = "lsp-tests")]
#[path = "../../ecosystems/node/consumers/support.rs"]
mod node_consumers;

#[path = "../../support/fixtures.rs"]
mod fixtures;

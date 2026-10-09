//! typescript integration checks.

mod classes;
mod compiler;
mod declarations;
mod documentation;
mod functions;
mod generics;
mod modules;
mod types;

#[cfg(feature = "lsp-tests")]
#[path = "../../ecosystems/node/consumers/support.rs"]
mod node_consumers;

#[path = "../../support/fixtures.rs"]
mod fixtures;

//! node integration checks.

mod assets;
mod bundlers;
mod consumers;
mod modules;
mod packages;
mod reference;

#[cfg(feature = "lsp-tests")]
#[path = "../../languages/typescript/consumers/mod.rs"]
mod typescript_consumers;

#[path = "../../support/fixtures.rs"]
mod fixtures;

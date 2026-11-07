//! Runtime checks.

mod contracts;
mod dispatch_limits;
mod resource_management;

#[cfg(feature = "lsp-tests")]
mod consumer_contracts;

mod test262_coverage;

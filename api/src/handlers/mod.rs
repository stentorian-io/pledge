//! HTTP request handlers for the interlint API.

pub mod compare;
pub mod health;

// Re-export handlers for convenience
pub use compare::compare;
pub use health::health_check;

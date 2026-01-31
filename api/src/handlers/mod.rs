//! HTTP request handlers for the interlint API.

pub mod compare;
pub mod health;
pub mod pkg_config;

// Re-export handlers for convenience
pub use compare::compare;
pub use health::health_check;
pub use pkg_config::compare_package_config;

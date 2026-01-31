//! Application state for the interlint API.

/// Application state shared across handlers.
///
/// This struct is cloned for each request handler, so keep it lightweight
/// or wrap heavy resources in `Arc`.
#[derive(Clone, Default)]
pub struct AppState {
    // Currently empty, but allows future expansion for:
    // - Database connections
    // - Cache clients
    // - Configuration
    // - Metrics collectors
}

impl AppState {
    /// Create a new application state.
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_new() {
        let _state = AppState::new();
    }

    #[test]
    fn test_app_state_clone() {
        let state = AppState::new();
        let _cloned = state.clone();
    }
}

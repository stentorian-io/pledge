//! Router setup for the interlint API.

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::handlers::{compare, compare_package_config, health_check};
use crate::state::AppState;

/// Create the application router with all routes and middleware.
pub fn create_router(state: AppState) -> Router {
    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router with routes and middleware
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/compare", post(compare))
        .route("/v1/compare-config", post(compare_package_config))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_router_health_endpoint() {
        let state = AppState::new();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_router_compare_endpoint_exists() {
        let state = AppState::new();
        let app = create_router(state);

        // Send empty binary body with correct content type - route exists but will fail deserialization
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", "application/octet-stream")
                    .body(Body::from(vec![0u8; 0]))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should be 400 (Bad Request) for invalid binary data, not 404
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_router_compare_rejects_json() {
        let state = AppState::new();
        let app = create_router(state);

        // JSON content type should be rejected with 415
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_router_unknown_endpoint_returns_404() {
        let state = AppState::new();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_router_compare_config_endpoint_exists() {
        let state = AppState::new();
        let app = create_router(state);

        // Send empty binary body with correct content type - route exists but will fail deserialization
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
                    .header("Content-Type", "application/octet-stream")
                    .body(Body::from(vec![0u8; 0]))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should be 400 (Bad Request) for invalid binary data, not 404
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_router_compare_config_rejects_json() {
        let state = AppState::new();
        let app = create_router(state);

        // JSON content type should be rejected with 415
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}

//! Compare handler for detecting breaking changes between public interfaces.
//!
//! This handler accepts ONLY binary MessagePack format. Text JSON is not supported.

use axum::{extract::State, http::StatusCode, response::Response};

use crate::binary::{binary_error, BinaryJson, BinaryJsonResponse};
use crate::diff::compare_interfaces;
use crate::response::{CompareRequest, CompareResponse};
use crate::state::AppState;

/// Compare two public interfaces and detect breaking changes.
///
/// POST /v1/compare
///
/// Request body: CompareRequest (Binary MessagePack)
/// - Content-Type: application/octet-stream or application/msgpack
/// - previous: PublicInterface - The baseline/previous version
/// - current: PublicInterface - The new/current version
/// - min_severity: Option<Severity> - Minimum severity to report
/// - include_additions: bool - Whether to include non-breaking additions
///
/// Response: CompareResponse (Binary MessagePack)
/// - Content-Type: application/octet-stream
/// - changes: Vec<BreakingChange> - List of detected changes
/// - summary: ComparisonSummary - Statistics about the changes
/// - previous_version: String - Version from previous metadata
/// - current_version: String - Version from current metadata
/// - package_name: String - Package name from metadata
///
/// Errors:
/// - 415 Unsupported Media Type: If Content-Type is not binary MessagePack
/// - 400 Bad Request: If request body is invalid
pub async fn compare(
    State(_state): State<AppState>,
    BinaryJson(request): BinaryJson<CompareRequest>,
) -> Result<BinaryJsonResponse<CompareResponse>, Response> {
    // Validate that package names match
    if request.previous.metadata.name != request.current.metadata.name {
        return Err(binary_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Package names do not match: '{}' vs '{}'",
                request.previous.metadata.name, request.current.metadata.name
            ),
            Some("PACKAGE_NAME_MISMATCH"),
        ));
    }

    // Perform comparison
    let mut changes = compare_interfaces(
        &request.previous,
        &request.current,
        request.include_additions,
    );

    // Filter by minimum severity if specified
    if let Some(min_severity) = request.min_severity {
        changes.retain(|c| c.severity.weight() >= min_severity.weight());
    }

    // Count exports
    let previous_export_count = request.previous.export_count();
    let current_export_count = request.current.export_count();

    // Build response
    let response = CompareResponse::new(
        changes,
        request.previous.metadata.version.clone(),
        request.current.metadata.version.clone(),
        request.previous.metadata.name.clone(),
        previous_export_count,
        current_export_count,
    );

    Ok(BinaryJsonResponse(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::{BINARY_CONTENT_TYPE, MSGPACK_CONTENT_TYPE};
    use axum::{body::Body, http::Request, routing::post, Router};
    use interlint_format::{
        Export, FunctionSig, Metadata, Module, PublicInterface, Signature, TypeInfo, Visibility,
    };
    use tower::ServiceExt;

    fn create_test_app() -> Router {
        let state = AppState::new();
        Router::new()
            .route("/v1/compare", post(compare))
            .with_state(state)
    }

    fn create_test_metadata(version: &str) -> Metadata {
        Metadata {
            name: "test-package".to_string(),
            version: version.to_string(),
            description: None,
            repository: None,
            license: None,
            language: Some("typescript".to_string()),
        }
    }

    #[tokio::test]
    async fn test_compare_no_changes_binary() {
        let app = create_test_app();

        let prev = PublicInterface::new(create_test_metadata("1.0.0"));
        let curr = PublicInterface::new(create_test_metadata("1.0.1"));

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

        // Serialize request as MessagePack
        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", BINARY_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            BINARY_CONTENT_TYPE
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: CompareResponse = rmp_serde::from_slice(&body).unwrap();

        assert!(result.changes.is_empty());
        assert!(!result.summary.has_breaking_changes);
    }

    #[tokio::test]
    async fn test_compare_with_breaking_change_binary() {
        let app = create_test_app();

        let mut prev = PublicInterface::new(create_test_metadata("1.0.0"));
        let mut prev_module = Module::new("index.ts");
        prev_module.add_export(Export {
            name: "foo".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "foo".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        prev.add_module(prev_module);

        let mut curr = PublicInterface::new(create_test_metadata("2.0.0"));
        curr.add_module(Module::new("index.ts"));

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", BINARY_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: CompareResponse = rmp_serde::from_slice(&body).unwrap();

        assert_eq!(result.changes.len(), 1);
        assert!(result.summary.has_breaking_changes);
        assert_eq!(result.summary.critical_count, 1);
    }

    #[tokio::test]
    async fn test_compare_package_name_mismatch_binary() {
        let app = create_test_app();

        let prev = PublicInterface::new(Metadata {
            name: "package-a".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        });

        let curr = PublicInterface::new(Metadata {
            name: "package-b".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        });

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", BINARY_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Error response is also binary MessagePack
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: crate::binary::BinaryErrorResponse = rmp_serde::from_slice(&body).unwrap();

        assert!(error.error.contains("Package names do not match"));
        assert_eq!(error.code, Some("PACKAGE_NAME_MISMATCH".to_string()));
    }

    #[tokio::test]
    async fn test_reject_json_request() {
        let app = create_test_app();

        let prev = PublicInterface::new(create_test_metadata("1.0.0"));
        let curr = PublicInterface::new(create_test_metadata("1.0.1"));

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

        // Try to send as JSON - should be rejected
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_accept_msgpack_content_type() {
        let app = create_test_app();

        let prev = PublicInterface::new(create_test_metadata("1.0.0"));
        let curr = PublicInterface::new(create_test_metadata("1.0.1"));

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .header("Content-Type", MSGPACK_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_reject_missing_content_type() {
        let app = create_test_app();

        let prev = PublicInterface::new(create_test_metadata("1.0.0"));
        let curr = PublicInterface::new(create_test_metadata("1.0.1"));

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        // No Content-Type header
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare")
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}

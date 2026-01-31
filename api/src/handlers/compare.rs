//! Compare handler for detecting breaking changes between public interfaces.

use axum::{extract::State, http::StatusCode, Json};

use crate::diff::compare_interfaces;
use crate::response::{CompareRequest, CompareResponse, ErrorResponse};
use crate::state::AppState;

/// Compare two public interfaces and detect breaking changes.
///
/// POST /v1/compare
///
/// Request body: CompareRequest (JSON)
/// - previous: PublicInterface - The baseline/previous version
/// - current: PublicInterface - The new/current version
/// - min_severity: Option<Severity> - Minimum severity to report
/// - include_additions: bool - Whether to include non-breaking additions
///
/// Response: CompareResponse (JSON)
/// - changes: Vec<BreakingChange> - List of detected changes
/// - summary: ComparisonSummary - Statistics about the changes
/// - previous_version: String - Version from previous metadata
/// - current_version: String - Version from current metadata
/// - package_name: String - Package name from metadata
pub async fn compare(
    State(_state): State<AppState>,
    Json(request): Json<CompareRequest>,
) -> Result<Json<CompareResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate that package names match
    if request.previous.metadata.name != request.current.metadata.name {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "Package names do not match: '{}' vs '{}'",
                    request.previous.metadata.name, request.current.metadata.name
                ))
                .with_code("PACKAGE_NAME_MISMATCH"),
            ),
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

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn test_compare_no_changes() {
        let app = create_test_app();

        let prev = PublicInterface::new(create_test_metadata("1.0.0"));
        let curr = PublicInterface::new(create_test_metadata("1.0.1"));

        let request = CompareRequest {
            previous: prev,
            current: curr,
            min_severity: None,
            include_additions: false,
        };

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

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: CompareResponse = serde_json::from_slice(&body).unwrap();

        assert!(result.changes.is_empty());
        assert!(!result.summary.has_breaking_changes);
    }

    #[tokio::test]
    async fn test_compare_with_breaking_change() {
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

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: CompareResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(result.changes.len(), 1);
        assert!(result.summary.has_breaking_changes);
        assert_eq!(result.summary.critical_count, 1);
    }

    #[tokio::test]
    async fn test_compare_package_name_mismatch() {
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

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("Package names do not match"));
        assert_eq!(error.code, Some("PACKAGE_NAME_MISMATCH".to_string()));
    }
}

//! Package configuration comparison handler.
//!
//! This handler compares package.json configurations to detect breaking changes
//! in engine requirements, peer dependencies, entry points, and other fields.
//!
//! This handler accepts ONLY binary MessagePack format. Text JSON is not supported.

use axum::{extract::State, http::StatusCode, response::Response};
use serde::{Deserialize, Serialize};

use crate::binary::{binary_error, BinaryJson, BinaryJsonResponse};
use crate::pkg_config_diff::{compare_package_configs, PackageConfig, PackageConfigChange};
use crate::response::Severity;
use crate::state::AppState;

/// Request to compare two package configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparePackageConfigRequest {
    /// The previous/baseline package config
    pub previous: PackageConfig,
    /// The current/new package config
    pub current: PackageConfig,
    /// Whether to include non-breaking additions in the response
    #[serde(default)]
    pub include_additions: bool,
}

/// Summary statistics for package configuration comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChangeSummary {
    /// Total number of changes
    pub total_changes: usize,
    /// Number of critical changes
    pub critical_count: usize,
    /// Number of major changes
    pub major_count: usize,
    /// Number of minor changes
    pub minor_count: usize,
    /// Number of warnings
    pub warning_count: usize,
    /// Whether any breaking changes were detected
    pub has_breaking_changes: bool,
    /// Suggested version bump
    pub suggested_bump: String,
}

/// Response from comparing two package configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparePackageConfigResponse {
    /// List of detected changes
    pub changes: Vec<PackageConfigChange>,
    /// Summary statistics
    pub summary: ConfigChangeSummary,
    /// Package name
    pub package_name: String,
    /// Previous version
    pub previous_version: String,
    /// Current version
    pub current_version: String,
}

/// Compare two package configurations and detect breaking changes.
///
/// POST /v1/compare-config
///
/// Request body: ComparePackageConfigRequest (Binary MessagePack)
/// - Content-Type: application/octet-stream or application/msgpack
/// - previous: PackageConfig - The baseline/previous package.json config
/// - current: PackageConfig - The new/current package.json config
/// - include_additions: bool - Whether to include non-breaking additions
///
/// Response: ComparePackageConfigResponse (Binary MessagePack)
/// - Content-Type: application/octet-stream
/// - changes: Vec<PackageConfigChange> - List of detected changes
/// - summary: ConfigChangeSummary - Statistics about the changes
/// - package_name: String - Package name from the configs
/// - previous_version: String - Version from previous config
/// - current_version: String - Version from current config
///
/// Errors:
/// - 415 Unsupported Media Type: If Content-Type is not binary MessagePack
/// - 400 Bad Request: If request body is invalid or package names don't match
pub async fn compare_package_config(
    State(_state): State<AppState>,
    BinaryJson(request): BinaryJson<ComparePackageConfigRequest>,
) -> Result<BinaryJsonResponse<ComparePackageConfigResponse>, Response> {
    // Validate that package names match
    if request.previous.name != request.current.name {
        return Err(binary_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Package names do not match: '{}' vs '{}'",
                request.previous.name, request.current.name
            ),
            Some("PACKAGE_NAME_MISMATCH"),
        ));
    }

    // Perform comparison
    let changes = compare_package_configs(
        &request.previous,
        &request.current,
        request.include_additions,
    );

    // Compute summary statistics
    let summary = compute_summary(&changes);

    // Build response
    let response = ComparePackageConfigResponse {
        changes,
        summary,
        package_name: request.previous.name.clone(),
        previous_version: request.previous.version.clone(),
        current_version: request.current.version.clone(),
    };

    Ok(BinaryJsonResponse(response))
}

/// Compute summary statistics from a list of changes.
fn compute_summary(changes: &[PackageConfigChange]) -> ConfigChangeSummary {
    let mut critical_count = 0;
    let mut major_count = 0;
    let mut minor_count = 0;
    let mut warning_count = 0;

    for change in changes {
        match change.severity {
            Severity::Critical => critical_count += 1,
            Severity::Major => major_count += 1,
            Severity::Minor => minor_count += 1,
            Severity::Warning => warning_count += 1,
        }
    }

    let has_breaking_changes = critical_count > 0 || major_count > 0 || minor_count > 0;

    let suggested_bump = if critical_count > 0 || major_count > 0 {
        "major"
    } else if minor_count > 0 {
        "minor"
    } else if warning_count > 0 {
        "patch"
    } else {
        "none"
    };

    ConfigChangeSummary {
        total_changes: changes.len(),
        critical_count,
        major_count,
        minor_count,
        warning_count,
        has_breaking_changes,
        suggested_bump: suggested_bump.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::{BINARY_CONTENT_TYPE, MSGPACK_CONTENT_TYPE};
    use axum::{body::Body, http::Request, routing::post, Router};
    use tower::ServiceExt;

    fn create_test_app() -> Router {
        let state = AppState::new();
        Router::new()
            .route("/v1/compare-config", post(compare_package_config))
            .with_state(state)
    }

    fn create_test_config(name: &str, version: &str) -> PackageConfig {
        PackageConfig {
            name: name.to_string(),
            version: version.to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_compare_config_no_changes() {
        let app = create_test_app();

        let prev = create_test_config("test-package", "1.0.0");
        let curr = create_test_config("test-package", "1.0.1");

        let request = ComparePackageConfigRequest {
            previous: prev,
            current: curr,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
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
        let result: ComparePackageConfigResponse = rmp_serde::from_slice(&body).unwrap();

        assert!(result.changes.is_empty());
        assert!(!result.summary.has_breaking_changes);
        assert_eq!(result.summary.suggested_bump, "none");
    }

    #[tokio::test]
    async fn test_compare_config_engine_added() {
        use crate::pkg_config_diff::Engines;

        let app = create_test_app();

        let prev = create_test_config("test-package", "1.0.0");
        let mut curr = create_test_config("test-package", "2.0.0");
        curr.engines = Some(Engines {
            node: Some(">=18.0.0".to_string()),
            ..Default::default()
        });

        let request = ComparePackageConfigRequest {
            previous: prev,
            current: curr,
            include_additions: true,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
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
        let result: ComparePackageConfigResponse = rmp_serde::from_slice(&body).unwrap();

        assert!(!result.changes.is_empty());
        assert!(result.summary.has_breaking_changes || result.summary.minor_count > 0);
    }

    #[tokio::test]
    async fn test_compare_config_package_name_mismatch() {
        let app = create_test_app();

        let prev = create_test_config("package-a", "1.0.0");
        let curr = create_test_config("package-b", "1.0.0");

        let request = ComparePackageConfigRequest {
            previous: prev,
            current: curr,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
                    .header("Content-Type", BINARY_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

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

        let prev = create_test_config("test-package", "1.0.0");
        let curr = create_test_config("test-package", "1.0.1");

        let request = ComparePackageConfigRequest {
            previous: prev,
            current: curr,
            include_additions: false,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
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

        let prev = create_test_config("test-package", "1.0.0");
        let curr = create_test_config("test-package", "1.0.1");

        let request = ComparePackageConfigRequest {
            previous: prev,
            current: curr,
            include_additions: false,
        };

        let binary_body = rmp_serde::to_vec_named(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/compare-config")
                    .header("Content-Type", MSGPACK_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}

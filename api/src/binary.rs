//! Binary JSON (MessagePack) extraction and response types for the API.
//!
//! This module provides custom Axum extractors and response types for handling
//! binary MessagePack format exclusively for compare requests.
//!
//! # Overview
//!
//! The API uses binary MessagePack format instead of text JSON for the `/v1/compare`
//! endpoint. This provides:
//!
//! - **Smaller payloads**: 30-50% smaller than equivalent JSON
//! - **Faster parsing**: Binary format is faster to serialize/deserialize
//! - **Type safety**: Better handling of binary data and complex types
//!
//! # Accepted Content-Types
//!
//! - `application/octet-stream` (preferred)
//! - `application/msgpack`
//!
//! Any other Content-Type (including `application/json`) will be rejected with
//! HTTP 415 Unsupported Media Type.
//!
//! # Components
//!
//! - [`BinaryJson`]: Axum extractor that validates Content-Type and deserializes MessagePack
//! - [`BinaryJsonResponse`]: Response wrapper that serializes to MessagePack
//! - [`BinaryErrorResponse`]: Error response in MessagePack format
//! - [`binary_error`]: Helper function to create error responses with custom status codes
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::binary::{BinaryJson, BinaryJsonResponse};
//!
//! async fn handler(
//!     BinaryJson(request): BinaryJson<MyRequest>,
//! ) -> BinaryJsonResponse<MyResponse> {
//!     // Process request...
//!     BinaryJsonResponse(response)
//! }
//! ```

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequest, Request},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Content type for binary MessagePack format.
pub const BINARY_CONTENT_TYPE: &str = "application/octet-stream";

/// Alternative content type for MessagePack.
pub const MSGPACK_CONTENT_TYPE: &str = "application/msgpack";

/// Error returned when binary JSON extraction fails.
#[derive(Debug)]
pub struct BinaryJsonError {
    pub status: StatusCode,
    pub message: String,
    pub code: Option<&'static str>,
}

impl BinaryJsonError {
    /// Create a new error for invalid content type.
    pub fn invalid_content_type(actual: Option<&str>) -> Self {
        Self {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            message: format!(
                "Expected binary MessagePack format (Content-Type: {} or {}), got: {}",
                BINARY_CONTENT_TYPE,
                MSGPACK_CONTENT_TYPE,
                actual.unwrap_or("none")
            ),
            code: Some("INVALID_CONTENT_TYPE"),
        }
    }

    /// Create a new error for deserialization failure.
    pub fn deserialization_failed(err: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: format!("Failed to deserialize binary MessagePack: {}", err),
            code: Some("DESERIALIZATION_FAILED"),
        }
    }

    /// Create a new error for body read failure.
    pub fn body_read_failed(err: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: format!("Failed to read request body: {}", err),
            code: Some("BODY_READ_FAILED"),
        }
    }
}

impl IntoResponse for BinaryJsonError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": self.message,
            "code": self.code,
        });
        (self.status, axum::Json(body)).into_response()
    }
}

/// Extractor for binary MessagePack requests.
///
/// This extractor validates that incoming requests use binary MessagePack format
/// and rejects any other Content-Type with HTTP 415 Unsupported Media Type.
///
/// # Accepted Content-Types
///
/// - `application/octet-stream`
/// - `application/msgpack`
///
/// # Rejection
///
/// Returns [`BinaryJsonError`] in the following cases:
///
/// - **415 Unsupported Media Type**: Content-Type is missing or not binary MessagePack
/// - **400 Bad Request**: Request body cannot be read or deserialized
///
/// # Example
///
/// ```rust,ignore
/// use crate::binary::BinaryJson;
///
/// async fn handler(BinaryJson(payload): BinaryJson<MyPayload>) -> impl IntoResponse {
///     // payload is deserialized from MessagePack
///     // ...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BinaryJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for BinaryJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = BinaryJsonError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Check Content-Type header
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());

        let is_binary = content_type.map_or(false, |ct| {
            ct.starts_with(BINARY_CONTENT_TYPE) || ct.starts_with(MSGPACK_CONTENT_TYPE)
        });

        if !is_binary {
            return Err(BinaryJsonError::invalid_content_type(content_type));
        }

        // Read body bytes
        let bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
            .await
            .map_err(BinaryJsonError::body_read_failed)?;

        // Deserialize from MessagePack
        let value: T =
            rmp_serde::from_slice(&bytes).map_err(BinaryJsonError::deserialization_failed)?;

        Ok(BinaryJson(value))
    }
}

/// Response type for binary MessagePack responses.
///
/// Serializes the wrapped value to MessagePack format using `rmp_serde::to_vec_named`
/// (which preserves field names for easier debugging) and sets the Content-Type header
/// to `application/octet-stream`.
///
/// # Response Headers
///
/// - `Content-Type: application/octet-stream`
///
/// # Error Handling
///
/// If serialization fails, returns HTTP 500 Internal Server Error with a JSON error body.
///
/// # Example
///
/// ```rust,ignore
/// use crate::binary::BinaryJsonResponse;
///
/// async fn handler() -> BinaryJsonResponse<MyResponse> {
///     BinaryJsonResponse(MyResponse { ... })
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BinaryJsonResponse<T>(pub T);

impl<T> IntoResponse for BinaryJsonResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match rmp_serde::to_vec_named(&self.0) {
            Ok(bytes) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, BINARY_CONTENT_TYPE)
                .body(Body::from(bytes))
                .unwrap(),
            Err(err) => {
                let body = serde_json::json!({
                    "error": format!("Failed to serialize response: {}", err),
                    "code": "SERIALIZATION_FAILED",
                });
                (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(body)).into_response()
            }
        }
    }
}

/// Error response for binary JSON endpoints.
///
/// Returns errors in binary MessagePack format for consistency with successful responses.
/// Contains an error message and optional error code.
///
/// # Fields
///
/// - `error`: Human-readable error message
/// - `code`: Optional machine-readable error code (e.g., "PACKAGE_NAME_MISMATCH")
///
/// # Serialization
///
/// When used as a response, serializes to MessagePack with `Content-Type: application/octet-stream`.
/// The status code defaults to 400 Bad Request. For other status codes, use [`binary_error`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl BinaryErrorResponse {
    /// Create a new binary error response.
    #[allow(dead_code)]
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: None,
        }
    }

    /// Add an error code.
    #[allow(dead_code)]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

impl IntoResponse for BinaryErrorResponse {
    fn into_response(self) -> Response {
        match rmp_serde::to_vec_named(&self) {
            Ok(bytes) => Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, BINARY_CONTENT_TYPE)
                .body(Body::from(bytes))
                .unwrap(),
            Err(_) => {
                // Fallback to plain text if serialization fails
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("Internal server error"))
                    .unwrap()
            }
        }
    }
}

/// Helper to create a binary error response with a specific status code.
///
/// Creates a [`BinaryErrorResponse`] serialized to MessagePack with the specified
/// HTTP status code and `Content-Type: application/octet-stream`.
///
/// # Arguments
///
/// - `status`: HTTP status code for the response
/// - `error`: Human-readable error message
/// - `code`: Optional machine-readable error code
///
/// # Example
///
/// ```rust,ignore
/// use axum::http::StatusCode;
/// use crate::binary::binary_error;
///
/// return Err(binary_error(
///     StatusCode::BAD_REQUEST,
///     "Package names do not match",
///     Some("PACKAGE_NAME_MISMATCH"),
/// ));
/// ```
pub fn binary_error(
    status: StatusCode,
    error: impl Into<String>,
    code: Option<&str>,
) -> Response {
    let response = BinaryErrorResponse {
        error: error.into(),
        code: code.map(String::from),
    };

    match rmp_serde::to_vec_named(&response) {
        Ok(bytes) => Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, BINARY_CONTENT_TYPE)
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Internal server error"))
            .unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::post, Router};
    use serde::{Deserialize, Serialize};
    use tower::ServiceExt;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        name: String,
        value: i32,
    }

    async fn echo_handler(BinaryJson(payload): BinaryJson<TestPayload>) -> BinaryJsonResponse<TestPayload> {
        BinaryJsonResponse(payload)
    }

    fn create_test_app() -> Router {
        Router::new().route("/echo", post(echo_handler))
    }

    #[tokio::test]
    async fn test_binary_json_roundtrip() {
        let app = create_test_app();

        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };
        let binary_body = rmp_serde::to_vec_named(&payload).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
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
        let result: TestPayload = rmp_serde::from_slice(&body).unwrap();
        assert_eq!(result, payload);
    }

    #[tokio::test]
    async fn test_reject_json_content_type() {
        let app = create_test_app();

        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_reject_missing_content_type() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_accept_msgpack_content_type() {
        let app = create_test_app();

        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };
        let binary_body = rmp_serde::to_vec_named(&payload).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
                    .header("Content-Type", MSGPACK_CONTENT_TYPE)
                    .body(Body::from(binary_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}

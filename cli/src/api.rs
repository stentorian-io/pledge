//! API client for communicating with the Interlint analysis service.
//!
//! This module provides functions to send package public interfaces to
//! the Interlint API for breaking change analysis.
//!
//! # Binary Format
//!
//! All interface data is serialized using the interlint binary format:
//! - **Wire format**: MessagePack with 8-byte header and CRC32 checksum
//! - **Content-Type**: `application/x-interlint`
//! - **File extension**: `.ilmj` (for saved interface files)
//!
//! There is no JSON output option for interface data. The binary format provides
//! better performance and smaller payload sizes.

use anyhow::{bail, Context, Result};
use interlint_format::{binary, PublicInterface};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

/// Default API URL if INTERLINT_API_URL is not set.
const DEFAULT_API_URL: &str = "http://localhost:3000";

/// Timeout for API requests in seconds.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Severity level of a breaking change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Critical breaking change - will definitely break consumers
    Critical,
    /// Major breaking change - likely to break many consumers
    Major,
    /// Minor breaking change - may break some consumers
    Minor,
    /// Warning - potential breaking change
    Warning,
}

/// Type of breaking change detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangeType {
    /// An export was removed from the public interface
    ExportRemoved,
    /// An export was added (informational, not breaking)
    ExportAdded,
    /// A module was removed
    ModuleRemoved,
    /// A module was added (informational)
    ModuleAdded,
    /// An entrypoint was removed
    EntrypointRemoved,
    /// An entrypoint was added (informational)
    EntrypointAdded,
    /// Function signature changed (parameters, return type)
    SignatureChanged,
    /// A required parameter was added to a function
    ParameterAdded,
    /// A parameter was removed from a function
    ParameterRemoved,
    /// A parameter changed from optional to required
    ParameterBecameRequired,
    /// A parameter changed from required to optional (usually safe)
    ParameterBecameOptional,
    /// Parameter type changed
    ParameterTypeChanged,
    /// Return type changed
    ReturnTypeChanged,
    /// Type definition changed (interface, class, type alias)
    TypeChanged,
    /// Property added to interface/class
    PropertyAdded,
    /// Property removed from interface/class
    PropertyRemoved,
    /// Property type changed
    PropertyTypeChanged,
    /// Property became required
    PropertyBecameRequired,
    /// Property became optional
    PropertyBecameOptional,
    /// Method added to class/interface
    MethodAdded,
    /// Method removed from class/interface
    MethodRemoved,
    /// Method signature changed
    MethodSignatureChanged,
    /// Enum member added
    EnumMemberAdded,
    /// Enum member removed
    EnumMemberRemoved,
    /// Enum member value changed
    EnumMemberValueChanged,
    /// Type parameter added
    TypeParameterAdded,
    /// Type parameter removed
    TypeParameterRemoved,
    /// Type parameter constraint changed
    TypeParameterConstraintChanged,
    /// Visibility changed (e.g., public to private)
    VisibilityChanged,
    /// Async modifier changed
    AsyncModifierChanged,
    /// Export kind changed (e.g., function to class)
    ExportKindChanged,
    /// Default export changed
    DefaultExportChanged,
    /// Generic type arguments changed
    TypeArgumentsChanged,
}

/// A single breaking change detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChange {
    /// Type of breaking change
    pub change_type: BreakingChangeType,
    /// Severity level
    pub severity: Severity,
    /// Path to the affected item (e.g., "module.ts#functionName")
    pub path: String,
    /// Name of the affected export/item
    #[serde(default)]
    pub name: String,
    /// Human-readable message describing the change
    pub message: String,
    /// Previous state (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Current state (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Suggested fix or migration path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Request body for the comparison endpoint.
#[derive(Debug, Serialize)]
pub struct CompareRequest {
    /// Previous version's public interface
    pub previous: PublicInterface,
    /// Current version's public interface
    pub current: PublicInterface,
}

/// Suggested semantic version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionBump {
    /// No version bump needed
    None,
    /// Patch version bump (x.x.PATCH)
    Patch,
    /// Minor version bump (x.MINOR.0)
    Minor,
    /// Major version bump (MAJOR.0.0)
    Major,
}

/// Response from the comparison endpoint.
#[derive(Debug, Deserialize)]
pub struct CompareResponse {
    /// List of detected breaking changes
    #[serde(default)]
    pub changes: Vec<BreakingChange>,
    /// Summary statistics
    pub summary: ComparisonSummary,
    /// Previous version (from metadata)
    pub previous_version: String,
    /// Current version (from metadata)
    pub current_version: String,
    /// Package name
    pub package_name: String,
}

/// Summary of the comparison results.
#[derive(Debug, Deserialize)]
pub struct ComparisonSummary {
    /// Total number of breaking changes
    pub total_breaking_changes: usize,
    /// Number of critical severity changes
    #[serde(default)]
    pub critical_count: usize,
    /// Number of major severity changes
    #[serde(default)]
    pub major_count: usize,
    /// Number of minor severity changes
    #[serde(default)]
    pub minor_count: usize,
    /// Number of warning severity changes
    #[serde(default)]
    pub warning_count: usize,
    /// Whether any breaking changes were detected (excludes warnings)
    pub has_breaking_changes: bool,
    /// Suggested version bump based on changes
    pub suggested_bump: VersionBump,
    /// Number of exports in previous version
    #[serde(default)]
    pub previous_export_count: usize,
    /// Number of exports in current version
    #[serde(default)]
    pub current_export_count: usize,
}

/// API client for Interlint service.
pub struct ApiClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl ApiClient {
    /// Create a new API client.
    ///
    /// Uses INTERLINT_API_URL environment variable if set, otherwise uses default.
    pub fn new() -> Result<Self> {
        let base_url = env::var("INTERLINT_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string());

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { base_url, client })
    }

    /// Create a new API client with a custom base URL.
    pub fn with_url(base_url: impl Into<String>) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_url: base_url.into(),
            client,
        })
    }

    /// Compare two package versions for breaking changes.
    ///
    /// Sends interface data as interlint binary format (MessagePack with header).
    pub fn compare(
        &self,
        previous: PublicInterface,
        current: PublicInterface,
    ) -> Result<CompareResponse> {
        let url = format!("{}/v1/compare", self.base_url);

        let request = CompareRequest { previous, current };

        let body = binary::encode(&request)
            .context("Failed to encode request to binary format")?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-interlint")
            .header("Accept", "application/x-interlint")
            .body(body)
            .send()
            .context("Failed to send compare request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            bail!("API request failed with status {}: {}", status, body);
        }

        let bytes = response.bytes()
            .context("Failed to read response body")?;

        let result: CompareResponse = binary::decode(&bytes)
            .context("Failed to decode binary response")?;

        Ok(result)
    }

    /// Analyze a single package interface (check for issues without comparison).
    ///
    /// Sends interface data as interlint binary format (MessagePack with header).
    pub fn analyze(&self, interface: PublicInterface) -> Result<AnalysisResponse> {
        let url = format!("{}/v1/analyze", self.base_url);

        let body = binary::encode(&interface)
            .context("Failed to encode interface to binary format")?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-interlint")
            .header("Accept", "application/x-interlint")
            .body(body)
            .send()
            .context("Failed to send analyze request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            bail!("API request failed with status {}: {}", status, body);
        }

        let bytes = response.bytes()
            .context("Failed to read response body")?;

        let result: AnalysisResponse = binary::decode(&bytes)
            .context("Failed to decode binary response")?;

        Ok(result)
    }

    /// Health check for the API.
    pub fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .context("Failed to send health check request")?;

        Ok(response.status().is_success())
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default API client")
    }
}

/// Response from the analyze endpoint.
#[derive(Debug, Deserialize)]
pub struct AnalysisResponse {
    /// Package name
    pub package_name: String,
    /// Package version
    pub package_version: String,
    /// Total number of exports
    pub export_count: usize,
    /// Total number of modules
    pub module_count: usize,
    /// Total number of entrypoints
    pub entrypoint_count: usize,
    /// Any warnings or suggestions
    #[serde(default)]
    pub warnings: Vec<AnalysisWarning>,
}

/// A warning from the analysis.
#[derive(Debug, Deserialize)]
pub struct AnalysisWarning {
    /// Warning code
    pub code: String,
    /// Warning message
    pub message: String,
    /// Path to the affected item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Print breaking changes to stdout.
pub fn print_breaking_changes(response: &CompareResponse) {
    if !response.summary.has_breaking_changes {
        println!("\n  No breaking changes detected.");
        return;
    }

    println!("\n  Breaking Changes Detected:");
    println!("  ===========================\n");

    // Group by severity
    let critical: Vec<_> = response
        .changes
        .iter()
        .filter(|c| c.severity == Severity::Critical)
        .collect();
    let major: Vec<_> = response
        .changes
        .iter()
        .filter(|c| c.severity == Severity::Major)
        .collect();
    let minor: Vec<_> = response
        .changes
        .iter()
        .filter(|c| c.severity == Severity::Minor)
        .collect();
    let warnings: Vec<_> = response
        .changes
        .iter()
        .filter(|c| c.severity == Severity::Warning)
        .collect();

    if !critical.is_empty() {
        println!("  CRITICAL ({}):", critical.len());
        for change in critical {
            print_change(change, "    ");
        }
        println!();
    }

    if !major.is_empty() {
        println!("  MAJOR ({}):", major.len());
        for change in major {
            print_change(change, "    ");
        }
        println!();
    }

    if !minor.is_empty() {
        println!("  MINOR ({}):", minor.len());
        for change in minor {
            print_change(change, "    ");
        }
        println!();
    }

    if !warnings.is_empty() {
        println!("  WARNINGS ({}):", warnings.len());
        for change in warnings {
            print_change(change, "    ");
        }
        println!();
    }

    // Print summary
    let summary = &response.summary;
    println!("  Summary:");
    println!("  ---------");
    println!(
        "  Total breaking changes: {}",
        summary.total_breaking_changes
    );
    println!(
        "  Previous exports: {} -> Current exports: {}",
        summary.previous_export_count, summary.current_export_count
    );
}

/// Print a single breaking change.
fn print_change(change: &BreakingChange, indent: &str) {
    println!("{}- {} at '{}'", indent, change.message, change.path);
    if let Some(prev) = &change.previous {
        println!("{}  Previous: {}", indent, prev);
    }
    if let Some(curr) = &change.current {
        println!("{}  Current: {}", indent, curr);
    }
    if let Some(suggestion) = &change.suggestion {
        println!("{}  Suggestion: {}", indent, suggestion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert_eq!(Severity::Critical, Severity::Critical);
        assert_ne!(Severity::Critical, Severity::Major);
    }
}

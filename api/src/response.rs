//! Response types for the interlint API.

use interlint_format::PublicInterface;
use serde::{Deserialize, Serialize};

/// Severity level of a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Critical breaking change - will break all consumers
    Critical,
    /// Major breaking change - likely to break most consumers
    Major,
    /// Minor breaking change - may break some consumers
    Minor,
    /// Warning - potential compatibility issue
    Warning,
}

impl Severity {
    /// Get numeric weight for severity (higher = more severe).
    pub fn weight(&self) -> u8 {
        match self {
            Severity::Critical => 4,
            Severity::Major => 3,
            Severity::Minor => 2,
            Severity::Warning => 1,
        }
    }
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
    /// An asset file was removed from the package
    AssetRemoved,
    /// An asset file was added to the package
    AssetAdded,
}

impl BreakingChangeType {
    /// Get the default severity for this change type.
    pub fn default_severity(&self) -> Severity {
        match self {
            // Critical - always breaks consumers
            BreakingChangeType::ExportRemoved => Severity::Critical,
            BreakingChangeType::ModuleRemoved => Severity::Critical,
            BreakingChangeType::EntrypointRemoved => Severity::Critical,
            BreakingChangeType::ParameterAdded => Severity::Critical,
            BreakingChangeType::ParameterBecameRequired => Severity::Critical,
            BreakingChangeType::PropertyBecameRequired => Severity::Critical,
            BreakingChangeType::ExportKindChanged => Severity::Critical,

            // Major - likely breaks most consumers
            BreakingChangeType::SignatureChanged => Severity::Major,
            BreakingChangeType::ParameterRemoved => Severity::Major,
            BreakingChangeType::ParameterTypeChanged => Severity::Major,
            BreakingChangeType::ReturnTypeChanged => Severity::Major,
            BreakingChangeType::TypeChanged => Severity::Major,
            BreakingChangeType::PropertyRemoved => Severity::Major,
            BreakingChangeType::PropertyTypeChanged => Severity::Major,
            BreakingChangeType::MethodRemoved => Severity::Major,
            BreakingChangeType::MethodSignatureChanged => Severity::Major,
            BreakingChangeType::EnumMemberRemoved => Severity::Major,
            BreakingChangeType::EnumMemberValueChanged => Severity::Major,
            BreakingChangeType::TypeParameterRemoved => Severity::Major,
            BreakingChangeType::TypeParameterConstraintChanged => Severity::Major,
            BreakingChangeType::VisibilityChanged => Severity::Major,
            BreakingChangeType::TypeArgumentsChanged => Severity::Major,
            BreakingChangeType::DefaultExportChanged => Severity::Major,

            // Minor - may break some consumers
            BreakingChangeType::AsyncModifierChanged => Severity::Minor,

            // Warning - informational or potentially breaking
            BreakingChangeType::ExportAdded => Severity::Warning,
            BreakingChangeType::ModuleAdded => Severity::Warning,
            BreakingChangeType::EntrypointAdded => Severity::Warning,
            BreakingChangeType::ParameterBecameOptional => Severity::Warning,
            BreakingChangeType::PropertyBecameOptional => Severity::Warning,
            BreakingChangeType::PropertyAdded => Severity::Warning,
            BreakingChangeType::MethodAdded => Severity::Warning,
            BreakingChangeType::EnumMemberAdded => Severity::Warning,
            BreakingChangeType::TypeParameterAdded => Severity::Warning,
            BreakingChangeType::AssetAdded => Severity::Warning,

            // Major - asset removal breaks consumers
            BreakingChangeType::AssetRemoved => Severity::Major,
        }
    }
}

/// A detected breaking change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChange {
    /// Type of breaking change
    pub change_type: BreakingChangeType,
    /// Severity of the change
    pub severity: Severity,
    /// Path to the affected item (e.g., "module.ts#functionName")
    pub path: String,
    /// Name of the affected export/item
    pub name: String,
    /// Human-readable description of the change
    pub message: String,
    /// Previous value/state (for context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Current value/state (for context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Suggested fix or migration path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl BreakingChange {
    /// Create a new breaking change with default severity.
    pub fn new(
        change_type: BreakingChangeType,
        path: impl Into<String>,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let severity = change_type.default_severity();
        Self {
            change_type,
            severity,
            path: path.into(),
            name: name.into(),
            message: message.into(),
            previous: None,
            current: None,
            suggestion: None,
        }
    }

    /// Set a custom severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Add previous value context.
    pub fn with_previous(mut self, previous: impl Into<String>) -> Self {
        self.previous = Some(previous.into());
        self
    }

    /// Add current value context.
    pub fn with_current(mut self, current: impl Into<String>) -> Self {
        self.current = Some(current.into());
        self
    }

    /// Add a suggestion for fixing the issue.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Request to compare two public interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRequest {
    /// The previous/baseline public interface
    pub previous: PublicInterface,
    /// The current/new public interface
    pub current: PublicInterface,
    /// Optional: only report changes at or above this severity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_severity: Option<Severity>,
    /// Optional: include non-breaking changes (additions) in the response
    #[serde(default)]
    pub include_additions: bool,
}

/// Summary statistics for the comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonSummary {
    /// Total number of breaking changes
    pub total_breaking_changes: usize,
    /// Number of critical changes
    pub critical_count: usize,
    /// Number of major changes
    pub major_count: usize,
    /// Number of minor changes
    pub minor_count: usize,
    /// Number of warnings
    pub warning_count: usize,
    /// Whether any breaking changes were detected (excludes warnings)
    pub has_breaking_changes: bool,
    /// Suggested version bump based on changes
    pub suggested_bump: VersionBump,
    /// Number of exports in previous version
    pub previous_export_count: usize,
    /// Number of exports in current version
    pub current_export_count: usize,
}

/// Suggested semantic version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Response from comparing two public interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareResponse {
    /// List of detected breaking changes
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

impl CompareResponse {
    /// Create a new compare response from a list of changes and metadata.
    pub fn new(
        changes: Vec<BreakingChange>,
        previous_version: String,
        current_version: String,
        package_name: String,
        previous_export_count: usize,
        current_export_count: usize,
    ) -> Self {
        let summary = Self::compute_summary(&changes, previous_export_count, current_export_count);
        Self {
            changes,
            summary,
            previous_version,
            current_version,
            package_name,
        }
    }

    /// Compute summary statistics from a list of changes.
    fn compute_summary(
        changes: &[BreakingChange],
        previous_export_count: usize,
        current_export_count: usize,
    ) -> ComparisonSummary {
        let total_breaking_changes = changes.len();
        let critical_count = changes
            .iter()
            .filter(|c| c.severity == Severity::Critical)
            .count();
        let major_count = changes
            .iter()
            .filter(|c| c.severity == Severity::Major)
            .count();
        let minor_count = changes
            .iter()
            .filter(|c| c.severity == Severity::Minor)
            .count();
        let warning_count = changes
            .iter()
            .filter(|c| c.severity == Severity::Warning)
            .count();

        let has_breaking_changes = critical_count > 0 || major_count > 0 || minor_count > 0;

        let suggested_bump = if critical_count > 0 || major_count > 0 {
            VersionBump::Major
        } else if minor_count > 0 {
            VersionBump::Minor
        } else if warning_count > 0 {
            VersionBump::Patch
        } else {
            VersionBump::None
        };

        ComparisonSummary {
            total_breaking_changes,
            critical_count,
            major_count,
            minor_count,
            warning_count,
            has_breaking_changes,
            suggested_bump,
            previous_export_count,
            current_export_count,
        }
    }
}

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Service version
    pub version: String,
}


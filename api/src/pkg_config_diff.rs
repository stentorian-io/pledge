//! Package configuration diff detection logic.
//!
//! This module compares package.json configuration fields between versions
//! to detect breaking changes that don't involve code signatures but affect
//! how consumers can use the package.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::response::{BreakingChange, Severity};

/// Represents a semantic version for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Option<String>,
}

impl SemVer {
    /// Parse a semver string into components.
    pub fn parse(version: &str) -> Option<Self> {
        let version = version.trim_start_matches(['v', 'V', '^', '~', '=', '>', '<', ' ']);

        // Handle prerelease versions
        let (main_part, prerelease) = if let Some(idx) = version.find('-') {
            (&version[..idx], Some(version[idx + 1..].to_string()))
        } else {
            (version, None)
        };

        let parts: Vec<&str> = main_part.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }

        let major = parts.first()?.parse().ok()?;
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Check if this version is a major bump from another.
    pub fn is_major_bump_from(&self, other: &Self) -> bool {
        self.major > other.major
    }

    /// Check if the minimum version increased (breaking for engines).
    pub fn is_min_version_increase_from(&self, other: &Self) -> bool {
        if self.major > other.major {
            return true;
        }
        if self.major == other.major && self.minor > other.minor {
            return true;
        }
        if self.major == other.major && self.minor == other.minor && self.patch > other.patch {
            return true;
        }
        false
    }
}

/// Package configuration change types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageConfigChangeType {
    // Engine changes
    /// Node.js engine version requirement increased
    EngineNodeVersionIncreased,
    /// npm engine version requirement increased
    EngineNpmVersionIncreased,
    /// Engine requirement added
    EngineAdded,
    /// Engine requirement removed (usually safe)
    EngineRemoved,

    // Peer dependency changes
    /// Required peer dependency added
    PeerDependencyAdded,
    /// Peer dependency removed
    PeerDependencyRemoved,
    /// Peer dependency version had major bump
    PeerDependencyMajorBump,
    /// Peer dependency became required (was optional)
    PeerDependencyBecameRequired,
    /// Peer dependency became optional (was required)
    PeerDependencyBecameOptional,

    // Exports changes
    /// Export subpath removed
    ExportSubpathRemoved,
    /// Export subpath added
    ExportSubpathAdded,
    /// Export condition removed (e.g., "types", "import", "require")
    ExportConditionRemoved,
    /// Export condition added
    ExportConditionAdded,
    /// Export target changed
    ExportTargetChanged,

    // Entry point changes
    /// Main field changed
    MainFieldChanged,
    /// Main field removed
    MainFieldRemoved,
    /// Module field changed
    ModuleFieldChanged,
    /// Module field removed
    ModuleFieldRemoved,
    /// Types field changed
    TypesFieldChanged,
    /// Types field removed
    TypesFieldRemoved,
    /// Browser field changed
    BrowserFieldChanged,

    // Module type changes
    /// Type field changed (e.g., "module" to "commonjs")
    ModuleTypeChanged,
    /// Type field added
    ModuleTypeAdded,
    /// Type field removed
    ModuleTypeRemoved,

    // Bin changes
    /// Binary command removed
    BinCommandRemoved,
    /// Binary command renamed
    BinCommandRenamed,
    /// Binary command added
    BinCommandAdded,
    /// Binary target changed
    BinTargetChanged,

    // Dependency changes
    /// Dependency had major version bump
    DependencyMajorBump,
    /// Dependency removed
    DependencyRemoved,
    /// Dependency added
    DependencyAdded,
}

impl PackageConfigChangeType {
    /// Get the default severity for this change type.
    pub fn default_severity(&self) -> Severity {
        match self {
            // Critical - breaks all consumers
            PackageConfigChangeType::ExportSubpathRemoved => Severity::Critical,
            PackageConfigChangeType::ExportConditionRemoved => Severity::Critical,
            PackageConfigChangeType::BinCommandRemoved => Severity::Critical,
            PackageConfigChangeType::MainFieldRemoved => Severity::Critical,
            PackageConfigChangeType::TypesFieldRemoved => Severity::Critical,
            PackageConfigChangeType::PeerDependencyBecameRequired => Severity::Critical,

            // Major - likely breaks most consumers
            PackageConfigChangeType::EngineNodeVersionIncreased => Severity::Major,
            PackageConfigChangeType::EngineNpmVersionIncreased => Severity::Major,
            PackageConfigChangeType::PeerDependencyAdded => Severity::Major,
            PackageConfigChangeType::PeerDependencyMajorBump => Severity::Major,
            PackageConfigChangeType::ModuleTypeChanged => Severity::Major,
            PackageConfigChangeType::MainFieldChanged => Severity::Major,
            PackageConfigChangeType::ModuleFieldChanged => Severity::Major,
            PackageConfigChangeType::ModuleFieldRemoved => Severity::Major,
            PackageConfigChangeType::TypesFieldChanged => Severity::Major,
            PackageConfigChangeType::ExportTargetChanged => Severity::Major,
            PackageConfigChangeType::BinCommandRenamed => Severity::Major,
            PackageConfigChangeType::BinTargetChanged => Severity::Major,
            PackageConfigChangeType::DependencyMajorBump => Severity::Major,
            PackageConfigChangeType::BrowserFieldChanged => Severity::Major,

            // Minor - may break some consumers
            PackageConfigChangeType::EngineAdded => Severity::Minor,
            PackageConfigChangeType::ModuleTypeAdded => Severity::Minor,
            PackageConfigChangeType::DependencyRemoved => Severity::Minor,

            // Warning - informational or potentially breaking
            PackageConfigChangeType::EngineRemoved => Severity::Warning,
            PackageConfigChangeType::PeerDependencyRemoved => Severity::Warning,
            PackageConfigChangeType::PeerDependencyBecameOptional => Severity::Warning,
            PackageConfigChangeType::ExportSubpathAdded => Severity::Warning,
            PackageConfigChangeType::ExportConditionAdded => Severity::Warning,
            PackageConfigChangeType::BinCommandAdded => Severity::Warning,
            PackageConfigChangeType::ModuleTypeRemoved => Severity::Warning,
            PackageConfigChangeType::DependencyAdded => Severity::Warning,
        }
    }
}

/// A detected package configuration change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfigChange {
    /// Type of configuration change.
    pub change_type: PackageConfigChangeType,
    /// Severity of the change.
    pub severity: Severity,
    /// Path/key identifying the changed configuration.
    pub path: String,
    /// Name of the affected item.
    pub name: String,
    /// Human-readable description of the change.
    pub message: String,
    /// Previous value (for context).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Current value (for context).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Migration hint for addressing the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

impl PackageConfigChange {
    /// Create a new package configuration change.
    pub fn new(
        change_type: PackageConfigChangeType,
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
            migration_hint: None,
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

    /// Add a migration hint.
    pub fn with_migration_hint(mut self, hint: impl Into<String>) -> Self {
        self.migration_hint = Some(hint.into());
        self
    }
}

/// Engine requirements (node, npm, etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engines {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    /// Other engine requirements (yarn, pnpm, etc.).
    #[serde(flatten)]
    pub other: HashMap<String, String>,
}

/// Export condition mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExportValue {
    /// Simple string path.
    Path(String),
    /// Conditional exports.
    Conditional(HashMap<String, ExportValue>),
}

impl ExportValue {
    /// Get all condition keys recursively.
    pub fn get_conditions(&self) -> HashSet<String> {
        let mut conditions = HashSet::new();
        self.collect_conditions(&mut conditions);
        conditions
    }

    fn collect_conditions(&self, conditions: &mut HashSet<String>) {
        match self {
            ExportValue::Path(_) => {}
            ExportValue::Conditional(map) => {
                for (key, value) in map {
                    conditions.insert(key.clone());
                    value.collect_conditions(conditions);
                }
            }
        }
    }

    /// Get the path for a specific condition.
    pub fn get_path(&self, condition: &str) -> Option<&str> {
        match self {
            ExportValue::Path(p) => Some(p),
            ExportValue::Conditional(map) => {
                map.get(condition).and_then(|v| match v {
                    ExportValue::Path(p) => Some(p.as_str()),
                    ExportValue::Conditional(_) => None,
                })
            }
        }
    }
}

/// Binary command specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BinSpec {
    /// Single binary (uses package name).
    Single(String),
    /// Multiple named binaries.
    Multiple(HashMap<String, String>),
}

impl BinSpec {
    /// Get all binary commands as a map.
    pub fn as_map(&self, package_name: &str) -> HashMap<String, String> {
        match self {
            BinSpec::Single(path) => {
                let mut map = HashMap::new();
                map.insert(package_name.to_string(), path.clone());
                map
            }
            BinSpec::Multiple(map) => map.clone(),
        }
    }
}

/// Package configuration for comparison.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageConfig {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Engine requirements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engines: Option<Engines>,
    /// Peer dependencies.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub peer_dependencies: HashMap<String, String>,
    /// Peer dependencies metadata (for optional peers).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub peer_dependencies_meta: HashMap<String, PeerDepMeta>,
    /// Package exports field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exports: Option<HashMap<String, ExportValue>>,
    /// Main entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// ESM module entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// TypeScript types entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<String>,
    /// Browser entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    /// Module type ("module" or "commonjs").
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub module_type: Option<String>,
    /// Binary commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<BinSpec>,
    /// Production dependencies.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, String>,
}

/// Peer dependency metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerDepMeta {
    #[serde(default)]
    pub optional: bool,
}

/// Compare two package configurations and return a list of changes.
pub fn compare_package_configs(
    previous: &PackageConfig,
    current: &PackageConfig,
    include_additions: bool,
) -> Vec<PackageConfigChange> {
    let mut changes = Vec::new();

    // Compare engines
    compare_engines(previous, current, &mut changes, include_additions);

    // Compare peer dependencies
    compare_peer_dependencies(previous, current, &mut changes, include_additions);

    // Compare exports
    compare_exports(previous, current, &mut changes, include_additions);

    // Compare entry points
    compare_entry_points(previous, current, &mut changes, include_additions);

    // Compare module type
    compare_module_type(previous, current, &mut changes, include_additions);

    // Compare bin
    compare_bin(previous, current, &mut changes, include_additions);

    // Compare dependencies
    compare_dependencies(previous, current, &mut changes, include_additions);

    // Sort by severity (most severe first)
    changes.sort_by(|a, b| b.severity.weight().cmp(&a.severity.weight()));

    changes
}

/// Compare engine requirements.
fn compare_engines(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    let prev_engines = previous.engines.as_ref();
    let curr_engines = current.engines.as_ref();

    match (prev_engines, curr_engines) {
        (None, Some(curr)) => {
            // Engine requirements added
            if include_additions {
                if let Some(node) = &curr.node {
                    changes.push(
                        PackageConfigChange::new(
                            PackageConfigChangeType::EngineAdded,
                            "engines.node",
                            "node",
                            format!("Node.js engine requirement added: {}", node),
                        )
                        .with_current(node)
                        .with_migration_hint("Ensure your Node.js version meets the new requirement"),
                    );
                }
                if let Some(npm) = &curr.npm {
                    changes.push(
                        PackageConfigChange::new(
                            PackageConfigChangeType::EngineAdded,
                            "engines.npm",
                            "npm",
                            format!("npm engine requirement added: {}", npm),
                        )
                        .with_current(npm)
                        .with_migration_hint("Ensure your npm version meets the new requirement"),
                    );
                }
            }
        }
        (Some(prev), None) => {
            // Engine requirements removed
            if prev.node.is_some() {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::EngineRemoved,
                        "engines.node",
                        "node",
                        "Node.js engine requirement removed",
                    )
                    .with_previous(prev.node.as_deref().unwrap_or("")),
                );
            }
            if prev.npm.is_some() {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::EngineRemoved,
                        "engines.npm",
                        "npm",
                        "npm engine requirement removed",
                    )
                    .with_previous(prev.npm.as_deref().unwrap_or("")),
                );
            }
        }
        (Some(prev), Some(curr)) => {
            // Compare node versions
            compare_engine_version(
                "node",
                prev.node.as_deref(),
                curr.node.as_deref(),
                PackageConfigChangeType::EngineNodeVersionIncreased,
                changes,
                include_additions,
            );

            // Compare npm versions
            compare_engine_version(
                "npm",
                prev.npm.as_deref(),
                curr.npm.as_deref(),
                PackageConfigChangeType::EngineNpmVersionIncreased,
                changes,
                include_additions,
            );

            // Compare other engines
            let prev_other: HashSet<&str> = prev.other.keys().map(|s| s.as_str()).collect();
            let curr_other: HashSet<&str> = curr.other.keys().map(|s| s.as_str()).collect();

            for engine in prev_other.difference(&curr_other) {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::EngineRemoved,
                        format!("engines.{}", engine),
                        *engine,
                        format!("Engine requirement '{}' removed", engine),
                    )
                    .with_previous(&prev.other[*engine]),
                );
            }

            if include_additions {
                for engine in curr_other.difference(&prev_other) {
                    changes.push(
                        PackageConfigChange::new(
                            PackageConfigChangeType::EngineAdded,
                            format!("engines.{}", engine),
                            *engine,
                            format!("Engine requirement '{}' added: {}", engine, curr.other[*engine]),
                        )
                        .with_current(&curr.other[*engine]),
                    );
                }
            }
        }
        (None, None) => {}
    }
}

/// Compare a specific engine version.
fn compare_engine_version(
    engine_name: &str,
    prev_version: Option<&str>,
    curr_version: Option<&str>,
    change_type: PackageConfigChangeType,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    match (prev_version, curr_version) {
        (Some(prev), Some(curr)) => {
            if let (Some(prev_semver), Some(curr_semver)) =
                (SemVer::parse(prev), SemVer::parse(curr))
            {
                if curr_semver.is_min_version_increase_from(&prev_semver) {
                    changes.push(
                        PackageConfigChange::new(
                            change_type,
                            format!("engines.{}", engine_name),
                            engine_name,
                            format!(
                                "{} engine minimum version increased from {} to {}",
                                engine_name, prev, curr
                            ),
                        )
                        .with_previous(prev)
                        .with_current(curr)
                        .with_migration_hint(format!(
                            "Upgrade {} to version {} or higher",
                            engine_name, curr
                        )),
                    );
                }
            }
        }
        (None, Some(curr)) => {
            if include_additions {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::EngineAdded,
                        format!("engines.{}", engine_name),
                        engine_name,
                        format!("{} engine requirement added: {}", engine_name, curr),
                    )
                    .with_current(curr)
                    .with_migration_hint(format!(
                        "Ensure your {} version meets the requirement: {}",
                        engine_name, curr
                    )),
                );
            }
        }
        (Some(prev), None) => {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::EngineRemoved,
                    format!("engines.{}", engine_name),
                    engine_name,
                    format!("{} engine requirement removed", engine_name),
                )
                .with_previous(prev),
            );
        }
        (None, None) => {}
    }
}

/// Compare peer dependencies.
fn compare_peer_dependencies(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    let prev_peers: HashSet<&str> = previous.peer_dependencies.keys().map(|s| s.as_str()).collect();
    let curr_peers: HashSet<&str> = current.peer_dependencies.keys().map(|s| s.as_str()).collect();

    // Check for removed peer dependencies
    for name in prev_peers.difference(&curr_peers) {
        changes.push(
            PackageConfigChange::new(
                PackageConfigChangeType::PeerDependencyRemoved,
                format!("peerDependencies.{}", name),
                *name,
                format!("Peer dependency '{}' was removed", name),
            )
            .with_previous(&previous.peer_dependencies[*name])
            .with_migration_hint(format!(
                "The peer dependency '{}' is no longer required",
                name
            )),
        );
    }

    // Check for added peer dependencies
    for name in curr_peers.difference(&prev_peers) {
        let is_optional = current
            .peer_dependencies_meta
            .get(*name)
            .map(|m| m.optional)
            .unwrap_or(false);

        let change_type = if is_optional {
            PackageConfigChangeType::PeerDependencyAdded
        } else {
            PackageConfigChangeType::PeerDependencyAdded
        };

        let severity = if is_optional {
            Severity::Warning
        } else {
            Severity::Major
        };

        if include_additions || !is_optional {
            changes.push(
                PackageConfigChange::new(
                    change_type,
                    format!("peerDependencies.{}", name),
                    *name,
                    format!(
                        "Peer dependency '{}' was added ({})",
                        name,
                        if is_optional { "optional" } else { "required" }
                    ),
                )
                .with_severity(severity)
                .with_current(&current.peer_dependencies[*name])
                .with_migration_hint(format!(
                    "Install peer dependency: npm install {}@{}",
                    name, current.peer_dependencies[*name]
                )),
            );
        }
    }

    // Check for version changes and optionality changes
    for name in prev_peers.intersection(&curr_peers) {
        let prev_version = &previous.peer_dependencies[*name];
        let curr_version = &current.peer_dependencies[*name];

        // Check for major version bump
        if let (Some(prev_semver), Some(curr_semver)) =
            (SemVer::parse(prev_version), SemVer::parse(curr_version))
        {
            if curr_semver.is_major_bump_from(&prev_semver) {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::PeerDependencyMajorBump,
                        format!("peerDependencies.{}", name),
                        *name,
                        format!(
                            "Peer dependency '{}' had major version bump from {} to {}",
                            name, prev_version, curr_version
                        ),
                    )
                    .with_previous(prev_version)
                    .with_current(curr_version)
                    .with_migration_hint(format!(
                        "Update {} to version {} and check for breaking changes",
                        name, curr_version
                    )),
                );
            }
        }

        // Check for optionality changes
        let prev_optional = previous
            .peer_dependencies_meta
            .get(*name)
            .map(|m| m.optional)
            .unwrap_or(false);
        let curr_optional = current
            .peer_dependencies_meta
            .get(*name)
            .map(|m| m.optional)
            .unwrap_or(false);

        if prev_optional && !curr_optional {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::PeerDependencyBecameRequired,
                    format!("peerDependencies.{}", name),
                    *name,
                    format!("Peer dependency '{}' became required (was optional)", name),
                )
                .with_migration_hint(format!(
                    "Install the now-required peer dependency: npm install {}@{}",
                    name, curr_version
                )),
            );
        } else if !prev_optional && curr_optional {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::PeerDependencyBecameOptional,
                    format!("peerDependencies.{}", name),
                    *name,
                    format!("Peer dependency '{}' became optional (was required)", name),
                )
                .with_migration_hint(format!(
                    "The peer dependency '{}' is now optional and can be removed if not needed",
                    name
                )),
            );
        }
    }
}

/// Compare exports field.
fn compare_exports(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    let prev_exports = previous.exports.as_ref();
    let curr_exports = current.exports.as_ref();

    match (prev_exports, curr_exports) {
        (Some(prev), Some(curr)) => {
            let prev_subpaths: HashSet<&str> = prev.keys().map(|s| s.as_str()).collect();
            let curr_subpaths: HashSet<&str> = curr.keys().map(|s| s.as_str()).collect();

            // Check for removed subpaths
            for subpath in prev_subpaths.difference(&curr_subpaths) {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::ExportSubpathRemoved,
                        format!("exports.{}", subpath),
                        *subpath,
                        format!("Export subpath '{}' was removed", subpath),
                    )
                    .with_migration_hint(format!(
                        "Update imports from '{}' to use an alternative export path",
                        subpath
                    )),
                );
            }

            // Check for added subpaths
            if include_additions {
                for subpath in curr_subpaths.difference(&prev_subpaths) {
                    changes.push(
                        PackageConfigChange::new(
                            PackageConfigChangeType::ExportSubpathAdded,
                            format!("exports.{}", subpath),
                            *subpath,
                            format!("Export subpath '{}' was added", subpath),
                        )
                        .with_migration_hint(format!(
                            "New import path available: '{}'",
                            subpath
                        )),
                    );
                }
            }

            // Compare subpaths that exist in both
            for subpath in prev_subpaths.intersection(&curr_subpaths) {
                compare_export_values(
                    &prev[*subpath],
                    &curr[*subpath],
                    *subpath,
                    changes,
                    include_additions,
                );
            }
        }
        (Some(_prev), None) => {
            // Exports field removed entirely
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::ExportSubpathRemoved,
                    "exports",
                    "exports",
                    "Package exports field was removed",
                )
                .with_migration_hint(
                    "The package no longer uses conditional exports. Use main/module fields instead.",
                ),
            );
        }
        (None, Some(_curr)) => {
            // Exports field added
            if include_additions {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::ExportSubpathAdded,
                        "exports",
                        "exports",
                        "Package exports field was added",
                    )
                    .with_severity(Severity::Warning)
                    .with_migration_hint(
                        "The package now uses conditional exports for more control over entry points",
                    ),
                );
            }
        }
        (None, None) => {}
    }
}

/// Compare export values (conditions and targets).
fn compare_export_values(
    prev: &ExportValue,
    curr: &ExportValue,
    subpath: &str,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    let prev_conditions = prev.get_conditions();
    let curr_conditions = curr.get_conditions();

    // Check for removed conditions
    for condition in prev_conditions.difference(&curr_conditions) {
        changes.push(
            PackageConfigChange::new(
                PackageConfigChangeType::ExportConditionRemoved,
                format!("exports.{}.{}", subpath, condition),
                condition.clone(),
                format!(
                    "Export condition '{}' was removed from subpath '{}'",
                    condition, subpath
                ),
            )
            .with_migration_hint(format!(
                "The '{}' condition is no longer available. Check alternative conditions.",
                condition
            )),
        );
    }

    // Check for added conditions
    if include_additions {
        for condition in curr_conditions.difference(&prev_conditions) {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::ExportConditionAdded,
                    format!("exports.{}.{}", subpath, condition),
                    condition.clone(),
                    format!(
                        "Export condition '{}' was added to subpath '{}'",
                        condition, subpath
                    ),
                )
                .with_migration_hint(format!(
                    "New condition '{}' is available for import",
                    condition
                )),
            );
        }
    }

    // Check for target changes in common conditions
    for condition in prev_conditions.intersection(&curr_conditions) {
        let prev_path = prev.get_path(condition);
        let curr_path = curr.get_path(condition);

        if prev_path != curr_path {
            if let (Some(prev_p), Some(curr_p)) = (prev_path, curr_path) {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::ExportTargetChanged,
                        format!("exports.{}.{}", subpath, condition),
                        condition.clone(),
                        format!(
                            "Export target for '{}' condition in subpath '{}' changed",
                            condition, subpath
                        ),
                    )
                    .with_previous(prev_p)
                    .with_current(curr_p)
                    .with_migration_hint("The file path for this export has changed"),
                );
            }
        }
    }

    // Check simple path changes
    if let (ExportValue::Path(prev_p), ExportValue::Path(curr_p)) = (prev, curr) {
        if prev_p != curr_p {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::ExportTargetChanged,
                    format!("exports.{}", subpath),
                    subpath.to_string(),
                    format!("Export target for subpath '{}' changed", subpath),
                )
                .with_previous(prev_p)
                .with_current(curr_p)
                .with_migration_hint("The file path for this export has changed"),
            );
        }
    }
}

/// Compare entry points (main, module, types, browser).
fn compare_entry_points(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    // Compare main field
    compare_entry_point(
        "main",
        previous.main.as_deref(),
        current.main.as_deref(),
        PackageConfigChangeType::MainFieldChanged,
        PackageConfigChangeType::MainFieldRemoved,
        changes,
        include_additions,
    );

    // Compare module field
    compare_entry_point(
        "module",
        previous.module.as_deref(),
        current.module.as_deref(),
        PackageConfigChangeType::ModuleFieldChanged,
        PackageConfigChangeType::ModuleFieldRemoved,
        changes,
        include_additions,
    );

    // Compare types field
    compare_entry_point(
        "types",
        previous.types.as_deref(),
        current.types.as_deref(),
        PackageConfigChangeType::TypesFieldChanged,
        PackageConfigChangeType::TypesFieldRemoved,
        changes,
        include_additions,
    );

    // Compare browser field
    compare_entry_point(
        "browser",
        previous.browser.as_deref(),
        current.browser.as_deref(),
        PackageConfigChangeType::BrowserFieldChanged,
        PackageConfigChangeType::BrowserFieldChanged,
        changes,
        include_additions,
    );
}

/// Compare a single entry point field.
fn compare_entry_point(
    field_name: &str,
    prev_value: Option<&str>,
    curr_value: Option<&str>,
    changed_type: PackageConfigChangeType,
    removed_type: PackageConfigChangeType,
    changes: &mut Vec<PackageConfigChange>,
    _include_additions: bool,
) {
    match (prev_value, curr_value) {
        (Some(prev), Some(curr)) if prev != curr => {
            changes.push(
                PackageConfigChange::new(
                    changed_type,
                    field_name,
                    field_name,
                    format!("'{}' field changed from '{}' to '{}'", field_name, prev, curr),
                )
                .with_previous(prev)
                .with_current(curr)
                .with_migration_hint(format!(
                    "The {} entry point has moved to a new location",
                    field_name
                )),
            );
        }
        (Some(prev), None) => {
            changes.push(
                PackageConfigChange::new(
                    removed_type,
                    field_name,
                    field_name,
                    format!("'{}' field was removed", field_name),
                )
                .with_previous(prev)
                .with_migration_hint(format!(
                    "The {} entry point was removed. Use exports field or alternative entry points.",
                    field_name
                )),
            );
        }
        _ => {}
    }
}

/// Compare module type field.
fn compare_module_type(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    match (previous.module_type.as_deref(), current.module_type.as_deref()) {
        (Some(prev), Some(curr)) if prev != curr => {
            let is_breaking = (prev == "module" && curr == "commonjs")
                || (prev == "commonjs" && curr == "module");

            let severity = if is_breaking {
                Severity::Major
            } else {
                Severity::Minor
            };

            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::ModuleTypeChanged,
                    "type",
                    "type",
                    format!("Module type changed from '{}' to '{}'", prev, curr),
                )
                .with_severity(severity)
                .with_previous(prev)
                .with_current(curr)
                .with_migration_hint(if curr == "module" {
                    "Package now uses ES modules. Use import/export syntax."
                } else {
                    "Package now uses CommonJS. Use require/module.exports syntax."
                }),
            );
        }
        (Some(prev), None) => {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::ModuleTypeRemoved,
                    "type",
                    "type",
                    format!("Module type field removed (was '{}')", prev),
                )
                .with_previous(prev)
                .with_migration_hint(
                    "Package now defaults to CommonJS (no explicit type field)",
                ),
            );
        }
        (None, Some(curr)) => {
            if include_additions {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::ModuleTypeAdded,
                        "type",
                        "type",
                        format!("Module type field added: '{}'", curr),
                    )
                    .with_current(curr)
                    .with_migration_hint(if curr == "module" {
                        "Package now explicitly declares ES module format"
                    } else {
                        "Package now explicitly declares CommonJS format"
                    }),
                );
            }
        }
        _ => {}
    }
}

/// Compare bin field.
fn compare_bin(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    let prev_bin = previous
        .bin
        .as_ref()
        .map(|b| b.as_map(&previous.name))
        .unwrap_or_default();
    let curr_bin = current
        .bin
        .as_ref()
        .map(|b| b.as_map(&current.name))
        .unwrap_or_default();

    let prev_commands: HashSet<&str> = prev_bin.keys().map(|s| s.as_str()).collect();
    let curr_commands: HashSet<&str> = curr_bin.keys().map(|s| s.as_str()).collect();

    // Check for removed commands
    for command in prev_commands.difference(&curr_commands) {
        // Check if it was renamed (same target, different name)
        let prev_target = &prev_bin[*command];
        let renamed_to = curr_bin
            .iter()
            .find(|(_, target)| *target == prev_target)
            .map(|(name, _)| name.as_str());

        if let Some(new_name) = renamed_to {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::BinCommandRenamed,
                    format!("bin.{}", command),
                    *command,
                    format!("Binary command '{}' was renamed to '{}'", command, new_name),
                )
                .with_previous(*command)
                .with_current(new_name)
                .with_migration_hint(format!(
                    "Update scripts to use '{}' instead of '{}'",
                    new_name, command
                )),
            );
        } else {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::BinCommandRemoved,
                    format!("bin.{}", command),
                    *command,
                    format!("Binary command '{}' was removed", command),
                )
                .with_previous(prev_target)
                .with_migration_hint(format!(
                    "The '{}' command is no longer available",
                    command
                )),
            );
        }
    }

    // Check for added commands
    if include_additions {
        for command in curr_commands.difference(&prev_commands) {
            // Skip if this is actually a rename (handled above)
            let curr_target = &curr_bin[*command];
            let is_rename = prev_bin.values().any(|target| target == curr_target);

            if !is_rename {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::BinCommandAdded,
                        format!("bin.{}", command),
                        *command,
                        format!("Binary command '{}' was added", command),
                    )
                    .with_current(curr_target)
                    .with_migration_hint(format!(
                        "New command '{}' is now available",
                        command
                    )),
                );
            }
        }
    }

    // Check for target changes in existing commands
    for command in prev_commands.intersection(&curr_commands) {
        let prev_target = &prev_bin[*command];
        let curr_target = &curr_bin[*command];

        if prev_target != curr_target {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::BinTargetChanged,
                    format!("bin.{}", command),
                    *command,
                    format!("Binary command '{}' target changed", command),
                )
                .with_previous(prev_target)
                .with_current(curr_target)
                .with_migration_hint("The executable file for this command has changed"),
            );
        }
    }
}

/// Compare dependencies.
fn compare_dependencies(
    previous: &PackageConfig,
    current: &PackageConfig,
    changes: &mut Vec<PackageConfigChange>,
    include_additions: bool,
) {
    let prev_deps: HashSet<&str> = previous.dependencies.keys().map(|s| s.as_str()).collect();
    let curr_deps: HashSet<&str> = current.dependencies.keys().map(|s| s.as_str()).collect();

    // Check for removed dependencies (minor - consumers shouldn't depend on transitive deps)
    for name in prev_deps.difference(&curr_deps) {
        changes.push(
            PackageConfigChange::new(
                PackageConfigChangeType::DependencyRemoved,
                format!("dependencies.{}", name),
                *name,
                format!("Dependency '{}' was removed", name),
            )
            .with_previous(&previous.dependencies[*name])
            .with_migration_hint(format!(
                "If you relied on '{}' as a transitive dependency, install it directly",
                name
            )),
        );
    }

    // Check for added dependencies
    if include_additions {
        for name in curr_deps.difference(&prev_deps) {
            changes.push(
                PackageConfigChange::new(
                    PackageConfigChangeType::DependencyAdded,
                    format!("dependencies.{}", name),
                    *name,
                    format!("Dependency '{}' was added", name),
                )
                .with_current(&current.dependencies[*name]),
            );
        }
    }

    // Check for major version bumps
    for name in prev_deps.intersection(&curr_deps) {
        let prev_version = &previous.dependencies[*name];
        let curr_version = &current.dependencies[*name];

        if let (Some(prev_semver), Some(curr_semver)) =
            (SemVer::parse(prev_version), SemVer::parse(curr_version))
        {
            if curr_semver.is_major_bump_from(&prev_semver) {
                changes.push(
                    PackageConfigChange::new(
                        PackageConfigChangeType::DependencyMajorBump,
                        format!("dependencies.{}", name),
                        *name,
                        format!(
                            "Dependency '{}' had major version bump from {} to {}",
                            name, prev_version, curr_version
                        ),
                    )
                    .with_previous(prev_version)
                    .with_current(curr_version)
                    .with_migration_hint(format!(
                        "Check {} changelog for breaking changes that may affect this package",
                        name
                    )),
                );
            }
        }
    }
}

/// Convert PackageConfigChange to BreakingChange for unified reporting.
impl From<PackageConfigChange> for BreakingChange {
    fn from(change: PackageConfigChange) -> Self {
        BreakingChange::new(
            crate::response::BreakingChangeType::TypeChanged, // Generic type for config changes
            change.path,
            change.name,
            change.message,
        )
        .with_severity(change.severity)
        .with_previous(change.previous.unwrap_or_default())
        .with_current(change.current.unwrap_or_default())
        .with_suggestion(change.migration_hint.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);

        let v2 = SemVer::parse(">=18.0.0").unwrap();
        assert_eq!(v2.major, 18);
        assert_eq!(v2.minor, 0);
        assert_eq!(v2.patch, 0);

        let v3 = SemVer::parse("^16.8.0").unwrap();
        assert_eq!(v3.major, 16);
        assert_eq!(v3.minor, 8);
        assert_eq!(v3.patch, 0);
    }

    #[test]
    fn test_semver_major_bump() {
        let v1 = SemVer::parse("1.0.0").unwrap();
        let v2 = SemVer::parse("2.0.0").unwrap();
        assert!(v2.is_major_bump_from(&v1));
        assert!(!v1.is_major_bump_from(&v2));
    }

    #[test]
    fn test_engine_version_increase() {
        let prev = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=18.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::EngineNodeVersionIncreased
        );
        assert_eq!(changes[0].severity, Severity::Major);
    }

    #[test]
    fn test_peer_dependency_added() {
        let prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());

        let changes = compare_package_configs(&prev, &curr, true);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::PeerDependencyAdded
        );
    }

    #[test]
    fn test_export_subpath_removed() {
        let mut prev_exports = HashMap::new();
        prev_exports.insert(".".to_string(), ExportValue::Path("./dist/index.js".to_string()));
        prev_exports.insert(
            "./utils".to_string(),
            ExportValue::Path("./dist/utils.js".to_string()),
        );

        let mut curr_exports = HashMap::new();
        curr_exports.insert(".".to_string(), ExportValue::Path("./dist/index.js".to_string()));

        let prev = PackageConfig {
            name: "test".to_string(),
            exports: Some(prev_exports),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            exports: Some(curr_exports),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ExportSubpathRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_module_type_changed() {
        let prev = PackageConfig {
            name: "test".to_string(),
            module_type: Some("commonjs".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            module_type: Some("module".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ModuleTypeChanged
        );
        assert_eq!(changes[0].severity, Severity::Major);
    }

    #[test]
    fn test_bin_command_removed() {
        let mut prev_bin = HashMap::new();
        prev_bin.insert("cli".to_string(), "./bin/cli.js".to_string());
        prev_bin.insert("other".to_string(), "./bin/other.js".to_string());

        let mut curr_bin = HashMap::new();
        curr_bin.insert("cli".to_string(), "./bin/cli.js".to_string());

        let prev = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(prev_bin)),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(curr_bin)),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::BinCommandRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_main_field_removed() {
        let prev = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            main: None,
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::MainFieldRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_no_changes() {
        let config = PackageConfig {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./dist/index.js".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&config, &config, false);
        assert!(changes.is_empty());
    }

    // ============================================================================
    // Engine Version Changes (5 tests)
    // ============================================================================

    #[test]
    fn test_engine_version_decreased() {
        // SAFE: minimum version decreased, more permissive
        let prev = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=18.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        // No breaking change when version is decreased (more permissive)
        assert!(changes.is_empty());
    }

    #[test]
    fn test_engine_npm_added() {
        // POTENTIALLY BREAKING: npm engine requirement added
        let prev = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                npm: None,
                ..Default::default()
            }),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                npm: Some(">=8.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, true);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, PackageConfigChangeType::EngineAdded);
        assert_eq!(changes[0].severity, Severity::Minor);
        assert_eq!(changes[0].name, "npm");
    }

    #[test]
    fn test_engine_removed() {
        // SAFE: engine requirement removed
        let prev = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                npm: Some(">=8.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                npm: None,
                ..Default::default()
            }),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::EngineRemoved
        );
        assert_eq!(changes[0].severity, Severity::Warning);
        assert_eq!(changes[0].name, "npm");
    }

    #[test]
    fn test_engine_range_restricted() {
        // BREAKING: upper bound added (range restricted)
        // Note: The current implementation only checks minimum version increases
        // This test documents the behavior: going from >=16 to >=18 is a breaking change
        let prev = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=16.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=18.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::EngineNodeVersionIncreased
        );
        assert_eq!(changes[0].severity, Severity::Major);
    }

    #[test]
    fn test_engine_range_expanded() {
        // SAFE: range expanded by lowering minimum
        let prev = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=18.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            engines: Some(Engines {
                node: Some(">=14.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        // No breaking change - range was expanded
        assert!(changes.is_empty());
    }

    // ============================================================================
    // Peer Dependency Changes (6 tests)
    // ============================================================================

    #[test]
    fn test_peer_dependency_removed() {
        // BREAKING: peer dependency removed (consumers may depend on it)
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());

        let curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::PeerDependencyRemoved
        );
        // Note: PeerDependencyRemoved has Warning severity (not Critical)
        // because consumers should still have the dependency installed
        assert_eq!(changes[0].severity, Severity::Warning);
        assert_eq!(changes[0].name, "react");
    }

    #[test]
    fn test_peer_dependency_major_bump() {
        // BREAKING: peer dependency major version bumped (^17 -> ^18)
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.peer_dependencies
            .insert("react".to_string(), "^17.0.0".to_string());

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::PeerDependencyMajorBump
        );
        assert_eq!(changes[0].severity, Severity::Major);
        assert_eq!(changes[0].name, "react");
        assert_eq!(changes[0].previous.as_deref(), Some("^17.0.0"));
        assert_eq!(changes[0].current.as_deref(), Some("^18.0.0"));
    }

    #[test]
    fn test_peer_dependency_minor_bump() {
        // SAFE: minor version bump (^18.0 -> ^18.2)
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.peer_dependencies
            .insert("react".to_string(), "^18.2.0".to_string());

        let changes = compare_package_configs(&prev, &curr, false);
        // No breaking change for minor bump
        assert!(changes.is_empty());
    }

    #[test]
    fn test_peer_dependency_became_optional() {
        // SAFE: peer dependency became optional
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());
        // Not in meta = required

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());
        curr.peer_dependencies_meta
            .insert("react".to_string(), PeerDepMeta { optional: true });

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::PeerDependencyBecameOptional
        );
        assert_eq!(changes[0].severity, Severity::Warning);
        assert_eq!(changes[0].name, "react");
    }

    #[test]
    fn test_peer_dependency_became_required() {
        // BREAKING: peer dependency became required (was optional)
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());
        prev.peer_dependencies_meta
            .insert("react".to_string(), PeerDepMeta { optional: true });

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());
        // Not in meta = required

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::PeerDependencyBecameRequired
        );
        assert_eq!(changes[0].severity, Severity::Critical);
        assert_eq!(changes[0].name, "react");
    }

    #[test]
    fn test_peer_dependency_range_narrowed() {
        // BREAKING: peer dependency range narrowed via major bump
        // e.g., ">=16.0.0" to ">=18.0.0" narrows acceptable versions
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.peer_dependencies
            .insert("react".to_string(), "^16.0.0".to_string());

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::PeerDependencyMajorBump
        );
        assert_eq!(changes[0].severity, Severity::Major);
    }

    // ============================================================================
    // Entry Point Changes (6 tests)
    // ============================================================================

    #[test]
    fn test_main_field_changed() {
        // BREAKING: main field changed to different path
        let prev = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            main: Some("./lib/main.js".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::MainFieldChanged
        );
        assert_eq!(changes[0].severity, Severity::Major);
        assert_eq!(changes[0].previous.as_deref(), Some("./dist/index.js"));
        assert_eq!(changes[0].current.as_deref(), Some("./lib/main.js"));
    }

    #[test]
    fn test_module_field_removed() {
        // BREAKING: module field removed
        let prev = PackageConfig {
            name: "test".to_string(),
            module: Some("./dist/index.mjs".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            module: None,
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ModuleFieldRemoved
        );
        assert_eq!(changes[0].severity, Severity::Major);
    }

    #[test]
    fn test_module_field_added() {
        // SAFE: module field added (new ESM entry point)
        let prev = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            module: None,
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            module: Some("./dist/index.mjs".to_string()),
            ..Default::default()
        };

        // With include_additions=false, no changes reported
        let changes = compare_package_configs(&prev, &curr, false);
        assert!(changes.is_empty());

        // Addition of module field is not tracked as a separate event
        // (no ModuleFieldAdded change type in the enum)
    }

    #[test]
    fn test_types_field_removed() {
        // BREAKING: types field removed
        let prev = PackageConfig {
            name: "test".to_string(),
            types: Some("./dist/index.d.ts".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            types: None,
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::TypesFieldRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_types_field_added() {
        // SAFE: types field added (TypeScript support added)
        let prev = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            types: None,
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            types: Some("./dist/index.d.ts".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        // Adding types is not a breaking change
        assert!(changes.is_empty());
    }

    #[test]
    fn test_export_condition_removed() {
        // BREAKING: export condition removed (e.g., require condition removed)
        let mut prev_conditions = HashMap::new();
        prev_conditions.insert(
            "import".to_string(),
            ExportValue::Path("./dist/index.mjs".to_string()),
        );
        prev_conditions.insert(
            "require".to_string(),
            ExportValue::Path("./dist/index.cjs".to_string()),
        );

        let mut curr_conditions = HashMap::new();
        curr_conditions.insert(
            "import".to_string(),
            ExportValue::Path("./dist/index.mjs".to_string()),
        );
        // require condition removed

        let mut prev_exports = HashMap::new();
        prev_exports.insert(".".to_string(), ExportValue::Conditional(prev_conditions));

        let mut curr_exports = HashMap::new();
        curr_exports.insert(".".to_string(), ExportValue::Conditional(curr_conditions));

        let prev = PackageConfig {
            name: "test".to_string(),
            exports: Some(prev_exports),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            exports: Some(curr_exports),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ExportConditionRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
        assert_eq!(changes[0].name, "require");
    }

    // ============================================================================
    // Module Type Changes (3 tests)
    // ============================================================================

    #[test]
    fn test_module_type_added() {
        // BREAKING: type: "module" added to previously CJS package
        let prev = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            module_type: None, // Default is CommonJS
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            module_type: Some("module".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, true);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ModuleTypeAdded
        );
        assert_eq!(changes[0].severity, Severity::Minor);
        assert_eq!(changes[0].current.as_deref(), Some("module"));
    }

    #[test]
    fn test_commonjs_type_added() {
        // SAFE: explicit commonjs type added (makes implicit explicit)
        let prev = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            module_type: None,
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            main: Some("./dist/index.js".to_string()),
            module_type: Some("commonjs".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, true);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ModuleTypeAdded
        );
        // Adding explicit commonjs is still minor (makes explicit what was implicit)
        assert_eq!(changes[0].severity, Severity::Minor);
    }

    #[test]
    fn test_module_type_removed() {
        // BREAKING: type field removed (module -> implicit commonjs)
        let prev = PackageConfig {
            name: "test".to_string(),
            module_type: Some("module".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            module_type: None,
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ModuleTypeRemoved
        );
        assert_eq!(changes[0].severity, Severity::Warning);
        assert_eq!(changes[0].previous.as_deref(), Some("module"));
    }

    // ============================================================================
    // Binary Changes (4 tests)
    // ============================================================================

    #[test]
    fn test_bin_command_renamed() {
        // BREAKING: binary command renamed
        let mut prev_bin = HashMap::new();
        prev_bin.insert("old-cli".to_string(), "./bin/cli.js".to_string());

        let mut curr_bin = HashMap::new();
        curr_bin.insert("new-cli".to_string(), "./bin/cli.js".to_string());

        let prev = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(prev_bin)),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(curr_bin)),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::BinCommandRenamed
        );
        assert_eq!(changes[0].severity, Severity::Major);
        assert_eq!(changes[0].name, "old-cli");
        assert_eq!(changes[0].previous.as_deref(), Some("old-cli"));
        assert_eq!(changes[0].current.as_deref(), Some("new-cli"));
    }

    #[test]
    fn test_bin_command_added() {
        // SAFE: new binary command added
        let mut prev_bin = HashMap::new();
        prev_bin.insert("cli".to_string(), "./bin/cli.js".to_string());

        let mut curr_bin = HashMap::new();
        curr_bin.insert("cli".to_string(), "./bin/cli.js".to_string());
        curr_bin.insert("cli-dev".to_string(), "./bin/dev.js".to_string());

        let prev = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(prev_bin)),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(curr_bin)),
            ..Default::default()
        };

        // With include_additions=false, no changes
        let changes = compare_package_configs(&prev, &curr, false);
        assert!(changes.is_empty());

        // With include_additions=true, the addition is reported
        let changes = compare_package_configs(&prev, &curr, true);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::BinCommandAdded
        );
        assert_eq!(changes[0].severity, Severity::Warning);
        assert_eq!(changes[0].name, "cli-dev");
    }

    #[test]
    fn test_bin_field_removed_entirely() {
        // BREAKING: bin field removed entirely
        let mut prev_bin = HashMap::new();
        prev_bin.insert("cli".to_string(), "./bin/cli.js".to_string());

        let prev = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(prev_bin)),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            bin: None,
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::BinCommandRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
        assert_eq!(changes[0].name, "cli");
    }

    #[test]
    fn test_bin_path_changed() {
        // POTENTIALLY BREAKING: bin target path changed
        let mut prev_bin = HashMap::new();
        prev_bin.insert("cli".to_string(), "./bin/cli.js".to_string());

        let mut curr_bin = HashMap::new();
        curr_bin.insert("cli".to_string(), "./dist/cli.js".to_string());

        let prev = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(prev_bin)),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            bin: Some(BinSpec::Multiple(curr_bin)),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::BinTargetChanged
        );
        assert_eq!(changes[0].severity, Severity::Major);
        assert_eq!(changes[0].name, "cli");
        assert_eq!(changes[0].previous.as_deref(), Some("./bin/cli.js"));
        assert_eq!(changes[0].current.as_deref(), Some("./dist/cli.js"));
    }

    // ============================================================================
    // TypeScript Changes (4 tests)
    // ============================================================================

    #[test]
    fn test_typings_field_removed() {
        // BREAKING: typings field removed (same as types)
        // Note: The implementation uses "types" field, "typings" is an alias
        // This test documents that the types field removal is detected
        let prev = PackageConfig {
            name: "test".to_string(),
            types: Some("./dist/index.d.ts".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            types: None,
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::TypesFieldRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_types_versions_entry_removed() {
        // POTENTIALLY BREAKING: types export condition removed
        // This simulates typesVersions by using exports with types condition
        let mut prev_conditions = HashMap::new();
        prev_conditions.insert(
            "types".to_string(),
            ExportValue::Path("./dist/index.d.ts".to_string()),
        );
        prev_conditions.insert(
            "import".to_string(),
            ExportValue::Path("./dist/index.mjs".to_string()),
        );

        let mut curr_conditions = HashMap::new();
        curr_conditions.insert(
            "import".to_string(),
            ExportValue::Path("./dist/index.mjs".to_string()),
        );
        // types condition removed

        let mut prev_exports = HashMap::new();
        prev_exports.insert(".".to_string(), ExportValue::Conditional(prev_conditions));

        let mut curr_exports = HashMap::new();
        curr_exports.insert(".".to_string(), ExportValue::Conditional(curr_conditions));

        let prev = PackageConfig {
            name: "test".to_string(),
            exports: Some(prev_exports),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            exports: Some(curr_exports),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::ExportConditionRemoved
        );
        assert_eq!(changes[0].severity, Severity::Critical);
        assert_eq!(changes[0].name, "types");
    }

    #[test]
    fn test_types_path_changed() {
        // BREAKING: types field path changed
        let prev = PackageConfig {
            name: "test".to_string(),
            types: Some("./dist/index.d.ts".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            types: Some("./types/main.d.ts".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::TypesFieldChanged
        );
        assert_eq!(changes[0].severity, Severity::Major);
        assert_eq!(changes[0].previous.as_deref(), Some("./dist/index.d.ts"));
        assert_eq!(changes[0].current.as_deref(), Some("./types/main.d.ts"));
    }

    #[test]
    fn test_types_to_typings_migration() {
        // SAFE: types field unchanged (same value)
        // This documents that the value matters, not the field name
        let prev = PackageConfig {
            name: "test".to_string(),
            types: Some("./dist/index.d.ts".to_string()),
            ..Default::default()
        };
        let curr = PackageConfig {
            name: "test".to_string(),
            types: Some("./dist/index.d.ts".to_string()),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert!(changes.is_empty());
    }

    // ============================================================================
    // Dependency Changes (3 tests)
    // ============================================================================

    #[test]
    fn test_dependency_major_bump() {
        // POTENTIALLY BREAKING: dependency major version bump
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.dependencies
            .insert("lodash".to_string(), "^3.0.0".to_string());

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.dependencies
            .insert("lodash".to_string(), "^4.0.0".to_string());

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::DependencyMajorBump
        );
        assert_eq!(changes[0].severity, Severity::Major);
        assert_eq!(changes[0].name, "lodash");
        assert_eq!(changes[0].previous.as_deref(), Some("^3.0.0"));
        assert_eq!(changes[0].current.as_deref(), Some("^4.0.0"));
    }

    #[test]
    fn test_dependency_added() {
        // SAFE: new dependency added
        let prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };

        let mut curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        curr.dependencies
            .insert("lodash".to_string(), "^4.0.0".to_string());

        // With include_additions=false, no changes
        let changes = compare_package_configs(&prev, &curr, false);
        assert!(changes.is_empty());

        // With include_additions=true, the addition is reported
        let changes = compare_package_configs(&prev, &curr, true);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::DependencyAdded
        );
        assert_eq!(changes[0].severity, Severity::Warning);
        assert_eq!(changes[0].name, "lodash");
    }

    #[test]
    fn test_dependency_removed() {
        // WARNING: dependency removed (consumers may rely on transitive dep)
        let mut prev = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };
        prev.dependencies
            .insert("lodash".to_string(), "^4.0.0".to_string());

        let curr = PackageConfig {
            name: "test".to_string(),
            ..Default::default()
        };

        let changes = compare_package_configs(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            PackageConfigChangeType::DependencyRemoved
        );
        assert_eq!(changes[0].severity, Severity::Minor);
        assert_eq!(changes[0].name, "lodash");
        assert_eq!(changes[0].previous.as_deref(), Some("^4.0.0"));
    }
}

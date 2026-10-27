//! Prepare npm dependencies without executing package scripts or changing lockfiles.

use crate::ecosystems::node::npm;
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    #[error("dependency installation timed out")]
    Timeout,
    #[error("{0}")]
    Failed(String),
}

/// Install analysis dependencies without creating or rewriting package lockfiles.
/// Returns whether the package declares dependencies that needed preparation.
pub async fn prepare_dependencies(root_dir: &Path) -> Result<bool, DependencyError> {
    let manifest = std::fs::read(root_dir.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let has_dependencies = manifest.as_ref().is_some_and(|manifest| {
        ["dependencies", "optionalDependencies", "peerDependencies"]
            .iter()
            .any(|field| {
                manifest
                    .get(field)
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|entries| !entries.is_empty())
            })
    });
    if !has_dependencies {
        return Ok(false);
    }
    let mut command = npm::command(root_dir);
    command
        .args([
            "install",
            "--omit=dev",
            "--ignore-scripts",
            "--package-lock=false",
            "--no-audit",
            "--no-fund",
            "--workspaces=false",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(std::time::Duration::from_secs(120), command.output())
        .await
        .map_err(|_| DependencyError::Timeout)?
        .map_err(|error| {
            DependencyError::Failed(format!("dependency installation failed: {error}"))
        })?;
    if !output.status.success() {
        return Err(DependencyError::Failed(format!(
            "dependency installation exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(true)
}

#[cfg(all(test, feature = "lsp-tests"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dependency_installation_preserves_package_inputs() {
        let directory = tempfile::tempdir().unwrap();
        let dependency = directory.path().join("dependency");
        let package = directory.path().join("package");
        std::fs::create_dir(&dependency).unwrap();
        std::fs::create_dir(&package).unwrap();
        std::fs::write(
            dependency.join("package.json"),
            r#"{"name":"local-dependency","version":"1.0.0"}"#,
        )
        .unwrap();
        let manifest = r#"{"name":"test-package","version":"1.0.0","dependencies":{"local-dependency":"file:../dependency"}}"#;
        std::fs::write(package.join("package.json"), manifest).unwrap();
        prepare_dependencies(&package).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(package.join("package.json")).unwrap(),
            manifest
        );
        assert!(!package.join("package-lock.json").exists());
        assert!(package
            .join("node_modules/local-dependency/package.json")
            .exists());
        let lock = r#"{"name":"test-package","lockfileVersion":3,"packages":{}}"#;
        std::fs::write(package.join("package-lock.json"), lock).unwrap();
        prepare_dependencies(&package).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(package.join("package-lock.json")).unwrap(),
            lock
        );
    }
}

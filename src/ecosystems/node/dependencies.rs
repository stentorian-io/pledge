//! Prepare npm dependencies without executing package scripts or changing lockfiles.

use crate::ecosystems::node::npm;
use serde_json::Value;
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
    let root = root_dir.canonicalize().map_err(|error| {
        DependencyError::Failed(format!("cannot open dependency workspace: {error}"))
    })?;
    let parent = root.parent().ok_or_else(|| {
        DependencyError::Failed("dependency workspace must have a parent directory".into())
    })?;
    // npm resolves development peer obligations even with --omit=dev. Install
    // a runtime-only manifest beside the package, preserving the original
    // manifest and the depth of npm's relative local-dependency symlinks.
    let installation = tempfile::Builder::new()
        .prefix(".pledge-dependencies-")
        .tempdir_in(parent)
        .map_err(|error| {
            DependencyError::Failed(format!("cannot prepare dependency workspace: {error}"))
        })?;
    let manifest = manifest.ok_or_else(|| {
        DependencyError::Failed("package manifest disappeared before dependency preparation".into())
    })?;
    let prepared_manifest = runtime_manifest(manifest, &root);
    std::fs::write(
        installation.path().join("package.json"),
        prepared_manifest.to_string(),
    )
    .map_err(|error| {
        DependencyError::Failed(format!("cannot write dependency manifest: {error}"))
    })?;
    let npmrc = root.join(".npmrc");
    if npmrc.is_file() {
        std::fs::copy(npmrc, installation.path().join(".npmrc")).map_err(|error| {
            DependencyError::Failed(format!(
                "cannot preserve dependency npm configuration: {error}"
            ))
        })?;
    }
    // Keep npm's working directory so relative configuration paths (for
    // example cafile) still resolve against the original package.
    let mut command = npm::command(&root);
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
        .arg("--prefix")
        .arg(installation.path())
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
    install_prepared_modules(installation.path(), &root).map_err(|error| {
        DependencyError::Failed(format!("cannot install prepared dependencies: {error}"))
    })?;
    Ok(true)
}

fn runtime_manifest(mut manifest: Value, root: &Path) -> Value {
    if let Some(package) = manifest.as_object_mut() {
        package.remove("devDependencies");
        // Published dependency overrides are ignored by npm; retaining them here
        // would incorrectly apply them as policy of the temporary install root.
        package.remove("overrides");
        for field in ["dependencies", "optionalDependencies", "peerDependencies"] {
            if let Some(requirements) = package.get_mut(field) {
                resolve_local_requirements(requirements, root);
            }
        }
    }
    manifest
}

fn resolve_local_requirements(requirements: &mut Value, root: &Path) {
    match requirements {
        Value::String(specification) => {
            let local_path = specification.strip_prefix("file:").or_else(|| {
                let path = specification.as_str();
                (path.starts_with("./")
                    || path.starts_with("../")
                    || (!path.contains(':')
                        && (path.ends_with(".tgz") || path.ends_with(".tar.gz"))))
                .then_some(path)
            });
            if let Some(relative) = local_path {
                if Path::new(relative).is_relative() && !relative.starts_with('~') {
                    *specification = format!("file:{}", root.join(relative).display());
                }
            }
        }
        Value::Object(dependencies) => {
            for requirement in dependencies.values_mut() {
                resolve_local_requirements(requirement, root);
            }
        }
        _ => {}
    }
}

fn install_prepared_modules(installation: &Path, root: &Path) -> std::io::Result<()> {
    let prepared = installation.join("node_modules");
    std::fs::create_dir_all(&prepared)?;
    let destination = root.join("node_modules");
    let previous = installation.join("previous_node_modules");
    let replaced = match std::fs::rename(&destination, &previous) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };
    if let Err(error) = std::fs::rename(prepared, &destination) {
        if replaced {
            std::fs::rename(previous, destination)?;
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod manifest_tests {
    use super::runtime_manifest;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn runtime_installation_omits_development_and_root_policies_preserving_local_paths() {
        let manifest = json!({
            "name":"example", "version":"1.0.0",
            "devDependencies":{"unrelated-tool":"1.0.0"},
            "dependencies":{"local":"file:./dependency", "registry":"^2"},
            "peerDependencies":{"peer":"*"},
            "peerDependenciesMeta":{"peer":{"optional":true}},
            "overrides":{"local":"file:./dependency"}
        });

        let installation = runtime_manifest(manifest.clone(), Path::new("/package"));

        assert!(installation.get("devDependencies").is_none());
        assert_eq!(
            installation["dependencies"]["local"],
            "file:/package/./dependency"
        );
        assert_eq!(installation["dependencies"]["registry"], "^2");
        assert!(installation.get("overrides").is_none());
        assert_eq!(
            installation["peerDependenciesMeta"],
            manifest["peerDependenciesMeta"]
        );
        assert!(manifest.get("devDependencies").is_some());
    }
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

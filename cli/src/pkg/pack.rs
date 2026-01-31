//! Package manager detection and native packing utilities.
//!
//! This module provides functionality to:
//! - Detect which package manager is being used (npm, pnpm, yarn, bun)
//! - Execute native pack commands to create tarballs
//! - Read and process the resulting .tgz files

use anyhow::{bail, Context, Result};
use flate2::bufread::GzDecoder;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Archive;

/// Supported package managers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// Get the command name for this package manager.
    pub fn command(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    /// Get a human-readable name for this package manager.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "Yarn",
            Self::Bun => "Bun",
        }
    }

    /// Get the installation instructions for this package manager.
    pub fn install_instructions(&self) -> &'static str {
        match self {
            Self::Npm => "npm is included with Node.js. Install Node.js from https://nodejs.org/",
            Self::Pnpm => "Install pnpm with: npm install -g pnpm",
            Self::Yarn => "Install Yarn with: npm install -g yarn",
            Self::Bun => "Install Bun from https://bun.sh/",
        }
    }
}

/// Result of package manager detection.
#[derive(Debug)]
pub struct PackageManagerInfo {
    /// The detected package manager.
    pub manager: PackageManager,
    /// The full path to the package manager executable.
    pub path: PathBuf,
    /// The version of the package manager (if available).
    pub version: Option<String>,
}

/// Detect the package manager to use for a given package directory.
///
/// Detection order:
/// 1. Check for lockfiles in the directory (most reliable)
/// 2. Fall back to npm if available
///
/// # Arguments
/// * `pkg_dir` - The package directory to check
///
/// # Returns
/// The detected package manager info, or an error if none found.
pub fn detect_package_manager(pkg_dir: &Path) -> Result<PackageManagerInfo> {
    // Check for lockfiles first (most reliable indicator)
    let lockfile_managers = [
        ("bun.lockb", PackageManager::Bun),
        ("pnpm-lock.yaml", PackageManager::Pnpm),
        ("yarn.lock", PackageManager::Yarn),
        ("package-lock.json", PackageManager::Npm),
    ];

    for (lockfile, manager) in lockfile_managers {
        if pkg_dir.join(lockfile).exists() {
            if let Ok(info) = check_package_manager_installed(manager) {
                println!("Detected {} from lockfile: {}", manager.name(), lockfile);
                return Ok(info);
            }
        }
    }

    // Check packageManager field in package.json
    if let Ok(manager) = detect_from_package_json(pkg_dir) {
        if let Ok(info) = check_package_manager_installed(manager) {
            println!(
                "Detected {} from package.json packageManager field",
                manager.name()
            );
            return Ok(info);
        }
    }

    // Fall back to checking what's installed, prefer npm
    let fallback_order = [
        PackageManager::Npm,
        PackageManager::Pnpm,
        PackageManager::Yarn,
        PackageManager::Bun,
    ];

    for manager in fallback_order {
        if let Ok(info) = check_package_manager_installed(manager) {
            println!("Using {} as fallback package manager", manager.name());
            return Ok(info);
        }
    }

    bail!(
        "No package manager found. Please install one of the following:\n\
         - npm (included with Node.js): https://nodejs.org/\n\
         - pnpm: npm install -g pnpm\n\
         - yarn: npm install -g yarn\n\
         - bun: https://bun.sh/"
    );
}

/// Check if a specific package manager is installed.
fn check_package_manager_installed(manager: PackageManager) -> Result<PackageManagerInfo> {
    let cmd = manager.command();

    // Try to find the executable
    let path = find_executable(cmd)?;

    // Try to get version
    let version = get_package_manager_version(cmd);

    Ok(PackageManagerInfo {
        manager,
        path,
        version,
    })
}

/// Find an executable in PATH using `which` or `where` command.
fn find_executable(cmd: &str) -> Result<PathBuf> {
    #[cfg(unix)]
    let which_cmd = "which";
    #[cfg(windows)]
    let which_cmd = "where";

    let output = Command::new(which_cmd)
        .arg(cmd)
        .output()
        .with_context(|| format!("Failed to check for {} executable", cmd))?;

    if !output.status.success() {
        bail!("{} is not installed or not in PATH", cmd);
    }

    let path_str = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    if path_str.is_empty() {
        bail!("{} is not installed or not in PATH", cmd);
    }

    Ok(PathBuf::from(path_str))
}

/// Get the version of a package manager.
fn get_package_manager_version(cmd: &str) -> Option<String> {
    let output = Command::new(cmd).arg("--version").output().ok()?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();
        Some(version)
    } else {
        None
    }
}

/// Detect package manager from package.json packageManager field.
fn detect_from_package_json(pkg_dir: &Path) -> Result<PackageManager> {
    let pkg_json_path = pkg_dir.join("package.json");
    let content = fs::read_to_string(&pkg_json_path)
        .with_context(|| "Failed to read package.json")?;

    let pkg_json = json::parse(&content)
        .with_context(|| "Failed to parse package.json")?;

    let package_manager = pkg_json["packageManager"].as_str();

    if let Some(pm) = package_manager {
        // Format is typically "pnpm@8.0.0" or similar
        let pm_name = pm.split('@').next().unwrap_or(pm);

        match pm_name {
            "npm" => return Ok(PackageManager::Npm),
            "pnpm" => return Ok(PackageManager::Pnpm),
            "yarn" => return Ok(PackageManager::Yarn),
            "bun" => return Ok(PackageManager::Bun),
            _ => {}
        }
    }

    bail!("No packageManager field in package.json");
}

/// Result of packing a package.
#[derive(Debug)]
pub struct PackResult {
    /// The name of the generated tarball file.
    pub tarball_name: String,
    /// The raw tarball data (gzipped).
    pub tarball_data: Vec<u8>,
    /// The decompressed tar data.
    pub tar_data: Vec<u8>,
}

impl PackResult {
    /// Get the files in the tarball.
    pub fn get_files(&self) -> Result<Vec<PackedFile>> {
        let mut files = Vec::new();
        let mut archive = Archive::new(&self.tar_data[..]);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry
                .header()
                .path()?
                .strip_prefix("package")
                .unwrap_or(&entry.header().path()?)
                .to_path_buf();

            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;

            files.push(PackedFile { path, contents });
        }

        Ok(files)
    }

    /// Load a specific file from the tarball by path.
    pub fn load_file(&self, file_path: &Path) -> Result<Option<Vec<u8>>> {
        let mut archive = Archive::new(&self.tar_data[..]);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry
                .header()
                .path()?
                .strip_prefix("package")
                .unwrap_or(&entry.header().path()?)
                .to_path_buf();

            let file_path = normalize_path(file_path);

            if entry_path == file_path {
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer)?;
                return Ok(Some(buffer));
            }
        }

        Ok(None)
    }
}

/// A file from a packed tarball.
#[derive(Debug)]
pub struct PackedFile {
    /// The path of the file relative to package root.
    pub path: PathBuf,
    /// The file contents.
    pub contents: Vec<u8>,
}

/// Normalize a path by removing leading "./" or "/".
fn normalize_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix("./") {
        PathBuf::from(stripped)
    } else if let Some(stripped) = path_str.strip_prefix('/') {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Pack a package using the native package manager.
///
/// This function:
/// 1. Detects the package manager to use
/// 2. Executes the pack command
/// 3. Reads the resulting tarball
/// 4. Cleans up the tarball file
///
/// # Arguments
/// * `pkg_dir` - The package directory to pack
///
/// # Returns
/// The pack result containing the tarball data.
pub fn pack_package(pkg_dir: &Path) -> Result<PackResult> {
    let pm_info = detect_package_manager(pkg_dir)?;

    println!(
        "Packing with {} {}",
        pm_info.manager.name(),
        pm_info.version.as_deref().unwrap_or("(unknown version)")
    );

    // Execute pack command
    let tarball_name = execute_pack_command(&pm_info, pkg_dir)?;
    let tarball_path = pkg_dir.join(&tarball_name);

    // Read the tarball
    let tarball_data = fs::read(&tarball_path)
        .with_context(|| format!("Failed to read packed tarball: {}", tarball_path.display()))?;

    // Decompress the tarball
    let tar_data = decompress_gzip(&tarball_data)?;

    // Clean up the tarball file
    if let Err(e) = fs::remove_file(&tarball_path) {
        eprintln!(
            "Warning: Failed to clean up tarball file {}: {}",
            tarball_path.display(),
            e
        );
    }

    Ok(PackResult {
        tarball_name,
        tarball_data,
        tar_data,
    })
}

/// Execute the pack command for a package manager.
fn execute_pack_command(pm_info: &PackageManagerInfo, pkg_dir: &Path) -> Result<String> {
    let output = match pm_info.manager {
        PackageManager::Npm => Command::new(&pm_info.path)
            .arg("pack")
            .current_dir(pkg_dir)
            .output()
            .with_context(|| "Failed to execute npm pack")?,

        PackageManager::Pnpm => Command::new(&pm_info.path)
            .arg("pack")
            .current_dir(pkg_dir)
            .output()
            .with_context(|| "Failed to execute pnpm pack")?,

        PackageManager::Yarn => {
            // Yarn 1.x uses "yarn pack", Yarn 2+ uses "yarn pack" with different output
            Command::new(&pm_info.path)
                .arg("pack")
                .arg("--filename")
                .arg("package.tgz")
                .current_dir(pkg_dir)
                .output()
                .with_context(|| "Failed to execute yarn pack")?
        }

        PackageManager::Bun => Command::new(&pm_info.path)
            .arg("pack")
            .current_dir(pkg_dir)
            .output()
            .with_context(|| "Failed to execute bun pack")?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{} pack failed: {}",
            pm_info.manager.name(),
            stderr.trim()
        );
    }

    // Parse the output to get the tarball filename
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tarball_name = parse_pack_output(pm_info.manager, &stdout, pkg_dir)?;

    Ok(tarball_name)
}

/// Parse the output of the pack command to get the tarball filename.
fn parse_pack_output(manager: PackageManager, output: &str, pkg_dir: &Path) -> Result<String> {
    // Most package managers output the filename on the last non-empty line
    let output_line = output
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();

    // For yarn with --filename flag, we know the name
    if manager == PackageManager::Yarn {
        let yarn_tarball = "package.tgz";
        if pkg_dir.join(yarn_tarball).exists() {
            return Ok(yarn_tarball.to_string());
        }
    }

    // Check if the output line is a filename that exists
    if !output_line.is_empty() && pkg_dir.join(output_line).exists() {
        return Ok(output_line.to_string());
    }

    // Try to find any .tgz file that was just created
    let tgz_files = find_tgz_files(pkg_dir)?;
    if tgz_files.len() == 1 {
        return Ok(tgz_files[0].clone());
    }

    // If multiple .tgz files, try to match package name pattern
    if let Ok(pkg_json_content) = fs::read_to_string(pkg_dir.join("package.json")) {
        if let Ok(pkg_json) = json::parse(&pkg_json_content) {
            let name = pkg_json["name"].as_str().unwrap_or("");
            let version = pkg_json["version"].as_str().unwrap_or("");

            // npm/pnpm format: scope-name-version.tgz or name-version.tgz
            let expected_name = if name.starts_with('@') {
                let scoped_name = name.replace('@', "").replace('/', "-");
                format!("{}-{}.tgz", scoped_name, version)
            } else {
                format!("{}-{}.tgz", name, version)
            };

            if pkg_dir.join(&expected_name).exists() {
                return Ok(expected_name);
            }
        }
    }

    bail!(
        "Could not determine tarball filename from {} pack output: {}",
        manager.name(),
        output
    );
}

/// Find all .tgz files in a directory.
fn find_tgz_files(dir: &Path) -> Result<Vec<String>> {
    let mut tgz_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "tgz").unwrap_or(false) {
            if let Some(name) = path.file_name() {
                tgz_files.push(name.to_string_lossy().to_string());
            }
        }
    }

    Ok(tgz_files)
}

/// Decompress gzipped data.
fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut buffer = Vec::new();
    decoder
        .read_to_end(&mut buffer)
        .with_context(|| "Failed to decompress gzip data")?;
    Ok(buffer)
}

/// Format package files for API consumption.
///
/// This function extracts relevant files from a pack result and formats them
/// as a map of path to contents.
pub fn format_package_files(pack_result: &PackResult) -> Result<HashMap<String, Vec<u8>>> {
    let mut files = HashMap::new();

    for packed_file in pack_result.get_files()? {
        let path_str = packed_file.path.to_string_lossy().to_string();
        files.insert(path_str, packed_file.contents);
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(Path::new("./foo.js")), PathBuf::from("foo.js"));
        assert_eq!(normalize_path(Path::new("/foo.js")), PathBuf::from("foo.js"));
        assert_eq!(normalize_path(Path::new("foo.js")), PathBuf::from("foo.js"));
    }

    #[test]
    fn test_package_manager_command() {
        assert_eq!(PackageManager::Npm.command(), "npm");
        assert_eq!(PackageManager::Pnpm.command(), "pnpm");
        assert_eq!(PackageManager::Yarn.command(), "yarn");
        assert_eq!(PackageManager::Bun.command(), "bun");
    }
}

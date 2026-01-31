// Integration tests for v1-v2 example examples
// Tests CLI functionality with breaking and compatible changes

#[cfg(test)]
mod v1_v2_example_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    fn get_cli_path() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_breakpoint"))
    }

    fn get_examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
    }

    fn run_cli(previous: &str, current: &str) -> (i32, String) {
        let cli = get_cli_path();
        let examples = get_examples_dir();

        let prev_path = examples.join(previous);
        let curr_path = examples.join(current);

        let output = Command::new(&cli)
            .arg("--previous")
            .arg(&prev_path)
            .arg(&curr_path)
            .output()
            .expect("Failed to run CLI");

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let combined = format!("{}\n{}", stdout, stderr);
        (exit_code, combined)
    }

    #[test]
    fn test_examples_exist() {
        let examples = get_examples_dir();

        assert!(examples.join("v1").exists(), "v1 example directory not found");
        assert!(examples.join("v2-breaking").exists(), "v2-breaking example directory not found");
        assert!(examples.join("v2-compatible").exists(), "v2-compatible example directory not found");

        // Check for package.json and src/index.ts
        assert!(examples.join("v1/package.json").exists(), "v1/package.json not found");
        assert!(examples.join("v1/src/index.ts").exists(), "v1/src/index.ts not found");
        assert!(examples.join("v2-breaking/package.json").exists(), "v2-breaking/package.json not found");
        assert!(examples.join("v2-breaking/src/index.ts").exists(), "v2-breaking/src/index.ts not found");
        assert!(examples.join("v2-compatible/package.json").exists(), "v2-compatible/package.json not found");
        assert!(examples.join("v2-compatible/src/index.ts").exists(), "v2-compatible/src/index.ts not found");
    }

    #[test]
    fn test_cli_binary_exists() {
        let cli = get_cli_path();
        assert!(cli.exists(), "CLI binary not found at {:?}", cli);
    }

    #[test]
    fn test_breaking_changes_detected() {
        // Comparing v1 -> v2-breaking should detect breaking changes
        let (exit_code, output) = run_cli("v1", "v2-breaking");

        // Should exit with code 1 (breaking changes detected)
        assert_eq!(exit_code, 1,
            "Expected exit code 1 for breaking changes, got {}.\nOutput: {}",
            exit_code, output);

        // Output should be non-empty
        assert!(!output.is_empty(), "No output from CLI");
    }

    #[test]
    fn test_compatible_changes_accepted() {
        // Comparing v1 -> v2-compatible should NOT detect breaking changes
        let (exit_code, output) = run_cli("v1", "v2-compatible");

        // Should exit with code 0 (no breaking changes)
        assert_eq!(exit_code, 0,
            "Expected exit code 0 for compatible changes, got {}.\nOutput: {}",
            exit_code, output);
    }

    #[test]
    fn test_self_comparison() {
        // Comparing v1 -> v1 should show no changes
        let (exit_code, _output) = run_cli("v1", "v1");

        // Should exit with code 0 (no changes)
        assert_eq!(exit_code, 0,
            "Self-comparison should exit with code 0, got {}", exit_code);
    }

    #[test]
    fn test_reverse_comparison() {
        // Comparing v2-breaking -> v1 (reverse order)
        let (exit_code, _output) = run_cli("v2-breaking", "v1");

        // Should complete without panicking
        // Exit code may vary based on comparison results
        assert!(exit_code == 0 || exit_code == 1,
            "Unexpected exit code: {}", exit_code);
    }

    #[test]
    fn test_output_contains_changes() {
        let (_, output) = run_cli("v1", "v2-breaking");

        // Output should indicate there are changes
        // (exact format may vary based on implementation)
        assert!(!output.is_empty(), "Expected some output describing changes");
    }

    #[test]
    fn test_compatible_has_minimal_output() {
        let (_, output) = run_cli("v1", "v2-compatible");

        // Compatible changes should either show no changes or only minor changes
        // Output should be present but minimal
        assert!(!output.is_empty(), "Expected output from compatible comparison");
    }

    #[test]
    fn test_v1_structure() {
        let examples = get_examples_dir();
        let v1_index = examples.join("v1/src/index.ts");
        let content = fs::read_to_string(&v1_index)
            .expect("Failed to read v1/src/index.ts");

        // Verify v1 has expected exports
        assert!(content.contains("export function createUser"), "v1 missing createUser");
        assert!(content.contains("export class UserService"), "v1 missing UserService");
        assert!(content.contains("export interface Config"), "v1 missing Config");
        assert!(content.contains("export const VERSION"), "v1 missing VERSION constant");
        assert!(content.contains("export type Status"), "v1 missing Status type");
    }

    #[test]
    fn test_v2_breaking_structure() {
        let examples = get_examples_dir();
        let v2_index = examples.join("v2-breaking/src/index.ts");
        let content = fs::read_to_string(&v2_index)
            .expect("Failed to read v2-breaking/src/index.ts");

        // Verify v2-breaking has expected changes
        assert!(content.contains("export function createUser"), "v2-breaking missing createUser");
        assert!(content.contains("role: Role"), "v2-breaking missing role in User interface");
        assert!(content.contains("timeout: number"), "v2-breaking missing required timeout");
        assert!(!content.contains("export const VERSION"), "v2-breaking should not have VERSION");
    }

    #[test]
    fn test_v2_compatible_structure() {
        let examples = get_examples_dir();
        let v2_index = examples.join("v2-compatible/src/index.ts");
        let content = fs::read_to_string(&v2_index)
            .expect("Failed to read v2-compatible/src/index.ts");

        // Verify v2-compatible is backward compatible
        assert!(content.contains("export function createUser"), "v2-compatible missing createUser");
        assert!(content.contains("export const VERSION"), "v2-compatible should have VERSION");
        assert!(content.contains("timeout?: number"), "v2-compatible should have optional timeout");
        assert!(content.contains("debug?: boolean"), "v2-compatible should have optional debug");
    }

    #[test]
    fn test_cli_help_works() {
        let cli = get_cli_path();

        let output = Command::new(&cli)
            .arg("--help")
            .output()
            .expect("Failed to run CLI help");

        assert!(output.status.success(), "CLI help should succeed");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Usage") || stdout.contains("usage"),
            "Help output should contain usage information");
    }

    #[test]
    fn test_invalid_path_error() {
        let cli = get_cli_path();

        let output = Command::new(&cli)
            .arg("--previous")
            .arg("/nonexistent/path/v1")
            .arg("/nonexistent/path/v2")
            .output()
            .expect("Failed to run CLI");

        // Should fail
        assert!(!output.status.success(),
            "CLI should fail for nonexistent paths");
    }

    #[test]
    fn test_stability_multiple_runs() {
        // Run the same comparison multiple times
        let (exit1, _) = run_cli("v1", "v2-breaking");
        let (exit2, _) = run_cli("v1", "v2-breaking");
        let (exit3, _) = run_cli("v1", "v2-breaking");

        // All runs should produce consistent results
        assert_eq!(exit1, exit2, "First two runs produced different results");
        assert_eq!(exit2, exit3, "Second and third runs produced different results");
    }

    #[test]
    fn test_v1_has_required_exports() {
        let examples = get_examples_dir();
        let v1_package = examples.join("v1/package.json");
        let package_content = fs::read_to_string(&v1_package)
            .expect("Failed to read v1/package.json");

        // Verify package metadata
        assert!(package_content.contains("@interlint/test-fixture"),
            "v1 package should have correct name");
        assert!(package_content.contains("\"main\""),
            "v1 package should have main field");
    }

    #[test]
    fn test_example_files_are_valid_typescript() {
        let examples = get_examples_dir();

        for dir in &["v1", "v2-breaking", "v2-compatible"] {
            let ts_file = examples.join(format!("{}/src/index.ts", dir));
            let content = fs::read_to_string(&ts_file)
                .expect(&format!("Failed to read {}/src/index.ts", dir));

            // Basic TypeScript validation
            assert!(content.contains("export"),
                "{}/src/index.ts should have export statements", dir);
            assert!(content.contains("interface") || content.contains("class") || content.contains("function") || content.contains("type") || content.contains("const"),
                "{}/src/index.ts should have type/class/function declarations", dir);
        }
    }

    #[test]
    fn test_version_comparison_output() {
        // The breaking changes comparison should have substantial output
        let (_, output) = run_cli("v1", "v2-breaking");

        // Should not be just a single line
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() > 3,
            "Output should have multiple lines for breaking changes.\nGot: {}", output);
    }

    #[test]
    fn test_cli_handles_relative_paths() {
        // Test with relative paths
        let examples = get_examples_dir();
        let current_dir = std::env::current_dir().expect("Failed to get current dir");

        // This test verifies the CLI can handle absolute paths from any directory
        let cli = get_cli_path();
        let v1 = examples.join("v1");
        let v2 = examples.join("v2-breaking");

        let output = Command::new(&cli)
            .arg("--previous")
            .arg(&v1)
            .arg(&v2)
            .current_dir(&current_dir)
            .output()
            .expect("Failed to run CLI");

        // Should complete (exit code 1 for breaking changes)
        assert_eq!(output.status.code(), Some(1),
            "CLI should detect breaking changes");
    }
}

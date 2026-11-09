import { output } from "./command";

export function stagedFiles() {
  return output("git", ["diff", "--cached", "--name-only", "--no-renames", "-z"])
    .split("\0").filter(Boolean);
}

function isImplementationSource(path: string) {
  return path.endsWith(".rs")
    || (path.startsWith("src/") && path.endsWith(".cjs"));
}

export function isRustInput(path: string) {
  return isImplementationSource(path)
    || /(^|\/)(Cargo\.(toml|lock)|rust-toolchain(\.toml)?|\.?rustfmt\.toml|\.?clippy\.toml)$/.test(path)
    || path.startsWith(".cargo/")
    || (path.startsWith("tests/fixtures/") && !path.endsWith(".md"));
}

export function isToolingInput(path: string) {
  return path.startsWith("scripts/") || path.startsWith(".github/")
    || isLanguageCoverageInput(path)
    || path === "Makefile" || path === "Dockerfile" || path === ".dockerignore" || path === "action.yml" || path === ".coverage-baseline"
    || path === "release-please-config.json"
    || path === ".release-please-manifest.json";
}

export function isLanguageCoverageInput(path: string) {
  return path.startsWith("scripts/conformance/") || path.startsWith("docs/coverage/")
    || path === "docs/language-coverage.md";
}

export function classifyChanges(paths: string[]) {
  const kinds = new Set<string>();
  for (const path of paths) {
    if (path.endsWith(".md")) kinds.add("docs");
    else if (path.startsWith("tests/") || /\.(test|spec)\.ts$/.test(path)) kinds.add("test");
    else if (isImplementationSource(path)) kinds.add("source");
    else if (path.startsWith("scripts/") && (/\.(ts|cjs|sh)$/.test(path)
      || /^scripts\/hooks\/(pre-commit|prepare-commit-msg|commit-msg|pre-push)$/.test(path))) kinds.add("source");
    else if (path.startsWith(".github/")) kinds.add("ci");
    else kinds.add("build");
  }
  return kinds;
}

export function suggestedType(kinds: Set<string>) {
  if (kinds.has("source")) return "feat, fix, refactor, or perf";
  if (kinds.size === 1 && kinds.has("test")) return "test";
  if (kinds.size === 1 && kinds.has("docs")) return "docs";
  if (kinds.size === 1 && kinds.has("ci")) return "ci";
  return "chore or build";
}

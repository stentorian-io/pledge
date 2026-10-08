import { output } from "./command";

export function stagedFiles() {
  return output("git", ["diff", "--cached", "--name-only", "--no-renames", "-z"])
    .split("\0").filter(Boolean);
}

export function isRustInput(path: string) {
  return path.endsWith(".rs")
    || /(^|\/)(Cargo\.(toml|lock)|rust-toolchain(\.toml)?|\.?rustfmt\.toml|\.?clippy\.toml)$/.test(path)
    || path.startsWith(".cargo/")
    || (path.startsWith("tests/fixtures/") && !path.endsWith(".md"));
}

export function isToolingInput(path: string) {
  return path.startsWith("scripts/") || path === "Makefile";
}

export function classifyChanges(paths: string[]) {
  const kinds = new Set<string>();
  for (const path of paths) {
    if (path.endsWith(".md")) kinds.add("docs");
    else if (path.startsWith("tests/") || /\.(test|spec)\.ts$/.test(path)) kinds.add("test");
    else if (path.endsWith(".rs")) kinds.add("source");
    else if (path.startsWith("scripts/") && (/\.(ts|sh)$/.test(path)
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

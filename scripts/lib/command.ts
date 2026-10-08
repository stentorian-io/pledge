import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";

type RunOptions = {
  cwd?: string;
  input?: string;
  capture?: boolean;
  allowFailure?: boolean;
};

export function fail(prefix: string, message: string): never {
  console.error(`${prefix}: ${message}`);
  process.exit(1);
}

export function run(command: string, args: string[] = [], options: RunOptions = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    input: options.input,
    stdio: options.capture ? "pipe" : [options.input === undefined ? "inherit" : "pipe", "inherit", "inherit"],
    encoding: "utf8",
  });

  if (!options.allowFailure) {
    if (result.error) fail(command, result.error.message);
    if (result.signal) fail(command, `terminated by ${result.signal}`);
    if (result.status !== 0) {
      if (options.capture && result.stderr) console.error(result.stderr.trim());
      process.exit(result.status ?? 1);
    }
  }

  return result;
}

export function output(command: string, args: string[], options: RunOptions = {}) {
  return run(command, args, { ...options, capture: true }).stdout ?? "";
}

export function repoRoot() {
  return output("git", ["rev-parse", "--show-toplevel"]).trim();
}

export function requireCommand(command: string, installation: string) {
  if (!Bun.which(command)) fail(command, `not found. ${installation}`);
}

export function prepareRustEnvironment() {
  const candidates = [
    process.env.CARGO_HOME ? join(process.env.CARGO_HOME, "bin") : join(homedir(), ".cargo", "bin"),
    "/opt/homebrew/opt/rustup/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
  ];
  process.env.PATH = [process.env.PATH, ...candidates].filter(Boolean).join(delimiter);
  requireCommand("cargo", "Install Rust through rustup and add Cargo to PATH.");
}

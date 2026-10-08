#!/usr/bin/env bun
import { accessSync, constants } from "node:fs";
import { fileURLToPath } from "node:url";
import { output, run } from "../lib/command";

const scriptDirectory = fileURLToPath(new URL(".", import.meta.url));
const root = output("git", ["-C", scriptDirectory, "rev-parse", "--show-toplevel"]).trim();

for (const hook of ["pre-commit", "prepare-commit-msg", "commit-msg", "pre-push"]) {
  accessSync(`${scriptDirectory}/${hook}`, constants.X_OK);
}

run("git", ["-C", root, "config", "--local", "core.hooksPath", "scripts/hooks"]);
console.log("Local Git hooks activated from scripts/hooks. Changes to hooks take effect immediately.");

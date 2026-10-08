#!/usr/bin/env bun
import { isRustInput, isToolingInput, stagedFiles } from "../lib/changes";
import { checkRust, checkTools } from "../lib/checks";
import { repoRoot } from "../lib/command";

process.chdir(repoRoot());
const paths = stagedFiles();
if (paths.some(isToolingInput)) checkTools();
if (paths.some(isRustInput)) checkRust();
else console.log("pre-commit: no Rust or fixture changes staged; skipping Rust checks.");

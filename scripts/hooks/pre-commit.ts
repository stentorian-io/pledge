#!/usr/bin/env bun
import { isLanguageCoverageInput, isRustInput, isToolingInput, stagedFiles } from "../lib/changes";
import { checkLanguageCoverage, checkRust, checkTools } from "../lib/checks";
import { repoRoot } from "../lib/command";

process.chdir(repoRoot());
const paths = stagedFiles();
if (paths.some(isLanguageCoverageInput)) checkLanguageCoverage();
if (paths.some(isToolingInput)) checkTools();
if (paths.some(isRustInput)) checkRust();
else console.log("pre-commit: no Rust or fixture changes staged; skipping Rust checks.");

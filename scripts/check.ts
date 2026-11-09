#!/usr/bin/env bun
import { checkCoverage, checkLanguageCoverage, checkLsp, checkRust, checkRustStyle, checkTools } from "./lib/checks";
import { fail, repoRoot } from "./lib/command";

process.chdir(repoRoot());

switch (Bun.argv[2] ?? "default") {
  case "default":
    checkLanguageCoverage();
    checkRust();
    break;
  case "lsp":
    checkLsp();
    break;
  case "coverage":
    checkCoverage();
    break;
  case "tools":
    checkTools();
    break;
  case "all":
    checkLanguageCoverage();
    checkTools();
    checkRustStyle();
    checkCoverage();
    break;
  default:
    fail("check", "usage: bun scripts/check.ts [default|lsp|coverage|tools|all]");
}

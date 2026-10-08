#!/usr/bin/env bun
import { checkCoverage, checkLsp, checkRust, checkTools } from "./lib/checks";
import { fail, repoRoot } from "./lib/command";

process.chdir(repoRoot());

switch (Bun.argv[2] ?? "default") {
  case "default":
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
    checkTools();
    checkRust();
    checkLsp();
    checkCoverage();
    break;
  default:
    fail("check", "usage: bun scripts/check.ts [default|lsp|coverage|tools|all]");
}

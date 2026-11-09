#!/usr/bin/env bun
import { updateBaseline } from "./report";
import { fail, repoRoot } from "../lib/command";

process.chdir(repoRoot());
try { updateBaseline(); }
catch (error) { fail("coverage", String(error)); }

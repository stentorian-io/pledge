import { afterEach, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { load, repositoryFile, validate, type Loaded } from "./inventory";
import { render, updateDocument } from "./report";
import { ecmascriptReferences, markdownReferences, sha256, test262Features, typescriptReferences } from "./sources";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "pledge-language-coverage-")); roots.push(root);
  for (const path of ["docs/coverage", "tests", "bin", "scripts"]) mkdirSync(join(root, path), { recursive: true });
  writeFileSync(join(root, "docs/language-coverage.md"), "# Coverage\n\n## Limits\n\n<!-- language-coverage:start -->\n\n<!-- language-coverage:end -->\n");
  writeFileSync(join(root, "tests/consumer.rs"), "#[test]\nfn observes_contract() {}\n");
  writeFileSync(join(root, "tests/cases.json"), JSON.stringify([{ name: "breaking" }, { name: "compatible" }]));
  const references = [{ id: "ecma262:api", title: "API", kind: "clause", path: "spec.html" }];
  const index = JSON.stringify(references);
  writeFileSync(join(root, "docs/coverage/ecma262.json"), index);
  const loaded: Loaded = {
    sources: [{ id: "ecma262", name: "Pinned example", repository: "tc39/ecma262", revision: "a".repeat(40), index: "docs/coverage/ecma262.json", sha256: sha256(index), inputs: [{ path: "spec.html", sha256: "b".repeat(64) }], license: "BSD" }], references,
    inventory: { schema: 1,
      profiles: [{ id: "consumer", description: "Exact consumer", node: ["22.12.0"], typescript: [], options: {}, limitations: "One operation only" }],
      evidence: [{ id: "consumer", target: "javascript", name: "runtime::observes_contract", source: "tests/consumer.rs", fixture: "tests/cases.json", cases: ["breaking", "compatible"], profiles: ["consumer"], oracles: ["node"], outcomes: ["breaking", "compatible"] }],
      assessments: [{ id: "api", title: "API contract", layer: "runtime", status: "partial", baselines: { ecma262: "a".repeat(40) }, references: ["ecma262:api"], evidence: ["consumer"], reason: "Other behavior remains unverified", documentation: ["docs/language-coverage.md#limits"] }],
    },
  };
  function save() {
    writeFileSync(join(root, "docs/coverage/sources.json"), JSON.stringify(loaded.sources));
    writeFileSync(join(root, "docs/coverage/inventory.json"), JSON.stringify(loaded.inventory));
  }
  save();
  return { root, loaded, save };
}

test("new upstream references require an explicit assessment", () => {
  const { root, loaded } = fixture();
  loaded.references.push({ id: "ecma262:new", title: "New", kind: "clause", path: "spec.html" });
  expect(() => validate(root, loaded)).toThrow("unclassified upstream references");
  loaded.inventory.assessments.push({ id: "new", title: "New", layer: "scope", status: "unverified", baselines: { ecma262: "a".repeat(40) }, references: ["ecma262:new"], evidence: [], reason: "Not yet mapped", documentation: ["docs/language-coverage.md#limits"] });
  expect(() => validate(root, loaded)).not.toThrow();
});

test("upstream revision changes require explicit assessment review even with unchanged IDs", () => {
  const { root, loaded } = fixture();
  loaded.sources[0].revision = "c".repeat(40);
  expect(() => validate(root, loaded)).toThrow("reviewed for changed baseline");
  loaded.inventory.assessments[0].baselines.ecma262 = "c".repeat(40);
  expect(() => validate(root, loaded)).not.toThrow();
});

test("duplicate classification and empty inventories cannot inflate accounting", () => {
  const { root, loaded } = fixture();
  loaded.inventory.assessments[0].references.push("ecma262:api");
  expect(() => validate(root, loaded)).toThrow("assessment references");
  loaded.references = [];
  expect(() => validate(root, loaded)).toThrow("cannot be empty");
});

test("missing tests, fixture cases, and documentation anchors fail validation", () => {
  for (const change of [
    (loaded: Loaded) => { loaded.inventory.evidence[0].name = "runtime::missing_test"; },
    (loaded: Loaded) => { loaded.inventory.evidence[0].cases = ["missing"]; },
    (loaded: Loaded) => { loaded.inventory.assessments[0].documentation = ["docs/language-coverage.md#missing"]; },
  ]) {
    const { root, loaded } = fixture(); change(loaded);
    expect(() => validate(root, loaded)).toThrow();
  }
});

test("verified and unsupported claims require the corresponding evidence controls", () => {
  const { root, loaded } = fixture();
  loaded.inventory.assessments[0].status = "verified";
  loaded.inventory.evidence[0].outcomes = ["breaking"];
  expect(() => validate(root, loaded)).toThrow("compatible controls");
  loaded.inventory.assessments[0].status = "unsupported";
  expect(() => validate(root, loaded)).toThrow("incomplete-result coverage");
  loaded.inventory.evidence[0].outcomes = ["incomplete"];
  expect(() => validate(root, loaded)).not.toThrow();
  loaded.inventory.assessments[0].status = "verified";
  loaded.inventory.evidence[0].outcomes = ["breaking", "compatible"];
  loaded.inventory.evidence[0].oracles = ["pledge"];
  expect(() => validate(root, loaded)).toThrow("independent consumer oracle");
});

test("consumer fixture profiles and upstream references cannot drift from evidence", () => {
  const { root, loaded } = fixture();
  writeFileSync(join(root, "tests/cases.json"), JSON.stringify([{ name: "breaking", profile: "other" }, { name: "compatible" }]));
  expect(() => validate(root, loaded)).toThrow("fixture profile");
  writeFileSync(join(root, "tests/cases.json"), JSON.stringify([{ name: "breaking", upstream: "ecma262:missing" }, { name: "compatible" }]));
  expect(() => validate(root, loaded)).toThrow("unknown upstream reference");
});

test("npm evidence requires an exact package-manager profile", () => {
  const { root, loaded } = fixture();
  loaded.inventory.evidence[0].oracles = ["npm", "pledge"];
  expect(() => validate(root, loaded)).toThrow("npm evidence requires exact npm versions");
  loaded.inventory.profiles[0].npm = ["11"];
  expect(() => validate(root, loaded)).toThrow("profile version must be exact");
  loaded.inventory.profiles[0].npm = ["11.19.0"];
  expect(() => validate(root, loaded)).not.toThrow();
  expect(render(loaded)).toContain("| 11.19.0 |");
});

test("pinned indexes and imported tables cannot be silently altered or omitted", () => {
  const { root, loaded } = fixture();
  writeFileSync(join(root, loaded.sources[0].index), "[]\n");
  expect(() => load(root)).toThrow();
  loaded.sources[0].sha256 = sha256("[]\n");
  loaded.references[0] = { ...loaded.references[0], kind: "imported-table", path: "unlocked.html" };
  expect(() => validate(root, loaded)).toThrow("unaccounted imported source");
});

test("repository evidence paths reject traversal and external symlinks", () => {
  const { root } = fixture();
  const external = mkdtempSync(join(tmpdir(), "pledge-coverage-external-")); roots.push(external);
  writeFileSync(join(external, "evidence"), "outside");
  symlinkSync(join(external, "evidence"), join(root, "tests/escape"));
  expect(() => repositoryFile(root, "../evidence")).toThrow("unsafe");
  expect(() => repositoryFile(root, "tests/escape")).toThrow("escapes");
});

test("report generation keeps unverified units separate and requires unique markers", () => {
  const { loaded } = fixture();
  loaded.inventory.assessments[0].status = "unverified";
  const report = render(loaded);
  expect(report).toContain("| 1 | 0 | 0 | 0 | 1 | 0 |");
  expect(report).toContain("not equally weighted language features");
  expect(() => updateDocument("missing markers", report)).toThrow();
  const document = "<!-- language-coverage:start --><!-- language-coverage:end -->";
  expect(updateDocument(updateDocument(document, report), report)).toBe(updateDocument(document, report));
});

test("ECMAScript indexing retains nested sections, inline grammars, and imported tables", () => {
  const references = ecmascriptReferences(`<emu-clause id="sec-root"><h1>A &amp; B</h1>
    <emu-grammar type="definition">Root : Child</emu-grammar>
    <emu-clause id="sec-child"><h1>Child</h1><emu-grammar type="definition">Child[Yield] :: Token</emu-grammar></emu-clause>
    <emu-grammar type="definition" example>Example : Ignored</emu-grammar>
    <emu-import href="table-values.html"></emu-import></emu-clause>`);
  expect(references.map(reference => reference.id)).toEqual([
    "ecma262:import/table-values.html", "ecma262:sec-child", "ecma262:sec-child/grammar/Child", "ecma262:sec-root", "ecma262:sec-root/grammar/Root",
  ]);
  expect(references.find(reference => reference.id === "ecma262:sec-root")!.title).toBe("A & B");
  expect(references[0].parent).toBe("ecma262:sec-root");
  expect(() => ecmascriptReferences('<emu-clause><h1>Missing id</h1></emu-clause>')).toThrow();
  expect(() => ecmascriptReferences('<emu-clause id="a"><h1>A</h1>')).toThrow();
});

test("Test262 comments do not become feature names or change proposal classification", () => {
  const references = test262Features("## Proposed language features\nfuture\n## https://example.test\nfuture-two # comment\n## Standard language features\nSymbol\n## Test-Harness Features\nIsHTMLDDA\n");
  expect(references.map(reference => [reference.id, reference.kind])).toEqual([
    ["test262:future", "proposal"], ["test262:future-two", "proposal"], ["test262:Symbol", "feature"], ["test262:IsHTMLDDA", "harness"],
  ]);
});

test("truncated TypeScript trees fail instead of lowering the denominator", () => {
  expect(() => typescriptReferences({ tree: [], truncated: true }, "", "")).toThrow("truncated");
});

test("TypeScript syntax inventories include the final member without a trailing comma", () => {
  const types = "export const enum SyntaxKind {\n First,\n /* explanation */ Last = First + 1\n}\n";
  const options = 'export const commonOptionsWithBuild: Option[] = [\n        name: "strict"\n];\nname: "target";\nname: "module";\nexport const optionDeclarations: Option[] = [];';
  const tree = { tree: [{ path: "conformance/example.ts", type: "blob", sha: "a".repeat(40) }] };
  const refs = typescriptReferences(tree, types, options);
  expect(refs.filter(reference => reference.kind === "syntax-kind").map(reference => reference.title)).toEqual(["First", "Last"]);
  expect(() => typescriptReferences(tree, types.replace("Last = First + 1", "'unexpected name' = 1"), options)).toThrow("unrecognized SyntaxKind member");
});

test("host heading indexes ignore fenced examples and distinguish repeated headings", () => {
  const refs = markdownReferences("node", "api.md", "# API\n```md\n# Example\n```\n## Loading\n## Loading\n");
  expect(refs.map(reference => reference.anchor)).toEqual(["api", "loading", "loading-1"]);
});

function runnerFixture() {
  const setup = fixture();
  cpSync(fileURLToPath(new URL("./", import.meta.url)), join(setup.root, "scripts/conformance"), { recursive: true });
  for (const [name, body] of Object.entries({ git: "case \"$*\" in *rev-parse*) echo fixture-commit;; esac", node: "echo v22.12.0", cargo: "case \"$*\" in *--list*) echo 'runtime::observes_contract: test';; *) echo 'test runtime::observes_contract ... ok'; echo 'test result: ok. 1 passed; 0 failed; 0 ignored;';; esac" })) {
    writeFileSync(join(setup.root, "bin", name), `#!/bin/sh\n${body}\n`, { mode: 0o755 });
  }
  const output = join(setup.root, "result.json");
  function run(extra: string[] = []) {
    const result = Bun.spawnSync([process.execPath, "scripts/conformance/run.ts", "--profile", "consumer", "--output", output, ...extra], {
      cwd: setup.root, env: { ...process.env, PATH: `${setup.root}/bin:${process.env.PATH}` }, stdout: "pipe", stderr: "pipe",
    });
    return { exit: result.exitCode, report: JSON.parse(readFileSync(output, "utf8")), stderr: result.stderr.toString() };
  }
  return { ...setup, run };
}

test("consumer runner records actual exact tests and rejects silent zero-test success", () => {
  const { root, run } = runnerFixture();
  expect(run().report.results[0].status).toBe("passed");
  writeFileSync(join(root, "bin/cargo"), "#!/bin/sh\ncase \"$*\" in *--list*) echo 'runtime::observes_contract: test';; *) echo 'test result: ok. 0 passed; 0 failed; 0 ignored;';; esac\n", { mode: 0o755 });
  const result = run();
  expect(result.exit).toBe(1);
  expect(result.report.results[0].status).toBe("failed");
});

test("consumer batches deduplicate overlapping filters and verify every selected result", () => {
  const { root, run } = runnerFixture();
  const cargo = (lines: string) => writeFileSync(join(root, "bin/cargo"), `#!/bin/sh
printf '%s\\n' "$*" >> '${root}/cargo-calls'
case "$*" in
  *--list*) printf '%s\\n' 'runtime::observes_contract: test' 'runtime::additional_contract: test';;
  *) ${lines};;
esac
`, { mode: 0o755 });
  cargo("printf '%s\\n' 'test runtime::observes_contract ... ok' 'test runtime::additional_contract ... ok' 'test result: ok. 2 passed; 0 failed; 0 ignored;'");
  const result = run(["--filter", "javascript/runtime::"]);
  expect(result.exit).toBe(0);
  expect(result.report.results.map((entry: { name: string }) => entry.name).sort()).toEqual(["runtime::additional_contract", "runtime::observes_contract"]);
  expect(readFileSync(join(root, "cargo-calls"), "utf8").trim().split("\n")).toHaveLength(2);
  for (const outcomes of [
    ["runtime::observes_contract", "runtime::observes_contract"],
    ["runtime::observes_contract", "runtime::unexpected_contract"],
  ]) {
    cargo(`printf '%s\\n' ${outcomes.map(name => `'test ${name} ... ok'`).join(" ")} 'test result: ok. 2 passed; 0 failed; 0 ignored;'`);
    expect(run(["--filter", "javascript/runtime::"]).exit).toBe(1);
  }
  expect(run(["--filter", "javascript/absent"]).stderr).toContain("matched nothing");
});

test("prebuilt consumers run only for their exact clean checkout and registered target", () => {
  const { root, run } = runnerFixture();
  const directory = join(root, "compiled");
  mkdirSync(directory);
  cpSync(join(root, "bin/cargo"), join(directory, "javascript"));
  const manifest = { schema: 1, commit: "fixture-commit", root: realpathSync(root), targets: ["javascript"] };
  const write = (changes = {}) => writeFileSync(join(directory, "manifest.json"), JSON.stringify({ ...manifest, ...changes }));
  write();
  const passed = run(["--binaries", directory]);
  expect(passed.exit, passed.stderr).toBe(0);
  for (const change of [{ commit: "different" }, { root: "/another-checkout" }, { targets: [] }, { schema: 2 }]) {
    write(change);
    expect(run(["--binaries", directory]).stderr).toContain("do not match");
  }
  write();
  writeFileSync(join(root, "bin/git"), "#!/bin/sh\ncase \"$*\" in *rev-parse*) echo fixture-commit;; *) echo ' M src/lib.rs';; esac\n", { mode: 0o755 });
  expect(run(["--binaries", directory]).stderr).toContain("clean checkout");
});

test("consumer runner rejects unregistered tests and undeclared runtime versions", () => {
  const { root, run } = runnerFixture();
  writeFileSync(join(root, "bin/cargo"), "#!/bin/sh\necho 'other::test: test'\n", { mode: 0o755 });
  expect(run().stderr).toContain("not registered");
  writeFileSync(join(root, "bin/node"), "#!/bin/sh\necho v99.0.0\n", { mode: 0o755 });
  expect(run().stderr).toContain("outside the declared exact versions");
});

test("consumer runner records npm and rejects a different installed version", () => {
  const { root, loaded, save, run } = runnerFixture();
  loaded.inventory.profiles[0].npm = ["11.19.0"];
  loaded.inventory.evidence[0].oracles = ["npm", "pledge"];
  save();
  writeFileSync(join(root, "bin/npm"), "#!/bin/sh\necho 11.19.0\n", { mode: 0o755 });
  const passed = run();
  expect(passed.exit).toBe(0);
  expect(passed.report.npm).toBe("11.19.0");
  expect(passed.report.results[0].status).toBe("passed");
  writeFileSync(join(root, "bin/npm"), "#!/bin/sh\necho 10.9.0\n", { mode: 0o755 });
  const rejected = run();
  expect(rejected.exit).toBe(1);
  expect(rejected.report.npm).toBe("10.9.0");
  expect(rejected.stderr).toContain("npm 10.9.0 is outside the declared exact versions");
  expect(rejected.report.results).toEqual([]);
});

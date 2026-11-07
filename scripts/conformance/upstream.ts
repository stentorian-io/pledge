#!/usr/bin/env bun
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { type Source, load } from "./inventory";
import { download, ecmascriptReferences, gitTree, markdownReferences, sha256, test262Features, typescriptReferences, type Reference } from "./sources";

const root = fileURLToPath(new URL("../../", import.meta.url));
export const serializeIndex = (references: Reference[]) => "[\n" + references.map(reference => "  " + JSON.stringify(reference)).join(",\n") + "\n]\n";

async function extract(source: Source, revision: string, verify: boolean) {
  const inputs = new Map<string, string>();
  const hashes: Source["inputs"] = [];
  for (const input of source.inputs) {
    const bytes = await download(`https://raw.githubusercontent.com/${source.repository}/${revision}/${input.path}`);
    const digest = sha256(bytes);
    if (verify && digest !== input.sha256) throw new Error(`source checksum mismatch: ${source.id}/${input.path}`);
    hashes.push({ path: input.path, sha256: digest });
    inputs.set(input.path, new TextDecoder().decode(bytes));
  }
  let references: Reference[];
  let treeHash: string | undefined;
  if (source.id === "ecma262") references = ecmascriptReferences(inputs.get("spec.html")!);
  else if (source.id === "test262") references = test262Features(inputs.get("features.txt")!);
  else if (source.id === "typescript") {
    const tree = await gitTree(source.repository, revision, source.tree!.directory);
    const blobs = tree.tree.filter(entry => entry.type === "blob").map(entry => [entry.path, entry.sha]).sort((a, b) => a[0] < b[0] ? -1 : 1);
    treeHash = sha256(JSON.stringify(blobs));
    if (verify && treeHash !== source.tree!.sha256) throw new Error("TypeScript test tree checksum mismatch");
    references = typescriptReferences(tree, inputs.get("src/compiler/types.ts")!, inputs.get("src/compiler/commandLineParser.ts")!);
  } else if (source.id === "node" || source.id === "npm") {
    references = [...inputs].flatMap(([path, contents]) => markdownReferences(source.id, path, contents));
  } else throw new Error(`unknown upstream extractor: ${source.id}`);
  for (const reference of references.filter(reference => reference.kind === "imported-table")) {
    if (!inputs.has(reference.path)) throw new Error(`unaccounted imported source: ${reference.path}`);
  }
  return { references, inputs: hashes, treeHash };
}

if (import.meta.main) {
  try {
    const [mode, id, revision, directory, ...extra] = Bun.argv.slice(2);
    const loaded = load(root);
    if (mode === "verify" && !id) {
      for (const source of loaded.sources) {
        const result = await extract(source, source.revision, true);
        if (serializeIndex(result.references) !== readFileSync(resolve(root, source.index), "utf8")) throw new Error(`upstream extraction differs: ${source.id}`);
        console.log(`${source.id}: reproduced ${result.references.length} pinned references`);
      }
      const cases = new Map(loaded.inventory.evidence.flatMap(evidence => evidence.upstream ?? []).map(upstream => [`${upstream.source}/${upstream.path}`, upstream]));
      for (const upstream of cases.values()) {
        const source = loaded.sources.find(source => source.id === upstream.source)!;
        const bytes = await download(`https://raw.githubusercontent.com/${source.repository}/${source.revision}/${upstream.path}`);
        if (sha256(bytes) !== upstream.sha256) throw new Error(`upstream case checksum mismatch: ${upstream.path}`);
      }
      console.log(`verified ${cases.size} adapted upstream case sources`);
    } else if (mode === "candidate" && id && /^[0-9a-f]{40}$/.test(revision ?? "") && directory && !extra.length) {
      const source = loaded.sources.find(source => source.id === id);
      if (!source) throw new Error(`unknown source: ${id}`);
      const result = await extract(source, revision, false);
      const previous = new Map(loaded.references.filter(reference => reference.id.startsWith(`${id}:`)).map(reference => [reference.id, reference]));
      const current = new Map(result.references.map(reference => [reference.id, reference]));
      const changes = {
        source: id, revision,
        added: [...current.keys()].filter(key => !previous.has(key)),
        removed: [...previous.keys()].filter(key => !current.has(key)),
        changed: [...current.keys()].filter(key => previous.has(key) && JSON.stringify(previous.get(key)) !== JSON.stringify(current.get(key))),
        inputs: result.inputs, treeSha256: result.treeHash,
      };
      const output = resolve(directory);
      if (output === root || output.startsWith(root + "/")) throw new Error("write candidates outside the repository; baseline changes require review");
      mkdirSync(output, { recursive: true });
      writeFileSync(resolve(output, `${id}.json`), serializeIndex(result.references));
      writeFileSync(resolve(output, "changes.json"), JSON.stringify(changes, null, 2) + "\n");
      console.log(`${changes.added.length} added, ${changes.removed.length} removed, ${changes.changed.length} changed references; candidate written to ${output}`);
    } else throw new Error("usage: bun scripts/conformance/upstream.ts verify | candidate <source-id> <commit-sha> <output-directory>");
  } catch (error) {
    console.error(`upstream coverage: ${(error as Error).message}`);
    process.exitCode = 1;
  }
}

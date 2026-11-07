import { createHash } from "node:crypto";

export type Reference = {
  id: string;
  title: string;
  kind: string;
  path: string;
  anchor?: string;
  parent?: string;
  blob?: string;
};

export type GitTree = {
  truncated?: boolean;
  tree: { path: string; type: string; sha: string; url?: string }[];
};

export function sha256(contents: string | Uint8Array) {
  return createHash("sha256").update(contents).digest("hex");
}

function plainText(html: string) {
  return html.replace(/<[^>]*>/g, "").replace(/&(?:amp|lt|gt|quot|apos|nbsp);/g,
    entity => ({ "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&apos;": "'", "&nbsp;": " " })[entity]!)
    .replace(/\s+/g, " ").trim();
}

function attribute(tag: string, name: string) {
  return tag.match(new RegExp(`\\b${name}=["']([^"']+)["']`))?.[1];
}

export function ecmascriptReferences(source: string): Reference[] {
  const references = new Map<string, Reference>();
  const parents: string[] = [];
  const tokens = source.replace(/<!--[\s\S]*?-->/g, "").matchAll(
    /<emu-(?:clause|annex|intro|import)\b[^>]*>|<\/emu-(?:clause|annex|intro)>|<h1\b[^>]*>[\s\S]*?<\/h1>|<emu-grammar\b[^>]*>[\s\S]*?<\/emu-grammar>/g,
  );
  for (const [token] of tokens) {
    if (/^<emu-(clause|annex|intro)\b/.test(token)) {
      const anchor = attribute(token, "id");
      if (!anchor) throw new Error("ECMAScript section has no stable id");
      const id = `ecma262:${anchor}`;
      if (references.has(id)) throw new Error(`duplicate ECMAScript section: ${id}`);
      references.set(id, { id, title: "", kind: "clause", path: "spec.html", anchor, parent: parents.at(-1) });
      parents.push(id);
    } else if (token.startsWith("</emu-")) {
      if (!parents.pop()) throw new Error("unbalanced ECMAScript sections");
    } else if (token.startsWith("<h1")) {
      const reference = references.get(parents.at(-1)!);
      if (reference && !reference.title) reference.title = plainText(token);
    } else if (token.startsWith("<emu-import")) {
      const path = attribute(token, "href");
      if (!path || !/^[\w-]+\.html$/.test(path)) throw new Error("unexpected ECMAScript import");
      const id = `ecma262:import/${path}`;
      references.set(id, { id, title: path, kind: "imported-table", path, parent: parents.at(-1) });
    } else if (/\btype=["']definition["']/.test(token) && !/\bexample\b/.test(token.split(">", 1)[0])) {
      const parent = parents.at(-1);
      if (!parent) throw new Error("grammar definition outside a section");
      const grammar = token.slice(token.indexOf(">") + 1, token.lastIndexOf("</"));
      for (const match of grammar.matchAll(/(?:^|\n)\s*([A-Za-z][\w]*)(?:\[[^\]]*\])?\s*:{1,3}(?!:)/g)) {
        const name = match[1];
        const id = `${parent}/grammar/${name}`;
        references.set(id, { id, title: name, kind: "grammar", path: "spec.html", anchor: parent.slice(8), parent });
      }
    }
  }
  if (parents.length || !references.size || [...references.values()].some(reference => !reference.title)) {
    throw new Error("incomplete ECMAScript section extraction");
  }
  return [...references.values()].sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

export function test262Features(source: string): Reference[] {
  let kind = "feature";
  return source.split(/\r?\n/).flatMap(line => {
    if (line === "## Proposed language features") kind = "proposal";
    if (line === "## Standard language features") kind = "feature";
    if (line === "## Test-Harness Features") kind = "harness";
    const name = line.split("#", 1)[0].trim();
    if (name && /\s/.test(name)) throw new Error(`invalid Test262 feature: ${name}`);
    return !name || name.startsWith("#") ? [] : [{ id: `test262:${name}`, title: name, kind, path: "features.txt" }];
  });
}

export function markdownReferences(id: string, path: string, source: string): Reference[] {
  const references: Reference[] = [];
  const occurrences = new Map<string, number>();
  let fence: string | undefined;
  for (const line of source.split(/\r?\n/)) {
    const marker = line.match(/^\s*(`{3,}|~{3,})/)?.[1];
    if (marker) {
      if (!fence) fence = marker;
      else if (marker[0] === fence[0] && marker.length >= fence.length) fence = undefined;
      continue;
    }
    if (fence) continue;
    const title = line.match(/^#{1,6} (.+)$/)?.[1];
    if (!title) continue;
    const slug = title.toLowerCase().replace(/[^\w\s-]/g, "").trim().replace(/\s+/g, "-");
    const count = occurrences.get(slug) ?? 0;
    occurrences.set(slug, count + 1);
    const anchor = slug + (count ? `-${count}` : "");
    references.push({ id: `${id}:${path}#${anchor}`, title: title.replace(/`/g, ""), kind: "host-documentation", path, anchor });
  }
  if (!references.length) throw new Error(`no upstream headings in ${path}`);
  return references;
}

export function typescriptReferences(tree: GitTree, types: string, options: string): Reference[] {
  if (tree.truncated) throw new Error("refusing truncated TypeScript test tree");
  const references: Reference[] = tree.tree.filter(entry => entry.type === "blob" && /^(conformance|compiler)\//.test(entry.path))
    .map(entry => ({ id: `typescript:${entry.path}`, title: entry.path, kind: entry.path.startsWith("conformance/") ? "conformance-case" : "compiler-case", path: `tests/cases/${entry.path}`, blob: entry.sha }));
  const syntax = types.match(/export const enum SyntaxKind\s*\{([\s\S]*?)^\}/m)?.[1];
  if (!syntax) throw new Error("TypeScript SyntaxKind declaration changed");
  const declarations = syntax.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "").split(",");
  for (const declaration of declarations.map(declaration => declaration.trim()).filter(Boolean)) {
    const name = declaration.match(/^(\w+)(?:\s*=[\s\S]+)?$/)?.[1];
    if (!name) throw new Error(`unrecognized SyntaxKind member: ${declaration}`);
    references.push({ id: `typescript:syntax/${name}`, title: name, kind: "syntax-kind", path: "src/compiler/types.ts" });
  }
  // This bounded region contains the two arrays composing optionDeclarations,
  // including the separately declared target and module options.
  const start = options.indexOf("export const commonOptionsWithBuild:");
  const end = options.indexOf("export const optionDeclarations:");
  if (start < 0 || end <= start) throw new Error("TypeScript option declarations changed");
  const names = new Set([...options.slice(start, end).matchAll(/^ {8}name: "([^"]+)"/gm)].map(match => match[1]));
  for (const name of ["target", "module"]) {
    if (!options.slice(start, end).includes(`name: "${name}"`)) throw new Error(`missing ${name} option`);
    names.add(name);
  }
  for (const name of names) references.push({ id: `typescript:option/${name}`, title: name, kind: "compiler-option", path: "src/compiler/commandLineParser.ts" });
  if (!references.some(reference => reference.kind === "conformance-case") || names.size < 2) throw new Error("empty TypeScript inventory");
  return references.sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

export async function download(url: string) {
  const headers: Record<string, string> = { "User-Agent": "pledge-coverage" };
  if (new URL(url).origin === "https://api.github.com" && process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }
  const response = await fetch(url, { signal: AbortSignal.timeout(60_000), headers });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

export async function gitTree(repository: string, revision: string, directory: string): Promise<GitTree> {
  const base = `https://api.github.com/repos/${repository}/git/trees/`;
  let tree = JSON.parse(new TextDecoder().decode(await download(`${base}${revision}`))) as GitTree;
  let sha = revision;
  for (const name of directory.split("/")) {
    const entry = tree.tree.find(entry => entry.path === name && entry.type === "tree");
    if (!entry) throw new Error(`missing upstream directory ${directory}`);
    sha = entry.sha;
    tree = JSON.parse(new TextDecoder().decode(await download(`${base}${sha}`))) as GitTree;
  }
  tree = JSON.parse(new TextDecoder().decode(await download(`${base}${sha}?recursive=1`))) as GitTree;
  if (tree.truncated) throw new Error(`refusing truncated upstream directory ${directory}; select smaller subtrees`);
  return tree;
}

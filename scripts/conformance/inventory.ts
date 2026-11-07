import { existsSync, readFileSync, realpathSync } from "node:fs";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { sha256, type Reference } from "./sources";

export const statuses = ["verified", "partial", "unsupported", "unverified", "excluded"] as const;
export type Status = typeof statuses[number];
export type Source = {
  id: string;
  name: string;
  repository: string;
  revision: string;
  index: string;
  sha256: string;
  inputs: { path: string; sha256: string }[];
  tree?: { directory: string; sha256: string };
  license: string;
};
export type Profile = {
  id: string;
  description: string;
  node: string[];
  typescript: string[];
  options: Record<string, string | boolean>;
  limitations: string;
};
export type Evidence = {
  id: string;
  target: "javascript" | "typescript" | "node" | "core";
  name: string;
  source: string;
  fixture?: string;
  cases?: string[];
  oracles: string[];
  profiles: string[];
  outcomes: string[];
  upstream?: { source: string; path: string; sha256: string }[];
};
export type Assessment = {
  id: string;
  title: string;
  layer: string;
  status: Status;
  baselines: Record<string, string>;
  references: string[];
  evidence: string[];
  reason: string;
  documentation: string[];
};
export type Inventory = { schema: 1; profiles: Profile[]; evidence: Evidence[]; assessments: Assessment[] };
export type Loaded = { sources: Source[]; references: Reference[]; inventory: Inventory };

export function repositoryFile(root: string, path: string) {
  if (!path || isAbsolute(path) || path.includes("\\") || path.split("/").includes("..")) throw new Error(`unsafe repository path: ${path}`);
  const file = resolve(root, path);
  if (!existsSync(file)) throw new Error(`missing repository file: ${path}`);
  const located = relative(realpathSync(root), realpathSync(file));
  if (located === ".." || located.startsWith(`..${sep}`) || isAbsolute(located)) throw new Error(`path escapes repository: ${path}`);
  return file;
}

function unique<T extends { id: string }>(entries: T[], label: string) {
  if (!Array.isArray(entries)) throw new Error(`${label} must be an array`);
  const found = new Map<string, T>();
  for (const entry of entries) {
    if (!entry || typeof entry.id !== "string" || !entry.id.trim() || found.has(entry.id)) throw new Error(`invalid or duplicate ${label}: ${entry?.id}`);
    found.set(entry.id, entry);
  }
  return found;
}

function text(value: unknown, label: string) {
  if (typeof value !== "string" || !value.trim()) throw new Error(`missing ${label}`);
}

function strings(value: unknown, label: string, nonempty = true): asserts value is string[] {
  if (!Array.isArray(value) || (nonempty && !value.length) || value.some(v => typeof v !== "string" || !v.trim()) || new Set(value).size !== value.length) throw new Error(`invalid ${label}`);
}

function documentation(root: string, link: string) {
  const [path, anchor] = link.split("#");
  const body = readFileSync(repositoryFile(root, path), "utf8");
  if (anchor) {
    const headings = [...body.matchAll(/^#+ (.+)$/gm)].map(match => match[1].toLowerCase().replace(/[^\w\s-]/g, "").trim().replace(/\s+/g, "-"));
    if (!headings.includes(anchor)) throw new Error(`missing documentation anchor: ${link}`);
  }
}

export function validate(root: string, loaded: Loaded) {
  const { sources, references, inventory } = loaded;
  if (inventory.schema !== 1) throw new Error("unsupported coverage inventory schema");
  if (!sources.length || !references.length || !inventory.profiles?.length || !inventory.assessments?.length) throw new Error("coverage inventory cannot be empty");
  const sourceById = unique(sources, "source");
  const referenceById = unique(references, "reference");
  const profileById = unique(inventory.profiles, "profile");
  const evidenceById = unique(inventory.evidence, "evidence");
  unique(inventory.assessments, "assessment");
  for (const source of sources) {
    if (!/^[0-9a-f]{40}$/.test(source.revision) || !/^[0-9a-f]{64}$/.test(source.sha256)) throw new Error(`unpinned source: ${source.id}`);
    if (!/^[\w-]+\/[\w.-]+$/.test(source.repository)) throw new Error(`invalid source repository: ${source.id}`);
    text(source.name, "source name"); text(source.license, "source license");
    if (sha256(readFileSync(repositoryFile(root, source.index))) !== source.sha256) throw new Error(`upstream index checksum mismatch: ${source.id}`);
    if (!source.inputs.length) throw new Error(`source has no pinned inputs: ${source.id}`);
    for (const input of source.inputs) {
      if (!/^[0-9a-f]{64}$/.test(input.sha256) || !input.path || input.path.startsWith("/") || input.path.split("/").includes("..")) throw new Error(`unpinned or unsafe source input: ${input.path}`);
    }
    if (source.tree && !/^[0-9a-f]{64}$/.test(source.tree.sha256)) throw new Error(`unpinned source tree: ${source.id}`);
  }
  for (const reference of references) {
    if (!sourceById.has(reference.id.split(":", 1)[0])) throw new Error(`unknown reference source: ${reference.id}`);
    text(reference.title, "reference title"); text(reference.path, "reference path"); text(reference.kind, "reference kind");
    if (reference.parent && !referenceById.has(reference.parent)) throw new Error(`missing parent: ${reference.id}`);
    if (reference.kind === "imported-table" && !sourceById.get(reference.id.split(":", 1)[0])!.inputs.some(input => input.path === reference.path)) throw new Error(`unaccounted imported source: ${reference.path}`);
  }
  for (const profile of inventory.profiles) {
    text(profile.description, "profile description"); text(profile.limitations, "profile limitations");
    strings(profile.node, "Node versions"); strings(profile.typescript, "TypeScript versions", false);
    for (const version of [...profile.node, ...profile.typescript]) if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`profile version must be exact: ${version}`);
    if (!profile.options || typeof profile.options !== "object" || Array.isArray(profile.options)) throw new Error(`missing compiler options: ${profile.id}`);
  }
  for (const evidence of inventory.evidence) {
    if (!["javascript", "typescript", "node", "core"].includes(evidence.target)) throw new Error(`invalid test target: ${evidence.id}`);
    if (!/^(\w+::)+\w+$/.test(evidence.name)) throw new Error(`invalid test name: ${evidence.id}`);
    const source = readFileSync(repositoryFile(root, evidence.source), "utf8");
    const name = evidence.name.split("::").at(-1)!;
    if (!new RegExp(`\\bfn\\s+${name}\\s*\\(|consumer_contracts!\\(\\s*${name}\\s*,`).test(source)) throw new Error(`missing Rust test: ${evidence.name}`);
    strings(evidence.profiles, "evidence profiles"); strings(evidence.oracles, "evidence oracles"); strings(evidence.outcomes, "evidence outcomes");
    for (const oracle of evidence.oracles) if (!["node", "tsc", "pledge", "npm"].includes(oracle)) throw new Error(`unknown evidence oracle: ${oracle}`);
    for (const id of evidence.profiles) if (!profileById.has(id)) throw new Error(`unknown evidence profile: ${id}`);
    for (const outcome of evidence.outcomes) if (!["breaking", "compatible", "incomplete", "version-boundary"].includes(outcome)) throw new Error(`invalid evidence outcome: ${outcome}`);
    if (evidence.fixture) {
      const cases = JSON.parse(readFileSync(repositoryFile(root, evidence.fixture), "utf8"));
      if (!Array.isArray(cases) || !cases.length || new Set(cases.map(c => c.name)).size !== cases.length) throw new Error(`invalid consumer fixture: ${evidence.fixture}`);
      strings(evidence.cases, "fixture cases");
      for (const name of evidence.cases) {
        const consumer = cases.find(c => c.name === name);
        if (!consumer) throw new Error(`missing fixture case: ${name}`);
        if (consumer.profile && !evidence.profiles.includes(consumer.profile)) throw new Error(`fixture profile is not declared by evidence: ${name}`);
        if (consumer.upstream && !referenceById.has(consumer.upstream)) throw new Error(`fixture has unknown upstream reference: ${name}`);
      }
    }
    for (const upstream of evidence.upstream ?? []) {
      if (!sourceById.has(upstream.source) || !/^[0-9a-f]{64}$/.test(upstream.sha256) || !upstream.path || upstream.path.split("/").includes("..")) throw new Error(`unpinned upstream case: ${evidence.id}`);
    }
  }
  const assigned = new Set<string>();
  for (const assessment of inventory.assessments) {
    text(assessment.title, "assessment title"); text(assessment.reason, "assessment reason");
    if (!["syntax", "exports", "types", "runtime", "configuration", "scope"].includes(assessment.layer)) throw new Error(`invalid analysis layer: ${assessment.id}`);
    if (!statuses.includes(assessment.status)) throw new Error(`unknown coverage status: ${assessment.id}`);
    strings(assessment.references, "assessment references"); strings(assessment.evidence, "assessment evidence", false);
    const requiredSources = new Set(assessment.references.map(id => id.split(":", 1)[0]));
    if (!assessment.baselines || Object.keys(assessment.baselines).length !== requiredSources.size) throw new Error(`assessment needs exact baseline revisions: ${assessment.id}`);
    for (const id of requiredSources) {
      if (!sourceById.has(id) || assessment.baselines[id] !== sourceById.get(id)!.revision) throw new Error(`assessment must be reviewed for changed baseline ${id}: ${assessment.id}`);
    }
    strings(assessment.documentation, "assessment documentation");
    for (const link of assessment.documentation) documentation(root, link);
    if (["verified", "partial", "unsupported"].includes(assessment.status) && !assessment.evidence.length) throw new Error(`coverage claim needs empirical evidence: ${assessment.id}`);
    for (const id of assessment.evidence) if (!evidenceById.has(id)) throw new Error(`unknown evidence: ${id}`);
    for (const id of assessment.references) {
      if (!referenceById.has(id)) throw new Error(`unknown upstream reference: ${id}`);
      if (assigned.has(id)) throw new Error(`upstream reference classified twice: ${id}`);
      assigned.add(id);
    }
    if (assessment.status === "unsupported" && !assessment.evidence.some(id => evidenceById.get(id)!.outcomes.includes("incomplete"))) throw new Error(`unsupported analysis requires incomplete-result coverage: ${assessment.id}`);
    if (assessment.status === "verified") {
      const outcomes = assessment.evidence.flatMap(id => evidenceById.get(id)!.outcomes);
      if (!outcomes.includes("breaking") || !outcomes.includes("compatible")) throw new Error(`verified coverage needs breaking and compatible controls: ${assessment.id}`);
      if (!assessment.evidence.some(id => evidenceById.get(id)!.oracles.some(oracle => oracle !== "pledge"))) throw new Error(`verified coverage needs an independent consumer oracle: ${assessment.id}`);
    }
  }
  const missing = references.filter(reference => !assigned.has(reference.id));
  if (missing.length) throw new Error(`${missing.length} unclassified upstream references: ${missing.slice(0, 5).map(r => r.id).join(", ")}`);
  const usedEvidence = new Set(inventory.assessments.flatMap(assessment => assessment.evidence));
  for (const id of evidenceById.keys()) if (!usedEvidence.has(id)) throw new Error(`unreferenced evidence: ${id}`);
}

export function load(root: string): Loaded {
  const sources: Source[] = JSON.parse(readFileSync(repositoryFile(root, "docs/coverage/sources.json"), "utf8"));
  const references = sources.flatMap(source => JSON.parse(readFileSync(repositoryFile(root, source.index), "utf8")) as Reference[]);
  const inventory = JSON.parse(readFileSync(repositoryFile(root, "docs/coverage/inventory.json"), "utf8")) as Inventory;
  const loaded = { sources, references, inventory };
  validate(root, loaded);
  return loaded;
}

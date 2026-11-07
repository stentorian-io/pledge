import { readFileSync } from "node:fs";
import { fail } from "../lib/command";

export function checkVersion(root = ".", tag?: string) {
  const read = (path: string) => readFileSync(`${root}/${path}`, "utf8");
  const cargo = Bun.TOML.parse(read("Cargo.toml")) as { package: { name: string; version: string } };
  const lock = Bun.TOML.parse(read("Cargo.lock")) as { package: { name: string; version: string; source?: string }[] };
  const manifest = JSON.parse(read(".release-please-manifest.json"));
  const version = cargo.package.version;
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error("release versions must use major.minor.patch without a prerelease suffix");
  }
  const locked = lock.package.filter(entry => entry.name === cargo.package.name && !entry.source);
  if (locked.length !== 1 || locked[0].version !== version || manifest["."] !== version) {
    throw new Error("Cargo.toml, Cargo.lock, and the Release Please manifest must have the same version");
  }
  const action = Bun.YAML.parse(read("action.yml")) as { runs?: { image?: string } };
  if (action.runs?.image !== `docker://ghcr.io/stentorian-io/pledge:v${version}`) {
    throw new Error("The action image must match the Cargo and release manifest version");
  }
  if (tag && tag !== `v${version}`) throw new Error(`expected tag v${version}, received ${tag}`);
  return version;
}

if (import.meta.main) {
  try {
    console.log(`Release version: ${checkVersion(".", Bun.argv[2])}`);
  } catch (error) {
    fail("release-version", String(error));
  }
}

import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const packageRoot = path.join(root, "packages");
const allowed = new Map([
  ["@ephemeralai/fs", new Set()],
  ["@ephemeralai/fs-sqlite-node", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-sqlite-cloudflare", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-node-vfs", new Set(["@ephemeralai/fs", "@ephemeralai/fs-sqlite-node"])],
  ["@ephemeralai/fs-replication", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-testkit", new Set(["@ephemeralai/fs"])],
]);

const violations = [];
for (const directory of await readdir(packageRoot)) {
  const filename = path.join(packageRoot, directory, "package.json");
  const manifest = JSON.parse(await readFile(filename, "utf8"));
  const declared = {
    ...(manifest.dependencies ?? {}),
    ...(manifest.devDependencies ?? {}),
  };
  const expected = allowed.get(manifest.name);
  if (!expected) violations.push(`unexpected package ${manifest.name}`);
  for (const dependency of Object.keys(declared).filter((name) => name.startsWith("@ephemeralai/"))) {
    if (!expected?.has(dependency)) {
      violations.push(`${manifest.name} must not depend on ${dependency}`);
    }
  }
}

const fsSource = await readFile(path.join(packageRoot, "fs", "src", "index.ts"), "utf8");
for (const forbidden of ["node:", "cloudflare:", "@cloudflare/", "fuse", "rpc"]) {
  if (fsSource.toLowerCase().includes(forbidden)) {
    violations.push(`core public entry contains forbidden host import/text: ${forbidden}`);
  }
}

if (violations.length) {
  console.error(violations.join("\n"));
  process.exitCode = 1;
} else {
  console.log("architecture: dependency direction and host boundary valid");
}


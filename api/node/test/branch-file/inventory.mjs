// SDK documentation and shell spellings must track the built call table.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { table } from "../../src/index.js";

const root = fileURLToPath(new URL("../../../../", import.meta.url));
const command = process.env.LAYERFS_CLI ?? join(root, "target/debug/layerfs");
const docs = join(root, "docs/architecture/api/sdk");

test("the SDK index lists all 64 current calls, including branch/file additions", () => {
  assert.equal(table.length, 64);
  const index = readFileSync(join(docs, "README.md"), "utf8");
  assert.match(index, /64 calls in 9 families/);
  for (const spec of table) {
    assert.ok(existsSync(join(docs, spec.family, `${spec.call}.md`)), `${spec.family}.${spec.call} has a call page`);
    assert.ok(index.includes(`(${spec.family}/${spec.call}.md)`), `${spec.family}.${spec.call} appears in SDK inventory`);
  }
});

test("branch/file CLI help exposes the same additions and options as the SDK", () => {
  for (const spec of table.filter(s => s.family === "branch" || s.family === "file")) {
    const r = spawnSync(command, ["help", spec.family, spec.call.replaceAll("_", "-")], { encoding: "utf8", timeout: 5000 });
    assert.equal(r.error, undefined, String(r.error));
    assert.equal(r.status, 0, r.stderr);
    assert.ok(r.stdout.startsWith(`layerfs ${spec.family} ${spec.call.replaceAll("_", "-")}`), r.stdout);
    for (const opt of spec.options.filter(o => o.kind !== "function")) {
      assert.ok(r.stdout.includes(`--${opt.name.replaceAll("_", "-")}`), `${spec.family}.${spec.call}: ${opt.name}`);
    }
  }
});

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

test("the SDK pages name all 70 current calls, one `### family.call` heading each", () => {
  assert.equal(table.length, 70);
  const families = [...new Set(table.map((spec) => spec.family))];
  assert.equal(families.length, 10);
  const index = readFileSync(join(docs, "README.md"), "utf8");
  assert.ok(index.includes(`**${table.length} calls in ${families.length} families**`), "the SDK index states the count once");
  for (const family of families) {
    const page = join(docs, `${family}.md`);
    assert.ok(existsSync(page), `${family} has a family page`);
    const text = readFileSync(page, "utf8");
    const headings = text.split("\n").filter((line) => line.startsWith(`### ${family}.`)).map((line) => line.slice(4).trim());
    const calls = table.filter((spec) => spec.family === family).map((spec) => `${spec.family}.${spec.call}`);
    for (const name of calls) {
      assert.equal(headings.filter((heading) => heading === name).length, 1, `${name} has exactly one heading in ${family}.md`);
    }
    assert.deepEqual(headings.filter((heading) => !calls.includes(heading)), [], `${family}.md heads no call outside the table`);
    for (const name of calls) {
      const anchor = `${family}.md#${name.replace(".", "")}`;
      assert.ok(index.includes(`(${anchor})`), `${name} appears in the SDK index as ${anchor}`);
    }
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

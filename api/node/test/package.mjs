// What the package says about itself: its licence is the repository's, its
// typings are what the call table gives, and the files it ships are the ones
// it means to.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { table } from "../src/index.js";
import { render } from "../../scripts/gen-node-types.mjs";

const here = (path) => fileURLToPath(new URL(path, import.meta.url));
const manifest = JSON.parse(readFileSync(here("../package.json"), "utf8"));

test("the licence is the repository's MIT, shipped with the package", () => {
  const licence = readFileSync(here("../LICENSE"), "utf8");
  assert.equal(licence, readFileSync(here("../../../LICENSE"), "utf8"), "the package LICENSE is the repository's");
  assert.match(licence, /^MIT License/);
  assert.equal(manifest.license, "MIT");
  assert.ok(manifest.files.includes("LICENSE"));
});

test("the package names its repository, platform and npm page", () => {
  assert.deepEqual(manifest.os, ["linux"]);
  assert.deepEqual(manifest.cpu, ["x64"]);
  assert.deepEqual(manifest.libc, ["glibc"]);
  assert.equal(manifest.repository.type, "git");
  assert.equal(manifest.repository.url, "git+https://github.com/Ephemeral-AI-Lab/layerfs.git");
  assert.equal(manifest.repository.directory, "api/node");
  assert.equal(manifest.homepage, "https://github.com/Ephemeral-AI-Lab/layerfs#readme");
  assert.equal(manifest.bugs.url, "https://github.com/Ephemeral-AI-Lab/layerfs/issues");
  assert.ok(Array.isArray(manifest.keywords) && manifest.keywords.length >= 5);
  assert.ok(manifest.description.length > 0 && !manifest.description.includes("#533"));
});

test("index.d.ts is exactly what the call table generates", () => {
  assert.equal(readFileSync(here("../src/index.d.ts"), "utf8"), render(table), "run node api/scripts/gen-node-types.mjs");
  assert.equal(manifest.types, "./src/index.d.ts");
  assert.equal(manifest.exports["."].types, "./src/index.d.ts");
});

test("every call, argument and option of the table is in the typings", () => {
  const types = readFileSync(here("../src/index.d.ts"), "utf8");
  const pascal = (word) => word.replace(/(^|_)([a-z])/g, (_, __, letter) => letter.toUpperCase());
  for (const spec of table) {
    const block = types.split(`export interface ${pascal(spec.family)}Family {\n`)[1].split("\n}\n")[0];
    // the overloads that take the options right after the required positionals
    // come first; the full signature is the last
    const lines = block.split("\n").filter((text) => text.startsWith(`  ${spec.call}(`));
    const line = lines.at(-1);
    assert.ok(lines.length > 0 && lines.every((text) => text.endsWith("): Promise<Result>;")), `${spec.family}.${spec.call}`);
    assert.ok(block.includes(`  /** ${spec.help} */\n${lines[0]}`), `${spec.family}.${spec.call} help`);
    for (const arg of spec.args) assert.ok(line.includes(`${arg.name}${arg.optional ? "?" : ""}: `), `${spec.family}.${spec.call}(${arg.name})`);
    for (const opt of spec.options) assert.ok(types.includes(`  ${opt.name}?: `), `${spec.family}.${spec.call} ${opt.name}`);
  }
});

test("the package ships its source, typings, addon and licence, and requires Node 22", () => {
  assert.equal(manifest.engines.node, ">=22");
  const pack = spawnSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], { cwd: here(".."), encoding: "utf8", timeout: 60_000 });
  assert.equal(pack.status, 0, pack.stderr);
  const files = JSON.parse(pack.stdout)[0].files.map((file) => file.path).sort();
  for (const wanted of ["LICENSE", "package.json", "src/errors.js", "src/index.d.ts", "src/index.js", "src/native.js", "src/values.js"]) {
    assert.ok(files.includes(wanted), `${wanted} in ${files}`);
  }
  assert.ok(files.every((file) => !file.startsWith("test/")), "tests are not shipped");
});

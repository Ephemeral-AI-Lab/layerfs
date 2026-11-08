// Exact-file-list test and metadata test for @layerfs/cli
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (path) => fileURLToPath(new URL(path, import.meta.url));
const manifest = JSON.parse(readFileSync(here("../package.json"), "utf8"));

test("the licence is the repository's MIT, shipped with the package", () => {
  const licence = readFileSync(here("../LICENSE"), "utf8");
  assert.equal(licence, readFileSync(here("../../../LICENSE"), "utf8"), "the package LICENSE is the repository's");
  assert.match(licence, /^MIT License/);
  assert.equal(manifest.license, "MIT");
  assert.ok(manifest.files.includes("LICENSE"));
});

test("the package names its repository, platform and metadata", () => {
  assert.deepEqual(manifest.os, ["linux"]);
  assert.deepEqual(manifest.cpu, ["x64"]);
  assert.deepEqual(manifest.libc, ["glibc"]);
  assert.equal(manifest.repository.type, "git");
  assert.equal(manifest.repository.url, "git+https://github.com/Ephemeral-AI-Lab/layerfs.git");
  assert.equal(manifest.repository.directory, "api/cli");
  assert.equal(manifest.homepage, "https://layerfs.ai/");
  assert.equal(manifest.bugs.url, "https://github.com/Ephemeral-AI-Lab/layerfs/issues");
  assert.ok(Array.isArray(manifest.keywords) && manifest.keywords.length >= 5);
  assert.ok(manifest.description.length > 0);
  assert.deepEqual(manifest.bin, { layerfs: "bin/layerfs" });
});

test("the package ships exactly the expected files", () => {
  assert.equal(manifest.engines.node, ">=22");
  const pack = spawnSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], {
    cwd: here(".."),
    encoding: "utf8",
    timeout: 60_000,
  });
  assert.equal(pack.status, 0, pack.stderr);
  const files = JSON.parse(pack.stdout)[0].files.map((file) => file.path).sort();
  const expected = ["LICENSE", "README.md", "package.json"];
  if (existsSync(here("../bin/layerfs"))) expected.push("bin/layerfs");
  assert.deepEqual(files, expected.sort(), `the tarball is exactly ${expected.join(", ")}`);
});

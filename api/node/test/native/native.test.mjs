// The addon loader: the package's own `layerfs.node` or the file
// `LAYERFS_NATIVE` names, and nothing found by looking around. The loader runs
// in child processes from a copy of `src` placed where the source tree's old
// fallback paths would find a decoy addon.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const src = fileURLToPath(new URL("../src", import.meta.url));

/** `{ status, stdout, stderr }` of importing the copied native.js with `env`. */
function load(root, env) {
  const run = spawnSync(process.execPath, ["--input-type=module", "-e",
    `const { native } = await import(${JSON.stringify(join(root, "api/node/src/native.js"))}); console.log(typeof native.table);`], {
    env: { PATH: process.env.PATH, ...env },
    encoding: "utf8",
    timeout: 30_000,
  });
  assert.equal(run.error, undefined);
  return run;
}

test("a missing addon is one clear error, and no source-tree path is tried", () => {
  const root = mkdtempSync(join(tmpdir(), "layerfs-node-native-"));
  mkdirSync(join(root, "api/node"), { recursive: true });
  cpSync(src, join(root, "api/node/src"), { recursive: true });
  writeFileSync(join(root, "api/node/package.json"), '{"type":"module"}');
  // decoys where the old fallbacks looked: loading one would fail differently
  mkdirSync(join(root, "api/crates/layerfs-node"), { recursive: true });
  writeFileSync(join(root, "api/crates/layerfs-node/layerfs.node"), "not an addon");
  writeFileSync(join(root, "layerfs.node"), "not an addon");

  const none = load(root, {});
  assert.notEqual(none.status, 0);
  assert.match(none.stderr, /the LayerFS native addon is missing/);
  assert.ok(none.stderr.includes(join(root, "api/node/layerfs.node")), "names the path it looked at");
  assert.ok(none.stderr.includes(`${process.platform}/${process.arch}`), "names the platform and architecture");
  assert.match(none.stderr, /build-native\.mjs/);
  assert.match(none.stderr, /LAYERFS_NATIVE/);
  assert.doesNotMatch(none.stderr, /could not be loaded/, "the decoys were not tried");

  const named = load(root, { LAYERFS_NATIVE: join(root, "nowhere.node") });
  assert.notEqual(named.status, 0);
  assert.match(named.stderr, /named by LAYERFS_NATIVE/);
  assert.ok(named.stderr.includes(join(root, "nowhere.node")));

  // a file that is not an addon is named, with the platform
  const broken = load(root, { LAYERFS_NATIVE: join(root, "layerfs.node") });
  assert.notEqual(broken.status, 0);
  assert.match(broken.stderr, /could not be loaded on /);

  // the package's own addon is used when it is there
  cpSync(process.env.LAYERFS_NATIVE ?? fileURLToPath(new URL("../layerfs.node", import.meta.url)), join(root, "api/node/layerfs.node"));
  const own = load(root, {});
  assert.equal(own.status, 0, own.stderr);
  assert.equal(own.stdout.trim(), "function");

  // LAYERFS_NATIVE wins over the package's own
  const over = load(root, { LAYERFS_NATIVE: join(root, "nowhere.node") });
  assert.notEqual(over.status, 0);
  rmSync(root, { recursive: true, force: true });
});

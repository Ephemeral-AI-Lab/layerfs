// A refused lfs call must leave no Store folder. Run: node tools/orbstack/lfs-names-test.mjs
import { spawnSync } from "node:child_process";
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const tools = dirname(fileURLToPath(import.meta.url));
const work = mkdtempSync(join(tmpdir(), "lfs-names-"));
const root = join(work, "a/stores");
mkdirSync(root, { recursive: true });
// The real name rule, with no daemon: a stand-in keeps only the SDK's constructor check.
const stub = join(work, "stub.mjs");
writeFileSync(stub, `
  const { readFileSync } = await import("node:fs");
  const source = readFileSync(${JSON.stringify(join(tools, "../../sdk/js/src/index.js"))}, "utf8");
  const rule = new RegExp(source.match(/!\\/(\\^\\[a-z0-9\\].*?\\$)\\/\\.test\\(projectName\\)/)[1]);
  export class LayerFS {
    constructor(name) { if (!rule.test(name)) throw new Error("a project name is required"); this.branch = { list_commits: async () => [] }; }
    async close() {}
  }`);
const lfs = (...args) => spawnSync("node", [join(tools, "lfs.mjs"), ...args], {
  env: { ...process.env, LAYERFS_STORE_ROOT: root, LAYERFS_SDK: stub }, encoding: "utf8",
});
try {
  for (const args of [["project", "--help"], ["worktree", "create", "--help"], ["log", "--help"], ["log", "../escaped"], ["log", "Upper"]]) {
    const done = lfs(...args);
    assert.notEqual(done.status, 0, `lfs ${args.join(" ")} must fail`);
    assert.deepEqual(readdirSync(root), [], `lfs ${args.join(" ")} left a Store folder`);
    assert.equal(existsSync(join(work, "a/escaped")), false, `lfs ${args.join(" ")} wrote outside the Store root`);
  }
  const good = lfs("log", "remy.food", "main");
  assert.equal(good.status, 0, good.stderr);
  assert.deepEqual(readdirSync(root), ["remy.food"]);
  console.log("lfs-names-test: PASS");
} finally {
  rmSync(work, { recursive: true, force: true });
}

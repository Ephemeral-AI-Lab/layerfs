// Sandboxes an agent makes and closes (#428): sandbox.create starts the
// project's next Sandbox, project.list_sandboxes and sandbox.list_workspaces
// show it, workspace.mount(sandbox, branch) mounts only there (no placement
// since 2026-10-23), refused NoSuchSandbox and Capacity (at 16) with
// messages naming the calls to make instead, and sandbox.close refuses
// MountsActive, then stops it. Under the E2E launcher (its Store volume,
// home and deployment):
//   node api/agent/test/verify/sandbox_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: sandbox_real.mjs inner (under tools/worktree.py run)");
const { LayerFS, isRefusal } = await import("@layerfs/agent");
const project = `sb-${Date.now().toString(36)}`;
const source = mkdtempSync(join(tmpdir(), "layerfs-sb-"));
writeFileSync(join(source, "README.md"), "sandboxes\n");
const refusal = (promise) => promise.then((value) => assert.fail(`expected a refusal, got ${JSON.stringify(value)}`), (error) => error);
const started = Date.now();
const shop = await LayerFS.init(project, source);
const mounted = [];
try {
  await shop.branch.fork("lane");
  assert.deepEqual((await shop.project.list_sandboxes()).items, [], "Init starts no Sandbox");
  const own = await shop.sandbox.create();
  assert.deepEqual([own.sandbox, own.image], ["1", process.env.LAYERFS_IMAGE]);
  assert.deepEqual((await shop.project.list_sandboxes()).items.map(({ sandbox, workspaces }) => [sandbox, workspaces]), [["1", 0]]);
  const none = await refusal(shop.workspace.mount("9", "lane"));
  assert.equal(none.code, "NoSuchSandbox", none.message);
  assert.match(none.message, /sandbox\.create\(\)/, "the refusal names the call to make");
  // sixteen in the named Sandbox, then Capacity there; nothing starts another
  for (let i = 0; i < 16; i++) mounted.push(await shop.workspace.mount("1", "lane"));
  assert.ok(mounted.every((ws) => ws.sandbox === "1" && ws.image === process.env.LAYERFS_IMAGE));
  const full = await refusal(shop.workspace.mount("1", "lane"));
  assert.equal(full.code, "Capacity", full.message);
  assert.match(full.message, /sandbox\.create\(\).*sandbox\.list_workspaces\("1"\).*workspace\.unmount/s);
  assert.deepEqual((await shop.project.list_sandboxes()).items.map(({ sandbox, workspaces }) => [sandbox, workspaces]), [["1", 16]], "a refused mount starts no Sandbox");
  await shop.workspace.exec(mounted[0], "echo changed > note.txt");
  const listed = await shop.sandbox.list_workspaces("1", 20);
  assert.equal(listed.items.length, 16);
  assert.ok(listed.items.every((w) => w.sandbox === "1" && w.branch === "lane" && w.activity === "Idle"));
  assert.equal(listed.items.find((w) => w.path === mounted[0].path).changed, true);
  assert.equal(listed.items.find((w) => w.path === mounted[1].path).changed, false);
  const close = await refusal(shop.sandbox.close("1"));
  assert.equal(close.code, "MountsActive", close.message);
  for (const ws of mounted.splice(0)) await shop.workspace.unmount(ws, true);
  assert.deepEqual(await shop.sandbox.close("1"), { kind: "Closed", sandbox: "1" });
  assert.equal((await refusal(shop.sandbox.close("1"))).code, "NoSuchSandbox");
  assert.equal((await refusal(shop.sandbox.list_workspaces("1"))).code, "NoSuchSandbox");
  const closed = await refusal(shop.workspace.mount("1", "lane"));
  assert.equal(closed.code, "NoSuchSandbox", closed.message);
  assert.match(closed.message, /sandbox\.create\(\)/);
  assert.deepEqual((await shop.project.list_sandboxes()).items, []);
  // the closed client refuses
  await shop.close();
  assert.ok(isRefusal(await refusal(shop.project.list_sandboxes()), "InvalidInput"));
  console.log(`sandbox: ok in ${Date.now() - started} ms`);
} finally {
  for (const ws of mounted) await shop.workspace.unmount(ws, true).catch(() => {});
  await shop.close();
  rmSync(source, { recursive: true, force: true });
}

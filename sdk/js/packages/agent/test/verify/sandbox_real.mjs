// Sandboxes an agent makes and closes (#428): sandbox.create starts the
// project's next Sandbox, project.list_sandboxes and sandbox.list_workspaces
// show it, workspace.mount(branch, { sandbox }) mounts only there (refused
// NoSuchSandbox, InvalidInput for another image, Capacity at 16), placement
// still fills the lowest Sandbox with room (a 17th placed Workspace lands in
// Sandbox 2), and sandbox.close refuses MountsActive, then stops it. Under
// the E2E launcher (its Store volume, home and deployment):
//   node sdk/js/packages/agent/test/verify/sandbox_real.mjs inner
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
  assert.equal((await refusal(shop.workspace.mount("lane", { sandbox: "9" }))).code, "NoSuchSandbox");
  assert.equal((await refusal(shop.workspace.mount("lane", { sandbox: "1", image: `sha256:${"0".repeat(64)}` }))).code, "InvalidInput");
  // sixteen in the chosen Sandbox, then Capacity there; placement goes on to Sandbox 2
  for (let i = 0; i < 16; i++) mounted.push(await shop.workspace.mount("lane", { sandbox: "1" }));
  assert.ok(mounted.every((ws) => ws.sandbox === "1"));
  assert.equal((await refusal(shop.workspace.mount("lane", { sandbox: "1" }))).code, "Capacity");
  const placed = await shop.workspace.mount("lane");
  mounted.push(placed);
  assert.equal(placed.sandbox, "2", "a 17th placed Workspace lands in Sandbox 2");
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
  await shop.sandbox.close("2");
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

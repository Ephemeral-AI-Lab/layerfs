// Concurrent mounts and unmounts of different Workspaces of one project
// (#352 F-009, #373): every call answers for its own Workspace; none is
// refused because a sibling call of the same process is between its daemon
// step and its record step, and concurrent mounts take turns placing their
// Workspaces in one Sandbox. Under the E2E launcher (its Store volume, home
// and deployment):
//   node api/agent/test/verify/concurrent_unmount_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: concurrent_unmount_real.mjs inner (under tools/worktree.py run)");
const { LayerFS } = await import("@layerfs/agent");
const name = `cu-${Date.now().toString(36)}`;
let layerfs;
const outcome = (results) => results.map((r) => (r.status === "fulfilled" ? "ok" : `${r.reason?.code}: ${r.reason?.message}`));
const source = mkdtempSync(join(tmpdir(), "layerfs-cu-"));
writeFileSync(join(source, "README.md"), "base\n");
try {
  layerfs = await LayerFS.init(name, source);
  await layerfs.branch.fork("lane");
  const { sandbox } = await layerfs.sandbox.create();
  for (let round = 0; round < 3; round++) {
    const mounted = await Promise.allSettled([0, 1, 2].map(() => layerfs.workspace.mount(sandbox, "lane")));
    assert.deepEqual(outcome(mounted), ["ok", "ok", "ok"], `round ${round}: concurrent mounts`);
    const workspaces = mounted.map((r) => r.value);
    assert.deepEqual(workspaces.map((ws) => ws.sandbox), ["1", "1", "1"], `round ${round}: one Sandbox`);
    const unmounted = await Promise.allSettled(workspaces.map((ws) => layerfs.workspace.unmount(ws, true)));
    assert.deepEqual(outcome(unmounted), ["ok", "ok", "ok"], `round ${round}: concurrent unmounts`);
  }
  console.log("concurrent unmount: ok");
} finally {
  await layerfs.close();
  rmSync(source, { recursive: true, force: true });
}

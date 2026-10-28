// Idle close as the binding sees it (#373): a Sandbox whose daemon ended
// itself is a stopped container. Listing leaves it out but removes nothing
// (read-only since 2026-10-23); the next sandbox.create reclaims it (deletes
// the container and its records) and starts Sandbox 1 again (no placement:
// a mount names its Sandbox). The container is stopped here through
// the Engine, which is what the daemon's own exit leaves; that exit is
// checked by layerfs-daemon's observed_application test. Under the E2E
// launcher (its Store volume, home and deployment):
//   node api/agent/test/verify/idle_close_real.mjs inner
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: idle_close_real.mjs inner (under tools/worktree.py run)");

/** One Engine exchange over the Docker socket: its status. */
const engine = (method, path) => new Promise((resolve, reject) => {
  const sent = request({ socketPath: process.env.LAYERFS_DOCKER, method, path }, (response) => {
    response.resume();
    response.on("end", () => resolve(response.statusCode));
  });
  sent.on("error", reject);
  sent.end();
});

const { LayerFS } = await import("@layerfs/agent");
const project = `ic-${Date.now().toString(36)}`;
let layerfs;
const records = join(process.env.LAYERFS_HOME, project, "sandboxes", "1");
const container = () => readFileSync(join(records, "container"), "utf8").trim();
const source = mkdtempSync(join(tmpdir(), "layerfs-ic-"));
writeFileSync(join(source, "README.md"), "idle\n");
try {
  layerfs = await LayerFS.init(project, source);
  await layerfs.branch.fork("lane");
  assert.deepEqual((await layerfs.project.list_sandboxes()).items.map((s) => s.sandbox), [], "Import starts no Sandbox");

  // Listing leaves out a stopped Sandbox and reclaims nothing.
  const first = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "lane");
  assert.equal(first.sandbox, "1");
  const one = container();
  assert.deepEqual((await layerfs.project.list_sandboxes()).items.map((s) => s.sandbox), ["1"]);
  await layerfs.workspace.unmount(first, true);
  assert.equal(await engine("POST", `/v1.54/containers/${one}/stop?t=10`), 204);
  assert.deepEqual((await layerfs.project.list_sandboxes()).items, [], "a stopped Sandbox is not listed");
  assert.equal(existsSync(records), true, "listing keeps its records");
  assert.equal(await engine("GET", `/v1.54/containers/${one}/json`), 200, "listing keeps its container");

  // sandbox.create reclaims it, then starts Sandbox 1 again.
  const second = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "lane");
  const two = container();
  assert.equal(second.sandbox, "1");
  assert.notEqual(two, one, "a new container");
  assert.equal(await engine("GET", `/v1.54/containers/${one}/json`), 404, "create deleted the stopped container");
  await layerfs.workspace.unmount(second, true);
  assert.equal(await engine("POST", `/v1.54/containers/${two}/stop?t=10`), 204);
  const third = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "lane");
  assert.equal(third.sandbox, "1");
  assert.notEqual(container(), two);
  assert.equal(await engine("GET", `/v1.54/containers/${two}/json`), 404);
  const ran = await layerfs.workspace.exec(third, "cat README.md");
  assert.equal(ran.stdout, "idle\n");
  await layerfs.workspace.unmount(third, true);
  console.log("idle close: ok");
} finally {
  await layerfs.close();
  rmSync(source, { recursive: true, force: true });
}

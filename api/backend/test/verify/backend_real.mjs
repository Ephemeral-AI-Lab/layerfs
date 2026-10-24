// @layerfs/backend against a real Store and Sandbox (#428): Store reads with
// no Sandbox (history, storage, revisions, get_comparable of a Commit, a
// published Layer and Init's Layer), then a Workspace's daemon reads, a
// background process, and the Sandbox's daemon log through get_log. Under
// the E2E launcher (its Store volume, home and deployment):
//   node api/backend/test/verify/backend_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: backend_real.mjs inner (under tools/worktree.py run)");
const { LayerFS } = await import("@layerfs/agent");
const { LayerFSBackend } = await import("@layerfs/backend");
const project = `be-${Date.now().toString(36)}`;
const source = mkdtempSync(join(tmpdir(), "layerfs-be-"));
writeFileSync(join(source, "README.md"), "backend\n");
const refusal = (promise) => promise.then((value) => assert.fail(`expected a refusal, got ${JSON.stringify(value)}`), (error) => error);
const shop = await LayerFS.init(project, source);
const backend = LayerFSBackend.of(shop);
let ws = null;
try {
  // Store reads: no Sandbox
  const initial = await backend.store.project.get_history();
  assert.equal(initial.kind, "ok");
  const initLayer = initial.layers.at(-1).id;
  await shop.branch.fork("lane");
  const written = await shop.fs.write("lane", "/note.txt", "one\n", "Add note");
  const commit = written.commit.id;
  const published = await shop.branch.publish("lane");
  const layer = published.publishedLayer.id;
  const history = await backend.store.project.get_history();
  assert.notEqual(history.version, initial.version);
  assert.deepEqual(await backend.store.project.get_history(history.version), { kind: "unchanged", version: history.version });
  assert.deepEqual(await backend.store.project.get_comparable(commit), { commit, root: history.commits.find((c) => c.id === commit).root, exact: true });
  const same = await backend.store.project.get_comparable(layer);
  assert.deepEqual([same.commit, same.exact], [commit, true], "a published Layer compares as its source Commit");
  assert.equal((await refusal(backend.store.project.get_comparable(initLayer))).code, "NotComparable", "Init's Layer has no Commit");
  const revisions = await backend.store.fs.list_revisions("lane", "/note.txt");
  assert.equal(revisions.items[0].commit ?? revisions.items[0].id, commit);
  assert.ok((await backend.store.project.get_storage()).canonical > 0);
  assert.ok((await backend.store.branch.list_commit_storage("lane")).items.length >= 1);
  assert.deepEqual((await shop.project.list_sandboxes()).items, [], "Store reads start no Sandbox");
  assert.equal((await refusal(backend.daemon.sandbox.get_log("1"))).code, "NoSuchSandbox");

  // daemon reads: the Workspace's Sandbox runs
  ws = await shop.workspace.mount((await shop.sandbox.create()).sandbox, "lane");
  await shop.workspace.exec(ws, "echo two > fresh.txt");
  const status = await backend.daemon.workspace.status(ws);
  assert.equal(status.headMovedSinceMount, false);
  const changes = await backend.daemon.workspace.list_changes(ws, 20, 0);
  assert.ok(changes.items.some((change) => change.path === "/fresh.txt"), JSON.stringify(changes.items));
  assert.ok((await backend.daemon.workspace.get_storage(ws)).inodes >= 1);
  const proc = await backend.daemon.process.start(ws, "sleep 30");
  assert.match(proc.process, new RegExp(`^proc_${ws.sandbox}_`));
  assert.equal((await backend.daemon.process.list()).items.find((p) => p.id === proc.process)?.sandbox, ws.sandbox);
  await backend.daemon.process.cancel(proc.process);
  const log = await backend.daemon.sandbox.get_log(ws.sandbox, { lines: 50 });
  assert.equal(log.sandbox, ws.sandbox);
  assert.match(log.container, /^[0-9a-f]{64}$/);
  assert.equal(Buffer.byteLength(log.text) <= 64 * 1024, true);
  assert.equal(typeof log.bytes, "number");
  console.log(`daemon log tail: ${log.bytes} bytes`);
  await shop.workspace.unmount(ws, true);
  ws = null;
  console.log("backend real: ok");
} finally {
  if (ws) await shop.workspace.unmount(ws, true).catch(() => {});
  await shop.close();
  // closed: refused before any binding call
  assert.equal((await refusal(backend.store.project.get_history())).code, "InvalidInput");
  rmSync(source, { recursive: true, force: true });
}

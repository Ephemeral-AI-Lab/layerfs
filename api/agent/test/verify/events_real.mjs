// P9 events against one fresh real Disposable project, Docker and FUSE daemon.
// Run in the sealed real E2E environment used by calls_real.mjs:
//   node api/agent/test/verify/events_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { source } from "./scenario.mjs";

if (process.argv[2] !== "inner") throw new Error("usage: events_real.mjs inner (under the real E2E launcher)");
const { LayerFS } = await import("@layerfs/agent");
const work = mkdtempSync(join(tmpdir(), "layerfs-events-real-"));
process.env.LAYERFS_HOME = join(work, "home");
const project = `events-${Date.now().toString(36)}`;
let lf, other;
const ordered = (page, cursor, count) => {
  assert.equal(page.events.length, count);
  assert.deepEqual(page.events.map((event) => event.seq), Array.from({ length: count }, (_, i) => cursor + i + 1));
  assert.equal(page.cursor, cursor + count);
  assert.equal(page.stale, false);
  assert.equal(page.closed, false);
  return page.events;
};
try {
  lf = await LayerFS.init(project, source(join(work, "source")));
  const events = await lf.events.subscribe();
  assert.deepEqual(events.snapshot.branches.map(({ name }) => name), ["main"]);
  assert.deepEqual(events.snapshot.workspaces, []);
  assert.deepEqual(events.snapshot.sandboxes, []);
  assert.deepEqual(await events.poll(), { events: [], cursor: 0, hasMore: false, stale: false, closed: false }, "Init before subscribe creates no native event");

  const forked = await lf.branch.fork("feature");
  const file = await lf.fs.write("feature", "/file.txt", "one\n", "file");
  const { sandbox } = await lf.sandbox.create();
  const ws = await lf.workspace.mount(sandbox, "feature");
  const executed = await lf.workspace.exec(ws, "echo workspace > out.txt", "write Workspace file");
  assert.equal(executed.exitCode, 0);
  const committed = await lf.workspace.commit(ws, "Workspace event", false, { author: "P9 real", labels: ["p9", "workspace"] });
  assert.equal(committed.kind, "Committed");
  assert.equal((await lf.workspace.commit(ws, "no change")).kind, "UpToDate");
  assert.equal((await lf.workspace.unmount(ws)).kind, "Unmounted");
  assert.equal((await lf.sandbox.close(sandbox)).kind, "Closed");

  const lifecycle = ordered(await events.poll(), 0, 7);
  assert.deepEqual(lifecycle.map((event) => [event.type, event.source]), [
    ["branch.forked", "store"], ["fs.committed", "store"], ["sandbox.created", "daemon"],
    ["workspace.mounted", "daemon"], ["commit.created", "store"], ["workspace.unmounted", "daemon"], ["sandbox.closed", "daemon"],
  ]);
  assert.deepEqual(lifecycle[0].payload, { branch: "feature", forkPoint: { kind: "layer", id: forked.base } });
  assert.deepEqual(lifecycle[1].payload, { branch: "feature", method: "fs.write", path: "/file.txt", from: null, to: null, commit: file.commit });
  assert.deepEqual(lifecycle[2].payload, { sandbox });
  assert.deepEqual(lifecycle[3].payload, { workspace: { sandbox, path: ws.path }, branch: "feature" });
  assert.deepEqual(lifecycle[4].payload, { branch: "feature", workspace: { sandbox, path: ws.path }, commit: committed.commit });
  assert.equal(lifecycle[4].payload.commit.author, "P9 real");
  assert.deepEqual(lifecycle[4].payload.commit.labels, ["p9", "workspace"]);
  for (const field of ["id", "branch", "parent", "secondParent", "base", "root", "via", "message", "time"]) assert.ok(Object.hasOwn(lifecycle[4].payload.commit, field), `complete Commit has ${field}`);
  assert.deepEqual(lifecycle[5].payload, { workspace: { sandbox, path: ws.path } });
  assert.deepEqual(lifecycle[6].payload, { sandbox });

  await lf.branch.fork("left", "feature");
  await lf.branch.fork("right", "feature");
  await lf.fs.write("left", "/left.txt", "left\n", "left");
  const fast = await lf.branch.merge("left", "right", "fast", "fast");
  assert.equal(fast.fastForward, true);
  await lf.fs.write("right", "/right.txt", "right\n", "right");
  const slow = await lf.branch.merge("left", "right", "slow", "slow");
  assert.equal(slow.fastForward, false);
  const published = await lf.branch.publish("slow");
  assert.equal(published.kind, "Added");
  assert.equal((await lf.branch.publish("slow")).kind, "UpToDate");

  other = await LayerFS.open(project);
  const slow2 = await other.branch.merge("left", "right", "slow2", "slow2");
  assert.equal(slow2.root, slow.root);
  assert.notEqual(slow2.head, slow.head);
  await other.close();
  other = null;
  assert.equal((await lf.branch.publish("slow2")).kind, "NoChanges");

  const history = ordered(await events.poll(), 7, 7);
  assert.deepEqual(history.map((event) => event.type), ["branch.forked", "branch.forked", "fs.committed", "branch.merged", "fs.committed", "branch.merged", "branch.published"]);
  assert.equal(history[2].payload.path, "/left.txt");
  assert.equal(history[4].payload.path, "/right.txt");
  assert.deepEqual(history[3].payload, { merge: fast });
  assert.deepEqual(history[5].payload, { merge: slow });
  assert.deepEqual(history[6].payload, { branch: published.source.branch, commit: published.source.commit, layer: published.publishedLayer.id, basedOn: published.basedOn });
  assert.equal(history.some((event) => event.payload.merge?.branch === "slow2"), false, "the second client's operation is absent");

  await events.close();
  await events.close();
  const fresh = await lf.events.subscribe();
  assert.equal(fresh.cursor, 0);
  assert.deepEqual((await fresh.poll()).events, [], "a new subscription never replays the old queue");
  await fresh.close();
  await lf.close();
  lf = null;
  console.log("PASS events_real (14 native events; Workspace/fs/fork/merge/publish and lifecycle)");
} finally {
  await other?.close();
  await lf?.close();
  rmSync(work, { recursive: true, force: true });
}

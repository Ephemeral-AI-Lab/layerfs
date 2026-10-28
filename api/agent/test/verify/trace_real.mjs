// One real trace check on a Disposable Store: with LAYERFS_TRACE set, a fork, mount, exec, background process, Commit and merge write start and
// end lines that carry their result ids and no file content. Under the E2E
// launcher (its Store volume, home and deployment):
//   node api/agent/test/verify/trace_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: trace_real.mjs inner (under tools/worktree.py run)");
const work = mkdtempSync(join(tmpdir(), "layerfs-trace-"));
const file = join(work, "trace.jsonl");
Object.assign(process.env, { LAYERFS_TRACE: file, LAYERFS_TRACE_START: "1", LAYERFS_TRACE_STEP: "call_real" });
const { LayerFS } = await import("@layerfs/agent");
const { LayerFSBackend } = await import("@layerfs/backend");
const name = `trace-${Date.now().toString(36)}`;
let layerfs, backend;
writeFileSync(join(work, "README.md"), "base\n");
try {
  layerfs = await LayerFS.init(name, work);
  backend = LayerFSBackend.of(layerfs);
  await layerfs.branch.fork("feature");
  const ws = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "feature");
  await layerfs.workspace.exec(ws, "printf 'SECRET-%s\\n' CONTENT > note.txt && cat note.txt; exit 3").catch(() => {});
  const proc = await backend.daemon.process.start(ws, "sleep 30");
  assert.equal((await backend.daemon.process.list()).items.find((p) => p.id === proc.process)?.sandbox, ws.sandbox);
  await backend.daemon.process.cancel(proc.process);
  await layerfs.workspace.commit(ws, "Add note");
  await layerfs.workspace.unmount(ws);
  await layerfs.branch.fork("other");
  await layerfs.fs.write("other", "/other.txt", "SECRET-CONTENT\n", "Other");
  await layerfs.branch.merge("feature", "other", "joined", "Join");
} finally {
  await layerfs.close();
}
const text = readFileSync(file, "utf8");
rmSync(work, { recursive: true, force: true });
const lines = text.trimEnd().split("\n").map((line) => JSON.parse(line));
const end = (method) => lines.find((line) => line.phase === "end" && line.method === method);
for (const line of lines) assert.equal(line.step, "call_real");
assert.equal(lines.filter((l) => l.phase === "start").length, lines.filter((l) => l.phase === "end").length);
assert.ok(!text.includes("SECRET-CONTENT"), "no file content or exec output");
const fork = end("branch.fork");
assert.equal(fork.branch, "feature");
assert.match(fork.base, /^[0-9a-f]{64,}$/);
const mounted = end("workspace.mount");
assert.equal(mounted.sandbox, "1");
assert.match(mounted.path, /^\//);
assert.equal(mounted.branch, "feature");
assert.match(mounted.baseLayer, /^[0-9a-f]{64,}$/);
assert.equal(end("workspace.exec").exitCode, 3);
assert.match(end("backend.daemon.process.start").process, /^proc_1_/);
const merge = end("branch.merge");
assert.equal(merge.branch, "joined");
assert.equal(merge.head, merge.commit);
console.log(JSON.stringify({ lines: lines.length, fork: { branch: fork.branch, base: fork.base.slice(0, 12), head: fork.head }, mount: { sandbox: mounted.sandbox, path: mounted.path, base: mounted.base, baseLayer: mounted.baseLayer.slice(0, 12) }, exitCode: end("workspace.exec").exitCode, process: end("backend.daemon.process.start").process, merge: { branch: merge.branch, head: merge.head?.slice(0, 12) } }, null, 1));
console.log("PASS trace real");

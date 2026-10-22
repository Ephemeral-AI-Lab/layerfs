// C12 Store capture (#420): the same per-tool-call cycle as
// c12_cycle_real.mjs (empty root forked to `c12`, one identical untimed
// warm-up call terminally unmounted, then mount -> exec -> Commit ->
// unmount), with a consistent copy of the global Store taken between the
// phases, so the cycle's global-Store writes can be diffed row by row.
// Capture only: its phase times include nothing and are not a sample.
//
// Each copy is the SQLite online backup of the live WAL Store, run by a
// separate `node` process (its own POSIX locks; this process's Store
// connection is untouched) while no call is in flight:
//   s0-before-mount, s1-after-mount, s2-after-exec, s3-after-commit,
//   s4-after-unmount  (.sqlite files in the output directory)
//
//   python3 tools/worktree.py run --resources <resources.json> --bundle <sealed linux bundle> \
//     --timeout 90 -- node sdk/js/test/verify/c12_capture_real.mjs inner
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: c12_capture_real.mjs inner (under tools/worktree.py run)");
const OUT = process.argv[3] ?? "/receipts";
const C12 = "{ git init -q; for i in $(seq 1 100); do echo $i > f$i; done; git add -A; git -c user.email=a@b -c user.name=a commit -qm init; } >/dev/null";
const STORE = process.env.LAYERFS_STORE_PATH;
const BRANCH = "c12";
const BACKUP = `const { DatabaseSync, backup } = require("node:sqlite");
const db = new DatabaseSync(process.argv[1], { readOnly: true });
backup(db, process.argv[2]).then((pages) => { db.close(); console.log(pages); });`;

const report = { cell: "C12", purpose: "capture only (not a timing sample)", snapshots: {} };
function snapshot(name) {
  const path = join(OUT, `${name}.sqlite`);
  const pages = execFileSync(process.execPath, ["-e", BACKUP, STORE, path], { encoding: "utf8", timeout: 20_000 }).trim();
  report.snapshots[name] = { pages: Number(pages), bytes: statSync(path).size, wal: (() => { try { return statSync(`${STORE}-wal`).size; } catch { return null; } })() };
}

const { LayerFS } = await import("../../src/index.js");
const work = mkdtempSync(join(tmpdir(), "layerfs-c12c-"));
const source = join(work, "empty");
mkdirSync(source);
const lf = new LayerFS(`c12c-${Date.now().toString(36)}`);
try {
  process.env.LAYERFS_TRACE_STEP = "setup";
  await lf.project.init(source);
  await lf.branch.fork(BRANCH);
  process.env.LAYERFS_TRACE_STEP = "warmup";
  const warm = await lf.workspace.mount(BRANCH);
  assert.equal((await lf.workspace.exec(warm, C12)).exitCode, 0);
  await lf.workspace.unmount(warm, true);
  snapshot("s0-before-mount");
  process.env.LAYERFS_TRACE_STEP = "sample.mount";
  const ws = await lf.workspace.mount(BRANCH);
  snapshot("s1-after-mount");
  process.env.LAYERFS_TRACE_STEP = "sample.exec";
  assert.equal((await lf.workspace.exec(ws, C12)).exitCode, 0);
  snapshot("s2-after-exec");
  process.env.LAYERFS_TRACE_STEP = "sample.commit";
  report.commit = await lf.workspace.commit(ws, "C12");
  snapshot("s3-after-commit");
  process.env.LAYERFS_TRACE_STEP = "sample.unmount";
  await lf.workspace.unmount(ws);
  snapshot("s4-after-unmount");
} finally {
  await lf.close?.().catch(() => {});
  rmSync(work, { recursive: true, force: true });
}
writeFileSync(join(OUT, "capture.json"), `${JSON.stringify(report, null, 1)}\n`);
console.log(JSON.stringify(report.snapshots), report.commit?.kind);

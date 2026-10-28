// The Store-face lifecycle on the real native binding (owner direction
// 2026-10-25, route 2): project init, fork, a one-Commit fs.write, reading
// back on the new branch and on main, stat, the Commit listing and close —
// all in this process through the SDK's direct Store route. No Docker, no
// daemon, no WSL2. The Store's volume is LAYERFS_STORE_PATH's parent when
// the E2E launcher provides one (Linux); otherwise this machine's temporary
// volume, which the Windows route guards as NTFS:
//   node sdk/js/test/store_lifecycle.mjs
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const work = mkdtempSync(join(tmpdir(), "layerfs-lifecycle-"));
const volume = process.env.LAYERFS_STORE_PATH
  ? join(dirname(dirname(process.env.LAYERFS_STORE_PATH)), `lifecycle-${process.pid}`)
  : join(work, "volume");
const store = join(volume, "global", "store.sqlite");
try {
  const source = join(work, "source");
  mkdirSync(source);
  writeFileSync(join(source, "a.txt"), "hello windows-native store\n");
  process.env.LAYERFS_HOME = join(work, "home");
  mkdirSync(join(volume, "global"), { recursive: true });
  const { LayerFS } = await import("@layerfs/agent");
  process.env.LAYERFS_STORE_PATH = store;
  const layerfs = await LayerFS.init("lifecycle", source);

  assert.equal(layerfs.created.kind, "Initialized");
  await layerfs.branch.fork("dev");

  const body = "written by the native Store face\n";
  assert.equal((await layerfs.fs.write("dev", "/windows.md", body, "lifecycle write")).kind, "Committed");

  assert.equal((await layerfs.fs.read("dev", "/windows.md")).text, body);
  assert.equal((await layerfs.fs.read("main", "/a.txt")).text, "hello windows-native store\n");
  assert.equal((await layerfs.fs.stat("dev", "/windows.md")).size, Buffer.byteLength(body));

  const commits = await layerfs.branch.list_commits("dev");
  const records = commits.records ?? commits.items ?? commits;
  assert.ok(Array.isArray(records) && records.length >= 1, JSON.stringify(commits).slice(0, 200));

  await layerfs.close();
  console.log(`store_lifecycle: ok (${records.length} commit listed)`);
} finally {
  rmSync(work, { recursive: true, force: true });
  rmSync(volume, { recursive: true, force: true });
}

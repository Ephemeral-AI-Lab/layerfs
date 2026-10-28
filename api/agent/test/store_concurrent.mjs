// Cross-process Store writers on the real native binding (owner direction
// 2026-10-25, route 2): the parent initializes a project and forks one
// branch, then four re-exec'd child processes write distinct files to that
// branch concurrently, each through its own client of the same Store - the
// direct route's cross-process writer lane and the discovery record guard
// serialize them; every write must land. The parent reads all four files
// back and counts the Commits. No Docker, no daemon, no WSL2; on Linux the
// Store sits on the E2E launcher's volume (LAYERFS_STORE_PATH):
//   node sdk/js/test/store_concurrent.mjs            # parent run
//   node sdk/js/test/store_concurrent.mjs --child <home> <store> <name>
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

// One child writer: open the project's Store and write its own file once.
if (process.argv[2] === "--child") {
  const [, , , home, store, name] = process.argv;
  process.env.LAYERFS_HOME = home;
  const { LayerFS } = await import("@layerfs/agent");
  process.env.LAYERFS_STORE_PATH = store;
  const writer = await LayerFS.open("concurrent");
  const body = `written by ${name} in pid ${process.pid}\n`;
  const done = await writer.fs.write("load", `/${name}.md`, body, `write ${name}`);
  await writer.close();
  assert.equal(done.kind, "Committed");
  process.exit(0);
}

const WRITERS = 4;
const work = mkdtempSync(join(tmpdir(), "layerfs-concurrent-"));
const volume = process.env.LAYERFS_STORE_PATH
  ? join(dirname(dirname(process.env.LAYERFS_STORE_PATH)), `concurrent-${process.pid}`)
  : join(work, "volume");
const store = join(volume, "global", "store.sqlite");
const home = join(work, "home");
const retryRemove = (path) => {
  for (let attempt = 0; attempt < 5; attempt++) {
    try { rmSync(path, { recursive: true, force: true }); return; } catch { }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200);
  }
};
try {
  const source = join(work, "source");
  mkdirSync(source);
  writeFileSync(join(source, "README.md"), "concurrent writers\n");
  process.env.LAYERFS_HOME = home;
  mkdirSync(join(volume, "global"), { recursive: true });
  const { LayerFS } = await import("@layerfs/agent");
  process.env.LAYERFS_STORE_PATH = store;
  const parent = await LayerFS.init("concurrent", source);
  await parent.branch.fork("load");
  const base = (await parent.branch.list_commits("load")).items.length;

  const children = [];
  for (let n = 1; n <= WRITERS; n++) {
    children.push(spawn(process.execPath, [import.meta.url.pathname ?? process.argv[1], "--child", home, store, `w${n}`], {
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, LAYERFS_STORE_PATH: store },
    }));
  }
  const reports = await Promise.all(children.map(async (child, index) => {
    const [code, output] = await new Promise((resolve) => {
      let captured = "";
      child.stdout.on("data", (chunk) => { captured += chunk; });
      child.stderr.on("data", (chunk) => { captured += chunk; });
      const timer = setTimeout(() => {
        child.kill();
        resolve([-1, `${captured} (killed: 30s ceiling)`]);
      }, 30_000);
      child.on("exit", (exitCode) => { clearTimeout(timer); resolve([exitCode, captured]); });
    });
    return { index, code, output };
  }));
  for (const { index, code, output } of reports) {
    assert.equal(code, 0, `writer w${index + 1} failed (${code}): ${output.slice(0, 400)}`);
  }

  // Every writer's file is committed and readable; the branch carries them all.
  for (let n = 1; n <= WRITERS; n++) {
    const read = await parent.fs.read("load", `/w${n}.md`);
    assert.ok(read.text.startsWith(`written by w${n} in pid `), read.text);
  }
  const commits = await parent.branch.list_commits("load", 50);
  assert.equal(commits.items.length, base + WRITERS, `${commits.items.length} commits, expected ${base + WRITERS}`);
  await parent.close();
  console.log(`store_concurrent: ok (${WRITERS} writers, ${commits.items.length} commits)`);
} finally {
  retryRemove(work);
  retryRemove(volume);
}

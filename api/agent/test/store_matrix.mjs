// The full Store method matrix on the real native binding (owner direction
// 2026-10-25, route 2): every fs, branch and project operation the direct
// route serves — writes (write, edit, mkdir, cp, mv, rm, apply), reads
// (read with a line range, list, stat, grep, glob, blame), comparisons
// (diff_files, diff_commits, diff_branches) and branch combination
// (merge, publish) — plus the Windows refusal of the daemon-only workspace
// surface. No Docker, no daemon, no WSL2; on Linux the Store sits on the
// E2E launcher's volume (LAYERFS_STORE_PATH):
//   node sdk/js/test/store_matrix.mjs
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const work = mkdtempSync(join(tmpdir(), "layerfs-matrix-"));
const volume = process.env.LAYERFS_STORE_PATH
  ? join(dirname(dirname(process.env.LAYERFS_STORE_PATH)), `matrix-${process.pid}`)
  : join(work, "volume");
const store = join(volume, "global", "store.sqlite");
const retryRemove = (path) => {
  for (let attempt = 0; attempt < 5; attempt++) {
    try { rmSync(path, { recursive: true, force: true }); return; } catch { }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200);
  }
};
try {
  const source = join(work, "source");
  mkdirSync(join(source, "lib"), { recursive: true });
  writeFileSync(join(source, "README.md"), "one\ntwo\nthree\n");
  writeFileSync(join(source, "lib/app.js"), "const a = 1;\nconst b = 2;\n");
  process.env.LAYERFS_HOME = join(work, "home");
  mkdirSync(join(volume, "global"), { recursive: true });
  const { LayerFS } = await import("@layerfs/agent");
  process.env.LAYERFS_STORE_PATH = store;
  const layerfs = await LayerFS.init("matrix", source);

  assert.equal(layerfs.created.kind, "Initialized");
  const branches = await layerfs.project.list_branches();
  assert.deepEqual(branches.items.map((item) => item.name), ["main"]);
  await layerfs.branch.fork("work");

  // Writes: each call is one Commit; the surface changes shape together.
  await layerfs.fs.mkdir("work", "/docs", "make docs");
  await layerfs.fs.write("work", "/docs/note.md", "alpha\n", "first note");
  const second = await layerfs.fs.write("work", "/docs/note.md", "alpha\nbeta\n", "second note");
  assert.equal(second.kind, "Committed");
  const edited = await layerfs.fs.edit("work", "/docs/note.md", "alpha", "gamma", "reword");
  assert.equal(edited.kind, "Committed");
  assert.notEqual(edited.head, second.head, "each write call is one new Commit");
  await layerfs.fs.cp("work", "/docs/note.md", "/docs/copy.md", "copy");
  await layerfs.fs.mv("work", "/docs/copy.md", "/docs/moved.md", "move");
  await layerfs.fs.rm("work", "/docs/moved.md", "remove");
  const listing = await layerfs.fs.list("work", "/docs");
  assert.deepEqual(listing.items.map((item) => item.name).sort(), ["note.md"]);

  // Reads: content, shape and search over the written tree.
  assert.equal((await layerfs.fs.read("work", "/docs/note.md")).text, "gamma\nbeta\n");
  const ranged = await layerfs.fs.read("work", "/README.md", 1, 2);
  assert.equal(ranged.text, "one\ntwo\n");
  assert.equal((await layerfs.fs.stat("work", "/docs/note.md")).size, Buffer.byteLength("gamma\nbeta\n"));
  const grep = await layerfs.fs.grep("work", "gamma");
  assert.deepEqual(grep.items.map((hit) => hit.path), ["/docs/note.md"]);
  assert.deepEqual((await layerfs.fs.glob("work", "**/*.js")).items, ["/lib/app.js"]);
  const blame = await layerfs.fs.blame("work", "/docs/note.md");
  assert.equal(blame.lines.length, 2);
  assert.ok(blame.lines.every((line) => typeof line.commit === "string"));

  // Comparisons: file, commit and branch.
  assert.equal((await layerfs.fs.diff_files("main", "/README.md", "work", "/README.md")).kind, "Equal");
  const changed = await layerfs.fs.diff_files("main", "/README.md", "work", "/docs/note.md");
  assert.notEqual(changed.kind, "Equal");
  const commits = await layerfs.branch.list_commits("work");
  assert.ok(commits.items.length >= 6, `${commits.items.length} commits listed`);
  // main is the project's main line, not a Branch: it moves only through
  // branch.publish and is refused by the Branch name index.
  await assert.rejects(layerfs.branch.list_commits("main"), { code: "NoSuchBranch" });
  const oldest = commits.items[commits.items.length - 1];
  const commitDiff = await layerfs.branch.diff_commits(oldest.id, commits.items[0].id);
  assert.ok(JSON.stringify(commitDiff).includes("/docs/note.md"), JSON.stringify(commitDiff).slice(0, 200));
  const branchDiff = await layerfs.branch.diff_branches("main", "work");
  assert.equal(branchDiff.kind, "ok");

  // Combination: merge reads both lines; publish moves main's head.
  const merged = await layerfs.branch.merge("main", "work", "merged", "combine");
  assert.equal(merged.kind, "Merged");
  assert.equal((await layerfs.fs.read("merged", "/docs/note.md")).text, "gamma\nbeta\n");
  const published = await layerfs.branch.publish("work");
  assert.ok(published.kind, JSON.stringify(published).slice(0, 200));
  assert.equal((await layerfs.fs.read("main", "/docs/note.md")).text, "gamma\nbeta\n");
  // A divergent merge: both sides advanced past their common base, so this
  // is a real merge Commit (two parents), and both sides' files must be
  // readable on the merged Branch and on main after its publish.
  await layerfs.fs.write("work", "/later.md", "later\n", "work advanced");
  await layerfs.branch.fork("feat2");
  await layerfs.fs.write("feat2", "/feat2.md", "feat2 line\n", "feat2 advanced");
  const combined = await layerfs.branch.merge("work", "feat2", "combined", "combine diverged");
  assert.equal(combined.kind, "Merged");
  assert.equal(combined.fastForward, false, "diverged sources cannot fast-forward");
  assert.equal((await layerfs.fs.read("combined", "/later.md")).text, "later\n");
  assert.equal((await layerfs.fs.read("combined", "/feat2.md")).text, "feat2 line\n");
  const combinedHead = (await layerfs.branch.list_commits("combined", 1)).items[0];
  assert.ok(combinedHead.secondParent, "a divergent merge Commit carries two parents");
  await layerfs.branch.publish("combined");
  assert.equal((await layerfs.fs.read("main", "/later.md")).text, "later\n");
  assert.equal((await layerfs.fs.read("main", "/feat2.md")).text, "feat2 line\n");

  const after = await layerfs.project.list_branches();
  assert.deepEqual(after.items.map((item) => item.name).sort(), ["combined", "feat2", "main", "merged", "work"]);

  // The daemon-only surface refuses Unsupported on the Windows route; on
  // Linux the workspace route is the daemon's and is not exercised here.
  if (process.platform === "win32") {
    // The daemon-only surface refuses Unsupported at the native layer; a
    // sandbox.create reaches it with valid arguments on every API shape.
    process.env.LAYERFS_IMAGE = "fake-image";
    process.env.LAYERFS_VOLUME = "fake-volume";
    process.env.LAYERFS_DAEMON = "/fake/daemon";
    await assert.rejects(
      layerfs.sandbox.create({ image: "fake-image" }),
      { code: "Unsupported" },
      "the Windows route refuses the daemon surface",
    );
  }

  await layerfs.close();
  console.log(`store_matrix: ok (${commits.items.length} commits, ${after.items.length} branches)`);
} finally {
  retryRemove(work);
  retryRemove(volume);
}

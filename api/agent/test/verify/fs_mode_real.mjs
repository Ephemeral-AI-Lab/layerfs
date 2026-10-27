// #442: permission modes through the JS SDK and the real native binding.
// fs.write and fs.mkdir take { message, mode } (a message string still
// works), fs.apply takes `mode` on write/mkdir and a chmod change, and
// fs.stat reports `mode`. A mounted Workspace of the Branch shows the same
// modes through FUSE (`stat -c %a`, and the new script runs), and a chmod
// made in the Workspace and committed reads back through fs.stat. Refusals:
// set-user-ID is InvalidInput with reason Mode, a text mode is InvalidInput,
// and options that are neither a message nor an object are refused by the
// SDK. Nothing refused is committed. One attempt per step; nothing retried.
// Linux, under the E2E launcher (its Store volume, SDK home and Sandbox image):
//   python3 tools/worktree.py run --resources <resources.json> --bundle <bundle> \
//     --timeout 90 -- node api/agent/test/verify/fs_mode_real.mjs
import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const { LayerFS } = await import("@layerfs/agent");
const source = mkdtempSync(join(tmpdir(), "layerfs-442-"));
writeFileSync(join(source, "tool.sh"), "#!/bin/sh\necho tool\n");
chmodSync(join(source, "tool.sh"), 0o755);
mkdirSync(join(source, "private"));
writeFileSync(join(source, "private", "f"), "f\n");
chmodSync(join(source, "private"), 0o750);
writeFileSync(join(source, "a.txt"), "a\n");
chmodSync(join(source, "a.txt"), 0o644);
const layerfs = await LayerFS.init("site", source);
rmSync(source, { recursive: true, force: true });
await layerfs.branch.fork("work", "main");

const mode = async (ref, path) => (await layerfs.fs.stat(ref, path)).mode;
const commits = async () => (await layerfs.branch.list_commits("work")).items.length;

// Init keeps the source's modes; fs.stat reports them.
assert.equal(await mode("main", "/tool.sh"), 0o755);
assert.equal(await mode("main", "/private"), 0o750);
assert.equal(await mode("main", "/a.txt"), 0o644);

// fs.write: an options object, a plain message, and no options.
assert.equal((await layerfs.fs.write("work", "/new.sh", "#!/bin/sh\necho new\n", { mode: 0o755, message: "add new.sh" })).kind, "Committed");
assert.equal((await layerfs.fs.write("work", "/plain.txt", "p\n", "plain message")).kind, "Committed");
await layerfs.fs.write("work", "/tool.sh", "#!/bin/sh\necho changed\n");
const messages = (await layerfs.branch.list_commits("work")).items.map((c) => c.message);
assert.ok(messages.includes("add new.sh") && messages.includes("plain message"), JSON.stringify(messages));

// fs.mkdir with a mode; fs.apply with modes on write and mkdir, and chmod.
await layerfs.fs.mkdir("work", "/d/e", { mode: 0o750 });
const applied = await layerfs.fs.apply("work", [
  { op: "write", path: "/applied.sh", bytes: Buffer.from("#!/bin/sh\necho applied\n"), mode: 0o700 },
  { op: "mkdir", path: "/shared", mode: 0o1777 },
  { op: "chmod", path: "/a.txt", mode: 0o600 },
], "modes");
assert.equal(applied.kind, "Committed");

const want = {
  "/tool.sh": 0o755, // overwritten without a mode: kept
  "/new.sh": 0o755,
  "/plain.txt": 0o644,
  "/applied.sh": 0o700,
  "/a.txt": 0o600,
  "/private": 0o750,
  "/d": 0o755,
  "/d/e": 0o750,
  "/shared": 0o1777,
};
for (const [path, m] of Object.entries(want)) assert.equal(await mode("work", path), m, path);
assert.equal(await mode("main", "/a.txt"), 0o644, "main is untouched");
console.log("STAT ok");

// Refused, and nothing committed.
const before = await commits();
await assert.rejects(layerfs.fs.write("work", "/x", "x", { mode: 0o4755 }), (e) => e.code === "InvalidInput" && e.detail?.reason === "Mode" && e.detail?.mode === "4755" && e.detail?.path === "/x");
await assert.rejects(layerfs.fs.apply("work", [{ op: "chmod", path: "/a.txt", mode: 0o1644 }]), (e) => e.code === "InvalidInput" && e.detail?.reason === "Mode" && e.detail?.index === 0);
await assert.rejects(layerfs.fs.write("work", "/x", "x", { mode: "755" }), { code: "InvalidInput" });
await assert.rejects(layerfs.fs.mkdir("work", "/x", 755), { code: "InvalidInput" });
assert.equal(await commits(), before);
await assert.rejects(layerfs.fs.stat("work", "/x"), { code: "NoSuchPath" });
console.log("REFUSED ok");

// A Workspace of the Branch shows the same modes through FUSE.
const ws = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "work");
try {
  const paths = Object.keys(want).map((p) => p.slice(1));
  const seen = await layerfs.workspace.exec(ws, `stat -c '%a %n' ${paths.join(" ")} && ./new.sh && ./applied.sh`);
  assert.equal(seen.exitCode, 0, seen.stderr);
  const lines = paths.map((p) => `${want[`/${p}`].toString(8)} ${p}`);
  assert.equal(seen.stdout, `${lines.join("\n")}\nnew\napplied\n`);
  console.log("WORKSPACE ok");

  // The other way: a chmod in the Workspace, committed, reads back.
  assert.equal((await layerfs.workspace.exec(ws, "chmod 640 plain.txt")).exitCode, 0);
  await layerfs.workspace.commit(ws, "chmod in Workspace");
  assert.equal(await mode("work", "/plain.txt"), 0o640);
  console.log("ROUNDTRIP ok");
} finally {
  await layerfs.workspace.unmount(ws, true);
  await layerfs.close();
}
console.log("fs_mode_real: ok");

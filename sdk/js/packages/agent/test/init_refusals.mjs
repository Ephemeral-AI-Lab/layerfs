// project.init names the source entry it refuses (#410): InvalidInput with
// detail { reason, path, mode? }, no project entry and no installed Store
// left behind, and the same name initializes once the source is fixed. A
// source on overlayfs, the case #410 reported, initializes. A failure that
// is not a source refusal still keeps its entry. Real native binding,
// Disposable Store, no daemon; Linux as root, with LAYERFS_STORE_PATH on a
// local volume (the E2E launcher's):
//   node sdk/js/packages/agent/test/init_refusals.mjs
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, statfsSync, symlinkSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const OVERLAYFS = 0x794c7630;
const work = mkdtempSync(join(tmpdir(), "layerfs-init-refusals-"));
const home = join(work, "home");
process.env.LAYERFS_HOME = home;
const { LayerFS } = await import("@layerfs/agent");
// One Init through LayerFS.init, its client closed after: what Init returned.
const initOf = (name, options) => ({ project: { init: async (root) => { const shop = await LayerFS.init(name, root, options); await shop.close(); return shop.created; } } });
const volume = dirname(dirname(process.env.LAYERFS_STORE_PATH));
let made = 0;
// One client per Init, each with its own Store path on the local volume.
const client = (name) => {
  const folder = join(volume, `init-refusals-${process.pid}-${++made}`);
  mkdirSync(folder);
  const storePath = join(folder, "global", "store.sqlite");
  return { layerfs: initOf(name, { storePath }), storePath };
};
const source = (name, files = { "README.md": "hello\n" }) => {
  const root = join(work, name);
  for (const [path, body] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), body);
  }
  return root;
};

// A refused Init: a named InvalidInput, no entry, nothing installed; then the
// same name initializes from the fixed source.
async function refusedThenFixed(name, root, detail, fix) {
  const first = client(name);
  const error = await first.layerfs.project.init(root).then(
    (answer) => assert.fail(`${name}: expected a refusal, got ${JSON.stringify(answer)}`),
    (error) => error,
  );
  assert.equal(error.code, "InvalidInput", `${name}: ${error.message}`);
  assert.deepEqual(error.detail, detail, name);
  assert.match(error.message, /^InvalidInput: project\.init refused /, name);
  assert.ok(error.message.includes(detail.path), `${name}: message names the path`);
  assert.ok(!existsSync(join(home, name)), `${name}: no project entry is left`);
  assert.ok(!existsSync(first.storePath), `${name}: nothing was installed`);
  fix();
  const again = await client(name).layerfs.project.init(root);
  assert.equal(again.kind, "Initialized", name);
  console.log(`${name}: refused ${detail.reason}, then initialized`);
}

try {
  // #410: a one-file source on overlayfs initializes.
  const overlay = source("overlay");
  const type = statfsSync(overlay).type;
  assert.equal(type, OVERLAYFS, `source filesystem 0x${type.toString(16)} is not overlayfs`);
  const imported = await client("overlay").layerfs.project.init(overlay);
  assert.equal(imported.kind, "Initialized");
  console.log("overlay: a source on overlayfs initialized");

  // A set-group-ID source root, as an inherited g+s directory would be.
  const sgid = source("sgid");
  chmodSync(sgid, 0o2755);
  await refusedThenFixed("sgid", sgid, { reason: "Mode", path: sgid, mode: "2755" },
    () => chmodSync(sgid, 0o755));

  // A set-user-ID file deep in the tree, after earlier rows were written.
  const setuid = source("setuid", { "a/b/tool": "#!/bin/sh\n", "a/z.txt": "z\n", "y.txt": "y\n" });
  chmodSync(join(setuid, "a/b/tool"), 0o4755);
  await refusedThenFixed("setuid", setuid,
    { reason: "Mode", path: join(setuid, "a/b/tool"), mode: "4755" },
    () => chmodSync(join(setuid, "a/b/tool"), 0o755));

  // The sticky bit: refused on a file, kept on a directory.
  const sticky = source("sticky", { "shared/note": "n\n" });
  chmodSync(join(sticky, "shared"), 0o1777);
  chmodSync(join(sticky, "shared/note"), 0o1644);
  await refusedThenFixed("sticky", sticky,
    { reason: "Mode", path: join(sticky, "shared/note"), mode: "1644" },
    () => chmodSync(join(sticky, "shared/note"), 0o644));

  // A symlinked source.
  const target = source("target");
  const link = join(work, "link");
  symlinkSync(target, link);
  await refusedThenFixed("link", link, { reason: "NotDirectory", path: link },
    () => { rmSync(link); mkdirSync(link); writeFileSync(join(link, "README.md"), "hello\n"); });

  // Not a source refusal: a FIFO is Unsupported, and its entry is kept.
  const fifo = source("fifo");
  execFileSync("mkfifo", [join(fifo, "pipe")]);
  const kept = await client("fifo").layerfs.project.init(fifo).then(
    (answer) => assert.fail(`fifo: expected a failure, got ${JSON.stringify(answer)}`),
    (error) => error,
  );
  assert.equal(kept.code, "Failed", kept.message);
  assert.match(kept.message, /Unsupported/);
  assert.ok(existsSync(join(home, "fifo")), "fifo: a non-refusal failure keeps its entry");
  await assert.rejects(client("fifo").layerfs.project.init(fifo), { code: "Exists" });
  console.log("fifo: Unsupported keeps its entry (Exists on retry)");
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log("init_refusals: ok");

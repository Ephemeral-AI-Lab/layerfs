// A project is its home entry: Init refuses a name whose entry exists, and
// once that entry is removed the same process initializes the name again.
// A refused source is named (#410) and leaves no entry behind.
// Each Init installs its Store at its client's storePath (#373), so each
// uses its own, as each Import uses a new volume. Real native binding,
// Disposable Store, no Docker or daemon; Linux, with LAYERFS_STORE_PATH on a
// local volume (the E2E launcher's):
//   node api/agent/test/reinit.mjs
import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const work = mkdtempSync(join(tmpdir(), "layerfs-reinit-"));
try {
  const source = join(work, "website");
  mkdirSync(source);
  writeFileSync(join(source, "README.md"), "Hello\n");
  process.env.LAYERFS_HOME = join(work, "home");
  const { LayerFS } = await import("@layerfs/agent");
  // One Init through LayerFS.init with its own Store path (LAYERFS_STORE_PATH,
  // read when the client is made), its client closed after: what Init returned.
  const initOf = (name, storePath) => ({ project: { init: async (root, timeout) => {
    const saved = process.env.LAYERFS_STORE_PATH;
    process.env.LAYERFS_STORE_PATH = storePath;
    const making = LayerFS.init(name, root, timeout);
    process.env.LAYERFS_STORE_PATH = saved;
    const shop = await making;
    await shop.close();
    return shop.created;
  } } });
  // a Store path per Init, on the volume of LAYERFS_STORE_PATH
  const volume = dirname(dirname(process.env.LAYERFS_STORE_PATH));
  let made = 0;
  const client = () => {
    const folder = join(volume, `reinit-${process.pid}-${++made}`);
    mkdirSync(folder);
    return initOf("website", join(folder, "global", "store.sqlite"));
  };
  const layerfs = client();

  // A set-group-ID source directory has a mode LayerFS cannot store. The
  // bit does not exist on Windows, whose chmod maps only the read-only
  // attribute; the refusal contract is checked on unix platforms.
  if (process.platform !== "win32") {
    chmodSync(source, 0o2755);
    await assert.rejects(layerfs.project.init(source), {
      code: "InvalidInput",
      detail: { reason: "Mode", path: source, mode: "2755" },
    });
    assert.ok(!existsSync(join(work, "home", "website")), "a refused Init leaves no entry");
    chmodSync(source, 0o755);
  }

  const first = await layerfs.project.init(source);
  assert.equal(first.kind, "Initialized");
  await assert.rejects(layerfs.project.init(source), { code: "Exists", message: "Exists: project website already initialized" });

  rmSync(join(work, "home", "website"), { recursive: true });
  const again = await client().project.init(source);
  assert.equal(again.kind, "Initialized");
  assert.ok(existsSync(again.store));
  await assert.rejects(client().project.init(source), { code: "Exists" });
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log("reinit: ok");

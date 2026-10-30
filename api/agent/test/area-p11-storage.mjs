// P11 Store statistics over one stand-in binding and its clients.
//   node api/agent/test/area-p11-storage.mjs
// No Docker, Store or daemon is used.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { ENV } from "./fixture/cases.mjs";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
delete process.env.CLI_FAKE_LOG;
delete process.env.LAYERFS_TRACE;
const native = createRequire(import.meta.url)(FAKE);
const { LayerFS, LayerFSError } = await import("../src/index.js");
const { LayerFSBackend } = await import("@layerfs/backend");

const shop = await LayerFS.open("site");
const backend = LayerFSBackend.of(shop);

const scenarios = [];
const scenario = (name, run) => scenarios.push({ name, run });

scenario("1 default get_storage returns disk statistics and no commits", async () => {
  native.storageResult = {
    logical: 1000,
    canonical: 800,
    stored: 600,
    metadata: 200,
    file: 16384,
    free: 4096,
    dedupe: "1.250000",
    compression: "1.333333",
    init: null,
    disk: { main: 12288, wal: 4096, total: 16384 },
  };
  const stats = await backend.store.project.get_storage();
  assert.equal(stats.dedupe, 1.25);
  assert.equal(stats.compression, 1.333333);
  assert.deepEqual(stats.disk, { main: 12288, wal: 4096, total: 16384 });
  assert.equal("commits" in stats, false, "commits absent on default call");

  const sent = native.args[native.calls.lastIndexOf("backend.store.project.get_storage")];
  assert.equal("commits" in sent, false, "sent args do not have commits");
});

scenario("2 get_storage with { commits: false } also omits commits", async () => {
  native.storageResult = {
    logical: 1000,
    canonical: 800,
    stored: 600,
    metadata: 200,
    file: 16384,
    free: 4096,
    dedupe: null,
    compression: null,
    init: null,
    disk: { main: 8192, wal: 0, total: 8192 },
  };
  const stats = await backend.store.project.get_storage({ commits: false });
  assert.equal(stats.dedupe, null);
  assert.equal(stats.compression, null);
  assert.deepEqual(stats.disk, { main: 8192, wal: 0, total: 8192 });
  assert.equal("commits" in stats, false);

  const sent = native.args[native.calls.lastIndexOf("backend.store.project.get_storage")];
  assert.equal(sent.commits, false);
});

scenario("3 get_storage with { commits: true } returns all three kinds", async () => {
  native.storageResult = {
    logical: 5000,
    canonical: 4000,
    stored: 3000,
    metadata: 1000,
    file: 32768,
    free: 0,
    dedupe: "1.250000",
    compression: "1.333333",
    init: null,
    disk: { main: 24576, wal: 8192, total: 32768 },
    commits: {
      Workspace: { count: 3, newCanonical: 1200, newStored: 900, newMetadata: 300 },
      Files: { count: 1, newCanonical: 400, newStored: 300, newMetadata: 100 },
      Merge: { count: 0, newCanonical: 0, newStored: 0, newMetadata: 0 },
    },
  };
  const stats = await backend.store.project.get_storage({ commits: true });
  assert.deepEqual(stats.disk, { main: 24576, wal: 8192, total: 32768 });
  assert.ok("commits" in stats);
  assert.deepEqual(stats.commits.Workspace, { count: 3, newCanonical: 1200, newStored: 900, newMetadata: 300 });
  assert.deepEqual(stats.commits.Files, { count: 1, newCanonical: 400, newStored: 300, newMetadata: 100 });
  assert.deepEqual(stats.commits.Merge, { count: 0, newCanonical: 0, newStored: 0, newMetadata: 0 });

  const sent = native.args[native.calls.lastIndexOf("backend.store.project.get_storage")];
  assert.equal(sent.commits, true);
});

scenario("4 options validation in JS wrapper rejects invalid arguments", async () => {
  const badOptions = [
    null,
    "bad",
    123,
    true,
    [1, 2],
    { unknown: true },
    { extra: 1, commits: true },
    { commits: "yes" },
    { commits: 1 },
    { commits: null },
    { commits: {} },
  ];
  for (const bad of badOptions) {
    await assert.rejects(
      backend.store.project.get_storage(bad),
      (err) => {
        assert.ok(err instanceof LayerFSError, `expected LayerFSError for ${JSON.stringify(bad)}`);
        assert.equal(err.code, "InvalidInput");
        return true;
      },
      `rejected invalid option: ${JSON.stringify(bad)}`,
    );
  }
});

scenario("5 options: empty object is accepted", async () => {
  native.storageResult = {
    logical: 100,
    canonical: 80,
    stored: 60,
    metadata: 20,
    file: 4096,
    free: 0,
    dedupe: null,
    compression: null,
    init: null,
    disk: { main: 4096, wal: 0, total: 4096 },
  };
  const stats = await backend.store.project.get_storage({});
  assert.deepEqual(stats.disk, { main: 4096, wal: 0, total: 4096 });
});

let passed = 0;
for (const { name, run } of scenarios) {
  try {
    await run();
    passed++;
    console.log(`  ok ${name}`);
  } catch (error) {
    console.error(`  FAIL ${name}`);
    console.error(error);
    process.exitCode = 1;
    break;
  }
}

await shop.close();
if (process.exitCode) {
  console.error(`FAILED: ${passed}/${scenarios.length} passed`);
} else {
  console.log(`PASS area-p11-storage: ${passed}/${scenarios.length} scenarios ok`);
}

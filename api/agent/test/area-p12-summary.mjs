// P12 Store summary over one stand-in binding and its client.
//   node api/agent/test/area-p12-summary.mjs
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

scenario("1 get_summary returns the exact five-field shape", async () => {
  native.summaryResult = {
    logical: 123,
    branches: 2,
    commits: 5,
    mainLayer: "lay1",
    lastCommit: { id: "c0ffee", createdMs: 1761800000000 },
  };
  const value = await backend.store.project.get_summary();
  assert.deepEqual(value, native.summaryResult);
  assert.deepEqual(Object.keys(value).sort(), ["branches", "commits", "lastCommit", "logical", "mainLayer"]);
  assert.equal(native.calls.at(-1), "backend.store.project.get_summary");
  const sent = native.args.at(-1);
  assert.equal("commits" in sent, false);
  assert.equal("unknown" in sent, false);
});

scenario("2 get_summary preserves null main head values", async () => {
  native.summaryResult = { logical: 0, branches: 0, commits: 0, mainLayer: null, lastCommit: null };
  assert.deepEqual(await backend.store.project.get_summary(), native.summaryResult);
});

scenario("3 get_summary refuses every supplied options argument", async () => {
  const badOptions = [null, "bad", 123, true, [1, 2], {}, { unknown: true }];
  for (const bad of badOptions) {
    const before = native.calls.length;
    await assert.rejects(
      backend.store.project.get_summary(bad),
      (error) => {
        assert.ok(error instanceof LayerFSError);
        assert.equal(error.code, "InvalidInput");
        return true;
      },
      `rejected invalid argument: ${JSON.stringify(bad)}`,
    );
    assert.equal(native.calls.length, before, "invalid arguments never reach the binding");
  }
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
  console.log(`PASS area-p12-summary: ${passed}/${scenarios.length} scenarios ok`);
}

// P1 SDK/CLI agreement over the stand-in binding. Real-Store coverage:
// layerfs-api/tests/area_p1_history.rs. P14's budget traversal joins this
// runner when the P14 slice merges.
//   /usr/bin/node api/agent/test/area-p.mjs
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { ENV, contains, plain } from "./fixture/cases.mjs";
import { CASES } from "./fixture/cases-area-p.mjs";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
delete process.env.CLI_FAKE_LOG;
const native = createRequire(import.meta.url)(FAKE);
const { LayerFS } = await import("../src/index.js");
const lf = await LayerFS.open("site");
const failures = [];

for (const c of CASES) {
  native.branchList = null;
  native.branchReplyBudget = null;
  native.workspaceList = [];
  c.prepare?.(native);
  const before = native.calls.length;
  let result;
  let error;
  try {
    result = await c.sdk(lf);
  } catch (thrown) {
    error = thrown;
  }
  try {
    assert.equal(error, undefined, `threw ${error?.message}`);
    assert.deepEqual(native.calls.slice(before), [c.op], "binding calls");
    assert.ok(contains(plain(native.args[before]), c.sent), `sent ${JSON.stringify(plain(native.args[before]))}`);
    assert.deepEqual(plain(result), c.result);
  } catch (failure) {
    failures.push(`${c.name}: ${failure.message}`);
  }
}

await lf.close();
if (failures.length) {
  console.error(`FAIL area-p: ${failures.length} of ${CASES.length}\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`PASS area-p (${CASES.length} cases)`);

// P1 + P14 SDK/CLI agreement over the stand-in binding. Real-Store coverage:
// layerfs-api/tests/area_p1_history.rs and area_p14_paging.rs.
//   /usr/bin/node api/agent/test/area-p.mjs
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { ENV, contains, plain } from "./fixture/cases.mjs";
import { BUDGET_BRANCH_NAMES, CASES } from "./fixture/cases-area-p.mjs";

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

native.branchList = BUDGET_BRANCH_NAMES;
native.branchReplyBudget = 48 * 1024;
const before = native.calls.length;
const found = [];
let first;
let pagePos = 0;
let pages = 0;
let terminated = false;
while (pages <= BUDGET_BRANCH_NAMES.length && !terminated) {
  const page = await lf.project.list_branches(1000, pagePos);
  first ??= page;
  pages += 1;
  found.push(...page.items.map((item) => item.name));
  if (page.nextPagePos === null) {
    terminated = true;
  } else {
    pagePos = page.nextPagePos;
  }
}
try {
  assert.ok(first.items.length < 1000, "the reply budget makes a short page");
  assert.equal(first.hasMore, true);
  assert.equal(first.nextPagePos, first.items.length);
  assert.equal(terminated, true);
  assert.deepEqual(found, BUDGET_BRANCH_NAMES);
  assert.equal(native.calls.length - before, pages, "one binding call per page");
  assert.equal(pages, 2);
} catch (failure) {
  failures.push(`P14 list_branches budget traversal: ${failure.message}`);
}

await lf.close();
if (failures.length) {
  console.error(`FAIL area-p: ${failures.length} of ${CASES.length + 1}\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`PASS area-p (${CASES.length} cases + 1 budget traversal)`);

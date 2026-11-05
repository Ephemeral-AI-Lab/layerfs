// Area P SDK/CLI agreement over the stand-in binding. Real-Store coverage:
// layerfs-api/tests/area_p*.rs.
//   /usr/bin/node api/agent/test/area-p.mjs
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { ENV, contains, plain } from "./fixture/cases.mjs";
import { BUDGET_BRANCH_NAMES, CASES, P8_REFS } from "./fixture/cases-area-p.mjs";

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
  process.env.CLI_FAKE_REFUSE = (c.refuse ?? []).join(",");
  try {
    result = await c.sdk(lf);
  } catch (thrown) {
    error = thrown;
  } finally {
    delete process.env.CLI_FAKE_REFUSE;
  }
  try {
    assert.deepEqual(native.calls.slice(before), c.op ? [c.op] : [], "binding calls; no Workspace operation");
    if (c.op) assert.ok(contains(plain(native.args[before]), c.sent), `sent ${JSON.stringify(plain(native.args[before]))}`);
    if (c.refused) {
      assert.ok(error, `no refusal; returned ${JSON.stringify(plain(result))}`);
      assert.equal(error.name, "LayerFSError");
      assert.deepEqual({ code: error.code, message: error.message, detail: plain(error.detail) }, c.refused);
    } else {
      assert.equal(error, undefined, `threw ${error?.message}`);
      assert.deepEqual(plain(result), c.result);
    }
  } catch (failure) {
    failures.push(`${c.name}: ${failure.message}`);
  }
}

try {
  const ws = { sandbox: "1", path: "/workspaces/w1" };
  const before = native.calls.length;
  const totalOverflow = Array(64).fill("");
  totalOverflow[0] = "x".repeat(3904);
  const invalid = [
    null, { unknown: true }, { author: null }, { author: 7 }, { labels: null }, { labels: "call-7" }, { labels: [7] }, { labels: [undefined] }, { labels: Array(1) },
    { author: "a".repeat(4097) }, { labels: ["bad\u0007"] }, { labels: Array(65).fill("x") }, { labels: totalOverflow },
  ];
  for (const options of invalid) {
    await assert.rejects(lf.workspace.commit(ws, "invalid", false, options), { code: "InvalidInput" });
  }
  assert.equal(native.calls.length, before, "invalid P7 options send nothing");
} catch (failure) {
  failures.push(`P7 option validation: ${failure.message}`);
}

try {
  const ws = { sandbox: "1", path: "/workspaces/w1" };
  const labels = Array(64).fill("");
  labels[0] = "x".repeat(3903);
  assert.equal(Buffer.byteLength(JSON.stringify(labels)), 4096);
  const result = await lf.workspace.commit(ws, "boundary", false, { author: "a".repeat(4096), labels });
  assert.equal(result.commit.author.length, 4096);
  assert.deepEqual(result.commit.labels, labels);
} catch (failure) {
  failures.push(`P7 option boundaries: ${failure.message}`);
}

try {
  const before = native.calls.length;
  for (const options of [{ unknown: true }, { binary: "yes" }, null, [], "yes"]) {
    await assert.rejects(lf.fs.read(P8_REFS.commit, "/file.txt", 1, 1, options), { code: "InvalidInput" });
  }
  assert.equal(native.calls.length, before, "invalid P8 options send nothing");
} catch (failure) {
  failures.push(`P8 option validation: ${failure.message}`);
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
  console.error(`FAIL area-p: ${failures.length} of ${CASES.length + 4}\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`PASS area-p (${CASES.length} cases + 2 P7 validation groups + 1 P8 validation group + 1 budget traversal)`);

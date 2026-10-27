// SDK correctness over the stand-in binding: every case in
// fixture/cases.mjs (shared with the `layerfs` CLI tests) makes its one
// binding call with the expected arguments and returns the expected result,
// or throws the expected LayerFSError. No Docker, Store or daemon:
//   node api/agent/test/calls.mjs
import assert from "node:assert/strict";
import { readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { CASES, ENV, contains, made, plain } from "./fixture/cases.mjs";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
delete process.env.CLI_FAKE_LOG;
const native = createRequire(import.meta.url)(FAKE);
const { LayerFS } = await import("../src/index.js");

// every SDK call file has a case
const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const files = ["project", "branch", "fs", "workspace", "sandbox"].flatMap((family) =>
  readdirSync(`${SRC}${family}`).filter((f) => !["index.js", "body.js"].includes(f)).map((f) => `${family}.${f.slice(0, -3)}`));
assert.deepEqual([...new Set(CASES.map((c) => c.op).filter(Boolean))].sort(), files.sort(), "a case for every SDK call");

const lf = await LayerFS.open("site");
const failures = [];
for (const c of CASES) {
  const before = native.args.length;
  process.env.CLI_FAKE_REFUSE = (c.refuse ?? []).join(",");
  let result;
  let error;
  try {
    result = await c.sdk(lf, LayerFS);
  } catch (thrown) {
    error = thrown;
  } finally {
    delete process.env.CLI_FAKE_REFUSE;
  }
  try {
    const calls = made(native.calls.slice(before).map((op, i) => [op, plain(native.args[before + i])]));
    assert.deepEqual(calls.map(([op]) => op), c.op ? [c.op] : [], "binding calls");
    if (c.op) assert.ok(contains(calls[0][1], c.sent), `sent ${JSON.stringify(calls[0][1])}`);
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
await lf.close();
if (failures.length) {
  console.error(`FAIL calls: ${failures.length} of ${CASES.length}\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`PASS calls (${CASES.length} cases)`);

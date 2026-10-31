// P10 per-call results over one stand-in binding and its clients.
//   node api/agent/test/area-p10-results.mjs
// No Docker, Store or daemon is used.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
const { traced } = await import("../src/trace.js");
const lf = await LayerFS.open("site");
const WS = { sandbox: "1", path: "/workspaces/w1" };
const scenarios = [];
const scenario = (name, run) => scenarios.push({ name, run });
const pick = (record, fields) => Object.fromEntries(fields.filter((field) => field in record).map((field) => [field, record[field]]));

scenario("1 one record per call, gapless completion order, no binding call to read", async () => {
  const opened = lf.results();
  assert.equal(opened.items.length, 1, "LayerFS.open's project lookup is the first record");
  assert.equal(opened.items[0].method, "project.list_branches");
  assert.equal(opened.items[0].seq, 1);
  assert.equal(opened.items[0].ok, true);
  assert.equal(opened.next, 1);
  assert.equal(opened.evicted, 0);
  assert.equal("stale" in opened, false);
  const before = native.calls.length;
  const read = lf.results();
  assert.equal(native.calls.length, before, "results() makes no binding call");
  assert.deepEqual(read.items, opened.items, "reading is stable");
  await lf.project.list_branches();
  await lf.fs.list("main", "/");
  const all = lf.results();
  assert.deepEqual(all.items.map((record) => record.seq), [1, 2, 3]);
  assert.deepEqual(all.items.map((record) => record.method), ["project.list_branches", "project.list_branches", "fs.list"]);
  for (const record of all.items) {
    assert.ok(Number.isFinite(record.ms) && record.ms >= 0, "a client-side duration");
    assert.ok(!Number.isNaN(Date.parse(record.start)), "an ISO start");
    assert.equal("code" in record, false, "ok records carry no refusal");
    assert.equal("message" in record, false);
    assert.equal("args" in record, false, "no call arguments in a record");
    assert.equal("daemon" in record, false, "no daemon telemetry in a record");
  }
});

scenario("2 write outcomes: Commit, bytes added, kind; reads carry none", async () => {
  await lf.fs.write("feature", "/big.txt", "hello", "Write big");
  await lf.fs.rm("feature", "/dir");
  await lf.workspace.commit(WS, "Save");
  await lf.branch.merge("main", "feature", "merged");
  await lf.branch.publish("feature");
  await lf.branch.fork("p10-fork");
  await lf.fs.read("main", "/README.md", 1, 20);
  const records = lf.results(3).items;
  const byMethod = Object.fromEntries(records.map((record) => [record.method, record]));
  assert.equal(records.length, 7);
  assert.deepEqual(pick(byMethod["fs.write"], ["ok", "kind", "commit", "sizes"]), {
    ok: true, kind: "Committed", commit: "c0ffee",
    sizes: { logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 },
  });
  assert.deepEqual(pick(byMethod["fs.rm"], ["ok", "kind", "commit", "sizes"]), { ok: true, kind: "UpToDate", commit: "h3ad" });
  assert.deepEqual(pick(byMethod["workspace.commit"], ["ok", "kind", "commit", "sizes"]), {
    ok: true, kind: "Committed", commit: "c0ffee",
    sizes: { logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 },
  });
  assert.deepEqual(pick(byMethod["branch.merge"], ["ok", "kind", "commit", "sizes"]), { ok: true, kind: "Merged", commit: "m3rge" });
  assert.deepEqual(pick(byMethod["branch.publish"], ["ok", "kind", "commit", "layer"]), { ok: true, kind: "Added", commit: "pub1", layer: "lay1" });
  assert.equal(byMethod["branch.fork"].kind, "Forked");
  assert.equal("commit" in byMethod["branch.fork"], false, "a fork publishes nothing");
  assert.equal("commit" in byMethod["fs.read"], false, "a read publishes nothing");
  assert.equal("sizes" in byMethod["fs.read"], false);
});

scenario("3 refused answers; refusals before the call record nothing", async () => {
  const cursor = lf.results().next;
  await assert.rejects(lf.fs.read("main", "/missing"), { code: "NoSuchPath" });
  process.env.CLI_FAKE_REFUSE = "fs.list";
  await assert.rejects(lf.fs.list("main", "/"), { code: "Conflict" });
  delete process.env.CLI_FAKE_REFUSE;
  const records = lf.results(cursor).items;
  assert.equal(records.length, 2);
  assert.deepEqual(pick(records[0], ["method", "ok", "code"]), { method: "fs.read", ok: false, code: "NoSuchPath" });
  assert.match(records[0].message, /does not exist/);
  assert.deepEqual(pick(records[1], ["method", "ok", "code", "message"]), { method: "fs.list", ok: false, code: "Conflict", message: "fs.list refused by the fake" });
  for (const record of records) assert.equal("commit" in record, false, "a refused write names no Commit");
  const before = lf.results().next;
  await assert.rejects(lf.call("backend.store.project.get_history", {}), { code: "UnknownMethod" });
  await assert.rejects(lf.project.list_branches(0), { code: "InvalidInput" });
  assert.equal(lf.results().next, before, "a refusal before the call records nothing");
});

scenario("4 a thrown refusal records and rethrows (traced boundary)", async () => {
  const records = [];
  const sink = (record) => records.push(record);
  const error = new LayerFSError("Busy", "busy test", null);
  await assert.rejects(traced({}, "fs.read", {}, async () => { throw error; }, sink), (thrown) => thrown === error);
  assert.equal(records.length, 1);
  assert.deepEqual(pick(records[0], ["method", "ok", "code", "message"]), { method: "fs.read", ok: false, code: "Busy", message: "Busy: busy test" });
  assert.ok(Number.isFinite(records[0].ms));
  const ok = await traced({}, "project.list_branches", {}, async () => ({ value: { items: [] } }), sink);
  assert.deepEqual(ok, { value: { items: [] } });
  assert.deepEqual(pick(records[1], ["method", "ok"]), { method: "project.list_branches", ok: true });
  const plain = await traced({}, "fs.list", {}, async () => ({ value: { items: [] } }));
  assert.deepEqual(plain, { value: { items: [] } }, "no sink, no file: the call is untouched");
});

scenario("5 backend calls land in the same client's records", async () => {
  const cursor = lf.results().next;
  await LayerFSBackend.of(lf).store.project.get_storage();
  const records = lf.results(cursor).items;
  assert.equal(records.length, 1);
  assert.equal(records[0].method, "backend.store.project.get_storage");
  assert.equal(records[0].ok, true);
});

scenario("6 paging: after, next, empty pages, null and undefined", async () => {
  const all = lf.results();
  const last = all.next;
  const paged = lf.results(last - 2);
  assert.deepEqual(paged.items.map((record) => record.seq), [last - 1, last]);
  assert.equal(paged.next, last);
  const empty = lf.results(last);
  assert.deepEqual(empty.items, []);
  assert.equal(empty.next, last, "an empty page repeats the cursor");
  assert.deepEqual(lf.results(null), all, "null reads from the start");
  assert.deepEqual(lf.results(undefined), all, "undefined reads from the start");
});

scenario("7 invalid after values refuse InvalidInput", async () => {
  for (const after of [-1, 1.5, NaN, "1", {}, [], 2 ** 53, Infinity]) {
    assert.throws(() => lf.results(after), { name: "LayerFSError", code: "InvalidInput", message: "InvalidInput: after must be a nonnegative safe integer" }, JSON.stringify(after));
  }
});

scenario("8 parallel calls: one record each, unique consecutive seqs", async () => {
  const cursor = lf.results().next;
  await Promise.all([
    lf.fs.write("feature", "/a.txt", "a", "one"),
    lf.fs.read("main", "/README.md", 1, 20),
    lf.project.list_branches(),
  ]);
  const records = lf.results(cursor).items;
  assert.equal(records.length, 3);
  assert.deepEqual(records.map((record) => record.seq), [cursor + 1, cursor + 2, cursor + 3], "completion order");
  assert.deepEqual(records.map((record) => record.method).sort(), ["fs.read", "fs.write", "project.list_branches"]);
});

scenario("9 init records project.init and the open lookup", async () => {
  const made = await LayerFS.init("p10init", "/src/site");
  const records = made.results().items;
  assert.deepEqual(records.map((record) => record.method), ["project.init", "project.list_branches"]);
  assert.deepEqual(records.map((record) => record.seq), [1, 2]);
  assert.equal(records[0].kind, "Initialized");
  assert.equal(records[0].ok, true);
  await made.close();
});

scenario("10 the 1024-record ring drops oldest and reports staleness", async () => {
  const heavy = await LayerFS.open("site");
  for (let i = 0; i < 1030; i++) await heavy.project.list_branches();
  const total = 1031;
  const all = heavy.results();
  assert.equal(all.items.length, 1024, "the ring keeps its most recent 1024");
  assert.equal(all.evicted, total - 1024);
  assert.equal(all.stale, true, "records after 0 were dropped");
  assert.equal(all.items[0].seq, total - 1024 + 1, "oldest kept");
  assert.equal(all.next, total);
  assert.deepEqual(all.items.map((record) => record.seq).every((seq, index) => seq === total - 1024 + 1 + index), true, "no reused sequences");
  const edge = heavy.results(total - 1024);
  assert.equal("stale" in edge, false, "the cursor just before the oldest kept record is not stale");
  assert.equal(edge.items[0].seq, total - 1024 + 1);
  const dropped = heavy.results(total - 1024 - 2);
  assert.equal(dropped.stale, true, "a cursor into dropped records is stale");
  assert.equal(dropped.items.length, 1024, "kept records are still returned");
  await heavy.close();
});

scenario("11 trace agreement and unchanged call results", async () => {
  const dir = mkdtempSync(join(tmpdir(), "layerfs-p10-"));
  try {
    const file = join(dir, "trace.jsonl");
    const tracedClient = await LayerFS.open("site", { trace: file });
    await tracedClient.fs.write("feature", "/big.txt", "hello", "Write big");
    await assert.rejects(tracedClient.fs.read("main", "/missing"), { code: "NoSuchPath" });
    await tracedClient.branch.publish("feature");
    const lines = readFileSync(file, "utf8").trimEnd().split("\n").map((line) => JSON.parse(line));
    const records = tracedClient.results().items;
    assert.equal(records.length, lines.length, "one record per trace line");
    const FIELDS = ["method", "start", "ms", "ok", "code", "message", "kind", "commit", "layer", "sizes"];
    for (let i = 0; i < records.length; i++) {
      assert.deepEqual(pick(records[i], FIELDS), pick(lines[i], FIELDS), `${records[i].method}: record agrees with its trace line`);
      assert.ok(native.observed.includes(lines[i].scope), "traced calls still carry their scope");
    }
    await tracedClient.close();
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

const failures = [];
for (const { name, run } of scenarios) {
  try {
    await run();
  } catch (error) {
    failures.push(`${name}: ${error.stack ?? error}`);
    break;
  }
}
try {
  const before = lf.results().next;
  await lf.close();
  const after = lf.results();
  assert.equal(after.next, before, "close() records no call of its own");
  assert.equal(after.items.length, Math.min(before, 1024), "records stay readable after close()");
} catch (error) {
  failures.push(`cleanup: ${error.stack ?? error}`);
}
if (failures.length) {
  console.error(`FAIL area-p10-results (${failures.length})\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`PASS area-p10-results (${scenarios.length} scenarios)`);

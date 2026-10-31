// P9 opened-project events over one stateful stand-in binding and one client.
//   /usr/bin/node api/agent/test/area-p9-events.mjs
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { ENV, contains, plain } from "./fixture/cases.mjs";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
delete process.env.CLI_FAKE_LOG;
const native = createRequire(import.meta.url)(FAKE);
const { LayerFS } = await import("../src/index.js");
const lf = await LayerFS.open("site");
const WS = { sandbox: "1", path: "/workspaces/w1" };
const COMMON = { project: "site", home: ENV.LAYERFS_HOME, storePath: ENV.LAYERFS_STORE_PATH };
const scenarios = [];
const scenario = (name, run) => scenarios.push({ name, run });
const currentId = () => [...native.eventStates.keys()].at(-1);
const callsFrom = (start) => native.calls.slice(start);
const refuse = async (op, args) => {
  const answer = await native.call(op, args);
  assert.ok(answer.refused, `${op} unexpectedly returned ${JSON.stringify(plain(answer))}`);
  return answer.refused;
};
const committed = (commit, branch = "main") => ({ kind: "Committed", branch, head: commit.id, commit });
const FULL_COMMIT = {
  id: `12${"a".repeat(64)}`, branch: "main", parents: [], base: `32${"b".repeat(64)}`, root: `42${"c".repeat(64)}`,
  via: "Files", message: "attributed", author: "P9 author", labels: ["p9", "event"], time: 123456789,
  logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8, secondParent: null, mergeBase: null, lww: [], lwwTruncated: false,
};
const resetResults = () => {
  native.fsResults = {};
  native.mergeResults = {};
  native.publishResults = {};
  native.workspaceCommitResult = undefined;
  delete native.eventPayloadText;
};
const produceCommits = async (count, message = "event") => {
  const commit = { ...FULL_COMMIT, message };
  native.workspaceCommitResult = committed(commit);
  for (let i = 0; i < count; i++) await lf.workspace.commit(WS, "event");
  return commit;
};
const checkCall = (index, op, args, id, common = true) => {
  assert.equal(native.calls[index], op);
  const raw = native.args[index];
  const behavior = native.behaviorArgs[index];
  assert.equal(Object.hasOwn(behavior, "subscription"), false, "the private routing field is stripped");
  assert.deepEqual(plain(raw), plain({ ...behavior, ...(id ? { subscription: id } : {}) }));
  if (common) assert.ok(contains(behavior, COMMON), JSON.stringify(plain(behavior)));
  const extra = Object.fromEntries(Object.entries(behavior).filter(([key]) => !["project", "home", "storePath", "deploy"].includes(key)));
  assert.deepEqual(plain(extra), plain(args));
};

scenario("1 subscribe shape, complete projections, initial empty poll", async () => {
  native.branchList = ["main", "cart"];
  native.sandboxList = [{ sandbox: "1" }, { sandbox: "2" }];
  native.workspaceLists = {
    1: [{ mounted: true, directory: "/workspaces/w1", branch: "main" }, { mounted: false, directory: "/workspaces/old", branch: "old" }],
    2: [{ mounted: true, directory: "/workspaces/w2", branch: null }],
  };
  const start = native.calls.length;
  const events = await lf.events.subscribe();
  assert.deepEqual(Object.keys(events).sort(), ["close", "cursor", "poll", "snapshot"]);
  assert.deepEqual(Object.getOwnPropertySymbols(events), [Symbol.asyncIterator]);
  assert.equal(events.cursor, 0);
  assert.deepEqual(events.snapshot, {
    branches: [{ name: "main", head: null }, { name: "cart", head: "head1234567890ab" }],
    workspaces: [{ sandbox: "1", path: "/workspaces/w1", branch: "main" }, { sandbox: "2", path: "/workspaces/w2", branch: null }],
    sandboxes: [{ sandbox: "1" }, { sandbox: "2" }],
  });
  assert.match(currentId(), /^[0-9a-f]{64}$/);
  const page = await events.poll();
  assert.deepEqual(page, { events: [], cursor: 0, hasMore: false, stale: false, closed: false });
  assert.deepEqual(callsFrom(start), ["events.subscribe", "events.poll"]);
  checkCall(start, "events.subscribe", {}, null);
  checkCall(start + 1, "events.poll", { cursor: 0, timeoutMs: 0 }, currentId());
  lf.events.test = events;
});

scenario("2 all lifecycle/fork/commit mappings, exact envelopes and attribution", async () => {
  const events = lf.events.test;
  const startCursor = events.cursor;
  const fsCommit = { ...FULL_COMMIT, id: `12${"d".repeat(64)}`, message: "file" };
  native.workspaceCommitResult = committed(FULL_COMMIT);
  native.fsResults = { "fs.write": committed(fsCommit) };
  const start = native.calls.length;
  await lf.workspace.commit(WS, "attributed", false, { author: "P9 author", labels: ["p9", "event"] });
  await lf.fs.write("main", "/one.txt", "one", "file");
  await lf.branch.fork("from-commit");
  await lf.branch.fork("from-layer", "layer");
  const ws = await lf.workspace.mount("1", "main");
  await lf.workspace.unmount(ws, false, true);
  const sandbox = await lf.sandbox.create();
  await lf.sandbox.close(sandbox.sandbox);
  const page = await events.poll();
  assert.equal(page.events.length, 8);
  assert.deepEqual(page.events.map((event) => event.seq), Array.from({ length: 8 }, (_, i) => startCursor + i + 1));
  assert.deepEqual(page.events.map((event) => event.type), ["commit.created", "fs.committed", "branch.forked", "branch.forked", "workspace.mounted", "workspace.unmounted", "sandbox.created", "sandbox.closed"]);
  assert.deepEqual(page.events.map((event) => event.source), ["store", "store", "store", "store", "daemon", "daemon", "daemon", "daemon"]);
  assert.deepEqual(page.events[0].payload, { branch: "main", workspace: WS, commit: FULL_COMMIT });
  assert.deepEqual(page.events[1].payload, { branch: "main", method: "fs.write", path: "/one.txt", from: null, to: null, commit: fsCommit });
  assert.deepEqual(page.events[2].payload, { branch: "from-commit", forkPoint: { kind: "commit", id: "not-a-publication" } });
  assert.deepEqual(page.events[3].payload, { branch: "from-layer", forkPoint: { kind: "layer", id: "lay0" } });
  assert.deepEqual(page.events[4].payload, { workspace: WS, branch: "main" });
  assert.deepEqual(page.events[5].payload, { workspace: WS });
  assert.deepEqual(page.events[6].payload, { sandbox: "2" });
  assert.deepEqual(page.events[7].payload, { sandbox: "2" });
  assert.equal(events.cursor, startCursor + 8);
  assert.deepEqual(callsFrom(start), ["workspace.commit", "fs.write", "branch.fork", "branch.fork", "workspace.mount", "workspace.unmount", "sandbox.create", "sandbox.close", "events.poll"]);
  const id = currentId();
  checkCall(start, "workspace.commit", { ...WS, message: "attributed", force: false, author: "P9 author", labels: ["p9", "event"] }, id);
  checkCall(start + 1, "fs.write", { branch: "main", path: "/one.txt", content: "one", message: "file" }, id);
  checkCall(start + 4, "workspace.mount", { sandbox: "1", branch: "main" }, id);
  checkCall(start + 8, "events.poll", { cursor: startCursor, timeoutMs: 0 }, id);
});

scenario("3 every fs path/from/to projection", async () => {
  const events = lf.events.test;
  const startCursor = events.cursor;
  native.fsResults = Object.fromEntries(["fs.write", "fs.apply", "fs.edit", "fs.mkdir", "fs.rm", "fs.mv", "fs.cp"].map((op) => [op, committed({ ...FULL_COMMIT, message: op })]));
  await lf.fs.write("main", "/write", "w");
  await lf.fs.apply("main", [{ op: "write", path: "/a", content: "a" }]);
  await lf.fs.edit("main", "/edit", "a", "b");
  await lf.fs.mkdir("main", "/mkdir");
  await lf.fs.rm("main", "/rm");
  await lf.fs.mv("main", "/from-mv", "/to-mv");
  await lf.fs.cp("main", "/from-cp", "/to-cp");
  const page = await events.poll();
  assert.deepEqual(page.events.map((event) => event.seq), Array.from({ length: 7 }, (_, i) => startCursor + i + 1));
  assert.deepEqual(page.events.map(({ payload }) => ({ method: payload.method, path: payload.path, from: payload.from, to: payload.to })), [
    { method: "fs.write", path: "/write", from: null, to: null },
    { method: "fs.apply", path: null, from: null, to: null },
    { method: "fs.edit", path: "/edit", from: null, to: null },
    { method: "fs.mkdir", path: "/mkdir", from: null, to: null },
    { method: "fs.rm", path: "/rm", from: null, to: null },
    { method: "fs.mv", path: null, from: "/from-mv", to: "/to-mv" },
    { method: "fs.cp", path: null, from: "/from-cp", to: "/to-cp" },
  ]);
  for (const event of page.events) assert.deepEqual(event.payload.commit, { ...FULL_COMMIT, message: event.payload.method });
});

scenario("4 fast-forward and non-fast-forward merge emit once", async () => {
  const events = lf.events.test;
  const fast = { kind: "Merged", branch: "fast", head: "existing", root: "r-fast", sources: ["a", "b"], mergeBase: "base", fastForward: true, conflicts: [], lww: [], units: [], conflictsTruncated: false, lwwTruncated: false, unitsTruncated: false };
  const slow = { ...fast, branch: "slow", head: "new-merge", root: "r-slow", fastForward: false, secondParent: "b" };
  native.mergeResults = { fast, slow };
  await lf.branch.merge("a", "b", "fast");
  await lf.branch.merge("a", "b", "slow", "merge");
  const page = await events.poll();
  assert.deepEqual(page.events.map((event) => event.type), ["branch.merged", "branch.merged"]);
  assert.deepEqual(page.events.map((event) => event.payload), [{ merge: fast }, { merge: slow }]);
  assert.equal(page.events.some((event) => event.type === "commit.created"), false);
});

scenario("5 publish Added versus silent UpToDate and NoChanges", async () => {
  const events = lf.events.test;
  const added = { kind: "Added", source: { branch: "feature", commit: FULL_COMMIT.id }, publishedLayer: { id: `32${"e".repeat(64)}` }, basedOn: `32${"f".repeat(64)}` };
  native.publishResults = { added, up: { kind: "UpToDate" }, none: { kind: "NoChanges" } };
  await lf.branch.publish("added");
  await lf.branch.publish("up");
  await lf.branch.publish("none");
  const page = await events.poll();
  assert.equal(page.events.length, 1);
  assert.deepEqual(page.events[0], {
    seq: events.cursor, type: "branch.published", source: "store",
    payload: { branch: "feature", commit: FULL_COMMIT.id, layer: added.publishedLayer.id, basedOn: added.basedOn },
  });
});

scenario("6 no-op and every refused event-producing operation are silent", async () => {
  const events = lf.events.test;
  native.workspaceCommitResult = { kind: "UpToDate", head: FULL_COMMIT.id };
  native.fsResults = Object.fromEntries(["fs.write", "fs.apply", "fs.edit", "fs.mkdir", "fs.rm", "fs.mv", "fs.cp"].map((op) => [op, { kind: "UpToDate", head: FULL_COMMIT.id }]));
  const calls = [
    ["workspace.commit", WS], ["fs.write", { branch: "main", path: "/w" }], ["fs.apply", { branch: "main", changes: [] }],
    ["fs.edit", { branch: "main", path: "/e" }], ["fs.mkdir", { branch: "main", path: "/d" }], ["fs.rm", { branch: "main", path: "/r" }],
    ["fs.mv", { branch: "main", from: "/a", to: "/b" }], ["fs.cp", { branch: "main", from: "/a", to: "/b" }],
    ["branch.fork", { name: "silent" }], ["branch.merge", { branch: "a", with: "b", name: "silent" }], ["branch.publish", { branch: "silent" }],
    ["workspace.mount", { sandbox: "1", branch: "main" }], ["workspace.unmount", WS], ["sandbox.create", {}], ["sandbox.close", { sandbox: "2" }],
  ];
  for (const [op, args] of calls.slice(0, 8)) await lf.call(op, args);
  native.mergeResults = { conflict: { kind: "Conflict", conflicts: [] } };
  await lf.branch.merge("a", "b", "conflict");
  for (const [op, args] of calls) {
    process.env.CLI_FAKE_REFUSE = op;
    await assert.rejects(lf.call(op, args), { code: "Conflict" });
    delete process.env.CLI_FAKE_REFUSE;
  }
  const page = await events.poll();
  assert.deepEqual(page, { events: [], cursor: events.cursor, hasMore: false, stale: false, closed: false });
});

scenario("7 cursor advancement, byte paging and single-event progress", async () => {
  const events = lf.events.test;
  const first = events.cursor;
  await produceCommits(2, "x".repeat(30_000));
  const one = await events.poll();
  assert.equal(one.events.length, 1);
  assert.equal(one.cursor, first + 1);
  assert.equal(one.hasMore, true);
  const two = await events.poll();
  assert.equal(two.events.length, 1);
  assert.equal(two.cursor, first + 2);
  assert.equal(two.hasMore, false);
  await produceCommits(1, "y".repeat(60_000));
  const large = await events.poll();
  assert.equal(large.events.length, 1, "an over-budget first event still advances");
  assert.equal(large.cursor, first + 3);
  assert.equal(large.events[0].payload.commit.message.length, 60_000);
  assert.equal(large.hasMore, false);
});

scenario("8 fake-cap-4 overflow, stale snapshot, resume and old-cursor resync", async () => {
  const events = lf.events.test;
  process.env.LAYERFS_FAKE_EVENT_CAP = "4";
  await produceCommits(5, "overflow");
  const stale = await events.poll();
  assert.equal(stale.stale, true);
  assert.equal(stale.closed, false);
  assert.deepEqual(stale.events, []);
  assert.equal(stale.cursor, events.cursor);
  assert.deepEqual(stale.snapshot, events.snapshot);
  assert.ok(stale.cursor >= 5);
  assert.deepEqual(await events.poll(), { events: [], cursor: stale.cursor, hasMore: false, stale: false, closed: false });
  const old = await native.call("events.poll", { subscription: currentId(), cursor: 0, timeoutMs: 0 });
  assert.equal(old.value.stale, true);
  assert.equal(old.value.cursor, stale.cursor);
  assert.deepEqual(old.value.events, []);
  delete process.env.LAYERFS_FAKE_EVENT_CAP;
});

scenario("10 native validation, non-default limit, concurrent poll and safe integers", async () => {
  const events = lf.events.test;
  assert.equal((await refuse("events.poll", { subscription: "0".repeat(64), cursor: 0 })).message, "unknown subscription");
  for (const cursor of [-1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, "0", null]) {
    assert.equal((await refuse("events.poll", { subscription: currentId(), cursor })).code, "InvalidInput");
  }
  assert.equal((await refuse("events.poll", { subscription: currentId(), cursor: Number.MAX_SAFE_INTEGER })).message, "unknown cursor");
  for (const timeoutMs of [-1, 1.5, NaN, Infinity, 30_001, "0", null]) {
    assert.equal((await refuse("events.poll", { subscription: currentId(), cursor: events.cursor, timeoutMs })).code, "InvalidInput");
  }
  for (const limit of [0, -1, 1.5, 501, NaN, Infinity, "1", null]) {
    assert.equal((await refuse("events.poll", { subscription: currentId(), cursor: events.cursor, timeoutMs: 0, limit })).code, "InvalidInput");
  }
  await produceCommits(2, "limited");
  const limited = await native.call("events.poll", { subscription: currentId(), cursor: events.cursor, timeoutMs: 0, limit: 1 });
  assert.equal(limited.value.events.length, 1);
  assert.equal(limited.value.hasMore, true);
  const rest = await native.call("events.poll", { subscription: currentId(), cursor: limited.value.cursor, timeoutMs: 0, limit: 1 });
  assert.equal(rest.value.events.length, 1);
  assert.equal(rest.value.hasMore, false);
  events.cursor = rest.value.cursor;

  const waiting = events.poll(5000);
  await assert.rejects(events.poll(), { code: "InvalidInput", message: /a poll is already outstanding$/ });
  assert.equal((await refuse("events.poll", { subscription: currentId(), cursor: events.cursor, timeoutMs: 0 })).message, "a poll is already outstanding");
  await produceCommits(1, "wake");
  assert.equal((await waiting).events[0].payload.commit.message, "wake");

  const state = native.eventStates.get(currentId());
  state.seq = Number.MAX_SAFE_INTEGER;
  const maximum = await native.call("events.poll", { subscription: currentId(), cursor: Number.MAX_SAFE_INTEGER, timeoutMs: 0 });
  assert.equal(maximum.value.cursor, Number.MAX_SAFE_INTEGER);
  await produceCommits(1, "exhaust");
  assert.equal((await refuse("events.poll", { subscription: currentId(), cursor: Number.MAX_SAFE_INTEGER, timeoutMs: 0 })).message, "cursor exhausted; resubscribe");

  process.env.UV_THREADPOOL_SIZE = "2";
  const first = (await native.call("events.subscribe", {})).value;
  const second = (await native.call("events.subscribe", {})).value;
  const admitted = native.call("events.poll", { subscription: first.subscription, cursor: 0, timeoutMs: 5000 });
  const saturated = await refuse("events.poll", { subscription: second.subscription, cursor: 0, timeoutMs: 1000 });
  assert.equal(saturated.code, "Busy");
  assert.equal(saturated.message, "long-poll admission is saturated; no wait was started");
  assert.deepEqual((await native.call("events.poll", { subscription: second.subscription, cursor: 0, timeoutMs: 0 })).value, { events: [], cursor: 0, hasMore: false, stale: false, closed: false });
  await native.call("events.unsubscribe", { subscription: first.subscription });
  assert.equal((await admitted).value.closed, true);
  await native.call("events.unsubscribe", { subscription: second.subscription });
  delete process.env.UV_THREADPOOL_SIZE;
});

scenario("11 high-level timeout validation makes no binding call", async () => {
  const events = lf.events.test;
  const before = native.calls.length;
  for (const timeoutMs of [-1, 1.5, NaN, Infinity, 30_001, "0", null, undefined, {}]) {
    if (timeoutMs === undefined) continue;
    await assert.rejects(events.poll(timeoutMs), { code: "InvalidInput", message: /timeoutMs must be an integer in 0\.\.30000$/ });
  }
  assert.equal(native.calls.length, before);
});

scenario("12 complete larger-than-64KiB stale snapshot", async () => {
  const events = await lf.events.subscribe();
  const names = Array.from({ length: 70 }, (_, i) => `${String(i).padStart(2, "0")}-${"b".repeat(1100)}`);
  native.branchList = names;
  process.env.LAYERFS_FAKE_EVENT_CAP = "4";
  await produceCommits(5, "large snapshot");
  const stale = await events.poll();
  assert.equal(stale.stale, true);
  assert.ok(Buffer.byteLength(JSON.stringify(stale.snapshot)) > 65_519);
  assert.deepEqual(stale.snapshot.branches.map(({ name }) => name), names);
  assert.equal(stale.snapshot.branches.length, names.length);
  assert.deepEqual(events.snapshot, stale.snapshot);
  delete process.env.LAYERFS_FAKE_EVENT_CAP;
  native.branchList = ["main", "cart"];
});

scenario("13 iteration order, normal close, stale error and no Busy retry", async () => {
  let events = await lf.events.subscribe();
  await produceCommits(2, "iterate");
  const iterator = events[Symbol.asyncIterator]();
  const first = await iterator.next();
  const second = await iterator.next();
  assert.deepEqual([first.value.payload.commit.message, second.value.payload.commit.message], ["iterate", "iterate"]);
  assert.ok(second.value.seq > first.value.seq);
  const waiting = iterator.next();
  await events.close();
  assert.deepEqual(await waiting, { done: true, value: undefined });

  events = await lf.events.subscribe();
  process.env.LAYERFS_FAKE_EVENT_CAP = "4";
  await produceCommits(5, "stale iterator");
  await assert.rejects(events[Symbol.asyncIterator]().next(), { code: "InvalidInput", message: /stream went stale; resubscribe$/ });
  delete process.env.LAYERFS_FAKE_EVENT_CAP;

  events = await lf.events.subscribe();
  lf.events.test = events;
  native.pollRefusals = ["Busy"];
  const before = native.calls.length;
  await assert.rejects(events[Symbol.asyncIterator]().next(), { code: "Busy" });
  assert.deepEqual(callsFrom(before), ["events.poll"], "Busy is never retried");
});

scenario("9 resubscribe/no-replay, idempotent close and client close wakeup", async () => {
  let events = lf.events.test;
  await produceCommits(1, "old queue");
  const oldId = currentId();
  events = await lf.events.subscribe();
  assert.equal(events.cursor, 0);
  assert.deepEqual(await events.poll(), { events: [], cursor: 0, hasMore: false, stale: false, closed: false });
  await native.call("fs.write", { ...COMMON, branch: "main", path: "/old", content: "old", subscription: oldId });
  assert.deepEqual(await events.poll(), { events: [], cursor: 0, hasMore: false, stale: false, closed: false }, "an old in-flight ID cannot enqueue into the new queue");

  let before = native.calls.length;
  await events.close();
  await events.close();
  assert.deepEqual(callsFrom(before), ["events.unsubscribe"], "close unsubscribes once");
  events = await lf.events.subscribe();
  const waiting = events.poll(5000);
  before = native.calls.length;
  await lf.close();
  assert.deepEqual(callsFrom(before), ["events.unsubscribe"]);
  assert.equal((await waiting).closed, true);
  assert.equal(native.stores.at(-1)[0], "close");
  before = native.calls.length;
  await events.close();
  assert.equal(native.calls.length, before, "owner close and subscription close share one unsubscribe");
});

resetResults();
const failures = [];
for (const { name, run } of scenarios) {
  try {
    await run();
    resetResults();
  } catch (error) {
    failures.push(`${name}: ${error.stack ?? error}`);
    break;
  }
}
try {
  await lf.close();
} catch (error) {
  failures.push(`cleanup: ${error.stack ?? error}`);
}
delete process.env.LAYERFS_FAKE_EVENT_CAP;
delete process.env.CLI_FAKE_REFUSE;
if (failures.length) {
  console.error(`FAIL area-p9-events (${failures.length})\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`PASS area-p9-events (${scenarios.length} scenarios)`);

// @layerfs/backend's surface (#428), with the agent's stand-in binding; no
// Store, Docker or daemon. One case per command:
//   node api/backend/test/backend.mjs surface|closed|comparable|get-log
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, { LAYERFS_NATIVE: FAKE, LAYERFS_HOME: "/private/home", LAYERFS_STORE_PATH: "/fake/store.sqlite" });
for (const name of ["TRACE", "TRACE_STEP", "TRACE_START", "WORKER_ENDPOINT"]) delete process.env[`LAYERFS_${name}`];
const { LayerFS, CODES } = await import("@layerfs/agent");
const { LayerFSBackend } = await import("@layerfs/backend");
const native = (await import(FAKE)).default;
const selected = process.argv[2];
const shop = await LayerFS.open("website");
const backend = LayerFSBackend.of(shop);
const sent = (method) => native.args[native.calls.lastIndexOf(method)];

if (selected === "surface") {
  assert.throws(() => new LayerFSBackend(shop), { code: "InvalidInput", message: /LayerFSBackend\.of/ });
  for (const bad of [null, {}, { project: {} }]) assert.throws(() => LayerFSBackend.of(bad), { code: "InvalidInput" });
  const methods = (group) => Object.keys(group).sort();
  assert.deepEqual(Object.keys(backend).sort(), ["daemon", "store"]);
  assert.deepEqual(methods(backend.store.project), ["get_comparable", "get_history", "get_storage", "list_overwritten"]);
  assert.deepEqual(methods(backend.store.branch), ["list_commit_storage"]);
  assert.deepEqual(methods(backend.store.fs), ["get_storage", "list_revisions"]);
  assert.deepEqual(methods(backend.daemon.workspace), ["get_storage", "list_changes", "status"]);
  assert.deepEqual(methods(backend.daemon.process), ["cancel", "list", "start"]);
  assert.deepEqual(methods(backend.daemon.sandbox), ["get_log"]);
  assert.ok(CODES.includes("NotComparable"));
  // each goes to the backend table under its own name, with the client's Store
  await backend.store.fs.list_revisions("main", "/README.md", 10, 0);
  assert.deepEqual([sent("backend.store.fs.list_revisions").branch, sent("backend.store.fs.list_revisions").storePath], ["main", "/fake/store.sqlite"]);
  await backend.daemon.workspace.status({ sandbox: "1", path: "/workspaces/w1" });
  assert.equal(sent("backend.daemon.workspace.status").sandbox, "1");
  assert.deepEqual(await backend.store.project.get_storage(), { dedupe: 2, compression: null });
  // process.list pages the processes of every running Sandbox
  const page = await backend.daemon.process.list(2, 0);
  assert.deepEqual([page.items, page.hasMore, page.nextPagePos], [[], false, null]);
  await assert.rejects(backend.daemon.process.cancel(""), { code: "InvalidInput" });
} else if (selected === "closed") {
  // `of` opens nothing; once the client is closed every call is refused
  await shop.close();
  const calls = native.calls.length;
  for (const call of [() => backend.store.project.get_history(), () => backend.daemon.process.list(), () => backend.daemon.sandbox.get_log("1")]) {
    await assert.rejects(call(), { code: "InvalidInput", message: /closed/ });
  }
  assert.equal(native.calls.length, calls, "a closed client sends nothing");
} else if (selected === "comparable") {
  const id = (c) => c.repeat(64);
  native.history = {
    kind: "ok", version: "h2", branches: [], more: {}, workspaces: [],
    commits: [{ id: id("a"), root: id("1") }, { id: id("b"), root: id("2") }],
    layers: [
      { id: id("c"), root: id("2"), sourceCommit: id("b") }, // same files as Commit b
      { id: id("d"), root: id("3"), sourceCommit: id("e") }, // from a Commit not listed
      { id: id("f"), root: id("4"), sourceCommit: id("a") }, // a publish that merged
      { id: id("9"), root: id("5"), sourceCommit: null }, // Init's Layer
    ],
  };
  const reads = () => native.calls.filter((method) => method === "backend.store.project.get_history").length;
  const before = reads();
  assert.deepEqual(await backend.store.project.get_comparable(id("a")), { commit: id("a"), root: id("1"), exact: true });
  assert.deepEqual(await backend.store.project.get_comparable(id("c")), { commit: id("b"), root: id("2"), exact: true });
  assert.deepEqual(await backend.store.project.get_comparable(id("d")), { commit: id("e"), root: id("3"), exact: false });
  for (const ref of [id("f"), id("9")]) await assert.rejects(backend.store.project.get_comparable(ref), { code: "NotComparable", detail: { ref } });
  assert.equal(reads() - before, 1, "saved ids are learned once per client");
  // an id history does not list is read again, then refused
  await assert.rejects(backend.store.project.get_comparable(id("7")), { code: "NotComparable" });
  assert.equal(reads() - before, 2);
  await assert.rejects(backend.store.project.get_comparable("HEAD"), { code: "InvalidInput" });
  // another client learns for itself
  const other = await LayerFS.open("website");
  await LayerFSBackend.of(other).store.project.get_comparable(id("a"));
  assert.equal(reads() - before, 3);
  await other.close();
} else if (selected === "get-log") {
  await backend.daemon.sandbox.get_log("2");
  assert.deepEqual([sent("backend.daemon.sandbox.get_log").sandbox, sent("backend.daemon.sandbox.get_log").lines], ["2", 200]);
  await backend.daemon.sandbox.get_log("2", { lines: 2000 });
  assert.equal(sent("backend.daemon.sandbox.get_log").lines, 2000);
  for (const options of [{ lines: 0 }, { lines: 2001 }, { lines: 1.5 }, { since: "2026-10-23T00:00:00Z" }]) {
    await assert.rejects(backend.daemon.sandbox.get_log("2", options), { code: "InvalidInput" });
  }
  for (const bad of ["0", "x", 2]) await assert.rejects(backend.daemon.sandbox.get_log(bad), { code: "InvalidInput" });
} else {
  throw new Error("select surface, closed, comparable or get-log");
}
await shop.close();
console.log(`PASS backend/${selected}`);

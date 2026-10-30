// P13 folder-scoped client confinement over one stand-in binding.
//   node api/agent/test/area-p13-scope.mjs
// No Docker, Store, mounted Workspace or daemon is used.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { ENV } from "./fixture/cases.mjs";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
delete process.env.CLI_FAKE_LOG;
const native = createRequire(import.meta.url)(FAKE);
const { LayerFS } = await import("../src/index.js");
const { LayerFSBackend } = await import("@layerfs/backend");
const scenarios = [];
const scenario = (name, run) => scenarios.push({ name, run });
const SCOPE = { folder: "/packages/editor", branch: "team.editor" };
const WS = { sandbox: "1", path: "/workspaces/w1" };
const COMMIT = `12${"a".repeat(64)}`;
const open = (scope = SCOPE) => LayerFS.open("site", { scope });

async function outside(promise, kind = "path") {
  await assert.rejects(promise, (error) => {
    assert.equal(error.name, "LayerFSError");
    assert.equal(error.code, "InvalidInput");
    assert.equal(error.message, `InvalidInput: out of scope: ${kind}`);
    assert.equal(error.detail, null);
    return true;
  });
}

scenario("construction validates scope and init does not accept it", async () => {
  const before = native.calls.length;
  const stores = native.stores.length;
  const invalid = [
    null, [], "scope", {}, { branch: "team" }, { folder: "/a", unknown: true },
    { folder: "relative", branch: "team" }, { folder: "", branch: "team" },
    { folder: "/a/", branch: "team" }, { folder: "/a//b", branch: "team" },
    { folder: "/a/./b", branch: "team" }, { folder: "/a/../b", branch: "team" },
    { folder: "/a", branch: "" }, { folder: "/a", branch: "team/editor" },
    { folder: "/a", branch: "Team" }, { folder: "/a", branch: ".team" },
    { folder: "/a", branch: "team." }, { folder: "/a", branch: "a".repeat(64) },
  ];
  for (const scope of invalid) await assert.rejects(LayerFS.open("site", { scope }), { name: "LayerFSError", code: "InvalidInput" });
  await assert.rejects(LayerFS.init("fresh", "/tmp/fresh", undefined, { scope: SCOPE }), { name: "LayerFSError", code: "InvalidInput" });
  assert.equal(native.calls.length, before, "bad scopes do not dispatch");
  assert.equal(native.stores.length, stores, "bad scopes do not open a Store");
  const valid = await open({ folder: "/", branch: "a" });
  await valid.close();
});

scenario("in-scope file operations preserve arguments and results", async () => {
  const plain = await LayerFS.open("site");
  const scoped = await open();
  const prefix = "team.editor";
  const cases = [
    [(c, r) => c.fs.list(r, "/packages/editor", 5, 0), "fs.list"],
    [(c, r) => c.fs.glob(r, "**/*", "/packages/editor", 5, 0), "fs.glob"],
    [(c, r) => c.fs.grep(r, "needle", "/packages/editor", 5, 0), "fs.grep"],
    [(c, r) => c.fs.stat(r, "/packages/editor"), "fs.stat"],
    [(c, r) => c.fs.read(r, "/packages/editor/src/index.js", 1, 5), "fs.read"],
    [(c, r) => c.fs.blame(r, "/packages/editor/src/index.js", 1), "fs.blame"],
    [(c, r) => c.fs.write(r, "/packages/editor/a", "new", "write"), "fs.write"],
    [(c, r) => c.fs.edit(r, "/packages/editor/a", "old", "new", "edit"), "fs.edit"],
    [(c, r) => c.fs.apply(r, [{ op: "write", path: "/packages/editor/a", content: "a" }, { op: "rm", path: "/packages/editor/b" }], "apply"), "fs.apply"],
    [(c, r) => c.fs.mkdir(r, "/packages/editor/new", "mkdir"), "fs.mkdir"],
    [(c, r) => c.fs.rm(r, "/packages/editor/old", "rm"), "fs.rm"],
    [(c, r) => c.fs.cp(r, "/packages/editor/a", "/packages/editor/b", "cp"), "fs.cp"],
    [(c, r) => c.fs.mv(r, "/packages/editor/a", "/packages/editor/b", "mv"), "fs.mv"],
    [(c, r) => c.fs.diff_files(r, "/packages/editor/a", r === "main" ? "feature" : "team.editor.feature", "/packages/editor/b"), "fs.diff_files"],
  ];
  for (const [call, method] of cases) {
    const expected = await call(plain, `${prefix}.main`);
    const expectedArgs = native.args.at(-1);
    const actual = await call(scoped, "main");
    assert.deepEqual(actual, expected, method);
    assert.deepEqual(native.args.at(-1), expectedArgs, `${method} binding args`);
  }
  await plain.close();
  await scoped.close();
});

scenario("every file operation refuses all out-of-scope paths before dispatch", async () => {
  const client = await open();
  const calls = [
    () => client.fs.list("main", "/packages"),
    () => client.fs.glob("main", "**/*", "/packages"),
    () => client.fs.grep("main", "needle", "/packages"),
    () => client.fs.stat("main", "/packages/editor2"),
    () => client.fs.read("main", "/packages/other.txt"),
    () => client.fs.blame("main", "/packages/other.txt"),
    () => client.fs.write("main", "/packages/other.txt", "x"),
    () => client.fs.edit("main", "/packages/other.txt", "a", "b"),
    () => client.fs.apply("main", [{ op: "write", path: "/packages/editor/a", content: "x" }, { op: "rm", path: "/packages/other" }]),
    () => client.fs.apply("main", [{ op: "cp", from: "/packages/other/a", to: "/packages/editor/a" }]),
    () => client.fs.apply("main", [{ op: "cp", from: "/packages/editor/a", to: "/packages/other/a" }]),
    () => client.fs.apply("main", [{ op: "mv", from: "/packages/other/a", to: "/packages/editor/a" }]),
    () => client.fs.apply("main", [{ op: "mv", from: "/packages/editor/a", to: "/packages/other/a" }]),
    () => client.fs.mkdir("main", "/packages/other"),
    () => client.fs.rm("main", "/packages/other.txt"),
    () => client.fs.cp("main", "/packages/other/a", "/packages/editor/a"),
    () => client.fs.cp("main", "/packages/editor/a", "/packages/other/a"),
    () => client.fs.mv("main", "/packages/other/a", "/packages/editor/a"),
    () => client.fs.mv("main", "/packages/editor/a", "/packages/other/a"),
    () => client.fs.diff_files("main", "/packages/other/a", "main", "/packages/editor/a"),
    () => client.fs.diff_files("main", "/packages/editor/a", "main", "/packages/other/a"),
    () => client.call("fs.read", { branch: "main", path: "/packages/other.txt" }),
  ];
  const before = native.calls.length;
  for (const call of calls) await outside(call());
  assert.equal(native.calls.length, before, "no partial or out-of-scope operation reaches the binding");
  await client.close();
});

scenario("only canonical paths on component boundaries are in scope", async () => {
  const client = await open();
  const before = native.calls.length;
  for (const path of [
    "/packages/editor/../other", "/packages/editor//src", "packages/editor/src",
    "/packages/editor/.", "/packages/editor/", "/packages/editor2/file", "", ".", "..",
  ]) await outside(client.fs.read("main", path));
  assert.equal(native.calls.length, before);
  await client.fs.read("main", "/packages/editor");
  await client.fs.read("main", "/packages/editor/src");
  const root = await open({ folder: "/", branch: "team" });
  await root.fs.read("main", "/any/canonical/path");
  await root.close();
  await client.close();
});

scenario("refs map Branch names but preserve immutable Commit and Layer ids", async () => {
  const client = await open();
  await client.fs.read("main", "/packages/editor/a");
  assert.equal(native.args.at(-1).branch, "team.editor.main");
  await client.fs.read(COMMIT, "/packages/editor/a");
  assert.equal(native.args.at(-1).branch, COMMIT);
  await client.branch.fork("from-commit", COMMIT);
  assert.equal(native.args.at(-1).from, COMMIT);
  const filesOnly = await open({ folder: "/packages/editor" });
  await filesOnly.fs.read(COMMIT, "/packages/editor/a");
  assert.equal(native.args.at(-1).branch, COMMIT);
  await outside(filesOnly.fs.read("main", "/packages/editor/a"), "branch");
  await filesOnly.close();
  await client.close();
});

scenario("path-returning history operations refuse before dispatch", async () => {
  const client = await open();
  const before = native.calls.length;
  for (const call of [
    () => client.branch.list_commits("main"),
    () => client.branch.graph("main"),
    () => client.branch.changed_paths("c0ffee"),
    () => client.branch.diff_commits("c0ffee", "decaf"),
    () => client.branch.diff_branches("main", "feature"),
    () => client.call("branch.list_commits", { branch: "main" }),
  ]) await outside(call(), "history");
  assert.equal(native.calls.length, before);
  await client.close();
});

scenario("Branch mutation and publication stay in the namespace", async () => {
  const client = await open();
  await client.branch.fork("feature", "main");
  assert.equal(native.args.at(-1).name, "team.editor.feature");
  assert.equal(native.args.at(-1).from, "team.editor.main");
  await client.branch.merge("main", "feature", "merged", "merge");
  assert.equal(native.args.at(-1).branch, "team.editor.main");
  assert.equal(native.args.at(-1).with, "team.editor.feature");
  assert.equal(native.args.at(-1).name, "team.editor.merged");
  await client.branch.publish("feature", "c0ffee");
  assert.equal(native.args.at(-1).branch, "team.editor.feature");
  const before = native.calls.length;
  for (const call of [
    () => client.fs.read("team/editor/main", "/packages/editor/a"),
    () => client.branch.fork("../escape", "main"),
    () => client.branch.merge("main", "outsider/main", "merged"),
    () => client.branch.publish("Main", "c0ffee"),
    () => client.call("branch.fork", { name: "a".repeat(64), from: "main" }),
  ]) await outside(call(), "branch");
  assert.equal(native.calls.length, before);
  const long = await open({ folder: "/packages/editor", branch: "a".repeat(60) });
  const longBefore = native.calls.length;
  await outside(long.branch.fork("bbbb", "main"), "branch");
  assert.equal(native.calls.length, longBefore);
  await long.close();
  const filesOnly = await open({ folder: "/packages/editor" });
  const filesBefore = native.calls.length;
  await outside(filesOnly.branch.fork("feature", "main"), "branch");
  assert.equal(native.calls.length, filesBefore);
  await filesOnly.close();
  await client.close();
});

scenario("Branch listing filters and strips the namespace with scoped paging", async () => {
  native.branchList = ["main", "team.editor.main", "team.other", "team.editor.feature", "team.editor2.bad", "team.editor.review"];
  try {
    const client = await open();
    const first = await client.project.list_branches(2, 0);
    assert.deepEqual(first.items.map((item) => item.name), ["main", "feature"]);
    assert.equal(first.hasMore, true);
    assert.equal(first.nextPagePos, 2);
    const second = await client.call("project.list_branches", { pageSize: 2, pagePos: 2 });
    assert.deepEqual(second.items.map((item) => item.name), ["review"]);
    assert.equal(second.hasMore, false);
    assert.equal(second.nextPagePos, null);
    const filesOnly = await open({ folder: "/packages/editor" });
    await outside(filesOnly.project.list_branches(), "branch");
    await filesOnly.close();
    await client.close();
  } finally {
    native.branchList = null;
  }
});

scenario("Workspace mount selects namespace main and keeps normal refusals", async () => {
  const client = await open();
  const mounted = await client.workspace.mount("1");
  assert.deepEqual(mounted, { sandbox: "1", path: "/workspaces/w1", image: "fake-image" });
  assert.equal(native.args.at(-1).branch, "team.editor.main");
  await client.call("workspace.mount", { sandbox: "1" });
  assert.equal(native.args.at(-1).branch, "team.editor.main");
  const before = native.calls.length;
  await outside(client.workspace.mount("1", "feature"), "branch");
  await outside(client.call("workspace.mount", { sandbox: "1", branch: "feature" }), "branch");
  assert.equal(native.calls.length, before);
  process.env.CLI_FAKE_REFUSE = "workspace.mount";
  process.env.CLI_FAKE_CODE = "NoSuchBranch";
  try {
    await assert.rejects(client.workspace.mount("1"), { code: "NoSuchBranch" });
  } finally {
    delete process.env.CLI_FAKE_REFUSE;
    delete process.env.CLI_FAKE_CODE;
  }
  const filesOnly = await open({ folder: "/packages/editor" });
  await outside(filesOnly.workspace.mount("1"), "branch");
  await filesOnly.close();
  await client.close();
});

scenario("scoped Exec binds cwd internally and raw calls cannot override it", async () => {
  const client = await open();
  const result = await client.workspace.exec(WS, "exit 0", "scope test");
  assert.equal(result.exitCode, 0);
  assert.equal(native.args.at(-1).cwd, "/packages/editor");
  await client.call("workspace.exec", { ...WS, command: "exit 0" });
  assert.equal(native.args.at(-1).cwd, "/packages/editor");
  const before = native.calls.length;
  await outside(client.call("workspace.exec", { ...WS, command: "pwd", cwd: "/" }));
  assert.equal(native.calls.length, before);
  await client.close();
});

scenario("Workspace commit and unmount preserve their existing behavior", async () => {
  const plain = await LayerFS.open("site");
  const scoped = await open();
  const expectedCommit = await plain.workspace.commit(WS, "save");
  assert.deepEqual(await scoped.workspace.commit(WS, "save"), expectedCommit);
  const expectedUnmount = await plain.workspace.unmount(WS, true, false);
  assert.deepEqual(await scoped.workspace.unmount(WS, true, false), expectedUnmount);
  await plain.close();
  await scoped.close();
});

scenario("events and backend are unavailable to scoped clients", async () => {
  const client = await open();
  assert.equal(client.backend, undefined);
  const before = native.calls.length;
  await outside(client.events.subscribe(), "events");
  await outside(client.call("events.subscribe", {}), "events");
  await outside(Promise.resolve().then(() => LayerFSBackend.of(client)), "backend");
  assert.equal(native.calls.length, before);
  await client.close();
});

scenario("unknown raw methods retain their existing dispatch behavior", async () => {
  const client = await open();
  const before = native.calls.length;
  await client.call("future.operation", { value: 1 });
  assert.equal(native.calls.length, before + 1);
  assert.equal(native.calls.at(-1), "future.operation");
  await client.close();
});

scenario("unscoped clients keep their existing call shapes", async () => {
  const client = await LayerFS.open("site");
  await client.fs.read("main", "/packages/editor/a");
  assert.equal(native.args.at(-1).branch, "main");
  await client.workspace.exec(WS, "exit 0");
  assert.equal("cwd" in native.args.at(-1), false);
  await client.workspace.mount("1", "feature");
  assert.equal(native.args.at(-1).branch, "feature");
  await client.close();
});

let passed = 0;
const failures = [];
for (const { name, run } of scenarios) {
  try {
    await run();
    passed += 1;
    console.log(`  ok ${name}`);
  } catch (error) {
    failures.push(name);
    console.error(`  FAIL ${name}`);
    console.error(error);
  }
}
console.log(`${failures.length ? "FAILED" : "PASS"} area-p13-scope: ${passed}/${scenarios.length} scenarios ok`);
if (failures.length) process.exitCode = 1;

// @layerfs/agent's surface and client lifecycle (#428). Each case is one
// command; every case but `dispatch` uses the stand-in binding and creates no
// Store:
//   node sdk/js/packages/agent/test/packaging.mjs surface|lifecycle|copy|sandbox|worker-gate|transport-order|dispatch
// `dispatch` loads the real addon (LAYERFS_NATIVE or native/layerfs.node).
import assert from "node:assert/strict";
import { cpSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
const selected = process.argv[2];
if (selected !== "dispatch") {
  Object.assign(process.env, {
    LAYERFS_NATIVE: FAKE, LAYERFS_HOME: "/private/configured-home", LAYERFS_DOCKER: "/configured/docker.sock",
    LAYERFS_IMAGE: "configured-image", LAYERFS_VOLUME: "configured-volume", LAYERFS_DAEMON: "/configured/daemon",
    LAYERFS_STORE_PATH: "/fake/store.sqlite",
  });
}
for (const name of ["TRACE", "TRACE_STEP", "TRACE_START", "WORKER_ENDPOINT", "WORKER_ADAPTER"]) {
  delete process.env[`LAYERFS_${name}`];
  delete process.env[`DSH_LAYERFS_${name}`];
}
const agent = await import("@layerfs/agent");
const { LayerFS, CODES } = agent;
const native = selected === "dispatch" ? null : (await import(FAKE)).default;

if (selected === "surface") {
  const shop = await LayerFS.open("website");
  assert.equal(shop.projectName, "website");
  assert.equal(shop.options.home, "/private/configured-home");
  assert.deepEqual(shop.options.deploy, { docker: "/configured/docker.sock", image: "configured-image", volume: "configured-volume", daemon: "/configured/daemon" });
  // five groups: project 2, branch 6, fs 14, sandbox 3, workspace 4
  const methods = (group) => Object.keys(shop[group]).sort();
  assert.deepEqual(methods("project"), ["list_branches", "list_sandboxes"]);
  assert.deepEqual(methods("branch"), ["diff_branches", "diff_commits", "fork", "list_commits", "merge", "publish"]);
  assert.equal(methods("fs").length, 14);
  assert.deepEqual(methods("sandbox"), ["close", "create", "list_workspaces"]);
  assert.deepEqual(methods("workspace"), ["commit", "exec", "mount", "unmount"]);
  assert.equal(shop.inspect, undefined);
  assert.ok(CODES.includes("Capacity") && CODES.includes("MountsActive"));
  assert.equal(agent.currentMethod("operator.inspect.history"), "backend.store.project.get_history");
  // backend and runtime names are refused by the public call
  for (const method of ["backend.store.project.get_history", "runtime.sandbox.reclaim"]) {
    await assert.rejects(shop.call(method, {}), { code: "UnknownMethod" });
  }
  await assert.rejects(LayerFS.open("../foreign"), { code: "InvalidInput" });
  // the internal entry exports exactly what its header and the README list
  assert.deepEqual(Object.keys(await import("@layerfs/agent/internal")).sort(), [
    "STATE", "addonPath", "connection", "decode", "deployment", "encode", "endpointOf", "frame", "invokeBackend", "invokeClient",
    "invokeClientBackend", "live", "outcome", "page", "pageOf", "projectArgs", "readFrames", "sandboxArg", "storeArgs", "useWorker", "workspaceArgs",
  ]);
  await shop.close();
} else if (selected === "lifecycle") {
  // no public constructor
  assert.throws(() => new LayerFS("website"), { code: "InvalidInput", message: /LayerFS\.open/ });
  // open opens the Store once and looks the project up once
  const opens = native.stores.length;
  const shop = await LayerFS.open("website");
  assert.deepEqual(native.stores.slice(opens), [["open", "/fake/store.sqlite"]]);
  assert.equal(native.calls.at(-1), "project.list_branches");
  await shop.fs.read("main", "/README.md");
  // close releases it; every later call is refused InvalidInput; a second close does nothing
  await shop.close();
  assert.deepEqual(native.stores.at(-1), ["close", "/fake/store.sqlite"]);
  const calls = native.calls.length;
  for (const call of [() => shop.fs.read("main", "/README.md"), () => shop.workspace.mount("feature"), () => shop.sandbox.create(), () => shop.call("fs.read", {})]) {
    await assert.rejects(call(), { code: "InvalidInput", message: /closed/ });
  }
  assert.equal(native.calls.length, calls, "a closed client sends nothing");
  const stores = native.stores.length;
  await shop.close();
  assert.equal(native.stores.length, stores);
  // a missing project leaves nothing open
  const before = native.stores.length;
  await assert.rejects(LayerFS.open("website", { storePath: "/missing" }), { code: "NoSuchProject" });
  assert.deepEqual(native.stores.slice(before), [["open", "/missing"], ["close", "/missing"]]);
  // LayerFS.init runs project.init, then the same open
  const made = await LayerFS.init("fresh", "/src/fresh");
  assert.deepEqual(native.calls.slice(-2), ["project.init", "project.list_branches"]);
  await made.close();
  // `await using` closes at the end of the block
  assert.equal(typeof LayerFS.prototype[Symbol.asyncDispose], "function");
  {
    await using scoped = await LayerFS.open("website");
    await scoped.project.list_branches();
  }
  assert.deepEqual(native.stores.at(-1), ["close", "/fake/store.sqlite"]);
} else if (selected === "copy") {
  // a second copy of the package in one process refuses to load
  const dir = mkdtempSync(join(tmpdir(), "layerfs-agent-copy-"));
  try {
    cpSync(fileURLToPath(new URL("../src/", import.meta.url)), join(dir, "src"), { recursive: true });
    await assert.rejects(import(pathToFileURL(join(dir, "src/index.js")).href), /loaded twice/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
} else if (selected === "sandbox") {
  const shop = await LayerFS.open("website");
  const sent = (method) => native.args[native.calls.lastIndexOf(method)];
  // create starts one on the deployment's image, or the one given
  await shop.sandbox.create();
  assert.equal(sent("sandbox.create").deploy.image, "configured-image");
  await shop.sandbox.create({ image: "sha256:other" });
  assert.equal(sent("sandbox.create").deploy.image, "sha256:other");
  await assert.rejects(shop.sandbox.create({ image: 7 }), { code: "InvalidInput" });
  // mount places by default and names a chosen Sandbox only when asked
  await shop.workspace.mount("feature");
  assert.equal("sandbox" in sent("workspace.mount"), false);
  await shop.workspace.mount("feature", { sandbox: "2" });
  assert.equal(sent("workspace.mount").sandbox, "2");
  assert.equal("image" in sent("workspace.mount"), false, "only a given image is checked against the Sandbox's");
  await shop.workspace.mount("feature", { sandbox: "2", image: "sha256:other" });
  assert.equal(sent("workspace.mount").image, "sha256:other");
  for (const bad of ["0", "x", 2, ""]) await assert.rejects(shop.workspace.mount("feature", { sandbox: bad }), { code: "InvalidInput" });
  await shop.sandbox.close("2");
  assert.equal(sent("sandbox.close").sandbox, "2");
  // list_sandboxes pages the records and changes nothing
  native.sandboxList = [1, 2, 3].map((n) => ({ sandbox: String(n), name: `layerfs-website-${n}`, image: "configured-image", workspaces: n }));
  const first = await shop.project.list_sandboxes(2, 0);
  assert.deepEqual([first.items.map((s) => s.sandbox), first.hasMore, first.nextPagePos], [["1", "2"], true, 2]);
  const last = await shop.project.list_sandboxes(2, 2);
  assert.deepEqual([last.items, last.hasMore, last.nextPagePos], [[{ sandbox: "3", name: "layerfs-website-3", image: "configured-image", workspaces: 3 }], false, null]);
  // list_workspaces shapes the daemon's records
  native.workspaceList = [
    { workspace: "w1", branch: "feature", base: "c1", activity: "idle", mounted: true, directory: "/workspaces/w1", dirty: { inodes: 0, entries: 0 }, headMovedSinceMount: false, pendingMerge: false },
    { workspace: "w2", branch: "feature", base: "c1", activity: "committing", mounted: true, directory: "/workspaces/w2", dirty: { inodes: 2, entries: 1 }, headMovedSinceMount: true, pendingMerge: false },
    { workspace: "w3", branch: null, base: null, activity: "attaching", mounted: false, directory: null, dirty: null, headMovedSinceMount: null, pendingMerge: false },
    ...["attaching", "closing", "uncertain", "failed", "draining"].map((activity, n) => ({ workspace: `s${n}`, branch: "feature", base: "c1", activity, mounted: true, directory: `/workspaces/s${n}`, dirty: null, headMovedSinceMount: false, pendingMerge: false })),
  ];
  const page = await shop.sandbox.list_workspaces("1");
  assert.deepEqual(page.items.slice(0, 2).map(({ sandbox, path, branch, base, activity, changed }) => ({ sandbox, path, branch, base, activity, changed })), [
    { sandbox: "1", path: "/workspaces/w1", branch: "feature", base: "c1", activity: "Idle", changed: false },
    { sandbox: "1", path: "/workspaces/w2", branch: "feature", base: "c1", activity: "Busy", changed: true },
  ]);
  // only the daemon's busy states read Busy; the others keep their own name
  assert.deepEqual(page.items.slice(2).map((w) => [w.state, w.activity, w.changed]), [
    ["attaching", "Busy", null], ["closing", "Closing", null], ["uncertain", "Uncertain", null], ["failed", "Failed", null], ["draining", "draining", null],
  ]);
  assert.equal(page.version, "1");
  await shop.close();
} else if (selected === "worker-gate") {
  // the worker route opens only for the dsh adapter (AGENTS.md exceptions)
  process.env.LAYERFS_WORKER_ENDPOINT = "/nonexistent/endpoint.json";
  delete process.env.LAYERFS_NATIVE;
  await assert.rejects(LayerFS.open("website"), { code: "Unsupported", message: /dsh adapter/ });
} else if (selected === "transport-order") {
  // each client keeps the binding it was made with, whatever the call order
  const { fakeWorker } = await import("./fixture/fake-worker.mjs");
  const { useWorker } = await import("@layerfs/agent/internal");
  const worker = await fakeWorker();
  const dir = mkdtempSync(join(tmpdir(), "layerfs-order-"));
  try {
    const before = await LayerFS.open("website");                    // in-process, before any worker
    const file = join(dir, "endpoint.json");
    worker.endpoint(file, "/private/configured-home");
    await assert.rejects(async () => useWorker(file, "other"), { code: "Unsupported" });
    useWorker(file, "dsh");
    assert.throws(() => useWorker(join(dir, "another.json"), "dsh"), { code: "InvalidInput", message: /already uses/ });
    const after = await LayerFS.open("website");                     // through the worker
    assert.deepEqual(worker.calls.map((c) => [c.fn, c.op]), [["call", "project.list_branches"]]);
    const local = native.calls.length;
    await before.fs.read("main", "/README.md");
    assert.equal(native.calls.length, local + 1, "the earlier client stays in-process");
    await after.fs.read("main", "/README.md");
    assert.equal(native.calls.length, local + 1, "the later client never reaches the addon");
    assert.deepEqual(worker.calls.at(-1).op, "fs.read");
    assert.equal(worker.calls.at(-1).open, true, "the worker opens the later client's Store");
    await before.close();
    await after.close();
  } finally {
    worker.close();
    rmSync(dir, { recursive: true, force: true });
  }
} else if (selected === "dispatch") {
  const { inprocess } = await import("../src/transport/inprocess.js");
  const binding = inprocess();
  assert.equal(typeof binding.call, "function");
  assert.equal(typeof binding.backendCall, "function");
  for (const method of ["backend.store.project.get_history", "backend.daemon.process.list", "runtime.sandbox.reclaim"]) {
    assert.equal((await binding.call(method, {})).refused.code, "UnknownMethod");
  }
  assert.equal((await binding.backendCall("project.init", {})).refused.code, "UnknownMethod");
  assert.equal((await binding.backendCall("backend.missing", {})).refused.code, "UnknownMethod");
} else {
  throw new Error("select surface, lifecycle, copy, sandbox, worker-gate, transport-order or dispatch");
}
console.log(`PASS packaging/${selected}`);

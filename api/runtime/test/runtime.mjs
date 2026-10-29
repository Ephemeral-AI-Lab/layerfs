// @layerfs/runtime with the agent's stand-in binding and a stub Docker (#428):
// no Store, Docker or daemon. One case per command:
//   node api/runtime/test/runtime.mjs surface|home-mode|projects|init|state|events|leftovers|sandboxes|volumes|transport-order|macos-worker|worker-frames|docker-default|worker-docker|stores
import assert from "node:assert/strict";
import { appendFileSync, chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
for (const name of ["TRACE", "TRACE_STEP", "TRACE_START", "WORKER_ENDPOINT", "HOME", "STORE_PATH", "IMAGE", "DAEMON", "VOLUME"]) {
  delete process.env[`LAYERFS_${name}`];
  delete process.env[`DSH_LAYERFS_${name}`];
}
const { LayerFSRuntime } = await import("@layerfs/runtime");
const native = (await import(FAKE)).default;
const selected = process.argv[2];
const work = mkdtempSync(join(tmpdir(), "layerfs-runtime-"));
const home = join(work, "home");
const IMAGE = `sha256:${"a".repeat(64)}`;
/** A runtime on Linux whose Docker CLI calls go to `docker(args)`; `calls` lists them. */
async function opened(docker = () => "", settings = {}) {
  const runtime = await LayerFSRuntime.open({ home, image: IMAGE, daemon: "/bin/daemon", volumes: join(work, "volumes"), platform: "linux", sweep: false, ...settings });
  const calls = [];
  runtime.platform = { ...runtime.platform, docker: async (_settings, args) => { calls.push(args); return docker(args); } };
  return { runtime, calls };
}
const project = (name) => { mkdirSync(join(home, name), { recursive: true }); writeFileSync(join(home, name, "project.record"), "x"); };

try {
  if (selected === "surface") {
    assert.throws(() => new LayerFSRuntime({ home }), { code: "InvalidInput", message: /LayerFSRuntime\.open/ });
    await assert.rejects(LayerFSRuntime.open({}), { code: "InvalidInput", message: /home/ });
    await assert.rejects(LayerFSRuntime.open({ home: "relative" }), { code: "InvalidInput" });
    const { runtime } = await opened((args) => (args[0] === "version" ? "27.0.1" : ""));
    const methods = (group) => Object.keys(group).sort();
    assert.deepEqual(methods(runtime.project), ["get_init", "get_state", "list", "remove", "start_init"]);
    assert.deepEqual(methods(runtime.sandbox), ["clean_leftovers", "list", "list_leftovers", "reclaim", "sweep"]);
    assert.deepEqual(methods(runtime.volume), ["list", "remove"]);
    assert.deepEqual(methods(runtime.worker), ["restart", "start"]);
    assert.deepEqual(methods(runtime.events), ["subscribe"]);
    const state = await runtime.status();
    assert.deepEqual([state.platform, state.missing, state.docker, state.worker], ["linux", [], { level: "ok", version: "27.0.1" }, null]);
    // Linux has no worker; a restart needs confirm before anything else
    await assert.rejects(runtime.worker.start(), { code: "InvalidInput", message: /in-process/ });
    await assert.rejects(runtime.worker.restart(), { code: "InvalidInput", message: /confirm/ });
    await assert.rejects(runtime.worker.restart({ confirm: "restart" }), { code: "InvalidInput", message: /in-process/ });
    // missing settings are listed, not refused
    const { missing } = await import("../src/settings.js");
    assert.deepEqual(missing((await LayerFSRuntime.open({ home, platform: "linux", sweep: false })).settings), ["image", "daemon"]);
    // macOS needs no integration name: image and bundle are all it asks for
    const mac = await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle: "/b", sweep: false });
    assert.equal(process.env.LAYERFS_WORKER_ENDPOINT, undefined, "the endpoint is handed over in-process, not through the environment");
    assert.deepEqual(missing(mac.settings), []);
    assert.equal("adapter" in mac.settings, false);
    await mac.close();
    // closed: every call refused; a second close does nothing
    await runtime.close();
    await runtime.close();
    await assert.rejects(runtime.project.list(), { code: "InvalidInput", message: /closed/ });
    assert.throws(() => runtime.events.subscribe(), { code: "InvalidInput" });
  } else if (selected === "home-mode") {
    const { resolveSettings } = await import("../src/settings.js");
    const mode = (dir) => statSync(dir).mode & 0o777;
    // a new home is made private before .runtime and traces inside it
    const fresh = resolveSettings({ home, platform: "macos" });
    assert.equal(mode(home).toString(8), "700");
    assert.ok(existsSync(fresh.stateDir) && existsSync(fresh.traceDir));
    // an existing home keeps its mode: Init refuses a non-private one
    const shared = join(work, "shared");
    mkdirSync(shared);
    chmodSync(shared, 0o755);
    resolveSettings({ home: shared, platform: "macos" });
    assert.equal(mode(shared).toString(8), "755");
  } else if (selected === "projects") {
    project("alpha");
    project("beta");
    let running = "";
    const { runtime, calls } = await opened((args) => (args[0] === "ps" ? running : ""));
    mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
    writeFileSync(join(home, ".runtime", "projects", "beta.json"), JSON.stringify({ name: "beta", volume: "layerfs-beta-store-1", source: "/src/beta", created: 1 }));
    assert.deepEqual((await runtime.project.list()).map(({ name, volume, initialized }) => [name, volume, initialized]), [["alpha", null, true], ["beta", "layerfs-beta-store-1", true]]);
    // remove names what it destroys
    await assert.rejects(runtime.project.remove("beta"), { code: "InvalidInput", message: /confirm/ });
    await assert.rejects(runtime.project.remove("alpha", { confirm: "alpha" }), { code: "InvalidInput", message: /volume/ });
    await assert.rejects(runtime.project.remove("gamma", { confirm: "gamma", volume: "v" }), { code: "NoSuchProject" });
    // a running Sandbox on the volume refuses, unless asked to stop it
    running = "c1 running\nc2 exited";
    await assert.rejects(runtime.project.remove("beta", { confirm: "beta" }), { code: "Busy" });
    assert.ok(existsSync(join(home, "beta")), "a refused remove changes nothing");
    calls.length = 0;
    assert.deepEqual(await runtime.project.remove("beta", { confirm: "beta", stopSandboxes: true }), { kind: "Removed", project: "beta", volume: "layerfs-beta-store-1" });
    assert.deepEqual(calls.slice(1), [["rm", "-f", "c1", "c2"], ["volume", "rm", "layerfs-beta-store-1"]]);
    assert.ok(!existsSync(join(home, "beta")));
    assert.deepEqual((await runtime.project.list()).map((p) => p.name), ["alpha"]);
    await runtime.close();
  } else if (selected === "init") {
    const source = join(work, "source");
    mkdirSync(join(source, "src"), { recursive: true });
    writeFileSync(join(source, "README.md"), "hello\n");
    writeFileSync(join(source, "src", "a.js"), "1\n");
    const { runtime, calls } = await opened();
    // checked before anything changes
    for (const [name, folder, options, code] of [["Bad", source, {}, "InvalidInput"], ["shop", "relative", {}, "InvalidInput"], ["shop", join(work, "none"), {}, "InvalidInput"], ["shop", source, { image: "latest" }, "InvalidInput"], ...[0, -5, 2.5, "60000", null].map((timeout) => ["shop", source, { timeout }, "InvalidInput"])]) {
      await assert.rejects(runtime.project.start_init(name, folder, options), { code });
    }
    project("taken");
    await assert.rejects(runtime.project.start_init("taken", source), { code: "Exists" });
    assert.deepEqual(calls, [], "a refused Init makes no volume");
    const until = async (job) => {
      for (let i = 0; i < 200; i++) {
        const now = await runtime.project.get_init(job);
        if (now.ended) return now;
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error("the Init job did not end");
    };
    // one Init: count, volume, import, then its record
    const done = await until(await runtime.project.start_init("shop", source));
    assert.equal(done.step, "done", JSON.stringify(done.error));
    assert.deepEqual(done.totals, { entries: 4, files: 2, bytes: 8 });
    assert.equal(calls[0].join(" "), `volume create --label layerfs.role=store --label layerfs.home=${home} --label layerfs.project=shop ${calls[0].at(-1)}`);
    assert.match(calls[0].at(-1), /^layerfs-shop-store-[0-9a-z]+$/);
    const init = native.args[native.calls.lastIndexOf("project.init")];
    assert.deepEqual([init.path, init.name, init.storePath, init.deploy.volume], [source, "shop", join(work, "volumes", calls[0].at(-1), "_data/global/store.sqlite"), calls[0].at(-1)]);
    assert.equal((await runtime.project.list()).find((p) => p.name === "shop").volume, calls[0].at(-1));
    assert.equal(init.timeoutMs, 300_000, "no timeout: LayerFS.init's 300 s default, none of the runtime's own");
    // a timeout passes through to LayerFS.init as timeoutMs; the record keeps none
    calls.length = 0;
    const timed = await until(await runtime.project.start_init("timed", source, { timeout: 300_000 }));
    assert.equal(timed.step, "done", JSON.stringify(timed.error));
    assert.equal(native.args[native.calls.lastIndexOf("project.init")].timeoutMs, 300_000);
    assert.ok(!Object.hasOwn((await runtime.project.list()).find((p) => p.name === "timed"), "timeout"));
    // a Deadline is Failed: what the Init made is removed, as for any failure
    native.initRefusal = { code: "Failed", message: "InitFailure { error: Project(Deadline), initialized: None }", detail: null };
    calls.length = 0;
    const late = await until(await runtime.project.start_init("late", source, { timeout: 1 }));
    assert.deepEqual([late.step, late.error.code], ["failed", "Failed"]);
    assert.deepEqual(calls.map((c) => c.slice(0, 2)), [["volume", "create"], ["volume", "rm"]]);
    delete native.initRefusal;
    // a failed Init removes what it made; an uncertain one keeps it and is recorded
    native.initRefusal = { code: "InvalidInput", message: "Mode", detail: null };
    calls.length = 0;
    const failed = await until(await runtime.project.start_init("bad", source));
    assert.deepEqual([failed.step, failed.error.code], ["failed", "InvalidInput"]);
    assert.deepEqual(calls.map((c) => c.slice(0, 2)), [["volume", "create"], ["volume", "rm"]]);
    native.initRefusal = { code: "Unknown", message: "lost", detail: null };
    calls.length = 0;
    const unsure = await until(await runtime.project.start_init("unsure", source));
    assert.equal(unsure.error.code, "Unknown");
    assert.deepEqual(calls.map((c) => c.slice(0, 2)), [["volume", "create"]], "nothing is removed on a guess");
    assert.equal(JSON.parse(readFileSync(join(home, ".runtime", "init-failures.jsonl"), "utf8").trim()).project.name, "unsure");
    delete native.initRefusal;
    await runtime.close();
  } else if (selected === "state") {
    const { runtime } = await opened();
    project("shop");
    mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
    writeFileSync(join(home, ".runtime", "projects", "shop.json"), JSON.stringify({ name: "shop", volume: "vol-shop" }));
    native.sandboxList = [{ sandbox: "1", name: "layerfs-shop-1", image: IMAGE, workspaces: 1 }];
    native.workspaceList = [{ workspace: "w1", branch: "lane", mounted: true, directory: "/workspaces/w1", activity: "idle" }];
    const before = native.calls.length;
    const state = await runtime.project.get_state("shop");
    assert.deepEqual(native.calls.slice(before), ["project.list_branches", "backend.store.project.get_history", "project.list_sandboxes", "sandbox.list_workspaces", "backend.daemon.workspace.get_storage"]);
    assert.equal(native.args[before].storePath, join(work, "volumes", "vol-shop", "_data/global/store.sqlite"));
    assert.deepEqual([state.kind, state.sandboxes.map((s) => s.id), state.sandboxes[0].workspaces[0].path, state.gone], ["ok", ["1"], "/workspaces/w1", []]);
    assert.deepEqual(await runtime.project.get_state("shop", state.version), { kind: "unchanged", version: state.version });
    // the client is kept between reads
    assert.equal(native.calls.filter((c) => c === "project.list_branches").length, 1);
    await assert.rejects(runtime.project.get_state("other"), { code: "NoSuchProject" });
    // reclaim goes to the runtime's own native call
    await runtime.sandbox.reclaim("shop");
    assert.equal(native.calls.at(-1), "runtime.sandbox.reclaim");
    await runtime.close();
    assert.deepEqual(native.stores.at(-1)[0], "close", "close releases the clients");
  } else if (selected === "events") {
    const { runtime } = await opened(undefined, { pollMs: 20 });
    const traces = runtime.settings.traceDir;
    const line = (method, project, start) => JSON.stringify({ layerfs_trace: 1, id: "x-1", method, project, start, phase: "end", ok: true }) + "\n";
    writeFileSync(join(traces, "agent-a.jsonl"), line("fs.read", "shop", "2026-10-23T08:00:00.000Z") + line("fs.read", "other", "2026-10-23T08:00:01.000Z"));
    const old = runtime.events.subscribe({ project: "shop", since: "2026-10-23T07:00:00Z" });
    const first = await old.next();
    assert.deepEqual([first.value.actor, first.value.method, first.value.project], ["agent-a", "fs.read", "shop"]);
    old.close();
    assert.equal((await old.next()).done, true);
    // without since: only what is written from now on, from every file
    const live = runtime.events.subscribe();
    appendFileSync(join(traces, "agent-a.jsonl"), line("workspace.mount", "shop", new Date().toISOString()));
    writeFileSync(join(traces, "agent-b.jsonl"), "not json\n" + line("branch.fork", "other", new Date().toISOString()));
    const seen = [];
    for await (const event of live) {
      seen.push(`${event.actor}:${event.method}`);
      if (seen.length === 2) break;
    }
    assert.deepEqual(seen.sort(), ["agent-a:workspace.mount", "agent-b:branch.fork"]);
    assert.throws(() => runtime.events.subscribe({ since: "yesterday" }), { code: "InvalidInput" });
    await runtime.close();
  } else if (selected === "leftovers") {
    project("shop");
    const rows = ["layerfs-shop-1\trunning\tUp\tshop\t1\timg", "layerfs-shop-2\texited\tExited (0)\tshop\t2\timg", "layerfs-gone-1\texited\tExited\tgone\t1\timg", "other-x-1\texited\tExited\tx\t1\timg"].join("\n");
    const { runtime, calls } = await opened((args) => (args[0] === "ps" ? rows : ""));
    assert.deepEqual((await runtime.sandbox.list_leftovers()).map((c) => c.name), ["layerfs-shop-2", "layerfs-gone-1"]);
    const { results } = await runtime.sandbox.clean_leftovers(["layerfs-shop-2", "layerfs-shop-1", "other-x-1"]);
    assert.deepEqual(results.map((r) => [r.name, r.ok]), [["layerfs-shop-2", true], ["layerfs-shop-1", false], ["other-x-1", false]]);
    assert.deepEqual(calls.filter((c) => c[0] === "rm"), [["rm", "layerfs-shop-2"]]);
    await assert.rejects(runtime.sandbox.clean_leftovers([]), { code: "InvalidInput" });
    await runtime.close();
  } else if (selected === "sandboxes") {
    // runtime.sandbox.list: this home's containers with their scratch bytes; sweep reclaims stopped ones per project
    project("shop");
    mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
    writeFileSync(join(home, ".runtime", "projects", "shop.json"), JSON.stringify({ name: "shop", volume: "vol-shop" }));
    const inspected = (name, project, status, size, ports = {}) => ({ Name: `/${name}`, State: { Status: status, Running: status === "running", StartedAt: "2026-10-24T01:00:00Z", FinishedAt: status === "running" ? "0001-01-01T00:00:00Z" : "2026-10-24T02:00:00Z" }, Config: { Image: IMAGE, Labels: { "layerfs.role": "sandbox", "layerfs.project": project, "layerfs.sandbox": name.at(-1) } }, NetworkSettings: { Ports: ports }, SizeRw: size });
    const ps = ["layerfs-shop-1\trunning\tUp\tshop\t1\timg", "layerfs-shop-2\texited\tExited (0)\tshop\t2\timg", "layerfs-shop-3\tcreated\tCreated\tshop\t3\timg", "layerfs-gone-1\texited\tExited\tgone\t1\timg", "layerfs-issue9-shop-1\texited\tExited\tshop\t1\timg"].join("\n");
    const { runtime, calls } = await opened((args) => {
      if (args[0] === "ps") return args.includes("-q") ? "id1\nid2\nid3" : ps;
      if (args[0] === "inspect") {
        const error = new Error("Error: No such object: id3");
        error.stdout = JSON.stringify([inspected("layerfs-shop-1", "shop", "running", 4096, { "30421/tcp": [{ HostIp: "127.0.0.1", HostPort: "54321" }] }), inspected("layerfs-gone-1", "gone", "exited", 1_000_000), inspected("other-x-1", "x", "exited", 1)]);
        throw error; // id3 went away between ps and inspect: the others are still listed
      }
      return "";
    });
    const listed = await runtime.sandbox.list();
    assert.deepEqual(listed.map((c) => [c.name, c.state, c.scratch, c.owned]), [["layerfs-gone-1", "exited", 1_000_000, false], ["layerfs-shop-1", "running", 4096, true]]);
    assert.deepEqual([listed[1].ports, listed[1].finishedAt, listed[0].finishedAt], [[{ container: "30421/tcp", host: "127.0.0.1", port: 54321 }], null, "2026-10-24T02:00:00Z"]);
    assert.deepEqual(calls.find((c) => c[0] === "inspect"), ["inspect", "--size", "id1", "id2", "id3"]);
    // sweep: only shop has an ended container of this home's own naming; created ones and other projects are left alone
    native.reclaimed = ["2"];
    const before = native.calls.length;
    assert.deepEqual(await runtime.sandbox.sweep(), { reclaimed: { shop: ["2"] }, failed: [] });
    assert.deepEqual(native.calls.slice(before).filter((c) => c.startsWith("runtime.")), ["runtime.sandbox.reclaim"]);
    assert.equal(calls.filter((c) => c[0] === "rm").length, 0, "the sweep deletes through the project's records, never by name");
    delete native.reclaimed;
    await runtime.close();
    // open sweeps once in the background; status reports it and close waits for it
    const swept = await LayerFSRuntime.open({ home, image: IMAGE, daemon: "/bin/daemon", volumes: join(work, "volumes"), platform: "linux" });
    swept.platform = { ...swept.platform, docker: async (_s, args) => (args[0] === "version" ? "27.0.1" : args[0] === "ps" ? ps : "") };
    assert.equal((await swept.status()).sweep.state, "running");
    await swept.close();
    assert.equal(swept.swept.state, "done");
    assert.deepEqual(swept.swept.failed, []);
    const off = await LayerFSRuntime.open({ home, platform: "linux", sweep: false });
    assert.equal(off.swept, null);
    await off.close();
  } else if (selected === "volumes") {
    // owner from the Init labels or the project record; only this home's orphans are removable
    project("shop");
    project("unsure"); // an uncertain Init: home entry, no record
    mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
    writeFileSync(join(home, ".runtime", "projects", "shop.json"), JSON.stringify({ name: "shop", volume: "layerfs-shop-store-2" }));
    const vol = (Name, Size, Links = "0", Labels = "") => ({ Name, Size, Links, Labels });
    const df = { Volumes: [
      vol("layerfs-shop-store-2", "84.05MB", "1"), // recorded, unlabelled (made before labels)
      vol("layerfs-shop-store-1", "1.5GB", "0", "x"), // labelled shop of this home, no longer its volume
      vol("layerfs-old-store-1", "2kB", "1", "x"), // orphan still used by a container
      vol("layerfs-unsure-store-1", "10MB", "0", "x"), // the uncertain Init's
      vol("mocktest-mock-shop-store-1", "3GB", "0", "x"), // another home's
      vol("layerfs-issue352-p3", "N/A"), // no label, no record
    ] };
    const labels = (name, h = home) => ({ "layerfs.role": "store", "layerfs.home": h, "layerfs.project": name });
    const inspect = { "layerfs-shop-store-1": labels("shop"), "layerfs-old-store-1": labels("old"), "layerfs-unsure-store-1": labels("unsure"), "mocktest-mock-shop-store-1": labels("shop", "/elsewhere") };
    const { runtime, calls } = await opened((args) => {
      if (args[0] === "system") return JSON.stringify(df);
      if (args[0] === "volume" && args[1] === "inspect") return JSON.stringify(args.slice(2).map((Name) => ({ Name, Labels: inspect[Name] })));
      return "";
    });
    const listed = await runtime.volume.list();
    assert.deepEqual(listed.map((v) => [v.name, v.owner, v.project, v.live, v.removable, v.size, v.containers]), [
      ["layerfs-issue352-p3", "unknown", null, false, false, null, 0],
      ["layerfs-old-store-1", "this", "old", false, false, 2000, 1],
      ["layerfs-shop-store-1", "this", "shop", false, true, 1_500_000_000, 0],
      ["layerfs-shop-store-2", "this", "shop", true, false, 84_050_000, 1],
      ["layerfs-unsure-store-1", "this", "unsure", true, false, 10_000_000, 0],
      ["mocktest-mock-shop-store-1", "other", "shop", false, false, 3_000_000_000, 0],
    ]);
    const { results } = await runtime.volume.remove(["layerfs-shop-store-1", "layerfs-shop-store-2", "layerfs-old-store-1", "layerfs-unsure-store-1", "mocktest-mock-shop-store-1", "layerfs-issue352-p3", "nope"]);
    assert.deepEqual(results.map((r) => [r.name, r.ok]), [["layerfs-shop-store-1", true], ["layerfs-shop-store-2", false], ["layerfs-old-store-1", false], ["layerfs-unsure-store-1", false], ["mocktest-mock-shop-store-1", false], ["layerfs-issue352-p3", false], ["nope", false]]);
    assert.match(results[1].message, /remove the project/);
    assert.match(results[4].message, /another LayerFS home/);
    assert.deepEqual(calls.filter((c) => c[1] === "rm"), [["volume", "rm", "layerfs-shop-store-1"]]);
    await assert.rejects(runtime.volume.remove([]), { code: "InvalidInput" });
    await runtime.close();
  } else if (selected === "transport-order") {
    // an agent client made before the runtime opens keeps its in-process
    // binding; the runtime's clients, and any made after it, use the worker
    const { LayerFS } = await import("@layerfs/agent");
    const { fakeWorker } = await import("../../agent/test/fixture/fake-worker.mjs");
    const worker = await fakeWorker((request) => ({ value: request.op === "backend.store.project.get_history" ? { kind: "ok", version: "h1", branches: [], commits: [], more: {}, layers: [] } : { items: [], hasMore: false } }));
    try {
      process.env.LAYERFS_STORE_PATH = "/fake/store.sqlite";
      const early = await LayerFS.open("website");
      delete process.env.LAYERFS_STORE_PATH;
      mkdirSync(join(home, "worker"), { recursive: true });
      worker.endpoint(join(home, "worker", "endpoint.json"), home);
      const runtime = await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle: "/b" });
      assert.equal(process.env.LAYERFS_WORKER_ENDPOINT, undefined, "the runtime does not change the environment");
      project("shop");
      mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
      writeFileSync(join(home, ".runtime", "projects", "shop.json"), JSON.stringify({ name: "shop", volume: "vol-shop" }));
      const state = await runtime.project.get_state("shop");
      assert.equal(state.kind, "ok");
      assert.deepEqual(worker.calls.map((c) => c.op), ["project.list_branches", "backend.store.project.get_history", "project.list_sandboxes"]);
      assert.equal(worker.calls[0].args.storePath, "/var/lib/docker/volumes/vol-shop/_data/global/store.sqlite");
      const local = native.calls.length;
      await early.fs.read("main", "/README.md");
      assert.equal(native.calls.length, local + 1, "the earlier client stays in-process");
      process.env.LAYERFS_STORE_PATH = "/fake/store.sqlite";
      const late = await LayerFS.open("website");
      delete process.env.LAYERFS_STORE_PATH;
      assert.equal(worker.calls.at(-1).op, "project.list_branches", "a client made after the runtime uses the worker");
      await late.close();
      await early.close();
      await runtime.close();
    } finally {
      worker.close();
    }
  } else if (selected === "macos-worker") {
    // the macOS worker's start with a stubbed docker CLI and a stand-in worker
    const { fakeWorker } = await import("../../agent/test/fixture/fake-worker.mjs");
    const macos = (await import("../src/platform/macos/index.js")).default;
    const worker = await fakeWorker(undefined, { anySecret: true });
    const bundle = join(work, "bundle");
    mkdirSync(bundle);
    writeFileSync(join(bundle, "manifest.json"), JSON.stringify({ build_id: "build-1" }));
    const calls = [];
    let started = false;
    const real = macos.docker;
    macos.docker = async (_settings, args, options = {}) => {
      calls.push({ args, env: options.env });
      const [verb] = args;
      if (verb === "inspect" && !started) throw Object.assign(new Error("inspect"), { stderr: "Error: No such object: layerfs-worker" });
      if (verb === "inspect") return `true\tbuild-1\t2026-10-23T00:00:00Z\t${home}`;
      if (verb === "run") { started = true; return "c0ffee"; }
      if (verb === "port") return `127.0.0.1:${worker.port}`;
      throw new Error(`unexpected docker ${args.join(" ")}`);
    };
    try {
      // the Mac's own socket, as Docker Desktop has it with its default socket off
      const mac = "/Users/someone/.docker/run/docker.sock";
      const runtime = await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle, docker: mac });
      const state = await runtime.worker.start();
      assert.deepEqual([state.worker.state, state.worker.build, state.worker.answering], ["running", "build-1", true]);
      // the secret is in docker's environment and the endpoint file, never on its command line
      const run = calls.find((c) => c.args[0] === "run");
      const secret = run.env.LAYERFS_WORKER_SECRET;
      assert.match(secret, /^[0-9a-f]{64}$/);
      assert.ok(run.args.includes("LAYERFS_WORKER_SECRET"));
      assert.ok(!calls.some((c) => c.args.some((arg) => arg.includes(secret))), "no docker argument carries the secret");
      assert.deepEqual(worker.secrets, [secret, secret], "readiness and status pings used it");
      const endpoint = JSON.parse(readFileSync(join(home, "worker", "endpoint.json"), "utf8"));
      assert.deepEqual([endpoint.port, endpoint.secret, endpoint.build, endpoint.daemonHost], [worker.port, secret, "build-1", "host.docker.internal"]);
      assert.ok(!endpoint.init.argv.some((arg) => arg.includes(secret)), "Init's container gets no secret");
      // the Mac's docker CLI uses the Mac's socket; inside the VM, the worker and
      // Init mount and use the VM's own, and no Mac socket path reaches them
      const vm = "/var/run/docker.sock";
      assert.equal(endpoint.docker, vm);
      assert.deepEqual(endpoint.init.argv.slice(0, 2), ["-H", `unix://${mac}`]);
      for (const argv of [run.args, endpoint.init.argv.slice(2)]) {
        assert.ok(argv.includes(`${vm}:${vm}`), "the VM's socket is mounted");
        assert.ok(!argv.some((arg) => arg.includes(mac)), "no Mac socket path inside the VM");
      }
      assert.equal("oldStore" in (await runtime.status()), false, "no older Store container is looked for");
      // nothing but the worker is started or removed
      assert.deepEqual(calls.filter((c) => ["stop", "rm", "run"].includes(c.args[0])).map((c) => c.args[0]), ["run"]);
      await runtime.close();
    } finally {
      macos.docker = real;
      worker.close();
    }
  } else if (selected === "worker-frames") {
    // the real worker/server.js on a stand-in sealed bundle and binding: the
    // unauthenticated first frame is capped, a silent connection ends after
    // 5 s, a wrong secret is cut off, and trusted frames have no cap
    const { spawn } = await import("node:child_process");
    const { connect } = await import("node:net");
    const { randomBytes } = await import("node:crypto");
    const { frame, readFrames } = await import("@layerfs/agent/internal");
    const bundle = join(work, "bundle");
    mkdirSync(bundle);
    writeFileSync(join(bundle, "manifest.json"), JSON.stringify({ status: "sealed", platform: "linux", build_id: "build-f", files: {} }));
    const secret = randomBytes(32).toString("hex");
    const server = spawn(process.execPath, [fileURLToPath(new URL("../src/worker/server.js", import.meta.url))], {
      env: { ...process.env, LAYERFS_WORKER_SECRET: secret, LAYERFS_HOME: home, LAYERFS_BUNDLE: bundle, LAYERFS_NATIVE: FAKE, LAYERFS_WORKER_LISTEN: "127.0.0.1:0" },
      stdio: ["ignore", "ignore", "pipe"],
    });
    try {
      const port = await new Promise((resolve, reject) => {
        let text = "";
        server.stderr.on("data", (chunk) => { text += chunk; const found = /listening on 127\.0\.0\.1:(\d+)/.exec(text); if (found) resolve(Number(found[1])); });
        server.on("exit", (code) => reject(new Error(`the worker ended (${code}): ${text}`)));
      });
      /** Opens a connection, sends `bytes`, and answers when it closes (ms after open) or with the frames it got. */
      const session = (bytes, after = () => {}) => new Promise((resolve) => {
        const socket = connect({ host: "127.0.0.1", port }), got = [], opened = Date.now();
        socket.on("connect", () => bytes && socket.write(bytes));
        readFrames(socket, (value) => { got.push(value); after(value, socket, got); }, () => {});
        socket.on("error", () => {});
        socket.on("close", () => resolve({ ms: Date.now() - opened, got }));
      });
      const header = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
      // a first frame over 4 KiB is refused as soon as its length arrives
      const big = await session(Buffer.concat([header(1 << 20), Buffer.alloc(16)]));
      assert.ok(big.ms < 2000 && big.got.length === 0, `closed after ${big.ms} ms`);
      // a wrong secret is cut off
      assert.deepEqual((await session(frame({ secret: "0".repeat(64) }))).got, []);
      // a connection that sends nothing ends after 5 s
      const quiet = await session(null);
      assert.ok(quiet.ms >= 4500 && quiet.ms < 9000, `closed after ${quiet.ms} ms`);
      // the secret, then a ping and a 200 KiB call, which no cap refuses
      const trusted = await session(frame({ secret }), (value, socket, got) => {
        if (value.id === 0) socket.write(Buffer.concat([frame({ id: 1, fn: "ping" }), frame({ id: 2, fn: "call", op: "fs.write", args: { home, path: "/big", content: "x".repeat(200 * 1024) } })]));
        if (got.length === 3) socket.end();
      });
      assert.deepEqual(trusted.got.map((v) => v.id), [0, 1, 2]);
      assert.deepEqual([trusted.got[1].answer.value.ready, trusted.got[1].answer.value.build], [true, "build-f"]);
      assert.ok(trusted.got[2].answer.value, "the large trusted call was made");
    } finally {
      server.kill("SIGTERM");
    }
  } else if (selected === "docker-default") {
    // the socket when none is given: /var/run/docker.sock, except on macOS
    // when only Docker Desktop's ~/.docker/run/docker.sock exists
    const { resolveSettings } = await import("../src/settings.js");
    const user = join(work, "user"), desktop = join(user, ".docker", "run", "docker.sock");
    mkdirSync(join(user, ".docker", "run"), { recursive: true });
    const saved = process.env.HOME;
    process.env.HOME = user;
    try {
      const docker = (platform, given = {}) => resolveSettings({ home, platform, ...given }).docker;
      assert.equal(docker("macos"), "/var/run/docker.sock", "no socket at either path: the usual one");
      writeFileSync(desktop, "");
      const fallback = existsSync("/var/run/docker.sock") ? "/var/run/docker.sock" : desktop;
      assert.equal(docker("macos"), fallback);
      assert.equal(docker("linux"), "/var/run/docker.sock", "Linux keeps the usual path");
      assert.equal(docker("macos", { docker: "/given.sock" }), "/given.sock", "a given socket wins");
      console.log(`docker-default: macOS default is ${fallback === desktop ? "Docker Desktop's socket (/var/run/docker.sock absent)" : "/var/run/docker.sock (present here)"}`);
    } finally {
      process.env.HOME = saved;
    }
  } else if (selected === "worker-docker") {
    // the worker client replaces a call's deploy.docker with the endpoint's
    // (the worker's socket inside the VM), for worker calls and for Init's
    // own container; an endpoint without one (Windows) leaves it as given
    const { fakeWorker } = await import("../../agent/test/fixture/fake-worker.mjs");
    const { workerClient } = await import("../../agent/src/transport/worker_client.js");
    const worker = await fakeWorker(() => ({ value: { ok: true } }));
    const mac = "/Users/someone/.docker/run/docker.sock", vm = "/var/run/docker.sock";
    const deploy = { docker: mac, image: IMAGE, volume: "vol", daemon: "/b/layerfs-daemon" };
    // Init's container: a stand-in `docker` that records the request it gets on stdin
    const bin = join(work, "bin"), got = join(work, "init-request");
    mkdirSync(bin);
    const frames = new URL("../../agent/src/transport/frames.js", import.meta.url).href;
    const stub = [
      `#!${process.execPath}`,
      "(async () => {",
      `  const { decode, encode } = await import(${JSON.stringify(frames)});`,
      "  const chunks = [];",
      "  for await (const chunk of process.stdin) chunks.push(chunk);",
      `  require("node:fs").writeFileSync(${JSON.stringify(got)}, JSON.stringify({ argv: process.argv.slice(2), request: decode(Buffer.concat(chunks)) }));`,
      '  process.stdout.write(encode({ value: { kind: "Initialized" } }));',
      "})();",
    ];
    writeFileSync(join(bin, "docker"), `${stub.join("\n")}\n`, { mode: 0o755 });
    const path = process.env.PATH;
    process.env.PATH = `${bin}:${path}`;
    try {
      const file = join(work, "endpoint.json");
      writeFileSync(file, JSON.stringify({ home, port: worker.port, secret: worker.secret, build: "fake", daemonHost: "host.docker.internal", docker: vm, init: { argv: ["-H", `unix://${mac}`, "run"], image: IMAGE, script: "/b/init.js" } }));
      const client = workerClient(file);
      await client.call("sandbox.create", { home, storePath: "/s", deploy });
      await client.call("fs.read", { home, storePath: "/s", path: "/README.md" });
      await client.backendCall("backend.daemon.workspace.status", { home, storePath: "/s", deploy: { ...deploy, daemonHost: "given.host" } });
      assert.deepEqual(worker.calls.map((c) => c.args.deploy?.docker), [vm, vm, vm], "every worker call uses the VM's socket");
      assert.equal(worker.calls[0].args.deploy.image, IMAGE, "the rest of deploy is kept");
      assert.equal(worker.calls[2].args.deploy.daemonHost, "given.host", "a given daemonHost still wins");
      assert.deepEqual(await client.call("project.init", { home, storePath: "/s", path: work, deploy }), { value: { kind: "Initialized" } });
      const init = JSON.parse(readFileSync(got, "utf8"));
      assert.deepEqual(init.argv.slice(0, 2), ["-H", `unix://${mac}`], "the Mac's docker CLI uses the Mac's socket");
      assert.equal(init.request.args.deploy.docker, vm, "Init's container uses the VM's socket");
      // Windows: the worker shares its distribution's engine, so the given socket stays
      const windows = join(work, "windows.json");
      writeFileSync(windows, JSON.stringify({ home, port: worker.port, secret: worker.secret, build: "fake" }));
      await workerClient(windows).call("sandbox.create", { home, storePath: "/s", deploy: { ...deploy, docker: "/run/docker.sock" } });
      assert.equal(worker.calls.at(-1).args.deploy.docker, "/run/docker.sock");
    } finally {
      process.env.PATH = path;
      worker.close();
    }
  } else if (selected === "stores") {
    // #506 Store-first: a home holds many Stores, a Store many projects
    project("alpha");
    mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
    writeFileSync(join(home, ".runtime", "projects", "alpha.json"), JSON.stringify({ name: "alpha", volume: "layerfs-alpha-store-1", source: "/src/alpha", created: 1 }));
    let sandboxRows = "";
    const { runtime, calls } = await opened((args) => {
      if (args[0] === "volume" && args[1] === "inspect") throw Object.assign(new Error("no such volume"), { stderr: "Error: No such volume" });
      if (args[0] === "ps" && args.includes("label=layerfs.role=sandbox")) return sandboxRows;
      return "";
    });
    const methods = (group) => Object.keys(group).sort();
    assert.deepEqual(methods(runtime.store), ["attach_postgresql", "check", "create", "list", "remove", "start_import_postgresql", "start_load", "start_publish", "start_pull", "start_push", "start_send", "unlink"]);
    assert.deepEqual(methods(runtime.remote), ["add", "close_view", "list", "list_stores", "open_view", "providers", "remove", "view_stats"]);
    assert.deepEqual(methods(runtime.postgresql), ["status"]);
    assert.deepEqual(methods(runtime.sync), ["get"]);
    const ended = async (job) => {
      for (let i = 0; i < 200; i++) {
        const now = (await runtime.sync.get(job.id)) ?? (await runtime.project.get_init(job.id));
        if (now.ended) return now;
        await new Promise((next) => setTimeout(next, 10));
      }
      throw new Error(`job ${job.id} did not end`);
    };
    const nativeArgs = (op) => native.args[native.calls.lastIndexOf(op)];
    const work_dir = () => runtime.settings.volumes;
    // a new empty Store: its own labelled volume, then the binding's create there
    const work = await runtime.store.create("work");
    assert.deepEqual([work.name, work.kind, work.volume, work.projects], ["work", "sqlite", "layerfs-store-work", []]);
    assert.ok(calls.some((a) => a.join(" ") === `volume create --label layerfs.role=store --label layerfs.home=${home} --label layerfs.store=work layerfs-store-work`));
    assert.equal(nativeArgs("runtime.store.create").storePath, join(work_dir(), "layerfs-store-work", "_data/global/store.sqlite"));
    await assert.rejects(runtime.store.create("work"), { code: "Exists" });
    await assert.rejects(runtime.store.create("alpha"), { code: "Exists" }, "a project's own Store has its name");
    // the project made by its own Init is a Store holding it
    assert.deepEqual((await runtime.store.list()).map((s) => [s.name, s.legacy, s.projects]), [["alpha", true, ["alpha"]], ["work", false, []]]);
    // #506 E1/E2: sizes, open state and heads, from one stat call for every SQLite Store with a volume
    const stat = nativeArgs("runtime.store.stat");
    assert.deepEqual(stat.stores, [{ store: "alpha", storePath: join(work_dir(), "layerfs-alpha-store-1", "_data/global/store.sqlite") }, { store: "work", storePath: join(work_dir(), "layerfs-store-work", "_data/global/store.sqlite") }]);
    const sized = (await runtime.store.list()).find((s) => s.name === "work");
    assert.deepEqual([sized.bytes, sized.open, sized.heads, sized.importedFrom], [{ file: 4096, wal: 512 }, false, "11".repeat(32), null]);
    // a stat call that fails leaves the fields null; the list still answers
    process.env.CLI_FAKE_REFUSE = "runtime.store.stat";
    assert.deepEqual((await runtime.store.list()).map((s) => [s.name, s.bytes, s.open, s.heads]), [["alpha", null, null, null], ["work", null, null, null]]);
    delete process.env.CLI_FAKE_REFUSE;
    // two projects into one Store: no volume each, the binding adds each stack
    const folder = join(tmpdir(), `layerfs-runtime-src-${process.pid}`);
    mkdirSync(join(folder, "src"), { recursive: true });
    writeFileSync(join(folder, "src", "a.txt"), "a");
    const volumesBefore = calls.filter((a) => a[0] === "volume" && a[1] === "create").length;
    for (const name of ["shop", "api"]) {
      const job = await runtime.project.start_init(name, folder, { store: "work" });
      assert.equal((await ended(job)).step, "done", name);
      assert.deepEqual([nativeArgs("runtime.store.add_project").store, nativeArgs("runtime.store.add_project").name], ["work", name]);
      assert.ok(nativeArgs("runtime.store.add_project").timeoutMs > 0);
    }
    assert.equal(calls.filter((a) => a[0] === "volume" && a[1] === "create").length, volumesBefore, "no volume per added project");
    await assert.rejects(runtime.project.start_init("shop", folder, { store: "work" }), { code: "Exists" });
    await assert.rejects(runtime.project.start_init("x", folder, { store: "nowhere" }), { code: "NoSuchStore" });
    // a second project in the project's own Store
    assert.equal((await ended(await runtime.project.start_init("web", folder, { store: "alpha" }))).step, "done");
    assert.deepEqual((await runtime.store.list()).map((s) => [s.name, s.projects]), [["alpha", ["alpha", "web"]], ["work", ["shop", "api"]]]);
    const listed = (await runtime.project.list()).map((p) => [p.name, p.store, p.shared]);
    assert.deepEqual(listed, [["alpha", "alpha", false], ["api", "work", true], ["shop", "work", true], ["web", "alpha", true]]);
    // remotes: references only, never keys
    await assert.rejects(runtime.remote.add({ name: "minio-a", provider: "minio", endpoint: "http://h:9000", bucket: "b", credentials: { accessKey: "k", secretKey: "s" } }), { code: "InvalidInput" });
    assert.equal((await runtime.remote.add({ name: "minio-a", provider: "minio", endpoint: "http://h:9000", bucket: "b" })).credentials, "env:LAYERFS_REMOTE");
    await assert.rejects(runtime.remote.remove("minio-a"), { code: "InvalidInput", message: /confirm/ });
    // #509: an unlinked Store has nothing to push to; push and pull take no remote
    await assert.rejects(runtime.store.start_push("work", {}), { code: "NotLinked" });
    await assert.rejects(runtime.store.start_push("work", { remote: "minio-a" }), { code: "InvalidInput", message: /link/ });
    await assert.rejects(runtime.store.check("work", {}), { code: "NotLinked" });
    // publish: one job, keys for this call only, a new file (default the Store's name) linked
    await assert.rejects(runtime.store.start_publish("work", { remote: "minio-a", file: ".bad" }), { code: "InvalidInput" });
    const publish = await runtime.store.start_publish("work", { remote: "minio-a", credentials: { accessKey: "AK", secretKey: "SECRET-1" } });
    assert.deepEqual([publish.kind, publish.store, publish.remote, publish.file, publish.step], ["publish", "work", "minio-a", "work", "running"]);
    const published = await ended(publish);
    assert.deepEqual([published.step, published.bytes, published.result.kind], ["done", 8192, "Published"], JSON.stringify(published.error));
    assert.deepEqual([nativeArgs("runtime.store.publish").remote, nativeArgs("runtime.store.publish").file, nativeArgs("runtime.store.publish").credentials.secretKey], ["minio-a", "work", "SECRET-1"]);
    assert.ok(!JSON.stringify(published).includes("SECRET-1"), "a job keeps no key");
    assert.equal(nativeArgs("runtime.store.publish").job, publish.id, "the native call reports to the job's progress cell");
    const afterPublish = (await runtime.store.list()).find((s) => s.name === "work");
    assert.deepEqual([afterPublish.link.remote, afterPublish.link.file, typeof afterPublish.link.linked, "copies" in afterPublish, "remote" in afterPublish], ["minio-a", "work", "number", false, false]);
    assert.deepEqual(afterPublish.link.last, { direction: "push", at: 1, bytes: 8192, sha256: "ab".repeat(32), heads: "11".repeat(32) });
    // one link per Store; one Store per file
    await assert.rejects(runtime.store.start_publish("work", { remote: "minio-a", file: "other" }), { code: "Linked" });
    await assert.rejects(runtime.store.start_publish("alpha", { remote: "minio-a", file: "work" }), { code: "Exists", message: /linked to the Store work/ });
    // push goes to the link; #506 E4: a running job's phase and bytes come from the binding's progress cell
    let release;
    native.hold = new Promise((next) => { release = next; });
    const slow = await runtime.store.start_push("work", { force: true });
    assert.deepEqual([slow.remote, slow.file, slow.phase, slow.bytes, slow.pieces], ["minio-a", "work", null, null, null]);
    native.progress = { [slow.id]: { phase: "upload", bytes: 4096, total: 8192, pieces: { done: 1, of: 2 } } };
    const sampled = await runtime.sync.get(slow.id);
    assert.deepEqual([sampled.step, sampled.phase, sampled.bytes, sampled.total, sampled.pieces], ["running", "upload", 4096, 8192, { done: 1, of: 2 }]);
    assert.equal(nativeArgs("runtime.sync.progress").job, slow.id);
    // a sample that fails keeps the last one
    process.env.CLI_FAKE_REFUSE = "runtime.sync.progress";
    assert.equal((await runtime.sync.get(slow.id)).phase, "upload");
    delete process.env.CLI_FAKE_REFUSE;
    release();
    native.hold = null;
    const slowDone = await ended(slow);
    assert.deepEqual([slowDone.step, slowDone.phase, slowDone.bytes, slowDone.pieces], ["done", null, 8192, null]);
    assert.deepEqual([nativeArgs("runtime.store.push").force, nativeArgs("runtime.store.push").remote, nativeArgs("runtime.store.push").file], [true, "minio-a", "work"]);
    // #506 E3/E8/E9: link check, providers and bucket listing go to the binding once each
    const checked = await runtime.store.check("work", { credentials: { accessKey: "AK", secretKey: "SECRET-2" } });
    assert.deepEqual([checked.file, checked.present, checked.bytes, checked.sha256, checked.heads, checked.modified, checked.checkedAt], ["work", true, 8192, "ab".repeat(32), "11".repeat(32), 1000, 2000]);
    assert.deepEqual([nativeArgs("runtime.store.check").store, nativeArgs("runtime.store.check").remote, nativeArgs("runtime.store.check").file, nativeArgs("runtime.store.check").credentials.secretKey], ["work", "minio-a", "work", "SECRET-2"]);
    assert.deepEqual((await runtime.remote.list_stores("minio-a")).map((c) => [c.file, c.bytes, c.linkedBy]), [["work-snap", 8192, null], ["work", 8192, "work"]]);
    assert.equal(nativeArgs("runtime.remote.list_stores").remote, "minio-a");
    await assert.rejects(runtime.remote.list_stores(""), { code: "InvalidInput" });
    assert.deepEqual((await runtime.remote.providers()).map((p) => [p.id, p.available]), [["minio", true]]);
    // pull replaces every project from the link: confirm names the Store
    await assert.rejects(runtime.store.start_pull("work", {}), { code: "InvalidInput", message: /confirm/ });
    native.pullAdded = ["cart"];
    assert.equal((await ended(await runtime.store.start_pull("work", { confirm: "work" }))).step, "done");
    const pulled = (await runtime.store.list()).find((s) => s.name === "work");
    assert.deepEqual([pulled.projects, pulled.link.last.direction, pulled.link.last.heads, pulled.link.linked], [["shop", "api", "cart"], "pull", "22".repeat(32), afterPublish.link.linked]);
    assert.deepEqual([nativeArgs("runtime.store.pull").remote, nativeArgs("runtime.store.pull").file], ["minio-a", "work"]);
    assert.ok(nativeArgs("runtime.store.pull").job);
    // send: a copy to a new file, linked to nothing; the Store's link does not change
    await assert.rejects(runtime.store.start_send("work", { remote: "minio-a", file: "work" }), { code: "Exists" });
    const sent = await ended(await runtime.store.start_send("work", { remote: "minio-a", file: "work-snap" }));
    assert.deepEqual([sent.step, sent.result.remote, sent.result.file, sent.result.bytes, sent.result.sha256, sent.result.heads], ["done", "minio-a", "work-snap", 8192, "ab".repeat(32), "11".repeat(32)]);
    assert.deepEqual((await runtime.store.list()).find((s) => s.name === "work").link, pulled.link);
    // load: a new Store and volume from a file, linked to it; a linked file is refused, naming its Store
    await assert.rejects(runtime.store.start_load("minio-a", { file: "work", store: "again" }), { code: "Linked", message: /work/ });
    await assert.rejects(runtime.store.start_load("minio-a", { file: "work-snap", store: "work" }), { code: "Exists" });
    native.hold = new Promise((next) => { release = next; });
    const loading = await runtime.store.start_load("minio-a", { file: "work-snap", store: "copy" });
    // two loads of one file: the second is refused while the first runs
    await assert.rejects(runtime.store.start_load("minio-a", { file: "work-snap", store: "copy2" }), { code: "Exists" });
    release();
    native.hold = null;
    const loaded = await ended(loading);
    assert.equal(loaded.step, "done", JSON.stringify(loaded.error));
    assert.deepEqual([nativeArgs("runtime.store.load").remote, nativeArgs("runtime.store.load").file, nativeArgs("runtime.store.load").store], ["minio-a", "work-snap", "copy"]);
    const copy = (await runtime.store.list()).find((s) => s.name === "copy");
    assert.deepEqual([copy.volume, copy.link.remote, copy.link.file, copy.link.last.direction, copy.projects], ["layerfs-store-copy", "minio-a", "work-snap", "pull", ["docs", "web"]]);
    await assert.rejects(runtime.store.start_load("minio-a", { file: "work-snap", store: "copy2" }), { code: "Linked", message: /copy/ });
    // a connection a Store links to cannot be removed
    await assert.rejects(runtime.remote.remove("minio-a", { confirm: "minio-a" }), { code: "InUse", message: /copy, work/ });
    // unlink: the Store becomes local only; the file stays
    await assert.rejects(runtime.store.unlink("copy", {}), { code: "InvalidInput", message: /confirm/ });
    assert.deepEqual(await runtime.store.unlink("copy", { confirm: "copy" }), { kind: "Unlinked", store: "copy", remote: "minio-a", file: "work-snap" });
    assert.equal(nativeArgs("runtime.store.unlink").store, "copy");
    assert.equal((await runtime.store.list()).find((s) => s.name === "copy").link, null);
    await assert.rejects(runtime.store.unlink("copy", { confirm: "copy" }), { code: "NotLinked" });
    // PostgreSQL: an import makes a schema Store; the URL is never written
    native.storeProjects = [{ name: "api", stack: "s1", headLayer: "l1" }, { name: "shop", stack: "s2", headLayer: "l2" }];
    const url = "postgresql://lfc:PG-SECRET@host.docker.internal:5432/layerfs";
    await assert.rejects(runtime.store.start_import_postgresql("work", { name: "main", connection: "pg-local", url, schema: "Bad-Schema" }), { code: "InvalidInput" });
    const imported = await ended(await runtime.store.start_import_postgresql("work", { name: "main", connection: "pg-local", url, schema: "lfc_main" }));
    assert.equal(imported.step, "done", JSON.stringify(imported.error));
    assert.equal(nativeArgs("runtime.store.import_postgresql").postgresql.url, url);
    const pg = (await runtime.store.list()).find((s) => s.name === "main");
    assert.deepEqual([pg.kind, pg.connection, pg.schema, pg.synchronousCommit, pg.projects], ["postgresql", "pg-local", "lfc_main", "off", ["api", "shop"]]);
    // #506 E6: the import's origin is recorded; a PostgreSQL Store has no file sizes
    assert.deepEqual([pg.importedFrom.store, pg.importedFrom.bytes, typeof pg.importedFrom.at, pg.bytes, pg.heads], ["work", 4096, "number", null, null]);
    assert.ok(nativeArgs("runtime.store.import_postgresql").job);
    // #506 E5/E7: one status call per connection, the URL for that call only
    await assert.rejects(runtime.postgresql.status({ schemas: ["lfc_main"] }), { code: "InvalidInput", message: /url/ });
    await assert.rejects(runtime.postgresql.status({ url, schemas: ["Bad"] }), { code: "InvalidInput" });
    const pgStatus = await runtime.postgresql.status({ url, schemas: ["lfc_main"] });
    assert.deepEqual([pgStatus.reachable, pgStatus.serverVersion, pgStatus.schemas], [true, "17.6", { lfc_main: 4096 }]);
    assert.equal(nativeArgs("runtime.postgresql.status").url, url);
    assert.ok(!readFileSync(join(home, "stores.json"), "utf8").includes("PG-SECRET"), "stores.json holds no URL");
    assert.ok(!readFileSync(join(home, "stores.json"), "utf8").includes("PG-SECRET"), "stores.json holds no URL");
    const attached = await runtime.store.attach_postgresql({ name: "main", connection: "pg-local", url, schema: "lfc_main", synchronousCommit: "local" });
    assert.equal(attached.synchronousCommit, "local");
    // the same project name in two Stores: calls name the Store
    await assert.rejects(runtime.project.get_state("shop"), { code: "InvalidInput", message: /Stores main, work/ });
    await assert.rejects(runtime.project.start_init("shop", folder, { store: "main" }), { code: "Unsupported" });
    await assert.rejects(runtime.store.start_push("main", {}), { code: "Unsupported" });
    // a PostgreSQL project's state (its Sandboxes too) takes the URL for that call only
    await assert.rejects(runtime.project.get_state("shop", undefined, { store: "main" }), { code: "InvalidInput", message: /url/ });
    await runtime.project.get_state("shop", undefined, { store: "main", url });
    assert.equal(nativeArgs("project.list_sandboxes").postgresql.url, url);
    assert.equal(nativeArgs("project.list_sandboxes").storePath, "postgresql:main");
    // removing one project of a shared Store drops its stack; the Store and its other projects stay
    sandboxRows = "layerfs-work.api-1\trunning\tUp 1 minute\tapi\t1\timg\twork\nlayerfs-work.shop-1\trunning\tUp\tshop\t1\timg\twork\n";
    await assert.rejects(runtime.project.remove("api", { store: "work", confirm: "api" }), { code: "Busy", message: /1 running Sandbox/ });
    sandboxRows = "layerfs-work.api-1\texited\tExited (0)\tapi\t1\timg\twork\n";
    assert.deepEqual(await runtime.project.remove("api", { store: "work", confirm: "api" }), { kind: "Removed", project: "api", store: "work", stack: "5e", packsKept: true });
    assert.equal(nativeArgs("runtime.store.remove_project").project, "api");
    assert.ok(calls.some((a) => a.join(" ") === "rm layerfs-work.api-1"), "its stopped Sandbox goes with it");
    sandboxRows = "";
    assert.deepEqual((await runtime.store.list()).find((s) => s.name === "work").projects, ["shop", "cart"]);
    // a PostgreSQL Store's project: the URL is passed for the call
    await assert.rejects(runtime.project.remove("api", { store: "main", confirm: "api" }), { code: "InvalidInput", message: /url/ });
    await runtime.project.remove("api", { store: "main", confirm: "api", url });
    assert.equal(nativeArgs("runtime.store.remove_project").postgresql.url, url);
    // removing a Store names it, and takes its volume
    await assert.rejects(runtime.store.remove("copy"), { code: "InvalidInput", message: /confirm/ });
    assert.deepEqual(await runtime.store.remove("copy", { confirm: "copy" }), { kind: "Removed", store: "copy", volume: "layerfs-store-copy" });
    assert.ok(calls.some((a) => a.join(" ") === "volume rm layerfs-store-copy"));
    assert.equal(nativeArgs("runtime.store.forget").store, "copy");
    assert.deepEqual((await runtime.store.list()).map((s) => s.name), ["alpha", "main", "work"]);
    // a version 1 registry (per-remote copies) is read with no link; the first write keeps it as stores.v1.json
    const v2 = JSON.parse(readFileSync(join(home, "stores.json"), "utf8"));
    assert.equal(v2.version, 2);
    const v1 = JSON.stringify({ version: 1, stores: v2.stores.map(({ link, ...s }) => ({ ...s, remote: "minio-a", copies: [{ remote: "minio-a", pushed: null, pulled: null }] })) });
    writeFileSync(join(home, "stores.json"), v1);
    assert.deepEqual((await runtime.store.list()).map((s) => [s.name, s.link, "copies" in s]), [["alpha", null, false], ["main", null, false], ["work", null, false]]);
    await runtime.store.create("fresh");
    assert.equal(readFileSync(join(home, "stores.v1.json"), "utf8"), v1);
    assert.equal(JSON.parse(readFileSync(join(home, "stores.json"), "utf8")).version, 2);
    rmSync(folder, { recursive: true, force: true });
    await runtime.close();
  } else if (selected === "views") {
    // #510: a live read-only view of a remote Store file; its projects read through the view's store path
    const { runtime } = await opened();
    await assert.rejects(runtime.remote.open_view("minio-a", {}), { code: "InvalidInput", message: /file/ });
    const keys = { accessKey: "a", secretKey: "b" };
    const view = await runtime.remote.open_view("minio-a", { file: "work", credentials: keys });
    assert.equal(view.storePath, "remote:minio-a/work#v1");
    const opening = native.args.find((a) => a.file === "work" && a.remote === "minio-a");
    assert.deepEqual(opening.credentials, keys);
    assert.deepEqual((await runtime.project.list({ view: view.view })).map((p) => [p.name, p.kind, p.store, p.readOnly]), [["docs", "view", "view:v1", true], ["web", "view", "view:v1", true]]);
    const before = native.calls.length;
    const state = await runtime.project.get_state("web", undefined, { view: view.view });
    assert.deepEqual([state.kind, state.sandboxes, state.gone], ["ok", [], []]);
    const made = native.calls.slice(before);
    assert.ok(made.includes("backend.store.project.get_history"));
    assert.ok(!made.includes("project.list_sandboxes"), "a view lists no Sandboxes");
    assert.ok(native.stores.some(([what, path]) => what === "open" && path === view.storePath), "its client opens the view's store path");
    await assert.rejects(runtime.project.get_state("nope", undefined, { view: view.view }), { code: "NoSuchProject" });
    assert.equal((await runtime.remote.view_stats(view.view)).requests, 3);
    assert.deepEqual(await runtime.remote.close_view(view.view), { closed: true });
    assert.ok(native.stores.some(([what, path]) => what === "close" && path === view.storePath), "closing the view closes its clients");
    await assert.rejects(runtime.project.list({ view: view.view }), { code: "NoSuchView" });
    await runtime.close();
  } else {
    throw new Error("select surface, home-mode, projects, init, state, events, leftovers, sandboxes, volumes, transport-order, macos-worker, worker-frames, docker-default, worker-docker, stores or views");
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(`PASS runtime/${selected}`);

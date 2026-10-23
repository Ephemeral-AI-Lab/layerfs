// @layerfs/runtime with the agent's stand-in binding and a stub Docker (#428):
// no Store, Docker or daemon. One case per command:
//   node sdk/js/packages/runtime/test/runtime.mjs surface|projects|init|state|events|leftovers
import assert from "node:assert/strict";
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
for (const name of ["TRACE", "TRACE_STEP", "TRACE_START", "WORKER_ENDPOINT", "WORKER_ADAPTER", "HOME", "STORE_PATH", "IMAGE", "DAEMON", "VOLUME"]) {
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
  const runtime = await LayerFSRuntime.open({ home, image: IMAGE, daemon: "/bin/daemon", volumes: join(work, "volumes"), platform: "linux", ...settings });
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
    assert.deepEqual(methods(runtime.sandbox), ["clean_leftovers", "list_leftovers", "reclaim"]);
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
    assert.deepEqual(missing((await LayerFSRuntime.open({ home, platform: "linux" })).settings), ["image", "daemon"]);
    // macOS without the dsh adapter: no worker route for this process
    const mac = await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle: "/b" });
    assert.equal(process.env.LAYERFS_WORKER_ENDPOINT, undefined);
    assert.deepEqual(missing(mac.settings), ["adapter"]);
    await assert.rejects(mac.worker.start(), { code: "Unsupported", message: /dsh adapter/ });
    // with it, the endpoint is this process's worker route
    await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle: "/b", adapter: "dsh" });
    assert.deepEqual([process.env.LAYERFS_WORKER_ENDPOINT, process.env.LAYERFS_WORKER_ADAPTER], [join(home, "worker", "endpoint.json"), "dsh"]);
    delete process.env.LAYERFS_WORKER_ENDPOINT;
    delete process.env.LAYERFS_WORKER_ADAPTER;
    // closed: every call refused; a second close does nothing
    await runtime.close();
    await runtime.close();
    await assert.rejects(runtime.project.list(), { code: "InvalidInput", message: /closed/ });
    assert.throws(() => runtime.events.subscribe(), { code: "InvalidInput" });
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
    for (const [name, folder, options, code] of [["Bad", source, {}, "InvalidInput"], ["shop", "relative", {}, "InvalidInput"], ["shop", join(work, "none"), {}, "InvalidInput"], ["shop", source, { image: "latest" }, "InvalidInput"]]) {
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
    assert.match(calls[0].join(" "), /^volume create layerfs-shop-store-[0-9a-z]+$/);
    const init = native.args[native.calls.lastIndexOf("project.init")];
    assert.deepEqual([init.path, init.name, init.storePath, init.deploy.volume], [source, "shop", join(work, "volumes", calls[0][2], "_data/global/store.sqlite"), calls[0][2]]);
    assert.equal((await runtime.project.list()).find((p) => p.name === "shop").volume, calls[0][2]);
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
  } else {
    throw new Error("select surface, projects, init, state, events or leftovers");
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(`PASS runtime/${selected}`);

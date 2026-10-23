// @layerfs/runtime with the agent's stand-in binding and a stub Docker (#428):
// no Store, Docker or daemon. One case per command:
//   node sdk/js/packages/runtime/test/runtime.mjs surface|projects|init|state|events|leftovers|transport-order|macos-worker|worker-frames
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
    assert.match(calls[0].join(" "), /^volume create layerfs-shop-store-[0-9a-z]+$/);
    const init = native.args[native.calls.lastIndexOf("project.init")];
    assert.deepEqual([init.path, init.name, init.storePath, init.deploy.volume], [source, "shop", join(work, "volumes", calls[0][2], "_data/global/store.sqlite"), calls[0][2]]);
    assert.equal((await runtime.project.list()).find((p) => p.name === "shop").volume, calls[0][2]);
    assert.ok(!Object.hasOwn(init, "timeoutMs"), "no timeout: Init has no limit and gets no field");
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
  } else if (selected === "transport-order") {
    // an agent client made before the runtime opens keeps its in-process
    // binding; the runtime's clients, and any made after it, use the worker
    const { LayerFS } = await import("@layerfs/agent");
    const { fakeWorker } = await import("../../agent/test/fixture/fake-worker.mjs");
    const worker = await fakeWorker((request) => ({ value: request.op === "backend.store.project.get_history" ? { kind: "ok", version: "h1", branches: [], commits: [], more: {}, layers: [] } : { items: [], hasMore: false } }));
    try {
      process.env.LAYERFS_STORE_PATH = "/fake/store.sqlite";
      const early = await LayerFS.open("website", { storePath: "/fake/store.sqlite" });
      delete process.env.LAYERFS_STORE_PATH;
      mkdirSync(join(home, "worker"), { recursive: true });
      worker.endpoint(join(home, "worker", "endpoint.json"), home);
      const runtime = await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle: "/b", adapter: "dsh" });
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
      const late = await LayerFS.open("website", { storePath: "/fake/store.sqlite" });
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
    let started = false, old = null, failStop = false;
    const real = macos.docker;
    macos.docker = async (_settings, args, options = {}) => {
      calls.push({ args, env: options.env });
      const [verb] = args;
      if (verb === "ps") return old ? `old1\t${old}` : "";
      if (verb === "stop") { if (failStop) throw Object.assign(new Error("stop"), { stderr: "cannot stop" }); old = "exited"; return "old1"; }
      if (verb === "rm") { old = null; return "old1"; }
      if (verb === "inspect" && !started) throw Object.assign(new Error("inspect"), { stderr: "Error: No such object: layerfs-worker" });
      if (verb === "inspect") return `true\tbuild-1\t2026-10-23T00:00:00Z\t${home}`;
      if (verb === "run") { started = true; return "c0ffee"; }
      if (verb === "port") return `127.0.0.1:${worker.port}`;
      throw new Error(`unexpected docker ${args.join(" ")}`);
    };
    try {
      const runtime = await LayerFSRuntime.open({ home, platform: "macos", image: IMAGE, bundle, adapter: "dsh" });
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
      assert.equal((await runtime.status()).oldStore, null);

      // the older dsh Store container: listed, refused without confirm, retired with it
      old = "running";
      started = false;
      calls.length = 0;
      const listed = (await runtime.status()).oldStore;
      assert.deepEqual([listed.name, listed.state], ["layerfs-store", "running"]);
      assert.match(listed.message, /confirm: "restart"/);
      await assert.rejects(runtime.worker.start(), { code: "InvalidInput", message: /layerfs-store still runs/ });
      assert.equal((await runtime.worker.restart({ confirm: "restart" })).worker.state, "running");
      assert.equal(old, null, "restart with confirm retired it");
      assert.deepEqual(calls.filter((c) => ["stop", "rm", "run"].includes(c.args[0])).map((c) => c.args.slice(0, 2).join(" ")), ["stop -t", "rm old1", "run -d"]);
      // a stop that fails is not followed by rm or a start, and is not retried
      old = "running";
      failStop = true;
      calls.length = 0;
      await assert.rejects(runtime.worker.start({ confirm: "restart" }), { code: "Failed", message: /was not removed/ });
      assert.deepEqual(calls.filter((c) => ["stop", "rm", "run"].includes(c.args[0])).map((c) => c.args[0]), ["stop"]);
      assert.equal(old, "running");
      // a stopped one is only listed
      old = "exited";
      assert.equal((await runtime.status()).oldStore.state, "exited");
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
  } else {
    throw new Error("select surface, projects, init, state, events, leftovers, transport-order, macos-worker or worker-frames");
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(`PASS runtime/${selected}`);

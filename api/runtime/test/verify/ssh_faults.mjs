// The `vm:ssh` failure matrix (#515 §6, V3; task H40 §5), with the stand-in
// transport of `vm_placement.mjs` and the real provider code: no host, no VM.
// Each scenario states what the user sees and what is left behind; every await
// has a deadline and the whole run has a watchdog, so a stuck provider call
// fails this file instead of hanging a shell.
//   node api/runtime/test/verify/ssh_faults.mjs [port|store|ext4|host|drop|disconnect|remote|delete]
//   (no selector: every scenario, one fresh state folder each)
import assert from "node:assert/strict";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer, connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const SELF = fileURLToPath(import.meta.url);
const ROOT = fileURLToPath(new URL("../../..", import.meta.url));
const FAKE = fileURLToPath(new URL("../../../agent/test/fixture/fake-native.cjs", import.meta.url));
const SOURCE = fileURLToPath(new URL("../../src", import.meta.url));
const SSH = fileURLToPath(new URL("../fixture/fake-ssh.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
const { LayerFSRuntime } = await import("@layerfs/runtime");
const provider = (await import("../../src/platform/vm/ssh.js")).default;
const { endpointFile, ping } = await import("../../src/platform/shared.js");
const { deploymentEnv } = await import("../../src/clients.js");

const SCENARIOS = ["port", "store", "ext4", "host", "drop", "disconnect", "remote", "delete", "shared"];

/** One await's ceiling: a stuck call fails with its own name. */
const deadline = (ms, what, promise) =>
  Promise.race([
    promise,
    new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} did not finish within ${ms / 1000} s`)), ms).unref()),
  ]);

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const sleep = delay;
const fails = (promise, matcher) =>
  promise.then((value) => assert.fail(`expected a refusal, got ${JSON.stringify(value)}`), (error) => {
    assert.match(`${error.code} ${error.message}`, matcher, `${error.code}: ${error.message}`);
    return error;
  });

/** A sealed Linux bundle with this adapter's worker and the real SDK packages. */
function bundleAt(folder) {
  rmSync(folder, { recursive: true, force: true });
  cpSync(join(SOURCE, "worker"), join(folder, "api/runtime/src/worker"), { recursive: true });
  cpSync(join(ROOT, "node_modules/@layerfs"), join(folder, "api/node_modules/@layerfs"), { recursive: true, dereference: true });
  mkdirSync(join(folder, "images"), { recursive: true });
  writeFileSync(join(folder, "images", `${"a".repeat(64)}.tar`), "an uncompressed docker save archive\n");
  writeFileSync(join(folder, "manifest.json"), JSON.stringify({ version: 1, status: "sealed", platform: "linux", build_id: "faults", files: {} }));
  return folder;
}

/** One scenario's own world: a bundle, a home, a stand-in SSH on PATH, a state folder. */
function world(tag) {
  const work = mkdtempSync(join(tmpdir(), `layerfs-ssh-${tag}-`));
  const state = join(work, "state");
  mkdirSync(state, { recursive: true });
  const bin = join(work, "bin");
  mkdirSync(bin, { recursive: true });
  const ssh = join(bin, "ssh");
  writeFileSync(ssh, `#!/bin/sh\nexec "${process.execPath}" "${SSH}" "$@"\n`);
  chmodSync(ssh, 0o755);
  const saved = { path: process.env.PATH, state: process.env.FAKE_SSH_STATE, native: process.env.FAKE_SSH_NATIVE, dir: process.env.LAYERFS_SSH_STATE_DIR };
  process.env.PATH = `${bin}:${saved.path}`;
  process.env.FAKE_SSH_STATE = state;
  process.env.FAKE_SSH_NATIVE = FAKE;
  process.env.LAYERFS_SSH_STATE_DIR = join(state, "sessions");
  return {
    work,
    state,
    home: join(work, "home"),
    bundle: bundleAt(join(work, "bundle")),
    sessions: () => (existsSync(join(state, "sessions")) ? readdirSync(join(state, "sessions")).filter((file) => file.endsWith(".json")) : []),
    calls: () => (existsSync(join(state, "calls.jsonl")) ? readFileSync(join(state, "calls.jsonl"), "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line).argv) : []),
    guest: (...parts) => join(state, "guest", ...parts),
    done: () => {
      for (const [key, value] of [["PATH", saved.path], ["FAKE_SSH_STATE", saved.state], ["FAKE_SSH_NATIVE", saved.native], ["LAYERFS_SSH_STATE_DIR", saved.dir]]) {
        if (value === undefined) delete process.env[key]; else process.env[key] = value;
      }
      killStandIns(state);
      rmSync(work, { recursive: true, force: true });
    },
  };
}

/** Ends every process this world started: forwards from their records, workers from their pid files.
 * A process that refuses to end is reported: a leaked stand-in is a test failure, not a silent one. */
function killStandIns(state) {
  const pids = [];
  const sessions = join(state, "sessions");
  if (existsSync(sessions)) {
    for (const file of readdirSync(sessions).filter((name) => name.endsWith(".json"))) {
      try {
        const { forwardPid } = JSON.parse(readFileSync(join(sessions, file), "utf8"));
        if (forwardPid) pids.push(forwardPid);
      } catch { /* already gone */ }
    }
  }
  const pid = join(state, "guest/layerfs/state/worker.pid");
  if (existsSync(pid)) pids.push(Number(readFileSync(pid, "utf8").trim()));
  for (const one of pids) {
    try { process.kill(one, "SIGKILL"); } catch { /* already gone */ }
  }
  const left = pids.filter((one) => provider.isAlive(one));
  if (left.length) process.stderr.write(`ssh_faults: residue, these stand-ins did not end: ${left.join(", ")}\n`);
}

/** A project of the home, as Init leaves it: an entry and the runtime's record. */
function project(home, name) {
  mkdirSync(join(home, name), { recursive: true });
  writeFileSync(join(home, name, "project.record"), "x");
  mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
  writeFileSync(join(home, ".runtime", "projects", `${name}.json`), JSON.stringify({ name, volume: `lfs-${name}-store-1`, created: 1 }));
}

/** A local port nothing listens on now, for a forward that must succeed. */
const freePort = () => new Promise((resolve) => {
  const server = createServer();
  server.listen(0, "127.0.0.1", () => { const { port } = server.address(); server.close(() => resolve(port)); });
});

/** Whether something accepts a connection on a loopback port now. */
const answers = (port) => new Promise((resolve) => {
  const socket = connect({ host: "127.0.0.1", port });
  const done = (value) => { socket.destroy(); resolve(value); };
  socket.setTimeout(500);
  socket.on("connect", () => done(true));
  socket.on("timeout", () => done(false));
  socket.on("error", () => done(false));
});

const config = (world, overrides = {}) => ({
  name: "vm-a", host: "127.0.0.1", user: "root", port: 22, storePath: "/layerfs/store",
  home: world.home, storeName: "store-a", workerPort: 7070, hostPort: 0, ...overrides,
});

const scenarios = {
  // ① a local forward port someone else holds: create fails at once, and nothing
  // was uploaded or started on the host
  async port(world) {
    const taken = await freePort();
    const server = createServer(() => {});
    await new Promise((resolve) => server.listen(taken, "127.0.0.1", resolve));
    try {
      const error = await deadline(40_000, "create with a busy port", fails(provider.create(config(world, { hostPort: taken })), /could not open the private stream.*Address already in use|address already in use/i));
      assert.match(error.message, new RegExp(`127\\.0\\.0\\.1:${taken}`), error.message);
      assert.deepEqual(world.sessions(), [], "no session record is written");
      const calls = world.calls();
      assert.ok(!calls.some((argv) => argv.some((value) => typeof value === "string" && value.includes("tar -C"))), "nothing is uploaded");
      assert.ok(!calls.some((argv) => argv.some((value) => typeof value === "string" && value.includes("nohup node"))), "no worker is started");
      // the same VM on a free port is made: the refusal was about the port, not the host
      const made = await deadline(60_000, "create on a free port", provider.create(config(world, { hostPort: await freePort() })));
      assert.equal(made.state, "running");
      await deadline(30_000, "remove", provider.remove("vm-a"));
      assert.deepEqual(world.sessions(), [], "a delete clears the record");
    } finally {
      server.close();
    }
  },

  // ② the Store path does not exist: refused before the forward, with nothing left
  async store(world) {
    process.env.FAKE_SSH_STORE_MISSING = "/mnt/nope";
    try {
      await deadline(40_000, "create with a missing Store path", fails(provider.create(config(world, { storePath: "/mnt/nope" })), /InvalidInput.*does not exist/));
    } finally {
      delete process.env.FAKE_SSH_STORE_MISSING;
    }
    assert.deepEqual(world.sessions(), [], "no session record");
    assert.deepEqual(world.calls().filter((argv) => argv.includes("-N")), [], "no forward is opened");
  },

  // ③ the Store path is not ext4: refused by name, with the filesystem it is
  async ext4(world) {
    process.env.FAKE_SSH_STORE_NON_EXT4 = "xfs";
    try {
      const error = await deadline(40_000, "create on a non-ext4 path", fails(provider.create(config(world, { storePath: "/mnt/store" })), /Unsupported.*not ext4/));
      assert.match(error.message, /xfs/);
    } finally {
      delete process.env.FAKE_SSH_STORE_NON_EXT4;
    }
    assert.deepEqual(world.sessions(), []);
  },

  // ④ a host that is not Linux: refused by what it answers, not by a guess
  async host(world) {
    process.env.FAKE_SSH_OS = "Darwin";
    try {
      await deadline(40_000, "create against a non-Linux host", fails(provider.create(config(world)), /Unsupported.*not Linux/));
    } finally {
      delete process.env.FAKE_SSH_OS;
    }
    assert.deepEqual(world.sessions(), []);
  },

  // ⑤ the network is down, then it is back: the first call fails clearly, the
  // second one makes the VM
  async drop(world) {
    process.env.FAKE_SSH_UNREACHABLE = "1";
    try {
      const error = await deadline(40_000, "create while unreachable", fails(provider.create(config(world)), /Failed.*(SSH to 127\.0\.0\.1 failed|could not reach)/));
      assert.match(error.message, /Network is unreachable/, "the refusal carries the transport's own reason");
      assert.match(error.message, /127\.0\.0\.1/, "and names the host");
    } finally {
      delete process.env.FAKE_SSH_UNREACHABLE;
    }
    assert.deepEqual(world.sessions(), [], "an unreachable host leaves no record");
    const made = await deadline(60_000, "create after reconnect", provider.create(config(world, { hostPort: await freePort() })));
    assert.equal(made.state, "running");
    await deadline(30_000, "remove", provider.remove("vm-a"));
  },

  // ⑥ the SSH forward dies under a running VM (a dropped connection): the VM
  // reads stopped, its endpoint refuses, and start brings it back
  async disconnect(world) {
    const hostPort = await freePort();
    const made = await deadline(60_000, "create", provider.create(config(world, { hostPort })));
    assert.equal(made.state, "running");
    assert.equal(await answers(hostPort), true, "the forward answers while it runs");
    const pid = made.ports[0].host;
    const record = JSON.parse(readFileSync(join(world.state, "sessions", "vm-a.json"), "utf8"));
    process.kill(record.forwardPid);
    for (let waited = 0; waited < 2_000 && (await answers(pid)); waited += 50) await sleep(50);
    assert.equal(await answers(pid), false, "the dropped forward refuses, so a call fails with a clear connection error");
    assert.equal((await deadline(20_000, "inspect after the drop", provider.inspect("vm-a"))).state, "stopped");
    const started = await deadline(60_000, "start after the drop", provider.start("vm-a"));
    assert.equal(started.state, "running");
    assert.equal(await answers(pid), true, "the same port answers again");
    await deadline(30_000, "remove", provider.remove("vm-a"));
  },

  // ⑦ the host (or its worker) is gone, then back: the runtime re-opens the
  // worker without copying the bundle again
  async remote(world) {
    project(world.home, "shop");
    const runtime = await LayerFSRuntime.open({
      home: world.home, bundle: world.bundle, placement: "vm:ssh",
      containerPrefix: "lfs-fault-", platform: "linux", sweep: false,
      vm: { host: "127.0.0.1", port: 22, storePath: "/layerfs/store" },
    });
    try {
      const first = await deadline(90_000, "worker.start", runtime.worker.start("shop"));
      assert.deepEqual([first.vm.state, first.worker.state], ["running", "running"]);
      const copies = () => world.calls().filter((argv) => argv.some((value) => typeof value === "string" && value.includes("tar -C"))).length;
      assert.equal(copies(), 1, "the bundle is uploaded once");
      const endpoint = JSON.parse(readFileSync(endpointFile(world.home, "shop"), "utf8"));
      assert.equal((await deadline(5_000, "ping a running VM", ping(endpoint)))?.ready, true);
      // the host reboots: its worker and the forward are gone, the disk stays
      await deadline(30_000, "stop", runtime.vm.stop("shop"));
      const pid = join(world.state, "guest/layerfs/state/worker.pid");
      assert.equal(existsSync(pid), true, "the stand-in recorded the worker it started");
      assert.equal((await deadline(2_000, "ping a stopped VM", ping(endpoint))), null, "a stopped VM's worker does not answer");
      const back = await deadline(90_000, "worker.start after the reboot", runtime.worker.start("shop"));
      assert.deepEqual([back.vm.state, back.worker.state], ["running", "running"]);
      assert.equal(copies(), 1, "the bundle on the host's disk was reused, not uploaded again");
      assert.equal((await deadline(5_000, "ping after the restart", ping(JSON.parse(readFileSync(endpointFile(world.home, "shop"), "utf8")))))?.ready, true);
      // an un-pushed Store is never destroyed by a delete
      await deadline(20_000, "delete with a project", fails(runtime.vm.delete("shop", { confirm: "delete" }), /Unsupported.*never destroys a Store/));
      assert.equal(world.sessions().length, 1, "the refused delete left the VM alone");
    } finally {
      await runtime.close();
    }
  },

  // ⑧ delete after the project is gone: the record, the forward and this
  // adapter's remote copy go; the user's Store path is untouched
  async delete(world) {
    mkdirSync(world.guest("layerfs/store/global"), { recursive: true });
    writeFileSync(world.guest("layerfs/store/global/store.sqlite"), "the user's Store\n");
    project(world.home, "shop");
    const runtime = await LayerFSRuntime.open({
      home: world.home, bundle: world.bundle, placement: "vm:ssh",
      containerPrefix: "lfs-fault-", platform: "linux", sweep: false,
      vm: { host: "127.0.0.1", port: 22, storePath: "/layerfs/store" },
    });
    try {
      await deadline(90_000, "worker.start", runtime.worker.start("shop"));
      // the project goes: its entry and the runtime's record
      rmSync(join(world.home, "shop"), { recursive: true, force: true });
      rmSync(join(world.home, ".runtime", "projects", "shop.json"), { force: true });
      const gone = await deadline(60_000, "vm.delete", runtime.vm.delete("shop", { confirm: "delete" }));
      assert.deepEqual([gone.state, gone.store], ["deleted", "kept"], "the Store disk is kept by default");
      assert.deepEqual(world.sessions(), [], "the session record is gone");
      assert.equal(existsSync(world.guest("layerfs/bundle")), false, "the adapter's bundle copy is removed");
      assert.equal(existsSync(world.guest("layerfs/state")), false, "its worker state is removed");
      assert.equal(readFileSync(world.guest("layerfs/store/global/store.sqlite"), "utf8"), "the user's Store\n", "the user's Store path is untouched");
    } finally {
      await runtime.close();
    }
  },

  // ⑨ an un-pushed Store is refused by the runtime, and one host never serves two
  async shared(world) {
    const first = await deadline(60_000, "create the first VM", provider.create(config(world, { hostPort: await freePort() })));
    assert.equal(first.state, "running");
    const second = await deadline(30_000, "create a second VM on the same host", fails(provider.create(config(world, { name: "vm-b", storeName: "store-b", hostPort: await freePort() })), /Busy.*one host serves one Store/));
    assert.match(second.message, /vm-a/, "the refusal names the VM that already holds it");
    assert.deepEqual(world.sessions(), ["vm-a.json"], "the refused create wrote no record");
    await deadline(30_000, "remove", provider.remove("vm-a"));
  },
};

const selected = process.argv[2];
if (!selected) {
  // One child per scenario: a runtime binds this process's worker route once
  // per project (`useWorker`), so two runtimes cannot share a process, and a
  // scenario that hangs ends its own child instead of the whole file.
  const { spawn } = await import("node:child_process");
  const results = [];
  for (const name of SCENARIOS) {
    const code = await new Promise((resolve) => {
      const child = spawn(process.execPath, [SELF, name], { stdio: ["ignore", "inherit", "inherit"], env: process.env });
      const timer = setTimeout(() => { child.kill("SIGKILL"); }, 120_000);
      child.on("close", (value) => { clearTimeout(timer); resolve(value); });
    });
    results.push([name, code]);
  }
  const failed = results.filter(([, code]) => code !== 0);
  process.stdout.write(`${results.map(([name, code]) => `${code === 0 ? "PASS" : "FAIL"} ssh_faults/${name}`).join("\n")}\n`);
  if (failed.length) {
    process.stderr.write(`ssh_faults: ${failed.length} of ${results.length} scenarios failed: ${failed.map(([name]) => name).join(", ")}\n`);
    process.exit(1);
  }
  process.stdout.write("PASS ssh_faults\n", () => process.exit(0));
} else {
  if (!scenarios[selected]) throw new Error(`select ${SCENARIOS.join(", ")}`);
  setTimeout(() => {
    process.stderr.write(`ssh_faults/${selected}: FAIL, the scenario did not finish in 120 s\n`);
    process.exit(1);
  }, 120_000).unref();

  const box = world(selected);
  const started = Date.now();
  try {
    await deadline(110_000, `scenario ${selected}`, scenarios[selected](box));
    console.log(`PASS ssh_faults/${selected} (${Date.now() - started} ms)`);
  } finally {
    box.done();
  }
  process.stdout.write("PASS ssh_faults\n", () => process.exit(0));
}

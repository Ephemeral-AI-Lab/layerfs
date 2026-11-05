// Independent homes sharing TMPDIR: starting/stopping a worker must not remove
// another live worker's overlay/spool directories or retained failure evidence.
// Real addon, Disposable Store, Docker and daemon, under tools/worktree.py run:
//   --timeout 90 -- node api/runtime/test/verify/worker_temp_real.mjs container
//   --timeout 90 -- node api/runtime/test/verify/worker_temp_real.mjs stdin
// stdin exercises the Windows worker's startup frame and graceful EOF shutdown;
// container exercises the macOS worker's environment startup (on Linux here).
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { frame, readFrames } from "@layerfs/agent/internal";

const mode = process.argv[2];
assert.ok(["container", "stdin"].includes(mode), "choose container or stdin");
const bundle = dirname(process.env.LAYERFS_NATIVE);
const server = fileURLToPath(new URL("../../src/worker/server.js", import.meta.url));
const work = mkdtempSync(join(tmpdir(), "layerfs-worker-temp-test-"));
// All workers, including a broken version's broad cleanup, are confined here.
const shared = join(work, "tmp");
mkdirSync(shared, { mode: 0o700 });
const retained = join(shared, "layerfs-worker-retained-evidence");
mkdirSync(retained);
writeFileSync(join(retained, "evidence"), "retain original failure");
const workers = [];
let client, sandbox, ws, passed = false;

async function bounded(promise, label, ms = 10_000) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} exceeded ${ms} ms`)), ms);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

async function start(name) {
  const home = join(work, name), secret = randomBytes(32).toString("hex");
  mkdirSync(home, { mode: 0o700 });
  const before = new Set(readdirSync(shared));
  const child = spawn(process.execPath, [server, ...(mode === "stdin" ? ["--stdin"] : [])], {
    env: { ...process.env, TMPDIR: shared, LAYERFS_HOME: home, LAYERFS_BUNDLE: bundle,
      LAYERFS_WORKER_SECRET: secret, LAYERFS_WORKER_LISTEN: "127.0.0.1:0" },
    stdio: ["pipe", "pipe", "pipe"],
  });
  const worker = { child, ended: new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal }))) };
  workers.push(worker);
  let stderr = "";
  const ready = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => reject(new Error(`${name} exited before readiness: ${code ?? signal}\n${stderr}`)));
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (mode === "container") {
        const match = /listening on 127\.0\.0\.1:(\d+)/.exec(stderr);
        if (match) resolve(Number(match[1]));
      }
    });
    if (mode === "stdin") readFrames(child.stdout, (r) => resolve(r.port), reject);
    else child.stdout.resume();
  });
  if (mode === "stdin") child.stdin.write(frame({ secret, home, bundle }));
  const port = await bounded(ready, `${name} readiness`);
  const made = readdirSync(shared).filter((entry) => !before.has(entry));
  assert.equal(made.length, 1, "each start creates exactly one private directory");
  worker.temp = join(shared, made[0]);
  assert.equal(statSync(worker.temp).mode & 0o777, 0o700);
  worker.marker = join(worker.temp, "live-owner");
  writeFileSync(worker.marker, name);
  worker.endpoint = join(work, `${name}.json`);
  writeFileSync(worker.endpoint, JSON.stringify({ home, port, secret, build: "test" }));
  worker.home = home;
  return worker;
}

async function stop(worker) {
  if (mode === "stdin") worker.child.stdin.end();
  else worker.child.kill("SIGTERM");
  const exit = await bounded(worker.ended, "worker shutdown", 5_000);
  assert.deepEqual(exit, mode === "stdin" ? { code: 0, signal: null } : { code: null, signal: "SIGTERM" });
  if (mode === "stdin") assert.equal(existsSync(worker.temp), false, "graceful shutdown removes its own directory");
}

try {
  const a = await start("home-a");
  process.env.LAYERFS_HOME = a.home;
  process.env.LAYERFS_WORKER_ENDPOINT = a.endpoint;
  const { LayerFS } = await import("@layerfs/agent");
  const source = join(work, "source");
  mkdirSync(source);
  writeFileSync(join(source, "README.md"), "worker isolation\n");
  client = await LayerFS.init("worker-temp", source);
  await client.branch.fork("work");
  await client.fs.write("work", "/before", "before B");
  sandbox = (await client.sandbox.create()).sandbox;
  ws = await client.workspace.mount(sandbox, "work");
  assert.equal((await client.workspace.exec(ws, "printf before")).stdout, "before");

  const b = await start("home-b");
  assert.notEqual(a.temp, b.temp);
  assert.equal(a.child.exitCode, null, "A remains alive");
  await client.fs.write("work", "/after", "after B");
  assert.equal((await client.fs.read("work", "/after")).text, "after B");
  // Beyond the 1 MiB inline limit: this needs A's original spool directory.
  const size = 2 * 1024 * 1024 + 1;
  const ran = await client.workspace.exec(ws, `head -c ${size} /dev/zero | tr '\\0' x`);
  assert.deepEqual([ran.exitCode, ran.stdout, ran.stderr], [0, "x".repeat(size), ""]);
  assert.equal(readFileSync(a.marker, "utf8"), "home-a");
  assert.equal(readFileSync(b.marker, "utf8"), "home-b");
  assert.equal(readFileSync(join(retained, "evidence"), "utf8"), "retain original failure");

  await stop(b);
  assert.equal(readFileSync(a.marker, "utf8"), "home-a", "B shutdown preserves A");
  await client.fs.write("work", "/after-stop", "B stopped");
  assert.equal((await client.fs.read("work", "/after-stop")).text, "B stopped");
  await client.workspace.unmount(ws, true);
  ws = null;
  await client.sandbox.close(sandbox);
  sandbox = null;
  await client.close();
  client = null;
  await stop(a);
  assert.equal(readFileSync(join(retained, "evidence"), "utf8"), "retain original failure");
  console.log(`worker_temp_real ${mode}: ok (independent writes, spooled output and cleanup)`);
  passed = true;
} finally {
  if (ws) await client.workspace.unmount(ws, true).catch((error) => { console.error(error); process.exitCode = 1; });
  if (sandbox) await client.sandbox.close(sandbox).catch((error) => { console.error(error); process.exitCode = 1; });
  if (client) await client.close().catch((error) => { console.error(error); process.exitCode = 1; });
  for (const worker of workers) {
    if (worker.child.exitCode === null && worker.child.signalCode === null) {
      worker.child.kill("SIGKILL");
      await bounded(worker.ended, "failed-test worker cleanup", 5_000);
    }
  }
  // Only a passing run releases its fixture. The launcher's failed-run custody
  // retains original files and stopped containers for inspection otherwise.
  if (passed) rmSync(work, { recursive: true, force: true });
}

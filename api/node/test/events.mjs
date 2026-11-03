// Events reach their handlers on their own handle (#533 review, group
// calls): a handler that throws does not stop the call or the process, and
// every event the call sent reaches its handler before the call settles.
// A `store.push` to a worker (`layerfs worker`, the repository's debug
// build) sends a progress event at each phase.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { createServer, connect } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { client } from "../src/index.js";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const target = process.env.CARGO_TARGET_DIR ? resolve(root, process.env.CARGO_TARGET_DIR) : join(root, "target");
const command = join(target, "debug", "layerfs");

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-events-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

const freePort = () => new Promise((done) => {
  const server = createServer();
  server.listen(0, "127.0.0.1", () => {
    const { port } = server.address();
    server.close(() => done(port));
  });
});

const listening = (port) => new Promise((done) => {
  const socket = connect(port, "127.0.0.1", () => { socket.destroy(); done(true); });
  socket.on("error", () => done(false));
});

/** A `layerfs worker` on a free loopback port, and its stop. */
async function worker(dir) {
  const secret = join(dir, "secret");
  writeFileSync(secret, "s3cret\n", { mode: 0o600 });
  const port = await freePort();
  const child = spawn(command, ["worker"], {
    env: { ...process.env, LAYERFS_WORKER_LISTEN: `127.0.0.1:${port}`, LAYERFS_WORKER_SECRET: secret, LAYERFS_WORKER_ROOT: join(dir, "root") },
    stdio: "ignore",
  });
  // the readiness wait: until it listens, at most 10 s
  const deadline = Date.now() + 10_000;
  while (!(await listening(port))) {
    if (Date.now() > deadline) { child.kill(); throw new Error("the worker did not listen"); }
    await new Promise((r) => setTimeout(r, 50));
  }
  return { address: `127.0.0.1:${port}`, stop: () => child.kill() };
}

test("a throwing on_progress does not stop a push; every event comes before it settles", { timeout: 60_000 }, async () => {
  const home = temp("home");
  const guest = temp("guest");
  const box = await worker(guest);
  try {
    const lfs = client({ home });
    await lfs.store.create("shop");
    mkdirSync(join(home, "machine", "box"), { recursive: true });
    writeFileSync(join(home, "machine", "box", "record.json"), JSON.stringify({ name: "box", platform: "lima" }));
    writeFileSync(join(home, "machine", "box", "worker.json"), JSON.stringify({ address: box.address, secret: "s3cret" }), { mode: 0o600 });

    const warnings = [];
    const onWarning = (warning) => warnings.push(String(warning.message));
    process.on("warning", onWarning);
    const thrown = [];
    const pushed = await lfs.store.push("shop", "box", {
      on_progress: (p) => {
        thrown.push(p.phase);
        throw new Error(`a handler that throws at ${p.phase}`);
      },
    });
    // the push went on to its end, and this process is alive to see it
    assert.equal((await lfs.store.get("shop")).machine, "box", JSON.stringify(pushed));
    // every event, the last phase included, reached the handler first
    assert.equal(thrown[thrown.length - 1], "retire", thrown.join(","));
    assert.ok(thrown.includes("check") && thrown.includes("send"), thrown.join(","));
    await new Promise((r) => setImmediate(r));
    process.off("warning", onWarning);
    assert.ok(warnings.some((w) => /a handler that throws at check/.test(w)), warnings.join("\n"));

    // and back, counting: no event after the call settles
    const phases = [];
    await lfs.store.pull("shop", { on_progress: (p) => phases.push(p.phase) });
    const settled = phases.slice();
    assert.equal(settled[0], "check");
    assert.equal(settled[settled.length - 1], "retire", settled.join(","));
    await new Promise((r) => setTimeout(r, 200));
    assert.deepEqual(phases, settled);
  } finally {
    box.stop();
    rmSync(home, { recursive: true, force: true });
    rmSync(guest, { recursive: true, force: true });
  }
});

// The live half needs the native daemon at LAYERFS_DAEMON; without it, it
// is skipped (NOT_RUN).
test("exec output: the stdout and stderr functions, and stream: true without them", { timeout: 60_000, skip: !process.env.LAYERFS_DAEMON && "NOT_RUN: LAYERFS_DAEMON unset" }, async () => {
  const home = temp("stream");
  const folder = temp("stream-folder");
  writeFileSync(join(folder, "README.md"), "streamed\n");
  const lfs = client({ home });
  await lfs.store.create("shop");
  const imported = spawnSync(command, ["import-env"], { env: { ...process.env, LAYERFS_HOME: home }, timeout: 30_000 });
  assert.equal(imported.status, 0, String(imported.stderr));
  await lfs.project.init("shop/api", folder);
  await lfs.branch.fork("shop/api", "dev", "main");
  await lfs.sandbox.create("shop/api", "s1", "native");
  let ws;
  try {
    ({ ws } = await lfs.workspace.mount("s1", "dev"));
    const out = [];
    const err = [];
    const ran = await lfs.workspace.exec(ws, "cat README.md; echo oops >&2", {
      stdout: (bytes) => out.push(Buffer.from(bytes).toString()),
      stderr: (bytes) => err.push(Buffer.from(bytes).toString()),
    });
    assert.equal(ran.exit_code, 0);
    assert.equal(out.join(""), "streamed\n", "before the call settled");
    assert.equal(err.join(""), "oops\n");

    // stream: true and no functions: this process's own stdout and stderr
    const script = `import { client } from ${JSON.stringify(new URL("../src/index.js", import.meta.url).href)};
      await client().workspace.exec(${JSON.stringify(ws)}, "cat README.md; echo oops >&2", { stream: true });`;
    const child = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      env: { ...process.env, LAYERFS_HOME: home },
      timeout: 60_000,
    });
    assert.equal(child.status, 0, String(child.stderr));
    assert.equal(String(child.stdout), "streamed\n");
    assert.equal(String(child.stderr), "oops\n");
  } finally {
    if (ws) await lfs.workspace.unmount(ws);
    await lfs.sandbox.close("s1");
    rmSync(home, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});

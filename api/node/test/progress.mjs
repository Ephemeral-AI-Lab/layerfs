// The progress contract of the package (#37), by running it: which calls
// take `on_progress`; what `project.init` reports in this process and
// through a stand-in worker, and `store.push`/`store.pull` to that worker;
// that events arrive while the call runs, before it settles and not after;
// and what a throwing handler does. Local Init needs no machine, daemon or
// bucket. The worker half needs the repository's debug build
// (target/debug/layerfs) and is skipped without it.
// The page that states this: docs/architecture/api/call-table/events.md.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { createServer, connect } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { client, table, LayerFSError } from "../src/index.js";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const target = process.env.CARGO_TARGET_DIR ? resolve(root, process.env.CARGO_TARGET_DIR) : join(root, "target");
const command = join(target, "debug", "layerfs");
const noCommand = !existsSync(command) && `the layerfs command is not built (${command})`;

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-progress-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

/** A folder of `files` small files, spread over 20 directories. */
function folderOf(files) {
  const folder = temp("folder");
  for (let i = 0; i < files; i++) {
    mkdirSync(join(folder, `d${i % 20}`), { recursive: true });
    writeFileSync(join(folder, `d${i % 20}`, `f${i}.txt`), `file ${i}\n`);
  }
  return folder;
}

const calling = (name) => table.filter((c) => c.options.some((o) => o.name === name && o.kind === "function")).map((c) => `${c.family}.${c.call}`);

test("the table: seven calls take on_progress, one takes the exec sinks, machine.create takes neither", async () => {
<<<<<<< HEAD
  assert.deepEqual(calling("on_progress"), ["project.init", "project.export", "workspace.exec", "store.push", "store.pull", "backup.create", "backup.restore"]);
=======
  assert.deepEqual(calling("on_progress"), [
    "project.init", "project.export", "workspace.exec", "store.push", "store.pull", "backup.create", "backup.restore",
  ]);
>>>>>>> addbf29dc (test(sdk): pin the seven progress calls and project.export across the front ends)
  assert.deepEqual(calling("stdout"), ["workspace.exec"]);
  assert.deepEqual(calling("stderr"), ["workspace.exec"]);
  assert.equal(table.flatMap((c) => c.options).filter((o) => o.kind === "function").length, 7 + 2);
  const home = temp("machine");
  try {
    // the library emits { phase: "create" } for this call, but no handler can be given
    await assert.rejects(
      client({ home }).machine.create("m1", "lima", { on_progress: () => {} }),
      (error) => error instanceof LayerFSError && error.code === "InvalidInput" && /unknown option "on_progress"/.test(error.message),
    );
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("project.init in this process: check, install, record with null counts, while it runs, before it settles", { timeout: 90_000 }, async () => {
  const home = temp("init");
  const folder = folderOf(2000);
  try {
    const lfs = client({ home });
    await lfs.store.create("shop");
    const started = performance.now();
    const events = [];
    let settled = false;
    const result = await lfs.project.init("shop/api", folder, {
      on_progress: (p) => events.push({ at: performance.now(), settled, p }),
    });
    settled = true;
    const returned = performance.now();
    assert.equal(result.project, "shop/api");
    // the payload is { phase, done, total }; the counts are null in this route
    assert.deepEqual(events.map((e) => e.p), [
      { phase: "check", done: null, total: null },
      { phase: "install", done: null, total: null },
      { phase: "record", done: null, total: null },
    ]);
    // every event was delivered before the call settled, in order
    assert.ok(events.every((e) => !e.settled), "an event arrived after the call settled");
    assert.ok(events.every((e, i) => i === 0 || events[i - 1].at <= e.at));
    // live: Init spends its time between `install` and `record`, and the
    // `install` event was in the handler before that time was spent
    assert.ok(events[2].at - events[1].at > 10, `install at ${events[1].at - started} ms, record at ${events[2].at - started} ms`);
    assert.ok(returned >= events[2].at);
    // none after the call returned
    await new Promise((r) => setTimeout(r, 200));
    assert.equal(events.length, 3);
  } finally {
    rmSync(home, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});

test("a throwing on_progress does not stop Init: every event is still delivered, each throw is a warning, the result is the call's", { timeout: 90_000 }, async () => {
  const home = temp("throw");
  const folder = folderOf(20);
  const warnings = [];
  const onWarning = (warning) => warnings.push(warning);
  process.on("warning", onWarning);
  try {
    const lfs = client({ home });
    await lfs.store.create("shop");
    const phases = [];
    const result = await lfs.project.init("shop/api", folder, {
      on_progress: (p) => {
        phases.push(p.phase);
        throw new Error(`boom at ${p.phase}`);
      },
    });
    assert.deepEqual(phases, ["check", "install", "record"]);
    assert.equal(result.project, "shop/api");
    assert.equal((await lfs.project.get("shop/api")).project, "shop/api", "the Init happened");
    await new Promise((r) => setImmediate(r));
    const mine = warnings.filter((w) => w.name === "LayerFSHandlerWarning");
    assert.equal(mine.length, 3, warnings.map(String).join("\n"));
    assert.match(mine[0].message, /project\.init: an event handler threw: Error: boom at check/);
  } finally {
    process.off("warning", onWarning);
    rmSync(home, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});

// What is NOT isolated, recorded as it is (events.md, Open decisions): the
// package catches a handler's throw, but not the rejection of a handler
// that is an async function, which Node reports as an unhandled rejection
// and, by default, ends the process for. Change this test with that decision.
test("an async handler that rejects is not caught: Node's unhandled-rejection rule applies (current behaviour)", { timeout: 90_000 }, () => {
  const home = temp("async");
  const folder = folderOf(1);
  try {
    const script = `
      import { client } from ${JSON.stringify(new URL("../src/index.js", import.meta.url).href)};
      const lfs = client({ home: ${JSON.stringify(home)} });
      await lfs.store.create("shop");
      await lfs.project.init("shop/api", ${JSON.stringify(folder)}, {
        on_progress: async (p) => { throw new Error("async boom at " + p.phase); },
      });
      await new Promise((r) => setTimeout(r, 300));
      console.log("survived");`;
    const run = spawnSync(process.execPath, ["--input-type=module", "-e", script], { timeout: 60_000 });
    assert.equal(run.error, undefined);
    assert.notEqual(run.status, 0, String(run.stdout));
    assert.ok(!String(run.stdout).includes("survived"));
    assert.match(String(run.stderr), /async boom at check/);
  } finally {
    rmSync(home, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});

// A `layerfs worker` on a free loopback port, as events.mjs starts one.
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
async function worker(dir) {
  const secret = join(dir, "secret");
  writeFileSync(secret, "s3cret\n", { mode: 0o600 });
  const port = await freePort();
  const child = spawn(command, ["worker"], {
    env: { ...process.env, LAYERFS_WORKER_LISTEN: `127.0.0.1:${port}`, LAYERFS_WORKER_SECRET: secret, LAYERFS_WORKER_ROOT: join(dir, "root") },
    stdio: "ignore",
  });
  const deadline = Date.now() + 10_000;
  while (!(await listening(port))) {
    if (Date.now() > deadline) { child.kill(); throw new Error("the worker did not listen"); }
    await new Promise((r) => setTimeout(r, 50));
  }
  return { address: `127.0.0.1:${port}`, stop: () => child.kill() };
}

/** The `send` events of one call, by phase name. */
const sends = (events) => events.filter((p) => p.phase === "send");

test("through a worker: push, pull and init report bytes per phase; null outside it; the last `send` precedes verification and installation", { timeout: 90_000, skip: noCommand }, async () => {
  const home = temp("worker-home");
  const guest = temp("worker-guest");
  const folder = folderOf(100);
  const box = await worker(guest);
  try {
    const lfs = client({ home });
    await lfs.store.create("shop");
    mkdirSync(join(home, "machine", "box"), { recursive: true });
    writeFileSync(join(home, "machine", "box", "record.json"), JSON.stringify({ name: "box", platform: "lima" }));
    writeFileSync(join(home, "machine", "box", "worker.json"), JSON.stringify({ address: box.address, secret: "s3cret" }), { mode: 0o600 });

    const pushed = [];
    await lfs.store.push("shop", "box", { on_progress: (p) => pushed.push(p) });
    assert.deepEqual(pushed.map((p) => p.phase).filter((p, i, all) => all.indexOf(p) === i),
      ["check", "snapshot", "send", "verify", "install", "record", "retire"]);

    const inited = [];
    await lfs.project.init("shop/api", folder, { on_progress: (p) => inited.push(p) });
    assert.deepEqual(inited.map((p) => p.phase).filter((p, i, all) => all.indexOf(p) === i),
      ["check", "send", "install", "record"]);

    const pulled = [];
    await lfs.store.pull("shop", { on_progress: (p) => pulled.push(p) });
    assert.equal(pulled[0].phase, "check");
    assert.equal(pulled[pulled.length - 1].phase, "retire");

    // `snapshot` and `verify` count their own bytes in push and pull
    // (layerfs-project#8 hardening); Init through a worker counts its tree.
    for (const [name, events, counted] of [
      ["store.push", pushed, ["snapshot", "send", "verify"]],
      ["project.init", inited, ["send"]],
      ["store.pull", pulled, ["snapshot", "send", "verify"]],
    ]) {
      // one shape: exactly phase, done and total
      for (const p of events) assert.deepEqual(Object.keys(p).sort(), ["done", "phase", "total"], `${name}: ${JSON.stringify(p)}`);
      // a counted phase has an entry marker with null counts, then byte
      // counts; every other phase has null counts throughout
      for (const p of events) {
        if (!counted.includes(p.phase)) assert.deepEqual([p.done, p.total], [null, null], `${name}: ${JSON.stringify(p)}`);
        else assert.ok((p.done === null && p.total === null) || (Number.isInteger(p.done) && Number.isInteger(p.total)), `${name}: ${JSON.stringify(p)}`);
      }
      for (const phase of counted) {
        const seen = events.filter((p) => p.phase === phase && p.done !== null);
        assert.ok(seen.length >= 1, `${name}: no ${phase} count: ${JSON.stringify(events)}`);
        // starts at 0, never goes down or past its total, and ends at it
        assert.equal(seen[0].done, 0, `${name} ${phase}: ${JSON.stringify(seen)}`);
        assert.equal(seen[0].total, seen.at(-1).total, `${name} ${phase}: the total changed`);
        assert.ok(seen.every((p, i) => p.done <= p.total && (i === 0 || seen[i - 1].done <= p.done)), `${name} ${phase}: ${JSON.stringify(seen)}`);
        assert.equal(seen.at(-1).done, seen[0].total, `${name} ${phase}: did not reach its total`);
      }
      const s = sends(events);
      assert.ok(s.length >= 2, `${name}: ${JSON.stringify(s)}`);
      // bytes sent is not completion: `send` reaches 100% and the call
      // still has phases to run (the worker verifies and installs after it)
      const last = events.map((p) => p.phase).lastIndexOf("send");
      assert.ok(last < events.length - 1, `${name}: nothing after the last send`);
    }
  } finally {
    box.stop();
    rmSync(home, { recursive: true, force: true });
    rmSync(guest, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});

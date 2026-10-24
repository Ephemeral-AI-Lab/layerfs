// The worker route end to end, in Linux (#428): runtime's worker/server.js
// (container mode, on loopback) with the bundle's real addon, and an agent
// process whose every call goes through @layerfs/agent's worker client
// (LAYERFS_WORKER_ENDPOINT alone; no integration is named): Init, writes, a
// Workspace's exec and Commit, backend reads, then close. Also: the wrong
// secret and another home are cut off. Under the E2E launcher (its Store volume, home and deployment):
//   node api/runtime/test/verify/worker_real.mjs inner
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SELF = fileURLToPath(import.meta.url);

if (process.argv[2] === "agent") {
  // the agent's side: no addon in this process
  const { LayerFS } = await import("@layerfs/agent");
  const { LayerFSBackend } = await import("@layerfs/backend");
  const source = mkdtempSync(join(tmpdir(), "layerfs-wk-src-"));
  writeFileSync(join(source, "README.md"), "worker\n");
  const shop = await LayerFS.init(process.env.WORKER_PROJECT, source);
  try {
    assert.equal(shop.created.kind, "Initialized");
    await shop.branch.fork("lane");
    await shop.fs.write("lane", "/note.txt", "one\n", "Add note");
    assert.equal((await shop.fs.read("lane", "/note.txt")).text, "one\n");
    const ws = await shop.workspace.mount((await shop.sandbox.create()).sandbox, "lane");
    const ran = await shop.workspace.exec(ws, "cat note.txt && echo two > fresh.txt");
    assert.deepEqual([ran.exitCode, ran.stdout], [0, "one\n"]);
    assert.equal((await shop.workspace.commit(ws, "Add fresh")).kind, "Committed");
    await shop.workspace.unmount(ws);
    const backend = LayerFSBackend.of(shop);
    assert.ok((await backend.store.project.get_history()).commits.length >= 2);
    assert.match((await backend.daemon.sandbox.get_log(ws.sandbox, { lines: 5 })).container, /^[0-9a-f]{64}$/);
  } finally {
    await shop.close();
    rmSync(source, { recursive: true, force: true });
  }
  console.log("agent over the worker: ok");
  process.exit(0);
}
if (process.argv[2] !== "inner") throw new Error("usage: worker_real.mjs inner (under tools/worktree.py run)");

const { connection, endpointOf } = await import("@layerfs/agent/internal");
const bundle = dirname(process.env.LAYERFS_NATIVE);
const secret = randomBytes(32).toString("hex");
const work = mkdtempSync(join(tmpdir(), "layerfs-wk-"));
const server = spawn(process.execPath, [fileURLToPath(new URL("../../src/worker/server.js", import.meta.url))], {
  env: { ...process.env, LAYERFS_WORKER_SECRET: secret, LAYERFS_BUNDLE: bundle, LAYERFS_WORKER_LISTEN: "127.0.0.1:0" },
  stdio: ["ignore", "inherit", "pipe"],
});
const run = (env) => new Promise((resolve) => {
  const child = spawn(process.execPath, [SELF, "agent"], { env, stdio: ["ignore", "pipe", "pipe"] });
  const out = [];
  child.stdout.on("data", (c) => out.push(c));
  child.stderr.on("data", (c) => out.push(c));
  child.on("close", (code) => resolve({ code, text: Buffer.concat(out).toString() }));
});
try {
  const started = Date.now();
  const port = await new Promise((resolve, reject) => {
    let text = "";
    server.stderr.on("data", (chunk) => {
      process.stderr.write(chunk);
      text += chunk;
      const found = /listening on 127\.0\.0\.1:(\d+), build (\S+)/.exec(text);
      if (found) resolve(Number(found[1]));
    });
    server.on("exit", (code) => reject(new Error(`the worker ended (${code})`)));
  });
  console.log(`worker ready after ${Date.now() - started} ms (bundle hashed)`);
  const file = join(work, "endpoint.json");
  writeFileSync(file, JSON.stringify({ home: process.env.LAYERFS_HOME, port, secret, build: "test" }));
  const info = endpointOf(file);
  const pong = await connection(info).call({ fn: "ping" });
  assert.equal(pong.value.ready, true);
  // the wrong secret and another home are cut off; nothing is sent again
  assert.equal((await connection({ port, secret: "0".repeat(64) }).call({ fn: "call", op: "project.list_branches", args: {} })).refused.code, "Failed");
  assert.equal((await connection(info).call({ fn: "call", op: "project.list_branches", args: { home: "/elsewhere" } })).refused.code, "Unknown");
  const env = { ...process.env, LAYERFS_WORKER_ENDPOINT: file, WORKER_PROJECT: `wk-${Date.now().toString(36)}` };
  delete env.LAYERFS_NATIVE;
  const agent = await run(env);
  assert.equal(agent.code, 0, agent.text);
  console.log(agent.text.trim());
  const after = await connection(endpointOf(file)).call({ fn: "ping" }).catch(() => null);
  console.log(`worker after the agent: ${JSON.stringify(after?.value ?? after)}`);
  console.log("worker real: ok");
} finally {
  server.kill("SIGTERM");
  rmSync(work, { recursive: true, force: true });
}

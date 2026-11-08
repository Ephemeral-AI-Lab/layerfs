// M3: stdout is the protocol only. A raw child, no SDK, reads every stdout line
// as JSON-RPC while a tool call logs to stderr; then stdin closes and the server
// exits 0 within 5 s.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { CLI, SERVER, temp } from "./helpers.mjs";

const INITIALIZE = {
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "purity", version: "1.0.0" } },
};
const INITIALIZED = { jsonrpc: "2.0", method: "notifications/initialized" };
const CREATE = { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "store_create", arguments: { name: "shop" } } };

test("only JSON-RPC lines on stdout while a call logs to stderr", async () => {
  const home = temp("purity");
  const child = spawn(process.execPath, [SERVER, "--home", home, "--profile", "admin"], {
    env: { ...process.env, LAYERFS_CLI: CLI },
    stdio: ["pipe", "pipe", "pipe"],
  });
  const out = [];
  const err = [];
  const lines = [];
  let waiter = null;
  const check = () => {
    if (waiter && lines.some((line) => line.id === waiter.id)) {
      const found = lines.find((line) => line.id === waiter.id);
      const done = waiter;
      waiter = null;
      clearTimeout(done.timer);
      done.resolve(found);
    }
  };
  const response = (id) =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`no response ${id} within 30 s; stderr: ${err.slice(-5).join("")}`)), 30_000);
      waiter = { id, resolve, timer };
      check();
    });
  child.stdout.on("data", (chunk) => {
    out.push(chunk);
    for (const line of chunk.toString().split("\n")) {
      if (line.trim() === "") continue;
      lines.push(JSON.parse(line));
      check();
    }
  });
  child.stderr.on("data", (chunk) => err.push(chunk.toString()));
  try {
    child.stdin.write(`${JSON.stringify(INITIALIZE)}\n${JSON.stringify(INITIALIZED)}\n`);
    assert.equal((await response(1)).result.serverInfo.name, "layerfs-mcp");
    child.stdin.write(`${JSON.stringify(CREATE)}\n`);
    const answered = await response(2);
    assert.equal(answered.result.isError, undefined);
    assert.equal(JSON.parse(answered.result.content[0].text).name, "shop");

    // every line the server wrote to stdout is a JSON-RPC message
    const text = Buffer.concat(out).toString();
    const written = text.split("\n").filter((line) => line.trim() !== "");
    assert.ok(written.length >= 2);
    for (const line of written) {
      const message = JSON.parse(line);
      assert.equal(message.jsonrpc, "2.0", line);
      assert.ok(message.id !== undefined || typeof message.method === "string", line);
    }
    assert.ok(err.join("").includes("store_create ok"), err.join("").slice(-400));

    child.stdin.end();
    const exited = await new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal })));
    assert.deepEqual(exited, { code: 0, signal: null });
  } finally {
    if (child.exitCode === null) child.kill("SIGKILL");
  }
});

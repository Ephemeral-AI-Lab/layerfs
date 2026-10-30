import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const SERVER = fileURLToPath(new URL("../src/server.js", import.meta.url));
export const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));

export function startServer(t, options = {}) {
  // A real Store needs a filesystem that carries SQLite locks (ext4/xfs/btrfs),
  // so a real-Store run names its scratch root; on Linux default to /var/tmp when unset.
  const scratch = process.env.LAYERFS_MCP_TEST_TMPDIR ?? (process.platform === "linux" ? "/var/tmp" : tmpdir());
  const work = mkdtempSync(join(scratch, "layerfs-mcp-test-"));
  const log = join(work, "calls.jsonl");
  const child = spawn(process.execPath, [SERVER], {
    env: {
      ...process.env,
      LAYERFS_NATIVE: FAKE,
      LAYERFS_STORE_PATH: "/fake/store.sqlite",
      LAYERFS_HOME: "/private/mcp-home",
      LAYERFS_DOCKER: "/fake.sock",
      LAYERFS_IMAGE: "fake-image",
      LAYERFS_VOLUME: "fake-volume",
      LAYERFS_DAEMON: "/fake/daemon",
      CLI_FAKE_LOG: log,
      ...options.env,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  const pending = new Map();
  const messages = [];
  const notificationWaiters = [];
  let buffer = "";
  let stderr = "";
  let nextId = 1;
  let exited = false;
  const exit = new Promise((resolve) => {
    child.once("exit", (code, signal) => {
      exited = true;
      for (const { reject, timer } of pending.values()) {
        clearTimeout(timer);
        reject(new Error(`server exited ${code}/${signal}: ${stderr}`));
      }
      pending.clear();
      resolve({ code, signal });
    });
  });
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    for (;;) {
      const end = buffer.indexOf("\n");
      if (end < 0) break;
      const line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      if (!line.trim()) continue;
      const message = JSON.parse(line);
      messages.push(message);
      if (message.id !== undefined && pending.has(message.id)) {
        const waiter = pending.get(message.id);
        pending.delete(message.id);
        clearTimeout(waiter.timer);
        if (message.error) waiter.reject(Object.assign(new Error(`${message.error.message}; stderr=${stderr}`), { response: message }));
        else waiter.resolve(message.result);
      } else if (message.method) {
        const index = notificationWaiters.findIndex((waiter) => waiter.method === message.method);
        if (index >= 0) {
          const [waiter] = notificationWaiters.splice(index, 1);
          clearTimeout(waiter.timer);
          waiter.resolve(message);
        }
      }
    }
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);
  const request = (method, params = {}, id = nextId++) => new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`timed out waiting for ${method}; stderr=${stderr}`));
    }, 30_000);
    pending.set(id, { resolve, reject, timer });
    send({ jsonrpc: "2.0", id, method, params });
  });
  const notify = (method, params = {}) => send({ jsonrpc: "2.0", method, params });
  const notification = (method) => new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${method}`)), 30_000);
    notificationWaiters.push({ method, resolve, reject, timer });
  });
  const calls = () => {
    try {
      return readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
    } catch (error) {
      if (error.code === "ENOENT") return [];
      throw error;
    }
  };
  const stop = async () => {
    if (!exited) child.stdin.end();
    await exit;
  };
  t.after(async () => {
    if (!exited) child.kill("SIGKILL");
    await exit;
    rmSync(work, { recursive: true, force: true });
  });
  return { child, request, send, notify, notification, messages, calls, stop, exit, stderr: () => stderr, work };
}

export async function initialize(server) {
  const initialized = await server.request("initialize", {
    protocolVersion: "2026-07-28",
    capabilities: {},
    clientInfo: { name: "layerfs-mcp-test", version: "1.0.0" },
  });
  assert.equal(initialized.serverInfo.name, "layerfs");
  server.notify("notifications/initialized");
}

export async function open(server) {
  await initialize(server);
  const result = await server.request("tools/call", { name: "layerfs_open", arguments: { name: "site" } });
  assert.equal(result.isError, undefined, JSON.stringify(result));
  return result;
}

export function call(server, name, args = {}) {
  return server.request("tools/call", { name, arguments: args });
}

export function payload(result) {
  assert.equal(result.content.length, 1);
  assert.equal(result.content[0].type, "text");
  return JSON.parse(result.content[0].text);
}

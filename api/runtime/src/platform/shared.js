// What the platform folders share: refusals, one Docker CLI call, the
// worker's endpoint file and a bounded ping of a worker.
import { execFile } from "node:child_process";
import { existsSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { join } from "node:path";
import { promisify } from "node:util";
import { LayerFSError } from "@layerfs/agent";
import { frame, readFrames } from "@layerfs/agent/internal";

export const run = promisify(execFile);
export const refuse = (code, message, detail = null) => new LayerFSError(code, message, detail);

/** One Docker CLI call on the configured engine: its stdout, trimmed. */
export const docker = (settings, args, options = {}) =>
  run("docker", ["-H", `unix://${settings.docker}`, ...args], { timeout: 5000, ...options }).then((r) => r.stdout.trim());

/** Where a home's worker endpoint lives: `<home>/worker/endpoint.json`. */
export const endpointFile = (home) => join(home, "worker", "endpoint.json");

/** Writes an endpoint atomically, readable by this user only. */
export function writeEndpoint(file, info) {
  writeFileSync(`${file}.new`, JSON.stringify(info), { mode: 0o600 });
  renameSync(`${file}.new`, file);
}

/** A worker's ping answer ({ ready, active, stores, build }) within `ms`, else null. */
export function ping(info, ms = 1000) {
  return new Promise((resolve) => {
    const socket = connect({ host: "127.0.0.1", port: info.port });
    const done = (value) => { clearTimeout(timer); socket.destroy(); resolve(value); };
    const timer = setTimeout(() => done(null), ms);
    socket.on("connect", () => socket.write(frame({ secret: info.secret })));
    readFrames(socket, ({ id, answer }) => {
      if (id === 0 && answer?.value === "hello") socket.write(frame({ id: 1, fn: "ping" }));
      else done(id === 1 ? answer?.value ?? null : null);
    }, () => done(null));
    socket.on("error", () => done(null));
    socket.on("close", () => done(null));
  });
}

/** Waits for a new worker to answer (a bounded readiness wait, not a retry). */
export async function ready(info, ms, what) {
  for (const until = Date.now() + ms; ;) {
    if ((await ping(info))?.ready) return;
    if (Date.now() > until) throw refuse("Failed", `${what} did not answer within ${ms / 1000} s`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/** The project names of a home folder this process can read: entries with a project record. */
export function homeNames(home) {
  if (!existsSync(home)) return [];
  return readdirSync(home, { withFileTypes: true }).filter((e) => e.isDirectory() && existsSync(join(home, e.name, "project.record"))).map((e) => e.name).sort();
}

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

/**
 * The refusal a Docker-shaped helper gives on a `vm:<provider>` home: host
 * Docker holds none of that home's Stores or Sandboxes, so answering from it
 * would describe another machine.
 */
export const noHostDocker = (runtime, what, instead) =>
  refuse("Unsupported", `${what} reads host Docker, and this home is placed in ${runtime.platform.name}: ${instead}`, null);

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

// The four Store hooks Init and project removal use, as Docker has them (the
// `local` placement): a project's Store is a labelled volume, its holders are
// containers, and removing the project removes the volume. A `vm:` placement
// implements the same four over its VM instead (vm/index.js). Each takes the
// runtime, so the hook uses that platform's own Docker CLI (Windows goes
// through WSL).

/** Container states that hold a volume but run nothing (Docker's; a namespace Sandbox's record has none). */
export const STOPPED = new Set(["created", "exited", "dead"]);

/** Init makes the project's Store volume before installing into it. */
export async function makeStore(runtime, project) {
  const { settings, platform } = runtime;
  await platform.docker(settings, ["volume", "create", "--label", "layerfs.role=store", "--label", `layerfs.home=${settings.home}`, "--label", `layerfs.project=${project.name}`, project.volume], { timeout: 30_000 });
}

/** The containers holding the volume now: [{ id, state }], running or not. */
export async function storeHolders(runtime, volume) {
  const { settings, platform } = runtime;
  const rows = await platform.docker(settings, ["ps", "-a", "--filter", `volume=${volume}`, "--format", "{{.ID}} {{.State}}"]);
  return rows.split("\n").filter(Boolean).map((line) => { const [id, state] = line.split(" "); return { id, state }; });
}

/** Removes the volume's holders: the running ones only when `force`. */
export async function releaseStoreHolders(runtime, volume, holders, { force = false } = {}) {
  const { settings, platform } = runtime;
  if (!holders.length) return;
  await platform.docker(settings, ["rm", ...(force ? ["-f"] : []), ...holders.map((holder) => holder.id)], { timeout: 30_000 });
}

/** The volume itself, once nothing holds it. */
export async function removeStore(runtime, volume) {
  const { settings, platform } = runtime;
  await platform.docker(settings, ["volume", "rm", volume], { timeout: 30_000 });
}
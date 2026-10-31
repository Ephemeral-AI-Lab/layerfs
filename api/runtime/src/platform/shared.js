// What the placements share (docker/ for `local`, vm/ for `vm:<provider>`):
// refusals, the worker's endpoint file and a bounded ping of a worker.
import { execFile } from "node:child_process";
import { existsSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
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

/**
 * The refusal a call on the home's Stores or remotes gives on a home whose
 * placement keeps no Store of its own (`homeStores: false`, a `vm:<provider>`
 * home, #527 F3): each project's Store is in that project's own VM, so a call
 * that names a Store or the home rather than one project has no VM to run in.
 */
export const noHomeStores = (placement, what) =>
  refuse("Unsupported", `${what} is not supported on a ${placement} home: each project's Store is in that project's own VM, so a call on a Store or remote of the home, not one project, has no VM to run in`, null);

/** Refuses `what` before any effect where the platform holds no home Stores. */
export function homeStores(runtime, what) {
  if (runtime.platform.homeStores === false) throw noHomeStores(runtime.platform.name, what);
}

/** Where a `vm:` home's Sandboxes are, for the refusals of the Sandbox helpers that read host Docker. */
export const VM_SANDBOXES = "its Sandboxes are Linux namespaces inside the VM; the SDK's project.list_sandboxes reads them";

/** The placement's Docker group (docker/index.js), or the refusal of `what` where it has none. */
export function dockerOf(runtime, what, instead) {
  if (!runtime.platform.docker) throw noHostDocker(runtime, what, instead);
  return runtime.platform.docker;
}

/**
 * Where a worker's endpoint lives: `<home>/worker/endpoint.json` for a home
 * whose one worker serves it (macOS, Windows), or `<home>/worker/<project>.json`
 * when each project's Store is in its own VM (`vm:<provider>`).
 */
export const endpointFile = (home, project = undefined) => join(home, "worker", project ? `${project}.json` : "endpoint.json");

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

/** A platform's home helpers where this process reads and writes the home itself (Linux, macOS, `vm:`). */
export const localHome = {
  homeNames: (settings) => homeNames(settings.home),
  async removeHome(settings, name) { rmSync(join(settings.home, name), { recursive: true, force: true }); },
};

/** Free bytes from the last line of `df -kP <path>`'s output, else null. */
export function dfFree(stdout) {
  const kb = Number(String(stdout).trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

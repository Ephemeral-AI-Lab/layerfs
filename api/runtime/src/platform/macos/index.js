// macOS: the worker is one long-lived Linux container per LayerFS home,
// `<prefix>worker` (label layerfs.role=worker), whose main process is
// worker/server.js of a sealed Linux bundle, making every Store call with
// the real Linux addon (AGENTS.md: open to any integration, 2026-10-24). Copied
// from the dsh Host's box.js (#428). Its port is published to the Mac's
// loopback only; port and secret go to `<home>/worker/endpoint.json`, with
// how Init runs: in a container of its own with the source folder mounted
// read-only (worker/init.js). It mounts only what calls use, each at its own
// path so a path means the same inside and out: the Docker socket, Docker's
// volume directory (inside Docker's VM), the home (writable) and the bundle
// (read-only); never a broad host folder (sharing /Users stalled Docker
// Desktop, 2026-10-20). A container of another build is replaced; one of
// another home is never touched.
import { randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { docker as cli, endpointFile, homeNames, ping, ready, refuse, writeEndpoint } from "../shared.js";

const VOLUMES = "/var/lib/docker/volumes";
const WORKER_PORT = 7070;
const READY_MS = 15_000, RESTART_MS = 20_000;
/** Where the worker reaches a daemon's loopback-published port: the Mac. */
const DAEMON_HOST = "host.docker.internal";
const SERVER = "api/runtime/src/worker/server.js";
const INIT = "api/runtime/src/worker/init.js";

// every Docker call goes through the platform object, so a test can stub it
const docker = (...args) => platform.docker(...args);
const name = (settings) => `${settings.containerPrefix}worker`;

const build = (settings) => JSON.parse(readFileSync(join(settings.bundle, "manifest.json"), "utf8")).build_id;
/**
 * The engine's socket inside its VM, as Docker Desktop and colima provide it:
 * mounted into the worker and Init at the same path, and the socket their
 * calls use. The Mac's own socket (settings.docker) is a path they cannot see.
 */
const VM_DOCKER = "/var/run/docker.sock";
const mounts = ({ home, bundle }) => [
  "-v", `${VM_DOCKER}:${VM_DOCKER}`, "-v", `${VOLUMES}:${VOLUMES}`,
  "-v", `${home}:${home}`, "-v", `${bundle}:${bundle}:ro`,
  "-e", `LAYERFS_NATIVE=${join(bundle, "layerfs.node")}`,
];

/** The worker container's state: { state: running | stopped | missing | unknown, build, answering, … }. */
async function status(settings) {
  let row;
  try { row = await docker(settings, ["inspect", "-f", '{{.State.Running}}\t{{index .Config.Labels "layerfs.build"}}\t{{.State.StartedAt}}\t{{index .Config.Labels "layerfs.home"}}', name(settings)]); } catch (error) {
    const missing = /No such object|No such container/i.test(error.stderr ?? "");
    return { worker: { name: name(settings), state: missing ? "missing" : "unknown", error: missing ? null : (error.stderr?.trim() || error.message) } };
  }
  const [running, label, startedAt, home] = row.split("\t");
  const worker = { name: name(settings), state: running === "true" ? "running" : "stopped", build: label, expected: settings.bundle ? build(settings) : null, home, startedAt };
  if (worker.state === "running") {
    let info = null;
    try { info = JSON.parse(readFileSync(endpointFile(settings.home), "utf8")); } catch { /* not written yet */ }
    worker.answering = !!(info && (await ping(info))?.ready);
  }
  return { worker };
}

let starting = null;
/** Starts the worker unless it runs, and writes the endpoint; one start at a time. */
function start(settings) {
  starting ??= launch(settings).finally(() => { starting = null; });
  return starting;
}

async function launch(settings) {
  if (!settings.bundle || !settings.image) throw refuse("InvalidInput", "the macOS worker needs image and bundle");
  const expected = build(settings), container = name(settings);
  let row = await docker(settings, ["inspect", "-f", '{{.State.Running}}\t{{index .Config.Labels "layerfs.build"}}\t{{index .Config.Labels "layerfs.home"}}', container]).catch(() => null);
  const [running, label, home] = row?.split("\t") ?? [];
  if (home && home !== settings.home) throw refuse("Busy", `the worker ${container} serves the LayerFS home ${home}; give this home its own containerPrefix`);
  if (row && label !== expected) {
    await docker(settings, ["rm", "-f", container]);
    row = null;
  }
  let secret;
  if (!row) {
    secret = randomBytes(32).toString("hex");
    // `-e NAME` with no value: docker copies it from its own environment, so
    // the secret is never on a command line (the Mac's process list)
    await docker(settings, ["run", "-d", "--pull", "never", "--name", container, "--init", "--restart", "unless-stopped",
      "--label", "layerfs.role=worker", "--label", `layerfs.build=${expected}`, "--label", `layerfs.home=${settings.home}`,
      "-p", `127.0.0.1::${WORKER_PORT}`, "-e", "LAYERFS_WORKER_SECRET", "-e", `LAYERFS_HOME=${settings.home}`, "-e", `LAYERFS_BUNDLE=${settings.bundle}`,
      ...mounts(settings), "--entrypoint", "node", settings.image, join(settings.bundle, SERVER)], { timeout: 30_000, env: { ...process.env, LAYERFS_WORKER_SECRET: secret } });
  } else {
    if (running === "false") await docker(settings, ["start", container]);
    const env = await docker(settings, ["inspect", "-f", "{{range .Config.Env}}{{println .}}{{end}}", container]);
    secret = /^LAYERFS_WORKER_SECRET=([0-9a-f]{64})$/m.exec(env)?.[1];
    if (!secret) throw refuse("Failed", `the worker ${container} has no secret`);
  }
  // "127.0.0.1:55012" (a second line for IPv6 when Docker adds one)
  const port = Number(/:(\d+)$/m.exec(await docker(settings, ["port", container, `${WORKER_PORT}/tcp`]))?.[1]);
  if (!port) throw refuse("Failed", `the worker ${container} publishes no port`);
  const info = {
    home: settings.home, port, secret, build: expected, daemonHost: DAEMON_HOST, docker: VM_DOCKER,
    init: { argv: ["-H", `unix://${settings.docker}`, "run", "--rm", "-i", "--pull", "never", ...mounts(settings), "--entrypoint", "node"], image: settings.image, script: join(settings.bundle, INIT) },
  };
  // hashing the bundle comes first in the worker, so its readiness takes a while
  await ready(info, READY_MS, `the worker ${container}`);
  mkdirSync(join(settings.home, "worker"), { recursive: true, mode: 0o700 });
  writeEndpoint(endpointFile(settings.home), info);
  return status(settings);
}

/** One `docker restart`, then its new port and readiness. Every project's Store calls fail meanwhile. */
async function restart(settings) {
  const { worker } = await status(settings);
  if (worker.state !== "running") throw refuse("InvalidInput", `the worker is ${worker.state}; start it instead`);
  rmSync(endpointFile(settings.home), { force: true });
  await docker(settings, ["restart", "-t", "5", worker.name], { timeout: RESTART_MS });
  return start(settings);
}

const platform = {
  name: "macos",
  endpoint: (settings) => endpointFile(settings.home),
  status,
  start,
  restart,
  /** Init runs in its own container beside the worker, which must run. */
  async init(settings) {
    const { worker } = await status(settings);
    if (worker.state !== "running" || !worker.answering) await start(settings);
  },
  path: (settings, hostPath) => hostPath,
  docker: cli,
  storePath: (settings, volume) => `${VOLUMES}/${volume}/_data/global/store.sqlite`,
  homeNames: (settings) => homeNames(settings.home),
  async removeHome(settings, project) { rmSync(join(settings.home, project), { recursive: true, force: true }); },
  async releaseStore() {},
  /** Free bytes of Docker's volume disk, as the worker sees it. */
  async free(settings) {
    try {
      const out = await docker(settings, ["exec", name(settings), "df", "-kP", VOLUMES]);
      const kb = Number(out.split("\n").pop().trim().split(/\s+/)[3]);
      return Number.isFinite(kb) ? kb * 1024 : null;
    } catch { return null; }
  },
};

export default platform;

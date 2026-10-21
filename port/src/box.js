// The Store container of ported Live projects (macOS; binding.js),
// `<prefix>store` (`layerfs-store` by default, #373): one long-lived Linux
// container per LayerFS home whose main process is the worker (worker.mjs)
// that makes every ported SDK call with the real Linux binding
// (owner direction of 2026-10-20, AGENTS.md). The worker's port is published
// to the Mac's loopback only, and its secret and port are written to the
// LayerFS home (storeFile) for the Host's and agents' processes. It is
// labelled with its worker's code and its build, so the Host replaces it when
// either changes; the Host starts it when it starts, before any dsh run.
// A box of the older naming (`dsh-layerfs-box-…`) on the same home goes then.
// It mounts only what the calls use, each at its own path so a path means the
// same inside as outside, for Docker too: the Docker socket, Docker's volume
// directory (inside Docker's VM), the LayerFS home (writable), the LayerFS
// build and this folder (read-only). Never a broad host folder: Docker Desktop
// forwards every file event under a shared folder into its VM, and sharing
// /Users stalled it (2026-10-20). Init, the one call that reads a source
// folder, runs in its own container with that folder (binding.js).
import { execFile } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { frame, readFrames } from "./frames.mjs";

const run = promisify(execFile);
export const VOLUMES = "/var/lib/docker/volumes";
/** This folder, which every call's container mounts for worker.mjs and run.mjs. */
export const PORT = dirname(fileURLToPath(import.meta.url));
/** Where a Store container process reaches a daemon's loopback-published port: the Mac. */
export const DAEMON_HOST = "host.docker.internal";
const WORKER_PORT = 7070;
const READY_MS = 15_000;

/** The Docker socket of a deployment that names none, as the SDK defaults it. */
export const DEFAULT_DOCKER = "/var/run/docker.sock";
/** Where a ported call's process reaches Docker: the engine's own socket, mounted from its VM. */
export const CALL_DOCKER = "/var/run/docker.sock";

/** `docker run` mounts and settings of a ported call's process: the Store container's worker, or Init's own. */
export const callMounts = ({ home, bundle }) => [
  "-v", `${CALL_DOCKER}:${CALL_DOCKER}`, "-v", `${VOLUMES}:${VOLUMES}`,
  "-v", `${home}:${home}`, "-v", `${bundle}:${bundle}:ro`, "-v", `${PORT}:${PORT}:ro`,
  "-e", `LAYERFS_NATIVE=${join(bundle, "layerfs.node")}`,
];

/** Where a ported call opens the Store of the project volume `volume`. */
export const storePathOf = (volume) => `${VOLUMES}/${volume}/_data/global/store.sqlite`;

/** The Store container's name: one per LayerFS home, whose container prefix sets it apart. */
export const storeName = (prefix) => `${prefix}store`;

/** The file that tells a home's processes how to reach its Store container's worker: { box, port, secret }. */
export const storeFile = (home) => join(home, ".layerfs-store.json");
/** The file of the older naming, removed when the Store container starts. */
const OLD_FILE = ".dsh-layerfs-box.json";

/** A short id of a build folder, as the Store container's label. */
export const buildOf = (bundle) => createHash("sha256").update(bundle).digest("hex").slice(0, 12);

/**
 * The worker's code and the folder it runs from, as the Store container's
 * label: a changed worker replaces it, and so does a moved one, since the
 * container mounts and starts the worker from the folder it was made with.
 */
const workerVersion = () => {
  const hash = createHash("sha256").update(PORT);
  for (const file of ["worker.mjs", "call.mjs", "frames.mjs"]) hash.update(readFileSync(join(PORT, file)));
  return hash.digest("hex").slice(0, 12);
};

let starting = null;
/** Starts the Store container and its worker unless they run, and writes storeFile; one start at a time. */
export function ensureBox(spec) {
  starting ??= start(spec).finally(() => { starting = null; });
  return starting;
}

async function start(spec) {
  const docker = (...args) => run("docker", ["-H", `unix://${spec.docker}`, ...args]).then((r) => r.stdout.trim());
  const version = workerVersion(), build = buildOf(spec.bundle);
  // the older naming's box on this home (its home folder is mounted): replaced
  const old = (await docker("ps", "-aq", "--filter", "label=dsh-layerfs.box=1", "--filter", `volume=${spec.home}`)).split("\n").filter(Boolean);
  if (old.length) await docker("rm", "-f", ...old);
  rmSync(join(spec.home, OLD_FILE), { force: true });
  let state = await docker("inspect", "-f", '{{.State.Running}} {{index .Config.Labels "layerfs.worker"}} {{index .Config.Labels "layerfs.build"}} {{index .Config.Labels "layerfs.home"}}', spec.box).catch(() => null);
  const home = state?.split(" ").slice(3).join(" ");
  // another home's: never replaced, since that home's runs use it
  if (home && home !== spec.home) throw new Error(`the Store container ${spec.box} serves the LayerFS home ${home}; give this home its own containerPrefix`);
  if (state && (state.split(" ")[1] !== version || state.split(" ")[2] !== build || !home)) {
    // another worker or another build (or one made before the home label): replaced
    await docker("rm", "-f", spec.box);
    state = null;
  }
  let secret;
  if (!state) {
    secret = randomBytes(32).toString("hex");
    await docker("run", "-d", "--pull", "never", "--name", spec.box, "--init",
      // Docker starts it again after its own restart; the Host's readiness check then finds its new port
      "--restart", "unless-stopped",
      "--label", "layerfs.role=store", "--label", `layerfs.worker=${version}`, "--label", `layerfs.build=${build}`, "--label", `layerfs.home=${spec.home}`,
      "-p", `127.0.0.1::${WORKER_PORT}`, "-e", `DSH_BOX_SECRET=${secret}`, "-e", "UV_THREADPOOL_SIZE=64",
      ...callMounts(spec), "--entrypoint", "node", spec.image, join(PORT, "worker.mjs"));
  } else {
    if (state.startsWith("false")) await docker("start", spec.box);
    const env = await docker("inspect", "-f", "{{range .Config.Env}}{{println .}}{{end}}", spec.box);
    secret = /^DSH_BOX_SECRET=([0-9a-f]+)$/m.exec(env)?.[1];
    if (!secret) throw new Error(`the Store container ${spec.box} has no secret`);
  }
  // "127.0.0.1:55012" (a second line for IPv6 when Docker adds one)
  const port = Number(/:(\d+)$/m.exec(await docker("port", spec.box, `${WORKER_PORT}/tcp`))?.[1]);
  if (!port) throw new Error(`the Store container ${spec.box} publishes no worker port`);
  await listening(port, secret, spec.box);
  const file = storeFile(spec.home);
  writeFileSync(`${file}.tmp`, JSON.stringify({ box: spec.box, port, secret }), { mode: 0o600 });
  renameSync(`${file}.tmp`, file);
}

/** Waits until the worker answers a ping (a readiness wait, bounded): Docker's proxy may accept before it listens. */
async function listening(port, secret, box) {
  for (const until = Date.now() + READY_MS; ; ) {
    const ok = await new Promise((resolve) => {
      const socket = connect({ host: "127.0.0.1", port }, () => socket.write(Buffer.concat([frame({ secret }), frame({ id: 1, fn: "ping" })])));
      const done = (value) => { clearTimeout(timer); socket.destroy(); resolve(value); };
      const timer = setTimeout(() => done(false), 1000);
      readFrames(socket, (value) => { if (value?.id === 1) done(value.answer?.value === "ready"); }, () => done(false));
      socket.on("error", () => done(false));
      socket.on("close", () => done(false));
    });
    if (ok) return;
    if (Date.now() > until) throw new Error(`the Store container ${box}'s worker did not answer within ${READY_MS / 1000} s`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

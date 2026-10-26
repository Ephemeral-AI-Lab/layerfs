// Runs a verify script's `inner` part in the #352 runtime image (Node 24,
// Linux), as the direct route needs (#352 F-020): the checkout at its host
// path, the binaries' folders read-only, the Docker socket, and a Store volume
// at /layerfs-store. The client dials daemons through LAYERFS_DAEMON_HOST
// unless `env` sets it to "". Every container the inner run names starts
// with `prefix` and is removed afterwards; the volume is the caller's.
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
export const RUNTIME = "layerfs-352-runtime:node24";
export const STORE_PATH = "/layerfs-store/global/store.sqlite";

export const docker = (args, options = {}) => spawnSync("docker", args, { encoding: "utf8", ...options });

/** One inner run; returns `{ status, out }` (stdout then stderr). */
export function inRuntime({ script, args = [], daemon, addon, image, volume, prefix, env = {}, timeout = 100_000, mounts = [] }) {
  const all = {
    LAYERFS_DOCKER: "/var/run/docker.sock", LAYERFS_IMAGE: image, LAYERFS_VOLUME: volume,
    LAYERFS_DAEMON: daemon, LAYERFS_NATIVE: addon, LAYERFS_CONTAINER_PREFIX: prefix,
    LAYERFS_CONSTRUCTION_WORKERS: "1", LAYERFS_STORE_PATH: STORE_PATH,
    LAYERFS_DAEMON_HOST: "host.docker.internal", ...env,
  };
  const folders = [...new Set([dirname(daemon), dirname(addon)])].flatMap((f) => ["-v", `${f}:${f}:ro`]);
  const run = docker(["run", "--rm", "--name", `layerfs-issue352-${prefix}client`,
    "-v", `${REPO}:${REPO}`, ...folders, ...mounts.flatMap((m) => ["-v", m]),
    "-v", "/var/run/docker.sock:/var/run/docker.sock", "-v", `${volume}:/layerfs-store`,
    ...Object.entries(all).filter(([, v]) => v !== "").flatMap(([k, v]) => ["-e", `${k}=${v}`]),
    "-w", REPO, "--entrypoint", "node", RUNTIME, script, "inner", ...args], { timeout });
  const left = docker(["ps", "-aq", "--filter", `name=^${prefix}`]).stdout.trim().split("\n").filter(Boolean);
  if (left.length) docker(["rm", "-f", ...left]);
  return { status: run.status, signal: run.signal, out: `${run.stdout ?? ""}${run.stderr ?? ""}`.trim() };
}

// Apple Container (`container`, #512 §6, V3's third adapter). The VM is one
// container of the `container` CLI; the Store is a named volume the CLI keeps
// across stop and start; the private stream is `-p 127.0.0.1:H:G`.
//
// NOT VERIFIED. `container` is not installed on the machine H37 was written on
// (macOS 27.2 / M2, 2026-10-31), so every command shape below comes from
// Apple's CLI documentation and #512's use of it, not from a run. The flow
// around it is shared with the verified `microsandbox.js` provider (vm/index.js);
// what is unverified is this file. It therefore reads its facts strictly: an
// `inspect` answer that is not the shape this mapping knows is refused by name
// rather than guessed at, so a changed or misread CLI shows up as an error and
// never as a VM the adapter believes is running. Verify with:
//   msb-free: `container run --rm --name h37check node:22-trixie-slim true`
//   then `container inspect --format json h37check`, and compare the fields
//   read below; run `api/runtime/test/vm_provider.mjs apple-container` against
//   the real CLI.
import { answer, capture, readFile, writeFile } from "./cli.js";

const CLI = "container";
const CREATE_MS = 120_000, CALL_MS = 30_000, EXEC_MS = 60_000;
/** The guest kernel shares FUSE, cgroup and loopback with these capabilities (#512: they were required). */
const CAPABILITIES = ["--cap-add", "SYS_ADMIN", "--cap-add", "SYS_RESOURCE", "--cap-add", "NET_ADMIN"];

const call = (args, options = {}) => answer(CLI, args, { timeout: CALL_MS, ...options });

/** One `container exec`, plain streams. The worker detaches itself with a shell, so this ends with it. */
const exec = (name, argv, { input = undefined, timeout = EXEC_MS } = {}) => capture(CLI, ["exec", name, ...argv], { input, timeout });

const STRANGE = (what) => new Error(`the Apple Container CLI's ${what} is not the shape this unverified adapter knows; check the CLI's output before using it`);

/** The VM's record, or null when it does not exist. See the header: strict, never guessed. */
export async function inspect(name) {
  let text;
  try {
    text = await call(["inspect", "--format", "json", name]);
  } catch (error) {
    const said = `${error.stderr ?? ""}${error.message ?? ""}`;
    if (/not found|no such (container|sandbox)/i.test(said)) return null;
    throw error;
  }
  const parsed = JSON.parse(text);
  const record = Array.isArray(parsed) ? parsed[0] : parsed;
  if (!record || typeof record !== "object") throw STRANGE("inspect");
  const configuration = record.configuration ?? record.config;
  const status = typeof record.status === "string" ? record.status : undefined;
  if (typeof status !== "string" || !configuration) throw STRANGE("inspect (status or configuration)");
  const mounts = configuration.mounts ?? [];
  if (!Array.isArray(mounts)) throw STRANGE("inspect (mounts)");
  const mounted = (guest) => mounts.find((mount) => mount.destination === guest || mount.target === guest) ?? null;
  const ports = configuration.publishedPorts ?? configuration.ports ?? [];
  if (!Array.isArray(ports)) throw STRANGE("inspect (publishedPorts)");
  return {
    state: status.toLowerCase(),
    image: configuration.image?.reference ?? configuration.image ?? null,
    digest: configuration.image?.descriptor?.digest ?? null,
    labels: configuration.labels ?? {},
    ports: ports.map((port) => ({ guest: Number(port.containerPort ?? port.guestPort), host: Number(port.hostPort), bind: port.hostAddress ?? port.hostIp ?? null })),
    home: mounted("/layerfs/home")?.source ?? null,
    store: mounted("/layerfs/store")?.source ?? null,
    created: record.createdAt ?? null,
  };
}

export async function create(config) {
  await call([
    "run", "--detach", "--name", config.name, "--cpus", String(config.cpus), "--memory", config.memory,
    ...CAPABILITIES,
    "--mount", `type=volume,source=${config.storeName},target=/layerfs/store`,
    "--mount", `type=bind,source=${config.home},target=/layerfs/home`,
    "--publish", `127.0.0.1:${config.hostPort}:${config.workerPort}`,
    "--label", "layerfs.role=vm", "--label", `layerfs.home=${config.home}`,
    config.image, "sleep", "infinity",
  ], { timeout: CREATE_MS });
  return inspect(config.name);
}

export const start = (name) => call(["start", name], { timeout: CREATE_MS });
export const stop = (name) => call(["stop", name]);
export const restart = async (name) => { await stop(name); await start(name); };
/** The container record only: the Store volume is a separate object and is never removed here. */
export const remove = (name) => call(["delete", "--force", name], { timeout: CREATE_MS });
export const removeStore = (storeName) => call(["volume", "delete", storeName]);

export async function storeFree(name) {
  const { code, stdout } = await exec(name, ["df", "-kP", "/layerfs/store"]);
  if (code !== 0) return null;
  const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

/** Copies a host folder into a guest folder. */
export const copy = (name, source, destination) => call(["copy", source, `${name}:${destination}`], { timeout: CREATE_MS });

export const read = (name, path) => readFile(CLI, name, path, exec);
export const write = (name, path, text) => writeFile(CLI, name, path, text, exec);

export default {
  name: "apple-container",
  defaultImage: "node:22-trixie-slim",
  inspect, create, start, stop, restart, remove, removeStore, storeFree, read, write, copy, exec,
};

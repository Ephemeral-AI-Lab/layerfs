// Apple Container (`container`, #515 §6, V3's third adapter). The VM is one
// container of the `container` CLI; the Store is a named volume the CLI keeps
// across stop and start, mounted at the Store directory; the private stream is
// `-p 127.0.0.1:H:G`.
//
// Verified against `container` 1.6.1 (build aa8d76b) on macOS 27.2 / M2,
// 2026-10-31, one VM per Store. What the CLI actually does, and how this file
// answers it:
//   - `inspect NAME` prints JSON by default (it has no `--format`): an
//     **array** of `[{ id, status: { state,
//     startedDate, networks }, configuration: { image, labels, mounts,
//     publishedPorts, creationDate, … } }]`. `status` is an object, not a
//     string; mounts carry `destination`/`source`/`type.volume.name`; ports are
//     `publishedPorts[].{containerPort,hostPort,hostAddress}`; the image
//     reference and digest are `image.reference`/`image.descriptor.digest`.
//   - a volume keeps its size: `volume create -s <size>` (the CLI's default is
//     a sparse 504 GiB image), so `ensureStore` sizes it from `storeSize`;
//   - `exec` needs `-i` for a caller's stdin (`cat > file` gets EOF at once
//     without it), which `write` relies on;
//   - `copy` moves about 35 MB/s, so a folder goes in through the home bind
//     mount instead (see `copy` below);
//   - the guest's `/proc/sys` is mounted **read-only** and user namespaces are
//     open by default: V4's block needs `mount -o remount,rw /proc/sys` first
//     (CAP_SYS_ADMIN, which this adapter adds anyway), then the write;
//   - `delete --force` ends a running container; `start`/`stop` keep the
//     published port; `volume delete` removes the Store disk.
// It reads its facts strictly: an `inspect` answer that is not this shape is
// refused by name rather than guessed at, so a changed CLI shows up as an error
// and never as a VM this adapter believes is running.
import { constants, cpSync, mkdtempSync, rmSync } from "node:fs";
import { basename, isAbsolute, join, relative, resolve, sep } from "node:path";
import { GUEST } from "../../settings.js";
import { answer, capture, readFile, writeFile } from "./cli.js";

const CLI = "container";
const CREATE_MS = 120_000, CALL_MS = 30_000, EXEC_MS = 60_000;
/** The guest kernel shares FUSE, cgroup and loopback with these capabilities (#512: they were required). */
const CAPABILITIES = ["--cap-add", "SYS_ADMIN", "--cap-add", "SYS_RESOURCE", "--cap-add", "NET_ADMIN"];

const call = (args, options = {}) => answer(CLI, args, { timeout: CALL_MS, ...options });

/** One `container exec`, plain streams, stdin kept open when the caller sends some. */
const exec = (name, argv, { input = undefined, timeout = EXEC_MS } = {}) =>
  capture(CLI, ["exec", ...(input === undefined ? [] : ["-i"]), name, ...argv], { input, timeout });

const STRANGE = (what) => new Error(`the Apple Container CLI's ${what} is not the shape this adapter knows; check the CLI's output before using it`);

/** The VM's record, or null when it does not exist. See the header: strict, never guessed. */
export async function inspect(name) {
  let text;
  try {
    text = await call(["inspect", name]);
  } catch (error) {
    const said = `${error.stderr ?? ""}${error.message ?? ""}`;
    if (/not found|no such (container|sandbox)/i.test(said)) return null;
    throw error;
  }
  const parsed = JSON.parse(text);
  const record = Array.isArray(parsed) ? parsed[0] : parsed;
  if (!record || typeof record !== "object") throw STRANGE("inspect");
  const configuration = record.configuration;
  const status = record.status;
  if (!configuration || typeof status !== "object" || typeof status?.state !== "string") throw STRANGE("inspect (status or configuration)");
  const mounts = configuration.mounts ?? [];
  if (!Array.isArray(mounts)) throw STRANGE("inspect (mounts)");
  const mounted = (guest) => mounts.find((mount) => mount.destination === guest) ?? null;
  const ports = configuration.publishedPorts ?? [];
  if (!Array.isArray(ports)) throw STRANGE("inspect (publishedPorts)");
  return {
    state: status.state.toLowerCase(),
    image: configuration.image?.reference ?? null,
    digest: configuration.image?.descriptor?.digest ?? null,
    labels: configuration.labels ?? {},
    ports: ports.map((port) => ({ guest: Number(port.containerPort), host: Number(port.hostPort), bind: port.hostAddress ?? null })),
    home: mounted("/layerfs/home")?.source ?? null,
    // a named Store volume is named in the mount's type, and its source is the
    // volume's image file on the host
    store: mounted("/layerfs/store")?.type?.volume?.name ?? null,
    created: configuration.creationDate ?? null,
  };
}

/** Newly created named Store volumes in this session; cleaned up if create fails. */
const newlyCreatedStores = new Set();

/**
 * The Store disk: a named volume of `size`, made once. An existing volume keeps
 * the size it was made with (the CLI has no resize), so only its presence is
 * checked here.
 */
export async function ensureStore(name, size) {
  const found = await capture(CLI, ["volume", "inspect", name]);
  if (found.code === 0) return;
  const made = await capture(CLI, ["volume", "create", "-s", size, name], { timeout: CREATE_MS });
  if (made.code !== 0) throw new Error(`could not create the Store volume ${name}: ${(made.stderr || made.stdout).trim()}`);
  newlyCreatedStores.add(name);
}

export async function create(config) {
  const wasNew = newlyCreatedStores.has(config.storeName);
  const existedBefore = wasNew ? false : (await capture(CLI, ["volume", "inspect", config.storeName])).code === 0;
  try {
    await call([
      "run", "--detach", "--name", config.name, "--cpus", String(config.cpus), "--memory", config.memory,
      ...CAPABILITIES,
      "--mount", `type=volume,source=${config.storeName},target=/layerfs/store`,
      "--mount", `type=bind,source=${config.home},target=/layerfs/home`,
      "--publish", `127.0.0.1:${config.hostPort}:${config.workerPort}`,
      "--label", "layerfs.role=vm", "--label", `layerfs.home=${config.home}`,
      config.image, "sleep", "infinity",
    ], { timeout: CREATE_MS });
    newlyCreatedStores.delete(config.storeName);
    return inspect(config.name);
  } catch (error) {
    if (wasNew || !existedBefore) {
      newlyCreatedStores.delete(config.storeName);
      await removeStore(config.storeName).catch(() => {});
    }
    throw error;
  }
}

export const start = (name) => call(["start", name], { timeout: CREATE_MS });
export const stop = (name) => call(["stop", name]);
export const restart = async (name) => { await stop(name); await start(name); };
/** The container record only: the Store volume is a separate object and is never removed here. */
export const remove = (name) => call(["delete", "--force", name], { timeout: CREATE_MS });
export const removeStore = (storeName) => call(["volume", "delete", storeName]);

/**
 * V4: the VM-wide user-namespace block. The guest mounts `/proc/sys`
 * read-only, so it is remounted read-write first; the caller reads the sysctl
 * back, so a block that did not take is reported, not assumed.
 */
export const blockUserNamespaces = (name) =>
  exec(name, ["sh", "-c", "mount -o remount,rw /proc/sys 2>/dev/null; echo 0 > /proc/sys/user/max_user_namespaces"]);

/** Free bytes of the VM's Store volume, or null when `df` cannot answer. */
export async function storeFree(name) {
  const { code, stdout } = await exec(name, ["df", "-kP", "/layerfs/store"]);
  if (code !== 0) return null;
  const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

/**
 * Copies a host folder into a guest folder, as a copy the VM owns. Measured on
 * 1.6.1 (#527, a 2.7 GB bundle): `container copy` moved about 35 MB/s (77 s),
 * a `tar` stream through `exec -i` about 117 MB/s (23.5 s), while the guest
 * reads the VM's home bind mount (virtiofs) at about 650 MB/s (4 s). So the
 * folder is cloned into a private staging folder in the home the VM already
 * mounts (an APFS clone where the volume allows, else a plain copy), the guest
 * copies it from there onto its own disk with `cp -Rp` (modes, owners and
 * times kept, symlinks as symlinks), and the staging folder is removed
 * whatever the outcome. One attempt; no step is retried.
 */
export async function copy(name, source, destination) {
  const found = await inspect(name);
  if (!found?.home) throw new Error(`the VM ${name} mounts no LayerFS home at ${GUEST.home}, so a folder cannot be staged for it`);
  const guestCopy = async (from) => {
    const done = await exec(name, ["sh", "-c", "mkdir -p \"$2\" && cp -Rp \"$1/.\" \"$2\"", "sh", from, destination], { timeout: CREATE_MS });
    if (done.code !== 0) throw new Error(`could not copy ${source} into ${name}:${destination}: ${(done.stderr || done.stdout).trim()}`);
  };
  // a folder inside the home is already in the guest's view: nothing to stage
  const inside = relative(found.home, resolve(source));
  if (inside && !inside.startsWith("..") && !isAbsolute(inside)) return guestCopy(`${GUEST.home}/${inside.split(sep).join("/")}`);
  const stage = mkdtempSync(join(found.home, ".vm-copy-"));
  try {
    cpSync(source, join(stage, "c"), { recursive: true, mode: constants.COPYFILE_FICLONE, verbatimSymlinks: true, preserveTimestamps: true });
    await guestCopy(`${GUEST.home}/${basename(stage)}/c`);
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

export const read = (name, path) => readFile(CLI, name, path, exec);
export const write = (name, path, text) => writeFile(CLI, name, path, text, exec);

export default {
  name: "apple-container",
  defaultImage: "node:22-trixie-slim",
  inspect, ensureStore, create, start, stop, restart, remove, removeStore, blockUserNamespaces, storeFree, read, write, copy, exec,
};

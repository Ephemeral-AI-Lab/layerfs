// Microsandbox (`msb` 0.7.8, libkrun microVMs): the VM is one sandbox record
// and the Store is a named `disk` volume that survives stop and start (V8).
// #512 read this provider's facts; the gaps it named are handled here:
//   - the name is the only identity: every read goes through `msb inspect` by
//     name, and a record that is not ours (another home) is refused, never
//     reused;
//   - a failed create can leave hidden state under the name, so `create` is
//     called once and its failure is reported as it is (the message says what
//     `msb` suggests); nothing is replayed or replaced on a guess;
//   - `msb exec -u` capability drop is unverified, so the worker, the helper
//     and the daemon run as root (the guest's only identity) and the
//     namespaces backend drops each command itself (#515 §4.3).
// One CLI call per call, one attempt, no retry.
import { answer, capture, readFile, writeFile } from "./cli.js";

const CLI = "msb";
const CREATE_MS = 120_000, CALL_MS = 30_000, EXEC_MS = 60_000;

const call = (args, options = {}) => answer(CLI, args, { timeout: CALL_MS, ...options });

/**
 * One `msb exec` in the VM. `--no-tty` keeps the streams plain (stdout and
 * stderr arrive separately). The worker detaches itself with a shell, so this
 * call ends when its shell does.
 */
const exec = (name, argv, { input = undefined, timeout = EXEC_MS } = {}) =>
  capture(CLI, ["exec", name, "--no-tty", "--", ...argv], { input, timeout });

/** The VM's record as `msb inspect --format json` gives it, or null when it does not exist. */
export async function inspect(name) {
  let text;
  try {
    text = await call(["inspect", "--format", "json", name]);
  } catch (error) {
    const said = `${error.stderr ?? ""}${error.message ?? ""}`;
    if (/sandbox not found|no such sandbox/i.test(said)) return null;
    throw error;
  }
  const record = JSON.parse(text);
  const config = record.active_config ?? record.config ?? {};
  const mounted = (guest) => (config.mounts ?? []).find((mount) => mount.guest === guest) ?? null;
  return {
    state: String(record.status ?? "").toLowerCase(),
    image: config.image?.Oci?.reference ?? null,
    digest: config.image?.Oci?.manifest_digest ?? null,
    labels: config.labels ?? {},
    ports: (config.network?.ports ?? []).map((port) => ({ guest: port.guest_port, host: port.host_port, bind: port.host_bind })),
    home: mounted("/layerfs/home")?.host ?? null,
    store: mounted("/layerfs/store")?.name ?? null,
    created: record.created_at ?? null,
  };
}

/**
 * Step ①: the VM, from `config` when it does not exist yet. Answers the record
 * read back after the call, so a create that ended without a usable VM is
 * reported by the caller from fact, not from the command's exit code.
 */
export async function create(config) {
  await call([
    "create", "--name", config.name, "--cpus", String(config.cpus), "--memory", config.memory,
    "--mount-named", `${config.storeName}:/layerfs/store:kind=disk,size=${config.storeSize}`,
    "-v", `${config.home}:/layerfs/home`,
    "-p", `127.0.0.1:${config.hostPort}:${config.workerPort}`,
    "--label", "layerfs.role=vm", "--label", `layerfs.home=${config.home}`,
    config.image,
  ], { timeout: CREATE_MS });
  return inspect(config.name);
}

export const start = (name) => call(["start", name], { timeout: CREATE_MS });
export const stop = (name) => call(["stop", name]);
export const restart = (name) => call(["restart", name], { timeout: CREATE_MS });
/** The VM record only: its Store volume is a separate object and is never removed here. */
export const remove = (name) => call(["remove", "--force", name], { timeout: CREATE_MS });
export const removeStore = (storeName) => call(["volume", "remove", storeName]);

/**
 * V4: the VM-wide user-namespace block. Microsandbox's guest keeps
 * `/proc/sys` writable; the caller reads the sysctl back, so a block that did
 * not take is reported, not assumed.
 */
export const blockUserNamespaces = (name) => exec(name, ["sh", "-c", "echo 0 > /proc/sys/user/max_user_namespaces"]);

/** Free bytes of the VM's Store disk, or null when `df` cannot answer. */
export async function storeFree(name) {
  const { code, stdout } = await exec(name, ["df", "-kP", "/layerfs/store"]);
  if (code !== 0) return null;
  const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

/** Copies a host folder's contents into a guest folder. */
export const copy = (name, source, destination) => call(["copy", source, `${name}:${destination}`], { timeout: CREATE_MS });

export const read = (name, path) => readFile(CLI, name, path, exec);
export const write = (name, path, text) => writeFile(CLI, name, path, text, exec);

export default {
  name: "microsandbox",
  defaultImage: "node:22-trixie-slim",
  inspect, create, start, stop, restart, remove, removeStore, blockUserNamespaces, storeFree, read, write, copy, exec,
};

// `vm:<provider>` placement (#515 §6, V9): this LayerFS home lives in one
// Linux VM on this machine. The VM holds every project's Store on an ext4 disk
// it shares with the Sandboxes' daemons, and the Sandboxes themselves are Linux
// namespaces inside it (`LAYERFS_SANDBOX_BACKEND=namespaces`, H36); the host
// process reaches only the worker, the way macOS and Windows already do
// (`worker/server.js`, one attempt per call, no replay).
//
// This module is the platform object the runtime uses instead of the host's.
// Its five steps, each attempted once (§6):
//   ① ensure the VM exists and runs, with its Store disk attached;
//   ② copy the sealed bundle in, once per bundle hash;
//   ③ start the worker as root with its secret, and block user namespaces
//      (V4: `user.max_user_namespaces=0`); the namespaces backend refuses a
//      host where that is not set, so step ③ also proves it;
//   ④ open the private stream, `-p 127.0.0.1:<host>:<guest>`, and write the
//      endpoint `useWorker` reads;
//   ⑤ stop the VM, or delete it: a delete never destroys a Store that was not
//      pushed (V8), and there is no remote Store push yet (#506).
// The provider module behind it (`microsandbox.js`, `apple-container.js`) knows
// the platform; this file knows LayerFS's side and nothing platform-specific.
// Guest paths are in settings.js (GUEST): the home is a bind mount of the
// host's, the Store is the VM's own disk, and the bundle, Init sources, worker
// state and Sandbox directories live on the VM's root disk.
import { createHash, randomBytes } from "node:crypto";
import { createServer } from "node:net";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { connection } from "@layerfs/agent/internal";
import { endpointFile, homeNames, ping, ready, refuse, writeEndpoint } from "../shared.js";
import { callHome, GUEST } from "../../settings.js";
import microsandbox from "./microsandbox.js";
import appleContainer from "./apple-container.js";

const PROVIDERS = { microsandbox, "apple-container": appleContainer };
const WORKER_PORT = 7070;
const SERVER = "api/runtime/src/worker/server.js";
/** Bundle hashing in the worker comes first, so readiness takes a while. */
const READY_MS = 60_000;

const providerOf = (settings) => {
  const provider = PROVIDERS[settings.vm.provider];
  if (!provider) throw refuse("Unsupported", `no VM provider is implemented for vm:${settings.vm.provider}`, null);
  return provider;
};
const nameOf = (settings) => `${settings.containerPrefix}vm`;
const storeNameOf = (settings) => `${settings.containerPrefix}vm-store`;

/** A host loopback port this process just freed: the VM's published worker port. */
const freePort = () => new Promise((resolve, reject) => {
  const server = createServer();
  server.on("error", reject);
  server.listen(0, "127.0.0.1", () => {
    const { port } = server.address();
    server.close(() => resolve(port));
  });
});

/**
 * Step ①: the VM runs, or it is created once from the home's settings. A record
 * under our name that is not this home's, or does not match what this home
 * needs, is refused: nothing is replaced, restarted or repaired on a guess.
 */
async function ensure(settings) {
  const provider = providerOf(settings), name = nameOf(settings), storeName = storeNameOf(settings);
  const current = await provider.inspect(name);
  if (current) {
    if (current.labels?.["layerfs.home"] && current.labels["layerfs.home"] !== settings.home) {
      throw refuse("Busy", `the VM ${name} serves the LayerFS home ${current.labels["layerfs.home"]}; give this home its own containerPrefix`, null);
    }
    if (current.store && current.store !== storeName) throw refuse("Failed", `the VM ${name} has the Store volume ${current.store}, not ${storeName}; delete it before using this home`, null);
    if (current.home && current.home !== settings.home) throw refuse("Failed", `the VM ${name} mounts ${current.home} as its home, not ${settings.home}; delete it before using this home`, null);
    const port = current.ports?.find((entry) => entry.guest === WORKER_PORT)?.host ?? null;
    if (current.state === "running") {
      if (!port) throw refuse("Failed", `the VM ${name} publishes no host port for ${WORKER_PORT}; delete it and start again`, null);
      return { ...current, name, port };
    }
    if (current.state !== "stopped") throw refuse("Failed", `the VM ${name} is ${current.state}; stop or delete it before using this home`, null);
    await provider.start(name);
    const started = await provider.inspect(name);
    if (started?.state !== "running") throw refuse("Failed", `the VM ${name} did not start (${started?.state ?? "missing"})`, null);
    const again = started.ports?.find((entry) => entry.guest === WORKER_PORT)?.host ?? null;
    if (!again) throw refuse("Failed", `the VM ${name} publishes no host port for ${WORKER_PORT}; delete it and start again`, null);
    return { ...started, name, port: again };
  }
  const hostPort = await freePort();
  const created = await provider.create({
    name, image: settings.vm.image, cpus: settings.vm.cpus, memory: settings.vm.memory,
    storeName, storeSize: settings.vm.storeSize, home: settings.home, hostPort, workerPort: WORKER_PORT,
  });
  if (created?.state !== "running") throw refuse("Failed", `the VM ${name} was created but is ${created?.state ?? "missing"}; its record is kept for inspection, not replayed`, null);
  const port = created.ports?.find((entry) => entry.guest === WORKER_PORT)?.host ?? null;
  if (!port) throw refuse("Failed", `the VM ${name} was created without its port forward; delete it before using this home`, null);
  return { ...created, name, port };
}

/** The sealed bundle's identity: its manifest hash, which the worker checks file by file. */
function bundleIdentity(bundle) {
  if (!bundle || !existsSync(join(bundle, "manifest.json"))) throw refuse("InvalidInput", "a VM placement needs a sealed Linux bundle (settings.bundle)", null);
  const text = readFileSync(join(bundle, "manifest.json"), "utf8");
  const manifest = JSON.parse(text);
  if (manifest.status !== "sealed" || manifest.platform !== "linux") throw refuse("InvalidInput", "a VM placement needs a sealed Linux bundle", null);
  return { build: manifest.build_id, hash: createHash("sha256").update(text).digest("hex") };
}

/** Step ②: the bundle, copied in once per hash; a changed bundle replaces the copy. */
async function install(settings, provider, vm) {
  const { build, hash } = bundleIdentity(settings.bundle);
  const marker = await provider.read(vm.name, `${GUEST.bundle}/.sealed`);
  if (marker?.trim() === hash) return { build, copied: false };
  const { code, stderr } = await provider.exec(vm.name, ["rm", "-rf", GUEST.bundle]);
  if (code !== 0) throw refuse("Failed", `could not replace ${GUEST.bundle} in ${vm.name}: ${stderr.trim()}`, null);
  await provider.copy(vm.name, settings.bundle, GUEST.bundle);
  await provider.write(vm.name, `${GUEST.bundle}/.sealed`, `${hash}\n`);
  return { build, copied: true };
}

/**
 * Step ③, the guest's precondition for the namespace backend: its Sandbox
 * directory (`/layerfs/local`) and the VM-wide user-namespace block (V4). The
 * block is read back — the namespaces backend refuses a host without it — so a
 * VM that cannot block them is reported here, by fact.
 */
async function prepareGuest(provider, vm) {
  // the namespaces backend wants a root owned by this user (root), not
  // group/other writable, and traversable by others (0755): the nonroot
  // command identity enters it
  const made = await provider.exec(vm.name, ["sh", "-c", "umask 022 && mkdir -p \"$1\" && chmod 755 \"$1\"", "sh", GUEST.local]);
  if (made.code !== 0) throw refuse("Failed", `could not make ${GUEST.local} in ${vm.name}: ${made.stderr.trim()}`, null);
  const { code, stderr } = await provider.exec(vm.name, ["sh", "-c", "echo 0 > /proc/sys/user/max_user_namespaces"]);
  if (code !== 0) throw refuse("Unsupported", `the VM ${vm.name} cannot block user namespaces (${stderr.trim() || `exit ${code}`}); a Sandbox command could copy the Workspace mount into its own`, null);
  const left = (await provider.read(vm.name, "/proc/sys/user/max_user_namespaces"))?.trim();
  if (left !== "0") throw refuse("Unsupported", `the VM ${vm.name} still allows user namespaces (${left ?? "unreadable"})`, null);
}

/** The worker's own settings, read by `sh` from a 0600 file so the secret is in no command line. */
const workerEnv = (secret) => [
  `LAYERFS_WORKER_SECRET=${secret}`,
  `LAYERFS_WORKER_LISTEN=0.0.0.0:${WORKER_PORT}`,
  `LAYERFS_HOME=${GUEST.home}`,
  `LAYERFS_BUNDLE=${GUEST.bundle}`,
  `LAYERFS_NATIVE=${GUEST.bundle}/layerfs.node`,
  // a Sandbox reopened from its record has no daemon path to look beside
  `LAYERFS_SANDBOX_HELPER=${GUEST.bundle}/layerfs-sandbox-helper`,
  "",
].join("\n");

/**
 * Step ③ and ④: the worker as root, its secret in a 0600 file, then the
 * endpoint once it answers on the private port. A worker left by an earlier
 * start that no longer answers is ended by the pid that start wrote (a bounded
 * cleanup, not a replay); one launch, one readiness wait. A worker that does
 * not answer reports the last lines of its log.
 */
async function launch(settings, provider, vm, build) {
  const secret = randomBytes(32).toString("hex");
  await provider.write(vm.name, `${GUEST.state}/worker.env`, workerEnv(secret));
  const script = [
    // one bounded cleanup: a worker an earlier start left is ended, and its
    // port is waited for, so this start's worker can take it
    `if [ -f ${GUEST.state}/worker.pid ]; then kill "$(cat ${GUEST.state}/worker.pid)" 2>/dev/null;`,
    `for i in $(seq 1 50); do kill -0 "$(cat ${GUEST.state}/worker.pid)" 2>/dev/null || break; sleep 0.1; done; fi;`,
    `set -a; . ${GUEST.state}/worker.env; set +a;`,
    `nohup node ${GUEST.bundle}/${SERVER} >> ${GUEST.state}/worker.log 2>&1 &`,
    `echo $! > ${GUEST.state}/worker.pid`,
  ].join(" ");
  const started = await provider.exec(vm.name, ["sh", "-c", script]);
  if (started.code !== 0) throw refuse("Failed", `the worker did not start in ${vm.name}: ${(started.stderr || started.stdout).trim()}`, null);
  const info = { home: GUEST.home, port: vm.port, secret, build, vm: { provider: settings.vm.provider, name: vm.name } };
  try {
    await ready(info, READY_MS, `the worker in ${vm.name}`);
  } catch (error) {
    const log = (await provider.read(vm.name, `${GUEST.state}/worker.log`).catch(() => null))?.trim().split("\n").slice(-8).join("\n");
    throw refuse("Failed", `${error.message}${log ? `; its log ends: ${log}` : ""}`, null);
  }
  mkdirSync(join(settings.home, "worker"), { recursive: true, mode: 0o700 });
  writeEndpoint(endpointFile(settings.home), info);
  return info;
}

/** The endpoint this home's worker was last written at, or null. */
function endpoint(settings) {
  try { return JSON.parse(readFileSync(endpointFile(settings.home), "utf8")); } catch { return null; }
}

/** Nothing to start when the worker already answers with this home's build. */
async function answering(settings, expected) {
  const info = endpoint(settings);
  if (!info || (expected && info.build !== expected)) return null;
  return (await ping(info))?.ready ? info : null;
}

/** The runtime's `worker.start()`: steps ①–④. */
async function start(settings) {
  const provider = providerOf(settings), { build } = bundleIdentity(settings.bundle);
  const vm = await ensure(settings);
  const held = await answering(settings, build);
  if (held) return status(settings);
  await install(settings, provider, vm);
  await prepareGuest(provider, vm);
  await launch(settings, provider, vm, build);
  return status(settings);
}

/** One `provider restart`, then the steps that a rebooted VM needs again (③, ④). */
async function restart(settings) {
  const provider = providerOf(settings), { build } = bundleIdentity(settings.bundle);
  const vm = await ensure(settings);
  if (provider.restart) await provider.restart(vm.name);
  else { await provider.stop(vm.name); await provider.start(vm.name); }
  rmSync(endpointFile(settings.home), { force: true });
  await install(settings, provider, vm);
  await prepareGuest(provider, vm);
  await launch(settings, provider, vm, build);
  return status(settings);
}

/** The VM's state and, when it runs, whether its worker answers. It starts nothing. */
async function status(settings) {
  const provider = providerOf(settings), name = nameOf(settings);
  let record = null;
  try { record = await provider.inspect(name); } catch (error) { return { worker: { name, state: "unknown", error: error.message } }; }
  const vm = record ? { name, state: record.state, image: record.image, home: record.home, store: record.store, created: record.created } : { name, state: "missing" };
  if (record?.state !== "running") return { worker: { name, state: vm.state, build: null }, vm };
  const info = endpoint(settings);
  const reply = info ? await ping(info) : null;
  return { worker: { name, state: reply?.ready ? "running" : "unknown", answering: !!reply?.ready, build: reply?.build ?? info?.build ?? null, port: info?.port ?? null }, vm };
}

/** Step ⑤: stop; the Store disk and the projects' Stores stay. */
async function stop(settings) {
  const provider = providerOf(settings), name = nameOf(settings);
  const record = await provider.inspect(name);
  if (!record) return { name, state: "missing" };
  if (record.state !== "stopped") await provider.stop(name);
  const after = await provider.inspect(name);
  rmSync(endpointFile(settings.home), { force: true });
  return { name, state: after?.state ?? "missing" };
}

/**
 * Step ⑤, delete (V8): the VM record goes; its Store disk is a separate object
 * and is kept unless `discardStore` says otherwise. A delete that would leave a
 * project without its only Store copy is refused: every project's Store lies on
 * this VM's disk and nowhere else, and no remote Store push exists (#506).
 */
async function remove(settings, options = {}) {
  const provider = providerOf(settings), name = nameOf(settings);
  if (options?.confirm !== "delete") throw refuse("InvalidInput", `a VM delete ends this home's VM; confirm it with { confirm: "delete" }`, null);
  const record = await provider.inspect(name);
  if (!record) return { name, state: "missing", store: "kept" };
  const projects = await homeNames(settings.home);
  if (projects.length) {
    throw refuse("Unsupported", `the Stores of ${projects.length} project(s) (${projects.join(", ")}) are on ${name}'s disk and nowhere else; a VM delete never destroys a Store that was not pushed, and this build has no remote Store push (#506). Remove the projects first`, null);
  }
  await provider.remove(name);
  if (options.discardStore) await provider.removeStore(storeNameOf(settings));
  rmSync(endpointFile(settings.home), { force: true });
  return { name, state: "deleted", store: options.discardStore ? "discarded" : "kept" };
}

/** The four hooks Init and project removal use instead of host Docker (§4.2 B2, B9). */
const stores = {
  /**
   * Init examines the Store's directory before it installs into it (the
   * kernel guard checks its filesystem), so the volume's directory is made
   * here, as a Docker volume is on a local placement.
   */
  async makeStore(runtime, project) {
    const provider = providerOf(runtime.settings), name = nameOf(runtime.settings);
    const directory = `${GUEST.store}/${project.volume}`;
    const { code, stderr } = await provider.exec(name, ["sh", "-c", "umask 077 && mkdir -p \"$1\" && chmod 700 \"$1\"", "sh", directory]);
    if (code !== 0) throw refuse("Failed", `could not make ${directory} in ${name}: ${stderr.trim()}`, null);
  },
  /** The Sandboxes holding the volume now: the project's running Sandboxes, by number. */
  async storeHolders(runtime, volume) {
    const project = (await runtime.project.list()).find((entry) => entry.volume === volume);
    if (!project) return [];
    const held = await runtime.clients.use(project, (client) => client.project.list_sandboxes(1000, 0));
    return held.items.map(({ sandbox }) => ({ id: sandbox, state: "running" }));
  },
  /** Stops them the way the SDK does: `sandbox.close`, one per Sandbox. */
  async releaseStoreHolders(runtime, volume, holders) {
    const project = (await runtime.project.list()).find((entry) => entry.volume === volume);
    if (!project) return;
    await runtime.clients.use(project, async (client) => {
      for (const holder of holders) await client.sandbox.close(holder.id);
    });
  },
  /** Removes the Store's directory inside the VM: no Store copy is left anywhere. */
  async removeStore(runtime, volume) {
    if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(volume)) throw refuse("InvalidInput", "invalid Store volume name", null);
    const provider = providerOf(runtime.settings), name = nameOf(runtime.settings);
    const { code, stderr } = await provider.exec(name, ["rm", "-rf", `${GUEST.store}/${volume}`]);
    if (code !== 0) throw refuse("Failed", `could not remove ${volume} from ${name}: ${stderr.trim()}`, null);
  },
};

/** The guest home, and the deployment inside the VM: native Sandboxes, the namespace backend. */
export function deploymentEnv(settings, project) {
  return {
    HOME: callHome(settings),
    SANDBOX: "native",
    SANDBOX_BACKEND: process.env.LAYERFS_SANDBOX_BACKEND || "namespaces",
    DAEMON: GUEST.daemon,
    LOCAL_DIR: GUEST.local,
    STORE_PATH: `${GUEST.store}/${project.volume}/global/store.sqlite`,
    CONTAINER_PREFIX: settings.containerPrefix,
  };
}

/**
 * The platform object of a `vm:<provider>` home. `hostDocker`/`hostSandboxes`
 * tell the runtime that host Docker is not this home's: its Stores are
 * directories on the VM's disk and its Sandboxes are Linux namespaces there, so
 * `volume.list`, `volume.remove`, `sandbox.list`, `sandbox.list_leftovers` and
 * `sandbox.clean_leftovers` refuse with that reason instead of answering from
 * the host, and `sandbox.sweep` answers `{ host: false }`.
 */
export function vmPlatform(settingName) {
  if (!PROVIDERS[settingName]) throw refuse("Unsupported", `no VM provider is implemented for vm:${settingName}`, null);
  return {
    name: `vm:${settingName}`,
    hostDocker: false,
    hostSandboxes: false,
    endpoint: (settings) => endpointFile(settings.home),
    status,
    start,
    restart,
    /** Init runs in the worker; the source folder is copied into the VM first. */
    async init(settings, project) {
      await start(settings);
      const provider = providerOf(settings), name = nameOf(settings);
      const target = importPath(project.source);
      const { code, stderr } = await provider.exec(name, ["rm", "-rf", target]);
      if (code !== 0) throw refuse("Failed", `could not clear ${target} in ${name}: ${stderr.trim()}`, null);
      await provider.copy(name, project.source, target);
    },
    /** Where the SDK inside the VM reads a host folder: its copy under `/layerfs/import`. */
    path: (settings, hostPath) => (hostPath === settings.home || hostPath.startsWith(`${settings.home}/`) ? hostPath.replace(settings.home, GUEST.home) : importPath(hostPath)),
    deploymentEnv,
    docker: () => {
      throw refuse("Unsupported", `this home is placed in vm:${settingName}: its Stores and Sandboxes are inside the VM, so host Docker does not describe it`, null);
    },
    storePath: (settings, volume) => `${GUEST.store}/${volume}/global/store.sqlite`,
    homeNames: (settings) => homeNames(settings.home),
    async removeHome(settings, name) {
      rmSync(join(settings.home, name), { recursive: true, force: true });
    },
    /** Before a Store is deleted, the worker closes the Store it holds open. */
    async releaseStore(settings, volume) {
      const info = endpoint(settings);
      if (!info) return;
      const answer = await connection(info).call({ fn: "releaseStore", args: { home: GUEST.home, storePath: `${GUEST.store}/${volume}/global/store.sqlite` } });
      if (answer.refused) throw refuse(answer.refused.code, answer.refused.message, answer.refused.detail);
    },
    async free(settings) {
      const provider = providerOf(settings);
      try { return await provider.storeFree(nameOf(settings), GUEST.store); } catch { return null; }
    },
    ...stores,
    vm: { stop, delete: remove },
  };
}

/** A host folder's copy inside the VM, named by its path so the mapping needs no state. */
export function importPath(hostPath) {
  return `${GUEST.import}/${createHash("sha256").update(hostPath).digest("hex").slice(0, 16)}`;
}

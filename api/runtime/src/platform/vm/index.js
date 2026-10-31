// `vm:<provider>` placement (#515 §6, V9): **each of this home's projects —
// strictly, each Store — lives in one Linux VM of its own** (V1). That VM holds
// that project's Store on its own ext4 disk, shares that disk with the project's
// Sandboxes' daemons (Linux namespaces inside it, `LAYERFS_SANDBOX_BACKEND=
// namespaces`, H36), and runs its own worker with its own secret and its own
// private loopback channel (`worker/server.js`, one attempt per call, no replay).
// The host reaches only each VM's worker.
//
// This module is the platform object the runtime uses instead of the host's.
// Every step names the project, so the VM, its Store disk, its host port, its
// bundle copy and its worker secret are that project's alone:
//   ① ensure the project's VM exists and runs, with its Store disk attached;
//   ② copy the sealed bundle in, once per bundle hash and per VM;
//   ③ start the worker as root with its secret, and block user namespaces
//      (V4: `user.max_user_namespaces=0`); the namespaces backend refuses a
//      host where that is not set, so step ③ also proves it;
//   ④ open the private stream, `-p 127.0.0.1:<host>:<guest>`, and write the
//      endpoint `useWorker` reads for that project;
//   ⑤ stop the VM, or delete it: a delete never destroys a Store that was not
//      pushed (V8), and there is no remote Store push yet (#506).
// The provider module behind it (`microsandbox.js`, `apple-container.js`) knows
// the platform; this file knows LayerFS's side and nothing platform-specific.
// Guest paths are in settings.js (GUEST): the home is a bind mount of the
// host's, the Store disk is mounted at the Store directory itself, and the
// bundle, Init sources, worker state and Sandbox directories live on the VM's
// root disk.
import { createHash, randomBytes } from "node:crypto";
import { createServer } from "node:net";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { connection } from "@layerfs/agent/internal";
import { endpointFile, homeNames, ping, ready, refuse, writeEndpoint } from "../shared.js";
import { callHome, GUEST } from "../../settings.js";
import { NAME, record as projectRecord } from "../../project/list.js";
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

/** The project's VM and its Store disk: one VM per Store means one of each per project. */
const vmNameOf = (settings, project) => `${settings.containerPrefix}${project}-vm`;
const storeNameOf = (settings, project) => `${settings.containerPrefix}${project}-vm-store`;

/** A project name a VM can be named after and its driver paths hold. */
function checked(project) {
  if (typeof project !== "string" || !NAME.test(project)) throw refuse("InvalidInput", "a project name is required: each project's Store runs in its own VM", null);
  return project;
}

/** The project's Store file inside its own VM: the disk is the Store's directory. */
const storeFile = () => `${GUEST.store}/global/store.sqlite`;

/** A host loopback port this process just freed: a VM's published worker port. */
const freePort = () => new Promise((resolve, reject) => {
  const server = createServer();
  server.on("error", reject);
  server.listen(0, "127.0.0.1", () => {
    const { port } = server.address();
    server.close(() => resolve(port));
  });
});

/**
 * Step ①: the project's VM runs, or it is created once from the home's settings.
 * A record under our name that serves another home, or does not match what this
 * project needs, is refused: nothing is replaced, restarted or repaired on a
 * guess.
 */
async function ensure(settings, project) {
  const provider = providerOf(settings), name = vmNameOf(settings, project), storeName = storeNameOf(settings, project);
  const current = await provider.inspect(name);
  if (current) {
    if (current.labels?.["layerfs.home"] && current.labels["layerfs.home"] !== settings.home) {
      throw refuse("Busy", `the VM ${name} serves the LayerFS home ${current.labels["layerfs.home"]}; give this home its own containerPrefix`, null);
    }
    if (current.store && current.store !== storeName) throw refuse("Failed", `the VM ${name} has the Store volume ${current.store}, not ${storeName}; delete it before using this project`, null);
    if (current.home && current.home !== settings.home) throw refuse("Failed", `the VM ${name} mounts ${current.home} as its home, not ${settings.home}; delete it before using this project`, null);
    const port = current.ports?.find((entry) => entry.guest === WORKER_PORT)?.host ?? null;
    if (current.state === "running") {
      if (!port) throw refuse("Failed", `the VM ${name} publishes no host port for ${WORKER_PORT}; delete it and start again`, null);
      return { ...current, name, port };
    }
    if (current.state !== "stopped") throw refuse("Failed", `the VM ${name} is ${current.state}; stop or delete it before using this project`, null);
    await provider.start(name);
    const started = await provider.inspect(name);
    if (started?.state !== "running") throw refuse("Failed", `the VM ${name} did not start (${started?.state ?? "missing"})`, null);
    const again = started.ports?.find((entry) => entry.guest === WORKER_PORT)?.host ?? null;
    if (!again) throw refuse("Failed", `the VM ${name} publishes no host port for ${WORKER_PORT}; delete it and start again`, null);
    return { ...started, name, port: again };
  }
  if (provider.ensureStore) await provider.ensureStore(storeName, settings.vm.storeSize);
  const hostPort = await freePort();
  const created = await provider.create({
    name, image: settings.vm.image, cpus: settings.vm.cpus, memory: settings.vm.memory,
    storeName, storeSize: settings.vm.storeSize, home: settings.home, hostPort, workerPort: WORKER_PORT,
  });
  if (created?.state !== "running") throw refuse("Failed", `the VM ${name} was created but is ${created?.state ?? "missing"}; its record is kept for inspection, not replayed`, null);
  const port = created.ports?.find((entry) => entry.guest === WORKER_PORT)?.host ?? null;
  if (!port) throw refuse("Failed", `the VM ${name} was created without its port forward; delete it before using this project`, null);
  return { ...created, name, port };
}

/** The sealed bundle's identity: its manifest hash, which the worker checks file by file. */
function bundleIdentity(bundle) {
  if (!bundle || !existsSync(join(bundle, "manifest.json"))) throw refuse("InvalidInput", "a VM placement needs a sealed Linux bundle (settings.bundle)", null);
  const text = readFileSync(join(bundle, "manifest.json"), "utf8");
  const manifest = JSON.parse(text);
  if (manifest.status !== "sealed" || manifest.platform !== "linux") throw refuse("InvalidInput", "a VM placement needs a sealed Linux bundle", null);
  return { build: manifest.build_id, hash: createHash("sha256").update(text).digest("hex"), image: bundleImage(bundle) };
}

/**
 * The Sandbox image a sealed bundle carries (#515 §4.7 V7, the H38 namespaces
 * backend's convention): `<bundle>/images/<64 hex>.tar`, an uncompressed
 * `docker save` archive named by the image's config digest. It is copied in
 * with the rest of the bundle — each VM has its own copy, because each VM is
 * one Store — and the worker is told about it as a guest path. A bundle of
 * none (today's `requires_image: false` backend) sets neither.
 */
function bundleImage(bundle) {
  const folder = join(bundle, "images");
  const found = existsSync(folder) ? readdirSync(folder).filter((name) => /^[0-9a-f]{64}\.tar$/.test(name)).sort() : [];
  if (found.length > 1) throw refuse("InvalidInput", `the bundle holds ${found.length} Sandbox images (${found.join(", ")}); a VM placement describes one deployment image`, null);
  if (!found.length) return null;
  return { digest: `sha256:${found[0].slice(0, -4)}`, archive: `${GUEST.bundle}/images/${found[0]}` };
}

/** Step ②: the bundle, copied into this VM once per hash; a changed bundle replaces the copy. */
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
  const { code, stderr } = await provider.blockUserNamespaces(vm.name);
  if (code !== 0) throw refuse("Unsupported", `the VM ${vm.name} cannot block user namespaces (${stderr.trim() || `exit ${code}`}); a Sandbox command could copy the Workspace mount into its own`, null);
  const left = (await provider.read(vm.name, "/proc/sys/user/max_user_namespaces"))?.trim();
  if (left !== "0") throw refuse("Unsupported", `the VM ${vm.name} still allows user namespaces (${left ?? "unreadable"})`, null);
}

/** The worker's own settings, read by `sh` from a 0600 file so the secret is in no command line. */
const workerEnv = (secret, image) => [
  `LAYERFS_WORKER_SECRET=${secret}`,
  `LAYERFS_WORKER_LISTEN=0.0.0.0:${WORKER_PORT}`,
  `LAYERFS_HOME=${GUEST.home}`,
  `LAYERFS_BUNDLE=${GUEST.bundle}`,
  `LAYERFS_NATIVE=${GUEST.bundle}/layerfs.node`,
  // a Sandbox reopened from its record has no daemon path to look beside
  `LAYERFS_SANDBOX_HELPER=${GUEST.bundle}/layerfs-sandbox-helper`,
  // #515 §4.7 V7 (H38's backend): this VM's own copy of the Sandbox image
  ...(image ? [`LAYERFS_IMAGE=${image.digest}`, `LAYERFS_IMAGE_ARCHIVE=${image.archive}`] : []),
  "",
].join("\n");

/**
 * Step ③ and ④: the worker as root, its secret in a 0600 file, then the
 * project's endpoint once it answers on the private port. A worker left by an
 * earlier start that no longer answers is ended by the pid that start wrote (a
 * bounded cleanup, not a replay); one launch, one readiness wait. A worker that
 * does not answer reports the last lines of its log.
 */
async function launch(settings, provider, vm, project, bundle) {
  const secret = randomBytes(32).toString("hex");
  const file = endpointFile(settings.home, project);
  await provider.write(vm.name, `${GUEST.state}/worker.env`, workerEnv(secret, bundle.image));
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
  const info = { home: GUEST.home, port: vm.port, secret, build: bundle.build, vm: { provider: settings.vm.provider, name: vm.name, project } };
  try {
    await ready(info, READY_MS, `the worker in ${vm.name}`);
  } catch (error) {
    const log = (await provider.read(vm.name, `${GUEST.state}/worker.log`).catch(() => null))?.trim().split("\n").slice(-8).join("\n");
    throw refuse("Failed", `${error.message}${log ? `; its log ends: ${log}` : ""}`, null);
  }
  mkdirSync(join(settings.home, "worker"), { recursive: true, mode: 0o700 });
  writeEndpoint(file, info);
  return info;
}

/** The endpoint the project's worker was last written at, or null. */
function endpoint(settings, project) {
  try { return JSON.parse(readFileSync(endpointFile(settings.home, project), "utf8")); } catch { return null; }
}

/** Nothing to start when the project's worker already answers with this build. */
async function answering(settings, project, expected) {
  const info = endpoint(settings, project);
  if (!info || (expected && info.build !== expected)) return null;
  return (await ping(info))?.ready ? info : null;
}

/** One project's VM record and, when it runs, whether its worker answers. It starts nothing. `disk`
 * asks each running VM for its Store disk's free bytes (one guest call), which `status` reports. */
async function entry(settings, project, disk = false) {
  const provider = providerOf(settings), name = vmNameOf(settings, project);
  let found;
  try { found = await provider.inspect(name); } catch (error) {
    return { project, name, state: "unknown", error: error.message, worker: { state: "unknown", error: error.message } };
  }
  if (!found) return { project, name, state: "missing", worker: { state: "missing" } };
  const vm = { project, name, state: found.state, image: found.image, home: found.home, store: found.store, created: found.created };
  if (found.state !== "running") return { ...vm, worker: { state: found.state, build: null } };
  const info = endpoint(settings, project);
  const reply = info ? await ping(info) : null;
  const worker = { state: reply?.ready ? "running" : "unknown", answering: !!reply?.ready, build: reply?.build ?? info?.build ?? null, port: info?.port ?? null };
  if (!disk) return { ...vm, worker };
  return { ...vm, worker, disk: { free: await provider.storeFree(name, GUEST.store).catch(() => null) } };
}

/** `{ project, worker, vm }` of one project: what `worker.start` and `vm.stop` answer. */
async function stateOf(settings, project) {
  const vm = await entry(settings, checked(project));
  return { project: vm.project, worker: vm.worker, vm };
}

/** The runtime's `worker.start(project)`: steps ①–④ for that project's VM. */
async function start(settings, project) {
  checked(project);
  const provider = providerOf(settings), bundle = bundleIdentity(settings.bundle);
  const held = await answering(settings, project, bundle.build);
  if (held) return stateOf(settings, project);
  const vm = await ensure(settings, project);
  await install(settings, provider, vm);
  await prepareGuest(provider, vm);
  await launch(settings, provider, vm, project, bundle);
  return stateOf(settings, project);
}

/** One `provider restart`, then the steps a rebooted VM needs again (③, ④). */
async function restart(settings, project) {
  checked(project);
  const provider = providerOf(settings), bundle = bundleIdentity(settings.bundle);
  const vm = await ensure(settings, project);
  if (provider.restart) await provider.restart(vm.name);
  else { await provider.stop(vm.name); await provider.start(vm.name); }
  rmSync(endpointFile(settings.home, project), { force: true });
  await install(settings, provider, vm);
  await prepareGuest(provider, vm);
  await launch(settings, provider, vm, project, bundle);
  return stateOf(settings, project);
}

/** Every project's VM, or all of this home's: `status()` of the runtime and of the `vm` group. */
async function status(settings, projects = []) {
  const provider = providerOf(settings);
  const vms = [];
  for (const project of projects) vms.push(await entry(settings, checked(project), true));
  return { worker: null, vm: { provider: settings.vm.provider, placement: settings.placement.name, vms } };
}

/** Step ⑤: stop one project's VM; its Store disk and its Store stay. */
async function stop(settings, project) {
  const provider = providerOf(settings), name = vmNameOf(settings, checked(project));
  const found = await provider.inspect(name);
  if (!found) return { project, name, state: "missing" };
  if (found.state !== "stopped") await provider.stop(name);
  const after = await provider.inspect(name);
  rmSync(endpointFile(settings.home, project), { force: true });
  return { project, name, state: after?.state ?? "missing" };
}

/**
 * Step ⑤, delete (V8): one project's VM record and, only with `discardStore`,
 * its Store disk. A delete that would leave a project without its only Store
 * copy is refused: that project's Store lies on this VM's disk and nowhere
 * else, and no remote Store push exists (#506).
 */
async function remove(settings, project, options = {}) {
  const provider = providerOf(settings), name = vmNameOf(settings, checked(project));
  if (options?.confirm !== "delete") throw refuse("InvalidInput", `a VM delete ends the project's VM; confirm it with { confirm: "delete" }`, null);
  const found = await provider.inspect(name);
  if (!found) return { project, name, state: "missing", store: "kept" };
  if (exists(settings, project)) {
    throw refuse("Unsupported", `the Store of ${project} is on ${name}'s disk and nowhere else; a VM delete never destroys a Store that was not pushed, and this build has no remote Store push (#506). Remove the project first`, null);
  }
  await provider.remove(name);
  if (options.discardStore) await provider.removeStore(storeNameOf(settings, project));
  rmSync(endpointFile(settings.home, project), { force: true });
  return { project, name, state: "deleted", store: options.discardStore ? "discarded" : "kept" };
}

/** Whether this home still holds the project: its record, or its entry in the home. */
function exists(settings, project) {
  if (projectRecord(settings, project)) return true;
  try {
    return readdirSync(settings.home, { withFileTypes: true }).some((e) => e.isDirectory() && e.name === project && existsSync(join(settings.home, e.name, "project.record")));
  } catch { return false; }
}

/** The four hooks Init and project removal use instead of host Docker (§4.2 B2, B9). */
const stores = {
  /**
   * Init examines the Store's directory before it installs into it (the kernel
   * guard checks its filesystem), so the project's Store disk is mounted and
   * its directory made here, as a Docker volume is on a local placement.
   */
  async makeStore(runtime, project) {
    const provider = providerOf(runtime.settings), name = vmNameOf(runtime.settings, project.name);
    const { code, stderr } = await provider.exec(name, ["sh", "-c", "umask 022 && mkdir -p \"$1\" && chmod 755 \"$1\"", "sh", GUEST.store]);
    if (code !== 0) throw refuse("Failed", `could not make ${GUEST.store} in ${name}: ${stderr.trim()}`, null);
  },
  /** The Sandboxes holding the project's Store now: its running Sandboxes, by number. A stopped VM runs none. */
  async storeHolders(runtime, project) {
    const { settings } = runtime, provider = providerOf(settings);
    const found = await provider.inspect(vmNameOf(settings, project.name));
    if (!found || found.state !== "running") return [];
    const held = await runtime.clients.use(project, (client) => client.project.list_sandboxes(1000, 0));
    return held.items.map(({ sandbox }) => ({ id: sandbox, state: "running" }));
  },
  /** Stops them the way the SDK does: `sandbox.close`, one per Sandbox. */
  async releaseStoreHolders(runtime, project, holders) {
    const { settings } = runtime, provider = providerOf(settings);
    const found = await provider.inspect(vmNameOf(settings, project.name));
    if (!found || found.state !== "running") return;
    await runtime.clients.use(project, async (client) => {
      for (const holder of holders) await client.sandbox.close(holder.id);
    });
  },
  /** Removes the project's VM and its Store disk: no Store copy is left anywhere. */
  async removeStore(runtime, project) {
    const { settings } = runtime, provider = providerOf(settings), name = vmNameOf(settings, project.name);
    const found = await provider.inspect(name);
    if (found) await provider.remove(name);
    await provider.removeStore(storeNameOf(settings, project.name));
    rmSync(endpointFile(settings.home, project.name), { force: true });
  },
};

/** The guest home, and the deployment inside the project's VM: native Sandboxes, the namespace backend. */
export function deploymentEnv(settings, project) {
  const image = settings.bundle ? bundleImage(settings.bundle) : null;
  return {
    HOME: callHome(settings),
    SANDBOX: "native",
    SANDBOX_BACKEND: process.env.LAYERFS_SANDBOX_BACKEND || "namespaces",
    DAEMON: GUEST.daemon,
    LOCAL_DIR: GUEST.local,
    // the Sandbox image this VM's bundle carries, when it carries one (V7)
    ...(image ? { IMAGE: image.digest } : {}),
    STORE_PATH: storeFile(),
    CONTAINER_PREFIX: settings.containerPrefix,
    // a process an integration launches for this project reaches this VM's worker
    WORKER_ENDPOINT: endpointFile(settings.home, project.name),
  };
}

/**
 * The platform object of a `vm:<provider>` home. `hostDocker`/`hostSandboxes`
 * tell the runtime that host Docker is not this home's: its Stores are
 * directories on each VM's own disk and its Sandboxes are Linux namespaces
 * there, so `volume.list`, `volume.remove`, `sandbox.list`,
 * `sandbox.list_leftovers` and `sandbox.clean_leftovers` refuse with that
 * reason instead of answering from the host, and `sandbox.sweep` answers
 * `{ host: false }`. `endpoints` answers, per project, the endpoint file
 * `@layerfs/agent` sends that project's calls to.
 */
export function vmPlatform(settingName) {
  if (!PROVIDERS[settingName]) throw refuse("Unsupported", `no VM provider is implemented for vm:${settingName}`, null);
  return {
    name: `vm:${settingName}`,
    hostDocker: false,
    hostSandboxes: false,
    endpoints: (settings) => (project) => endpointFile(settings.home, project),
    status,
    start,
    restart,
    /** Init runs in that project's worker; the source folder is copied into its VM first. */
    async init(settings, project) {
      await start(settings, project.name);
      const provider = providerOf(settings), name = vmNameOf(settings, project.name);
      const target = importPath(project.source);
      const { code, stderr } = await provider.exec(name, ["rm", "-rf", target]);
      if (code !== 0) throw refuse("Failed", `could not clear ${target} in ${name}: ${stderr.trim()}`, null);
      await provider.copy(name, project.source, target);
    },
    /** Where the SDK inside the VM reads a host folder: its copy under `/layerfs/import`. */
    path: (settings, hostPath) => (hostPath === settings.home || hostPath.startsWith(`${settings.home}/`) ? hostPath.replace(settings.home, GUEST.home) : importPath(hostPath)),
    deploymentEnv,
    docker: () => {
      throw refuse("Unsupported", `this home is placed in vm:${settingName}: its Stores and Sandboxes are inside each project's VM, so host Docker does not describe it`, null);
    },
    storePath: () => storeFile(),
    homeNames: (settings) => homeNames(settings.home),
    async removeHome(settings, name) {
      rmSync(join(settings.home, name), { recursive: true, force: true });
    },
    /** Before a Store is deleted, the project's worker closes the Store it holds open. */
    async releaseStore(settings, project) {
      const info = endpoint(settings, project.name);
      if (!info) return;
      const answer = await connection(info).call({ fn: "releaseStore", args: { home: GUEST.home, storePath: storeFile() } });
      if (answer.refused) throw refuse(answer.refused.code, answer.refused.message, answer.refused.detail);
    },
    /** Free bytes of every project VM's Store disk, as that VM sees them. */
    async free(settings) {
      const provider = providerOf(settings);
      let total = null;
      for (const project of homeNames(settings.home)) {
        const free = await provider.storeFree(vmNameOf(settings, project), GUEST.store).catch(() => null);
        if (free !== null) total = (total ?? 0) + free;
      }
      return total;
    },
    ...stores,
    vm: { status, stop, delete: remove },
  };
}

/** A host folder's copy inside the VM, named by its path so the mapping needs no state. */
export function importPath(hostPath) {
  return `${GUEST.import}/${createHash("sha256").update(hostPath).digest("hex").slice(0, 16)}`;
}

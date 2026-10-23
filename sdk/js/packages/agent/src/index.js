// @layerfs/agent (#428): what an agent's script calls. A client is opened by
// `LayerFS.open(name)` (an existing project) or `LayerFS.init(name, folder)`
// (a new one) and released by `close()`; there is no public constructor.
// Calls name their Branch. `workspace.mount(branch)` returns a Workspace,
// `{ sandbox, path }`, placed by LayerFS in one of the project's Sandboxes,
// or in the one `{ sandbox }` names; the other Workspace calls take it.
// `sandbox.create` gives an agent a Sandbox of its own and `sandbox.close`
// stops it. Store calls (fs, branch, project) run against the project's
// Store, opened in this process on Linux (#352 F-020) or in the LayerFS
// home's worker on macOS and Windows (transport/).
import { BACKEND, STATE, invoke, invokeClient, live, projectArgs, storeArgs, storeClose, storeOpen, transportOf } from "./client.js";
import { binding } from "./transport/index.js";
import { LayerFSError } from "./errors.js";
import * as fsMethods from "./fs/index.js";
import { init } from "./project/init.js";
import { list_branches } from "./project/list_branches.js";
import { list_sandboxes } from "./project/list_sandboxes.js";
import { diff_branches } from "./branch/diff_branches.js";
import { diff_commits } from "./branch/diff_commits.js";
import { fork } from "./branch/fork.js";
import { list_commits } from "./branch/list_commits.js";
import { merge } from "./branch/merge.js";
import { publish } from "./branch/publish.js";
import { create } from "./sandbox/create.js";
import { list_workspaces } from "./sandbox/list_workspaces.js";
import { close as closeSandbox } from "./sandbox/close.js";
import { commit } from "./workspace/commit.js";
import { exec } from "./workspace/exec.js";
import { mount } from "./workspace/mount.js";
import { unmount } from "./workspace/unmount.js";

export { LayerFSError, isRefusal, CODES } from "./errors.js";
export { RENAMED_METHODS, currentMethod } from "./names.js";

// One copy of this package per process: a second copy would keep its own
// table of open Stores and could open the same Store twice.
const LOADED = Symbol.for("@layerfs/agent");
if (globalThis[LOADED] && globalThis[LOADED] !== import.meta.url) {
  throw new Error(`@layerfs/agent is loaded twice in this process (${globalThis[LOADED]} and ${import.meta.url}); give every package the same copy`);
}
globalThis[LOADED] = import.meta.url;

/** Held only by the factories: the constructor refuses without it. */
const FACTORY = Symbol("LayerFS.factory");

const bind = (client, methods) =>
  Object.fromEntries(Object.entries(methods).map(([name, method]) => [name, async (...args) => method(client, ...args)]));

/** Opens the client's Store and makes one project lookup on it, so a
 * missing project is NoSuchProject at once; nothing stays open on failure. */
async function opened(client) {
  if (!client.options.storePath) throw new LayerFSError("InvalidInput", "a LayerFS client needs options.storePath (LAYERFS_STORE_PATH), the project's installed Store", null);
  await storeOpen(client);
  try {
    await invoke("project.list_branches", { ...projectArgs(client), ...storeArgs(client), pageSize: 1, pagePos: 0 }, client.options, transportOf(client));
  } catch (error) {
    await storeClose(client).catch(() => {});
    throw error;
  }
  client[STATE].closed = false;
}

export class LayerFS {
  /** A ready client of the existing project `projectName`; see the constructor's options. */
  static async open(projectName, options = {}) {
    const client = new LayerFS(projectName, options, FACTORY);
    await opened(client);
    return client;
  }

  /** Creates the project `projectName` from `folder` (one Init), then opens
   * it. `created` on the client is what Init returned. An uncertain Init
   * keeps everything it made. `timeout` (milliseconds) bounds Init, which
   * then fails `Failed` (Deadline) when it runs longer; omitted, Init has
   * no time limit. */
  static async init(projectName, folder, options = {}, timeout = undefined) {
    const client = new LayerFS(projectName, options, FACTORY);
    const created = await init(client, folder, timeout);
    await opened(client);
    client.created = created;
    return client;
  }

  /** Not public: `LayerFS.open` and `LayerFS.init` make clients.
   * `options.trace` names a per-call trace file (overrides LAYERFS_TRACE,
   * then DSH_LAYERFS_TRACE); `options.traceStep` labels its lines
   * (overrides LAYERFS_TRACE_STEP, then DSH_LAYERFS_TRACE_STEP). Deployment
   * settings read LAYERFS_HOME, _DOCKER, _IMAGE, _VOLUME and _DAEMON, each
   * else its DSH_LAYERFS_ twin; a Sandbox this client starts runs _IMAGE
   * on _VOLUME. LAYERFS_CONTAINER_PREFIX (default `layerfs-`) names every
   * container it starts `<prefix><project>-<n>`.
   * `options.daemonHost` (else LAYERFS_DAEMON_HOST) is the host this process
   * dials for a daemon's port, published on the Docker host's 127.0.0.1 only;
   * set it when the client runs in a container (`host.docker.internal` on
   * Docker Desktop). Unset, the client dials 127.0.0.1.
   * `options.storePath` (else LAYERFS_STORE_PATH) is the installed Store
   * file this client's Store calls open; it must be on a local Linux
   * filesystem the daemons share, or the calls fail `Unsupported`.
   * `options.storePolicy` ({ smallFileThreshold, wholeFileDepth,
   * chunkDepth, metadataDepth }) is the Store policy `LayerFS.init` records
   * instead of the default. */
  constructor(projectName, options = {}, token = undefined) {
    if (token !== FACTORY) {
      throw new LayerFSError("InvalidInput", "use `await LayerFS.open(name)` or `await LayerFS.init(name, folder)`; there is no public constructor", null);
    }
    if (typeof projectName !== "string" || !/^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/.test(projectName)) {
      throw new LayerFSError("InvalidInput", "a project name is required", null);
    }
    if (options === null || typeof options !== "object") throw new LayerFSError("InvalidInput", "options must be an object", null);
    this.projectName = projectName;
    // Each LAYERFS_ setting falls back to its DSH_LAYERFS_ twin, the only
    // names deepseek-harness may set in a bash call's environment.
    const env = (name) => process.env[`LAYERFS_${name}`] || process.env[`DSH_LAYERFS_${name}`] || undefined;
    this.options = {
      home: env("HOME"),
      trace: options.trace ?? env("TRACE"),
      traceStep: options.traceStep,
      deploy: {
        docker: env("DOCKER"),
        image: env("IMAGE"),
        volume: env("VOLUME"),
        daemon: env("DAEMON"),
        ...(env("CONTAINER_PREFIX") ? { containerPrefix: env("CONTAINER_PREFIX") } : {}),
        ...((options.daemonHost ?? env("DAEMON_HOST")) ? { daemonHost: options.daemonHost ?? env("DAEMON_HOST") } : {}),
      },
      storePolicy: options.storePolicy,
      storePath: options.storePath ?? env("STORE_PATH"),
    };
    // not open until a factory has opened its Store; the binding is chosen now, once
    this[STATE] = { closed: null, transport: binding() };
    this.fs = bind(this, fsMethods);
    this.project = bind(this, { list_branches, list_sandboxes });
    this.branch = bind(this, { list_commits, diff_commits, diff_branches, fork, merge, publish });
    this.sandbox = bind(this, { create, list_workspaces, close: closeSandbox });
    this.workspace = bind(this, { mount, exec, commit, unmount });
  }

  /** One public call of this client; refused once the client is closed. */
  async call(method, args) {
    if (BACKEND.test(method)) throw new LayerFSError("UnknownMethod", "backend and runtime methods are @layerfs/backend's and @layerfs/runtime's", null);
    live(this);
    return invokeClient(this, method, args);
  }

  /** Releases this client's Store (the process's last client of it closes
   * it). Workspaces stay mounted. Every later call is refused InvalidInput;
   * a second close does nothing. */
  async close() {
    if (this[STATE].closed !== false) return;
    this[STATE].closed = true;
    await storeClose(this);
  }
}

// `await using shop = await LayerFS.open("shop")` closes it at the block's end.
if (typeof Symbol.asyncDispose === "symbol") {
  Object.defineProperty(LayerFS.prototype, Symbol.asyncDispose, { value() { return this.close(); }, writable: true, configurable: true });
}

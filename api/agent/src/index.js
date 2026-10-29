// @layerfs/agent (#428): what an agent's script calls. A client is opened by
// `LayerFS.open(name, options?)` (an existing project) or `LayerFS.init(name,
// folder, timeout?, options?)` (a new one) and released by `close()`; there
// is no public constructor. Settings come from the options object, else the
// environment, resolved once at construction.
// Calls name their Branch. `sandbox.create()` starts a Sandbox (the only
// call that does) and `sandbox.close` stops it; `workspace.mount(sandbox,
// branch)` returns a Workspace, `{ sandbox, path }`, in the Sandbox it names
// (there is no placement); the other Workspace calls take it. Store calls
// (fs, branch, project) run against the project's Store, opened in this
// process on Linux (#352 F-020) or in the LayerFS home's worker on macOS and
// Windows (transport/).
import { BACKEND, STATE, invoke, invokeClient, live, projectArgs, storeArgs, storeClose, storeOpen, transportOf } from "./client.js";
import { binding } from "./transport/index.js";
import { LayerFSError } from "./errors.js";
import * as fsMethods from "./fs/index.js";
import { INIT_TIMEOUT_MS, init } from "./project/init.js";
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
export { INIT_TIMEOUT_MS };
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
  if (!client.options.storePath) throw new LayerFSError("InvalidInput", "a LayerFS client needs storePath (LAYERFS_STORE_PATH), the project's installed Store", null);
  await storeOpen(client);
  try {
    await invoke("project.list_branches", { ...projectArgs(client), ...storeArgs(client), pageSize: 1, pagePos: 0 }, client.options, transportOf(client));
  } catch (error) {
    await storeClose(client).catch(() => {});
    throw error;
  }
  client[STATE].closed = false;
}

/** True for a plain object (not null, not an array). */
const isPlainObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

/** One string option field: explicit value wins, else the environment. */
function optionField(given, field, envName, env) {
  const value = given?.[field];
  if (value === undefined) return env(envName);
  if (typeof value !== "string" || !value) {
    throw new LayerFSError("InvalidInput", `LayerFS option "${field}" must be a non-empty string`, null);
  }
  return value;
}

/** The client's settings, resolved once from options over environment.
 * The result is a fresh snapshot: mutating the caller's object, or the
 * environment, after construction changes nothing. LAYERFS_TRACE_STEP and
 * LAYERFS_TRACE_START are not settings and are never snapshotted: trace.js
 * reads them at each call. */
function resolveOptions(given) {
  if (given !== undefined && !isPlainObject(given)) {
    throw new LayerFSError("InvalidInput", "LayerFS options must be a plain object", null);
  }
  for (const key of Object.keys(given ?? {})) {
    if (!["home", "trace", "deploy", "storePolicy", "storePath"].includes(key)) {
      throw new LayerFSError("InvalidInput", `unknown LayerFS option "${key}"`, null);
    }
  }
  const env = (name) => process.env[`LAYERFS_${name}`] || undefined;
  const deployGiven = given?.deploy;
  if (deployGiven !== undefined && !isPlainObject(deployGiven)) {
    throw new LayerFSError("InvalidInput", `LayerFS option "deploy" must be a plain object`, null);
  }
  for (const key of Object.keys(deployGiven ?? {})) {
    if (!["docker", "image", "volume", "daemon", "containerPrefix", "daemonHost"].includes(key)) {
      throw new LayerFSError("InvalidInput", `unknown LayerFS deploy option "${key}"`, null);
    }
  }
  const deployField = (field, envName) => {
    const value = deployGiven?.[field];
    if (value === undefined) return env(envName);
    if (typeof value !== "string" || !value) {
      throw new LayerFSError("InvalidInput", `LayerFS option "deploy.${field}" must be a non-empty string`, null);
    }
    return value;
  };
  const containerPrefix = deployField("containerPrefix", "CONTAINER_PREFIX");
  const daemonHost = deployField("daemonHost", "DAEMON_HOST");
  let storePolicy = given?.storePolicy;
  if (storePolicy === undefined) {
    storePolicy = env("STORE_POLICY");
  } else if (typeof storePolicy === "string") {
    if (!storePolicy) throw new LayerFSError("InvalidInput", `LayerFS option "storePolicy" must be JSON text or a plain object`, null);
  } else if (isPlainObject(storePolicy)) {
    try {
      storePolicy = JSON.stringify(storePolicy);
    } catch {
      throw new LayerFSError("InvalidInput", `LayerFS option "storePolicy" must be JSON text or a plain object`, null);
    }
  } else {
    throw new LayerFSError("InvalidInput", `LayerFS option "storePolicy" must be JSON text or a plain object`, null);
  }
  return {
    home: optionField(given, "home", "HOME", env),
    trace: optionField(given, "trace", "TRACE", env),
    deploy: {
      docker: deployField("docker", "DOCKER") ?? "/var/run/docker.sock",
      image: deployField("image", "IMAGE"),
      volume: deployField("volume", "VOLUME"),
      daemon: deployField("daemon", "DAEMON"),
      ...(containerPrefix ? { containerPrefix } : {}),
      ...(daemonHost ? { daemonHost } : {}),
    },
    storePolicy,
    storePath: optionField(given, "storePath", "STORE_PATH", env),
  };
}

export class LayerFS {
  /** A ready client of the existing project `projectName`; the constructor lists its settings. */
  static async open(projectName, ...rest) {
    if (rest.length > 1 || (rest.length === 1 && rest[0] !== undefined && !isPlainObject(rest[0]))) {
      throw new LayerFSError("InvalidInput", "LayerFS.open(name, options?) takes one options object at most", null);
    }
    const client = new LayerFS(projectName, FACTORY, rest[0]);
    await opened(client);
    return client;
  }

  /** Creates the project `projectName` from `folder` (one Init), then opens
   * it. `created` on the client is what Init returned. An uncertain Init
   * keeps everything it made. `timeout` (milliseconds, a positive whole
   * number; default INIT_TIMEOUT_MS, 300 s; no unlimited option) bounds
   * Init, which then fails `Failed` (Deadline) when it runs longer. */
  static async init(projectName, folder, timeout = undefined, ...rest) {
    if (timeout !== null && typeof timeout === "object") {
      throw new LayerFSError("InvalidInput", "LayerFS.init(name, folder, timeout?, options?) takes options as the fourth argument, not in the timeout position", null);
    }
    if (rest.length > 1 || (rest.length === 1 && rest[0] !== undefined && !isPlainObject(rest[0]))) {
      throw new LayerFSError("InvalidInput", "LayerFS.init(name, folder, timeout?, options?) takes one options object at most", null);
    }
    const client = new LayerFS(projectName, FACTORY, rest[0]);
    const created = await init(client, folder, timeout);
    await opened(client);
    client.created = created;
    return client;
  }

  /** Not public: `LayerFS.open` and `LayerFS.init` make clients. Every
   * setting is resolved once, here, into `options`: an explicit option
   * wins, else its LAYERFS_ name, else the built-in default.
   * LAYERFS_TRACE_STEP and LAYERFS_TRACE_START are not settings: trace.js
   * reads them at each call. LAYERFS_TRACE names a
   * per-call trace file; LAYERFS_TRACE_STEP, read at each call, labels its
   * lines. Deployment
   * settings read LAYERFS_HOME, _DOCKER, _IMAGE, _VOLUME and _DAEMON; a
   * Sandbox this client starts runs _IMAGE
   * on _VOLUME. LAYERFS_CONTAINER_PREFIX (default `layerfs-`) names every
   * container it starts `<prefix><project>-<n>`.
   * LAYERFS_DAEMON_HOST is the host this process
   * dials for a daemon's port, published on the Docker host's 127.0.0.1 only;
   * set it when the client runs in a container (`host.docker.internal` on
   * Docker Desktop). Unset, the client dials 127.0.0.1.
   * LAYERFS_STORE_PATH is the installed Store
   * file this client's Store calls open; it must be on a local Linux
   * filesystem the daemons share, or the calls fail `Unsupported`.
   * LAYERFS_STORE_POLICY, a JSON object { smallFileThreshold,
   * wholeFileDepth, chunkDepth, metadataDepth }, is the Store policy
   * `LayerFS.init` records instead of the default. */
  constructor(projectName, token = undefined, givenOptions = undefined) {
    if (token !== FACTORY) {
      throw new LayerFSError("InvalidInput", "use `await LayerFS.open(name)` or `await LayerFS.init(name, folder)`; there is no public constructor", null);
    }
    if (typeof projectName !== "string" || !/^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/.test(projectName)) {
      throw new LayerFSError("InvalidInput", "a project name is required", null);
    }
    this.projectName = projectName;
    this.options = resolveOptions(givenOptions);
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

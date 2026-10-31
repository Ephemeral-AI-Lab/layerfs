// One call through this process's binding (transport/index.js: the native
// addon in this process, or the worker client). The binding resolves
// `{ value }` or `{ refused }` (plus `telemetry` when traced); a refusal
// becomes a LayerFSError. Nothing is retried.
import { binding } from "./transport/index.js";

/** The binding a client was made with (see transport/index.js). */
export const transportOf = (client) => client[STATE].transport;
import { LayerFSError } from "./errors.js";
import { traced } from "./trace.js";

/** A client's private state: `{ closed, transport }`. */
export const STATE = Symbol("layerfs.client");

/** Names on the binding's backend table (#428), refused by the public one. */
export const BACKEND = /^(backend|runtime)\./;

/** Runs one operation once and returns its value or throws its refusal;
 * `options` are the client's settings, whose `trace` selects its trace file;
 * `transport` is the client's binding (default: the one a new client of
 * `args.project` would use). */
export async function invoke(op, args, options, transport = binding(args.project)) {
  return outcome(await traced(options, op, args, (scope) => {
    if (BACKEND.test(op)) throw new LayerFSError("UnknownMethod", "backend and runtime methods are @layerfs/backend's and @layerfs/runtime's", null);
    return transport.call(op, observed(args, scope));
  }));
}

/** One call on the binding's backend table (`backend.*`, `runtime.*`). */
export async function invokeBackend(op, args, options, transport = binding(args?.project)) {
  return outcome(await traced(options, op, args, (scope) => transport.backendCall(op, observed(args, scope))));
}

/** A traced call carries its scope; the binding removes it before dispatch. */
const observed = (args, scope) => (scope ? { ...args, observe: scope } : args);

/** A binding answer's value, or its refusal thrown as a LayerFSError. */
export function outcome(answer) {
  if (answer.refused) {
    const { code, message, detail } = answer.refused;
    throw new LayerFSError(code, message, detail);
  }
  return answer.value;
}

/** The call fields of a Workspace, `{ sandbox, path }` as `workspace.mount` returned it. */
export function workspaceArgs(ws) {
  if (!ws || typeof ws.sandbox !== "string" || !ws.sandbox || typeof ws.path !== "string" || !ws.path) {
    throw new LayerFSError("InvalidInput", "a Workspace { sandbox, path } from workspace.mount is required", null);
  }
  return { sandbox: ws.sandbox, path: ws.path };
}

/** A Sandbox's number, as `project.list_sandboxes` and `workspace.mount` name it. */
export function sandboxArg(sandbox) {
  if (typeof sandbox !== "string" || !/^[1-9][0-9]{0,9}$/.test(sandbox)) {
    throw new LayerFSError("InvalidInput", "a Sandbox is its number as a string, such as \"1\"", null);
  }
  return sandbox;
}

/** Deployment settings from the client options, else the environment
 * (LAYERFS_<NAME>). `sandbox` (LAYERFS_SANDBOX) picks the backend (#527 D3):
 * unset or `docker`, a Docker container, which needs every Docker setting but
 * the container prefix and daemon host, and, on a PostgreSQL Store
 * (`postgresql`), the volume: its daemon opens the server. `native` (Linux or
 * macOS) and `namespaces` (Linux) are host daemons (see `localDeployment`).
 * The binding is the one validator: it maps the former
 * `sandboxBackend` (LAYERFS_SANDBOX_BACKEND), kept for one release, and
 * refuses an unknown value or a conflicting pair, so this only checks that
 * each value's fields are present and passes both settings on. `host` is
 * the OS the Sandbox runs on (Linux for a worker, see platform.js): native and namespaces are
 * refused where they cannot run before anything is read. */
export function deployment(options = {}, postgresql = false, host = process.platform) {
  const env = (name) => process.env[`LAYERFS_${name}`] || undefined;
  // a field the options name (even as undefined, for a platform's own
  // runtime, #529) is not read from the environment
  const sandbox = "sandbox" in options ? options.sandbox : env("SANDBOX");
  const backend = "sandboxBackend" in options ? options.sandboxBackend : env("SANDBOX_BACKEND");
  const named = { ...(sandbox !== undefined ? { sandbox } : {}), ...(backend !== undefined ? { sandboxBackend: backend } : {}) };
  if (sandbox === "native" || sandbox === "namespaces") return localDeployment(options, env, named, host);
  // another value is the binding's to refuse, naming the values it takes
  if (sandbox !== undefined && sandbox !== "docker") return named;
  const deploy = {
    docker: options.docker ?? env("DOCKER") ?? "/var/run/docker.sock",
    image: options.image ?? env("IMAGE"),
    volume: options.volume ?? env("VOLUME"),
    daemon: options.daemon ?? env("DAEMON"),
    containerPrefix: options.containerPrefix ?? env("CONTAINER_PREFIX"),
    daemonHost: options.daemonHost ?? env("DAEMON_HOST"),
  };
  for (const [name, value] of Object.entries(deploy)) {
    if (name === "containerPrefix" || name === "daemonHost" || (postgresql && name === "volume")) continue;
    if (!value) throw new LayerFSError("InvalidInput", `deploy.${name} is not configured (LAYERFS_${name.toUpperCase()})`, null);
  }
  if (!deploy.volume) delete deploy.volume;
  if (!deploy.containerPrefix) delete deploy.containerPrefix;
  if (!deploy.daemonHost) delete deploy.daemonHost;
  // a former LAYERFS_SANDBOX_BACKEND beside Docker is the binding's to refuse
  return backend === undefined ? deploy : { ...deploy, sandboxBackend: backend };
}

/** A host-daemon Sandbox's settings (`sandbox` native or namespaces): the
 * daemon, the directory its Sandboxes live in and the optional name prefix.
 * Native on Linux uses FUSE with a root daemon and nonroot commands (501:20);
 * on macOS loopback NFS and this user. Namespaces (Linux) gives each Sandbox
 * a root filesystem from an image, so it also needs `image` (LAYERFS_IMAGE,
 * the same digest Docker deploys) and `imageArchive` (LAYERFS_IMAGE_ARCHIVE,
 * the docker-archive the bundle carries). The former `native` with
 * `sandboxBackend` `namespaces` needs the same. `host` is the platform the
 * Sandbox runs on: this process's, or Linux when a worker makes it (see
 * platform.js). */
function localDeployment(options, env, named, host) {
  const namespaces = named.sandbox === "namespaces" || named.sandboxBackend === "namespaces";
  const setting = named.sandbox === "namespaces" ? "deploy.sandbox (LAYERFS_SANDBOX) namespaces"
    : namespaces ? "deploy.sandboxBackend (LAYERFS_SANDBOX_BACKEND) namespaces" : "deploy.sandbox (LAYERFS_SANDBOX) native";
  // #527 step 8: Windows runs Docker Sandboxes through its WSL2 worker; it has no native or namespaces backend
  if (host === "win32") {
    throw new LayerFSError("Unsupported", `${setting} is not supported on Windows: a Windows Sandbox is Docker, through the WSL2 worker; native runs on Linux or macOS, namespaces on Linux`, null);
  }
  // namespaces is Linux only: refused before its image settings are read, so
  // a macOS deployment hears Unsupported, not a missing LAYERFS_IMAGE
  if (namespaces && host !== "linux") {
    throw new LayerFSError("Unsupported", `${setting} is Linux only; use native`, null);
  }
  const deploy = {
    ...named,
    daemon: options.daemon ?? env("DAEMON"),
    localDir: options.localDir ?? env("LOCAL_DIR"),
    containerPrefix: options.containerPrefix ?? env("CONTAINER_PREFIX"),
  };
  const required = [["daemon", "DAEMON"], ["localDir", "LOCAL_DIR"]];
  if (namespaces) {
    deploy.image = options.image ?? env("IMAGE");
    deploy.imageArchive = options.imageArchive ?? env("IMAGE_ARCHIVE");
    required.push(["image", "IMAGE"], ["imageArchive", "IMAGE_ARCHIVE"]);
  }
  for (const [name, setting] of required) {
    if (!deploy[name]) throw new LayerFSError("InvalidInput", `deploy.${name} is not configured (LAYERFS_${setting})`, null);
  }
  if (!deploy.containerPrefix) delete deploy.containerPrefix;
  return deploy;
}

/** Refuses every call on a closed client (or one no factory opened). */
export function live(client) {
  if (client?.[STATE]?.closed !== false) {
    throw new LayerFSError("InvalidInput", `this LayerFS client of ${client?.projectName} is closed; open another with LayerFS.open`, null);
  }
}

/** One call of an open client, with its deployment selection, discovery
 * home and the Store this process opened. */
export function invokeClient(client, op, args) {
  live(client);
  return invoke(op, { ...projectArgs(client), ...storeArgs(client), ...args }, client.options, transportOf(client));
}

/** One backend-table call of an open client, with the same fields. */
export function invokeClientBackend(client, op, args) {
  live(client);
  return invokeBackend(op, { ...projectArgs(client), ...storeArgs(client), ...args }, client.options, transportOf(client));
}

/** A call's project fields: the project, discovery home and the client's
 * deployment, whose daemonHost the binding dials when it reopens a Sandbox
 * from its record and whose settings start a new one. */
export function projectArgs(client) {
  return { project: client.projectName, home: client.options.home, deploy: client.options.deploy, ...(client.options.store ? { store: client.options.store } : {}) };
}

/** The Store fields of a client's calls: its Store file (or a PostgreSQL
 * Store's key) and, for a PostgreSQL Store, its per-call `postgresql: { url }`. */
export const storeArgs = (client) => ({
  ...(client.options.storePath ? { storePath: client.options.storePath } : {}),
  ...(client.options.postgresql ? { postgresql: client.options.postgresql } : {}),
});

/** Opens the client's Store in this process (one more client of it). */
export async function storeOpen(client) {
  return outcome(await transportOf(client).storeOpen(storeOf(client)));
}

/** Releases the client's Store; the last client of the process closes it. */
export async function storeClose(client) {
  return outcome(await transportOf(client).storeClose(storeOf(client)));
}

/** The fields that name a client's Store when it is opened or closed. */
const storeOf = (client) => ({ project: client.projectName, home: client.options.home, ...(client.options.store ? { store: client.options.store } : {}), ...storeArgs(client) });

export function page(pageSize = 50, pagePos = 0) {
  if (!Number.isSafeInteger(pageSize) || pageSize < 1 || pageSize > 1000 || !Number.isSafeInteger(pagePos) || pagePos < 0) {
    throw new LayerFSError("InvalidInput", "pageSize must be 1..1000 and pagePos a nonnegative safe integer", null);
  }
  return { pageSize, pagePos };
}

/** One page of `items`, a whole list a call returned: `{ items, hasMore, nextPagePos }`. */
export function pageOf(items, { pageSize, pagePos }) {
  const end = pagePos + pageSize;
  return { items: items.slice(pagePos, end), hasMore: end < items.length, nextPagePos: end < items.length ? end : null };
}

export function diffOptions({ cursor, hunks } = {}) {
  if ((cursor !== undefined && cursor !== null && typeof cursor !== "string") || (hunks !== undefined && typeof hunks !== "boolean")) {
    throw new LayerFSError("InvalidInput", "cursor must be a path string and hunks a boolean", null);
  }
  return { ...(cursor == null ? {} : { cursor }), ...(hunks === undefined ? {} : { hunks }) };
}

export function checkedMessage(message) {
  if (message === undefined) return undefined;
  if (typeof message !== "string" || Buffer.byteLength(message) > 4096 || [...message].some((c) => (c.codePointAt(0) < 32 && c !== "\n" && c !== "\t") || (c.codePointAt(0) >= 127 && c.codePointAt(0) <= 159) || (c.codePointAt(0) >= 0xd800 && c.codePointAt(0) <= 0xdfff))) {
    throw new LayerFSError("InvalidInput", "message must be UTF-8, at most 4096 bytes, with no controls except newline/tab", null);
  }
  return message;
}

/** A write's last argument as call fields: a Commit message string, or
 * `{ message?, mode? }` where `mode` is the permission bits (e.g. `0o755`).
 * The engine checks `mode`; other option names are ignored. */
export function writeOptions(options) {
  if (options === undefined || typeof options === "string") return { message: checkedMessage(options) };
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new LayerFSError("InvalidInput", "options must be a message string or { message, mode }", null);
  }
  const { message, mode } = options;
  return { message: checkedMessage(message), ...(mode === undefined ? {} : { mode }) };
}

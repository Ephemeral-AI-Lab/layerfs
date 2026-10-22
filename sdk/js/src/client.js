// One call through the native binding. The binding resolves `{ value }` or
// `{ refused }` (plus `telemetry` when traced); a refusal becomes a
// LayerFSError. Nothing is retried.
import { native } from "./native.js";
import { LayerFSError } from "./errors.js";
import { traced } from "./trace.js";

/** Runs one operation once and returns its value or throws its refusal;
 * `options` are the client's, whose `trace`/`traceStep` select its trace line. */
export async function invoke(op, args, options) {
  return outcome(await traced(options, op, args, (scope) => {
    if (op.startsWith("operator.")) throw new LayerFSError("UnknownMethod", "operator methods require the operator entry point", null);
    return native().call(op, observed(args, scope));
  }));
}

export async function invokeOperator(op, args, options) {
  return outcome(await traced(options, op, args, (scope) => native().operatorCall(op, observed(args, scope))));
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

/** Deployment settings from the client options, else the environment
 * (LAYERFS_<NAME>, else DSH_LAYERFS_<NAME>). A Workspace's Sandbox needs
 * every one but the container prefix and daemon host. With `sandbox`
 * (LAYERFS_SANDBOX) `local`, a Sandbox is a daemon process on this machine
 * instead of a container: it needs the daemon and `localDir`
 * (LAYERFS_LOCAL_DIR) and no Docker setting. Unset or `docker` is Docker;
 * any other value is refused. */
export function deployment(options = {}) {
  const env = (name) => process.env[`LAYERFS_${name}`] || process.env[`DSH_LAYERFS_${name}`] || undefined;
  const sandbox = options.sandbox ?? env("SANDBOX");
  if (sandbox === "local") return localDeployment(options, env);
  if (sandbox !== undefined && sandbox !== "docker") {
    throw new LayerFSError("InvalidInput", `deploy.sandbox (LAYERFS_SANDBOX) is docker or local, not ${sandbox}`, null);
  }
  const deploy = {
    docker: options.docker ?? env("DOCKER") ?? "/var/run/docker.sock",
    image: options.image ?? env("IMAGE"),
    volume: options.volume ?? env("VOLUME"),
    daemon: options.daemon ?? env("DAEMON"),
    containerPrefix: options.containerPrefix ?? env("CONTAINER_PREFIX"),
    daemonHost: options.daemonHost ?? env("DAEMON_HOST"),
  };
  for (const [name, value] of Object.entries(deploy)) {
    if (name === "containerPrefix" || name === "daemonHost") continue;
    if (!value) throw new LayerFSError("InvalidInput", `deploy.${name} is not configured (LAYERFS_${name.toUpperCase()})`, null);
  }
  if (!deploy.containerPrefix) delete deploy.containerPrefix;
  if (!deploy.daemonHost) delete deploy.daemonHost;
  return deploy;
}

/** A local Sandbox's settings: the daemon, the directory its Sandboxes
 * live in, and the optional name prefix. */
function localDeployment(options, env) {
  const deploy = {
    sandbox: "local",
    daemon: options.daemon ?? env("DAEMON"),
    localDir: options.localDir ?? env("LOCAL_DIR"),
    containerPrefix: options.containerPrefix ?? env("CONTAINER_PREFIX"),
  };
  for (const [name, setting] of [["daemon", "DAEMON"], ["localDir", "LOCAL_DIR"]]) {
    if (!deploy[name]) throw new LayerFSError("InvalidInput", `deploy.${name} is not configured (LAYERFS_${setting})`, null);
  }
  if (!deploy.containerPrefix) delete deploy.containerPrefix;
  return deploy;
}

/** One client's deployment selection, including cross-process discovery
 * home and the Store this process opens. */
export function invokeClient(client, op, args) {
  return invoke(op, { ...projectArgs(client), ...storeArgs(client), ...args }, client.options);
}

/** A call's project fields: the project, discovery home and the client's
 * deployment, whose daemonHost the binding dials when it reopens a Sandbox
 * from its record and whose settings start a new one. */
export function projectArgs(client) {
  return { project: client.projectName, home: client.options.home, deploy: client.options.deploy };
}

const storeArgs = (client) => (client.options.storePath ? { storePath: client.options.storePath } : {});

/** Opens the client's Store in this process (one more client of it). */
export async function storeOpen(client) {
  return outcome(await native().storeOpen({ project: client.projectName, home: client.options.home, ...storeArgs(client) }));
}

/** Releases the client's Store; the last client of the process closes it. */
export async function storeClose(client) {
  return outcome(await native().storeClose({ project: client.projectName, home: client.options.home, ...storeArgs(client) }));
}

export function page(pageSize = 50, pagePos = 0) {
  if (!Number.isSafeInteger(pageSize) || pageSize < 1 || pageSize > 1000 || !Number.isSafeInteger(pagePos) || pagePos < 0) {
    throw new LayerFSError("InvalidInput", "pageSize must be 1..1000 and pagePos a nonnegative safe integer", null);
  }
  return { pageSize, pagePos };
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

// Picks the binding a new client uses (#428): the worker client when a
// worker endpoint is set, else the real addon in this process. Each client
// keeps the binding it was made with, so the choice does not depend on call
// order: a client made before `@layerfs/runtime` hands over its endpoint
// (useWorker) stays in-process, and every client made after uses the worker.
// `@layerfs/agent` chooses its own transport because it cannot depend on
// `@layerfs/runtime`, whose platform launcher writes the endpoint.
//
// The worker route is open to any integration that names an endpoint
// (AGENTS.md, owner 2026-10-24): @layerfs/runtime starts the worker on macOS
// and Windows, where a process cannot open the Store itself, and in a
// `vm:<provider>` home, where each project's Store is in that project's VM
// (one endpoint per project). LayerFS names no integration here.
import { LayerFSError } from "../errors.js";
import { inprocess } from "./inprocess.js";
import { workerClient } from "./worker_client.js";

/** The worker `useWorker` handed over: { endpoint }, { resolve } or null. */
let handed = null;
let local = null;
/** Worker clients by endpoint file. */
const workers = new Map();

/** A setting from the environment: LAYERFS_<name>. */
export const setting = (name) => process.env[`LAYERFS_${name}`] || undefined;

/** Internal, for @layerfs/runtime: clients made from now on in this process
 * use the worker at `endpoint` (an endpoint file), or, when `endpoint` is a
 * function, the worker that function names for a client's project — one VM
 * per Store has one endpoint each. Takes precedence over the environment.
 * One worker per process: another endpoint is refused, and so is a second
 * resolver. */
export function useWorker(endpoint) {
  const next = typeof endpoint === "function" ? { resolve: endpoint }
    : typeof endpoint === "string" && endpoint ? { endpoint }
    : null;
  if (!next) throw new LayerFSError("InvalidInput", "a worker endpoint file is required", null);
  if (handed && (handed.endpoint !== next.endpoint || handed.resolve !== next.resolve)) {
    throw new LayerFSError("InvalidInput", `this process already uses the worker at ${handed.endpoint ?? "one endpoint per project"}`, null);
  }
  handed = next;
}

/** The binding a client made now uses: the handed-over worker (by project, or
 * the one endpoint), else LAYERFS_WORKER_ENDPOINT's, else the addon in this
 * process. A call without a project (an in-process backend call) uses the
 * environment's endpoint, never a project's resolver. */
export function binding(project) {
  const endpoint = handed?.resolve && typeof project === "string" ? handed.resolve(project) : handed?.endpoint ?? setting("WORKER_ENDPOINT");
  if (!endpoint) return (local ??= inprocess());
  let client = workers.get(endpoint);
  if (!client) workers.set(endpoint, (client = workerClient(endpoint)));
  return client;
}

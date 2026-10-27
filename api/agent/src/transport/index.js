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
// and Windows, where a process cannot open the Store itself. LayerFS names no
// integration here.
import { LayerFSError } from "../errors.js";
import { inprocess } from "./inprocess.js";
import { workerClient } from "./worker_client.js";

/** The worker `useWorker` handed over: { endpoint }, else null. */
let handed = null;
let local = null;
/** Worker clients by endpoint file. */
const workers = new Map();

/** A setting from the environment: LAYERFS_<name>. */
export const setting = (name) => process.env[`LAYERFS_${name}`] || undefined;

/** Internal, for @layerfs/runtime: clients made from now on in this process
 * use the worker at `endpoint` (an endpoint file). Takes precedence over the
 * environment. One worker per process: another endpoint is refused. */
export function useWorker(endpoint) {
  if (typeof endpoint !== "string" || !endpoint) throw new LayerFSError("InvalidInput", "a worker endpoint file is required", null);
  if (handed && handed.endpoint !== endpoint) throw new LayerFSError("InvalidInput", `this process already uses the worker at ${handed.endpoint}`, null);
  handed = { endpoint };
}

/** The binding a client made now uses: the handed-over worker, else
 * LAYERFS_WORKER_ENDPOINT's, else the addon in this process. */
export function binding() {
  const endpoint = handed?.endpoint ?? setting("WORKER_ENDPOINT");
  if (!endpoint) return (local ??= inprocess());
  let client = workers.get(endpoint);
  if (!client) workers.set(endpoint, (client = workerClient(endpoint)));
  return client;
}

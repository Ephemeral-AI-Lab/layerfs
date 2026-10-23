// Picks the binding a new client uses (#428): the worker client when a
// worker endpoint is set, else the real addon in this process. Each client
// keeps the binding it was made with, so the choice does not depend on call
// order: a client made before `@layerfs/runtime` hands over its endpoint
// (useWorker) stays in-process, and every client made after uses the worker.
// `@layerfs/agent` chooses its own transport because it cannot depend on
// `@layerfs/runtime`, whose platform launcher writes the endpoint.
//
// The worker route carries Store calls only under the AGENTS.md exceptions:
// the dsh adapter on macOS (2026-10-20) and on Windows (2026-10-22). Until
// the owner widens them, the route opens only when the adapter names itself
// (LAYERFS_WORKER_ADAPTER=dsh); this is a convention check, not a security
// boundary.
import { LayerFSError } from "../errors.js";
import { inprocess } from "./inprocess.js";
import { workerClient } from "./worker_client.js";

/** Adapters the worker route is allowed for. */
const ALLOWED = new Set(["dsh"]);

/** The worker `useWorker` handed over: { endpoint, adapter }, else null. */
let handed = null;
let local = null;
/** Worker clients by endpoint file. */
const workers = new Map();

/** A setting from the environment: LAYERFS_<name>, else DSH_LAYERFS_<name>. */
export const setting = (name) => process.env[`LAYERFS_${name}`] || process.env[`DSH_LAYERFS_${name}`] || undefined;

const gate = (adapter) => {
  if (!ALLOWED.has(adapter)) {
    throw new LayerFSError("Unsupported", `the worker route (LAYERFS_WORKER_ENDPOINT) is allowed only for the dsh adapter (AGENTS.md); LAYERFS_WORKER_ADAPTER is ${adapter ?? "unset"}`, null);
  }
};

/** Internal, for @layerfs/runtime: clients made from now on in this process
 * use the worker at `endpoint` (an endpoint file). Takes precedence over the
 * environment. One worker per process: another endpoint is refused. */
export function useWorker(endpoint, adapter) {
  gate(adapter);
  if (typeof endpoint !== "string" || !endpoint) throw new LayerFSError("InvalidInput", "a worker endpoint file is required", null);
  if (handed && handed.endpoint !== endpoint) throw new LayerFSError("InvalidInput", `this process already uses the worker at ${handed.endpoint}`, null);
  handed = { endpoint, adapter };
}

/** The binding a client made now uses: the handed-over worker, else
 * LAYERFS_WORKER_ENDPOINT's, else the addon in this process. */
export function binding() {
  const endpoint = handed?.endpoint ?? setting("WORKER_ENDPOINT");
  if (!endpoint) return (local ??= inprocess());
  gate(handed?.adapter ?? setting("WORKER_ADAPTER"));
  let client = workers.get(endpoint);
  if (!client) workers.set(endpoint, (client = workerClient(endpoint)));
  return client;
}

// Picks this process's binding once, at its first call (#428): the worker
// client when a worker endpoint is set, else the real addon in this process.
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

let chosen = null;

/** A setting from the environment: LAYERFS_<name>, else DSH_LAYERFS_<name>. */
export const setting = (name) => process.env[`LAYERFS_${name}`] || process.env[`DSH_LAYERFS_${name}`] || undefined;

/** The binding every call of this process uses. */
export function binding() {
  if (chosen) return chosen;
  const endpoint = setting("WORKER_ENDPOINT");
  if (!endpoint) return (chosen = inprocess());
  const adapter = setting("WORKER_ADAPTER");
  if (!ALLOWED.has(adapter)) {
    throw new LayerFSError("Unsupported", `the worker route (LAYERFS_WORKER_ENDPOINT) is allowed only for the dsh adapter (AGENTS.md); LAYERFS_WORKER_ADAPTER is ${adapter ?? "unset"}`, null);
  }
  return (chosen = workerClient(endpoint));
}

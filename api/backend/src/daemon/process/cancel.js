// backend.daemon.process.cancel: stops one background process, in the
// Sandbox its id names.
import { LayerFSError } from "@layerfs/agent";
import { invokeClientBackend } from "@layerfs/agent/internal";
export function cancel(shop, process) {
  if (typeof process !== "string" || !process) throw new LayerFSError("InvalidInput", "a process is its id from process.start or process.list", null);
  return invokeClientBackend(shop, "backend.daemon.process.cancel", { process });
}

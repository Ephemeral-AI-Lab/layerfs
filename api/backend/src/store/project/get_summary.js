// backend.store.project.get_summary: five bounded Store facts, with no
// object-catalog scan, disk statistics or per-Commit accounting.
import { LayerFSError } from "@layerfs/agent";
import { invokeClientBackend } from "@layerfs/agent/internal";

export function get_summary(shop) {
  if (arguments.length !== 1) {
    throw new LayerFSError("InvalidInput", "get_summary takes no arguments", null);
  }
  return invokeClientBackend(shop, "backend.store.project.get_summary", {});
}

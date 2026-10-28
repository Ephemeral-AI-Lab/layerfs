// backend.daemon.workspace.get_storage: a Workspace overlay's payload,
// inodes, entries and metadata rows.
import { invokeClientBackend, workspaceArgs } from "@layerfs/agent/internal";
export function get_storage(shop, ws) {
  return invokeClientBackend(shop, "backend.daemon.workspace.get_storage", workspaceArgs(ws));
}

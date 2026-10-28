// backend.daemon.workspace.list_changes: a Workspace's uncommitted added,
// modified and removed paths, one page.
import { invokeClientBackend, page, workspaceArgs } from "@layerfs/agent/internal";
export function list_changes(shop, ws, pageSize, pagePos) {
  return invokeClientBackend(shop, "backend.daemon.workspace.list_changes", { ...workspaceArgs(ws), ...page(pageSize, pagePos) });
}

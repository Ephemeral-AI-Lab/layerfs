// backend.daemon.workspace.status: a Workspace's state from its Sandbox's
// daemon (base Commit, activity, overlay counts, the Branch it is of) and
// whether the Branch head moved since mount, read in the project's Store.
import { invokeClientBackend, workspaceArgs } from "@layerfs/agent/internal";
export function status(shop, ws) {
  return invokeClientBackend(shop, "backend.daemon.workspace.status", workspaceArgs(ws));
}

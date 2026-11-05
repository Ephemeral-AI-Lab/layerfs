// workspace.commit: one Commit of the Workspace's changes.
import { invokeClient, workspaceArgs, checkedMessage, commitOptions } from "../client.js";
export function commit(client, ws, message, force = false, options = {}) {
  return invokeClient(client, "workspace.commit", { ...workspaceArgs(ws), message: checkedMessage(message), force, ...commitOptions(options) });
}

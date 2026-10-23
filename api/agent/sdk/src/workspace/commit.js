// workspace.commit: one Commit of the Workspace's changes.
import { invokeClient, workspaceArgs, checkedMessage } from "../client.js";
export function commit(client, ws, message, force = false) {
  return invokeClient(client, "workspace.commit", { ...workspaceArgs(ws), message: checkedMessage(message), force });
}

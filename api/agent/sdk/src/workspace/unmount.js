// workspace.unmount: ends the Workspace; `discard` drops its uncommitted
// changes. `force` aborts its kernel connection instead, also after a normal
// unmount stalled at Join, and drops uncommitted changes. A Sandbox left with
// no Workspace closes once it has been idle.
import { invokeClient, workspaceArgs } from "../client.js";
export function unmount(client, ws, discard = false, force = false) {
  return invokeClient(client, "workspace.unmount", { ...workspaceArgs(ws), discard, force });
}

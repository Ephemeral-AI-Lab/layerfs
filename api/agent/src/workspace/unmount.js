// workspace.unmount: ends the Workspace; `discard` drops its uncommitted
// changes. A normal unmount whose drain stops at Join makes one abort write
// on the already-detached connection and ends there. `force` aborts the
// live connection when an abort control is usable, and drops uncommitted
// changes. An attempted abort is never repeated. A Sandbox left with no Workspace closes once it
// has been idle.
import { invokeClient, workspaceArgs } from "../client.js";
export function unmount(client, ws, discard = false, force = false) {
  return invokeClient(client, "workspace.unmount", { ...workspaceArgs(ws), discard, force });
}

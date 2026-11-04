// workspace.unmount: ends the Workspace; `discard` drops its uncommitted
// changes. A normal unmount whose drain stops at Join makes one abort write
// on the already-detached connection and ends there. `force` aborts the
// live connection when an abort control is usable, and drops uncommitted
// changes. `kill: "workspace"` first cancels only this Workspace's holders
// (background processes and foreground execs), then unmounts normally; it
// never stops or deletes the Sandbox. An attempted abort is never repeated.
// A Sandbox left with no Workspace closes once it has been idle.
import { invokeClient, workspaceArgs } from "../client.js";
export function unmount(client, ws, discard = false, force = false, kill = undefined) {
  const args = { ...workspaceArgs(ws), discard, force };
  if (kill !== undefined) args.kill = kill;
  return invokeClient(client, "workspace.unmount", args);
}

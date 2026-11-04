// workspace.unmount: ends the Workspace; `discard` drops its uncommitted
// changes. A normal unmount whose drain stops at Join makes one abort write
// on the already-detached connection and ends there. `force` aborts the
// live connection when an abort control is usable, and drops uncommitted
// changes. `kill: "workspace"` first cancels only this Workspace's holders
// (background processes and foreground execs), then unmounts normally; it
// never stops or deletes the Sandbox. An attempted abort is never repeated.
// A Sandbox left with no Workspace closes once it has been idle.
// The third argument is an options object. The old positional form
// (`discard?, force?, kill?`) is kept for existing callers.
import { invokeClient, workspaceArgs } from "../client.js";

export function unmount(client, ws, options = {}, force = false, kill = undefined) {
  let discard;
  let killScope;
  if (options !== null && typeof options === "object" && !Array.isArray(options)) {
    discard = options.discard === true;
    force = options.force === true;
    killScope = options.kill;
  } else {
    discard = options === true;
    force = force === true;
    killScope = kill;
  }
  const args = { ...workspaceArgs(ws), discard, force };
  if (killScope !== undefined) args.kill = killScope;
  return invokeClient(client, "workspace.unmount", args);
}

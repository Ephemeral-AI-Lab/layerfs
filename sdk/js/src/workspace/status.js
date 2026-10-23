// workspace.status: the Workspace's state; the Branch head is read in this
// process's Store.
import { invokeBackend, projectArgs, workspaceArgs } from "../client.js";
export async function status(client, ws) {
  await client.store();
  const args = { ...projectArgs(client), storePath: client.options.storePath, ...workspaceArgs(ws) };
  return invokeBackend("backend.daemon.workspace.status", args, client.options);
}

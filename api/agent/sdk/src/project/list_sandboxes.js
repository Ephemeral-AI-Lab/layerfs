// project.list_sandboxes: the project's running Sandboxes, lowest number
// first, each { sandbox, name, image, workspaces } (`workspaces`: how many it
// holds). It reads their records and starts and changes nothing: a stopped
// Sandbox is left out, its records kept for the next sandbox.create or runtime.sandbox.reclaim.
import { invokeClient, page, pageOf } from "../client.js";
export async function list_sandboxes(client, pageSize = 50, pagePos = 0) {
  const at = page(pageSize, pagePos);
  const value = await invokeClient(client, "project.list_sandboxes", {});
  return pageOf(value.items.map(({ sandbox, name, image, workspaces }) => ({ sandbox, name, image, workspaces })), at);
}

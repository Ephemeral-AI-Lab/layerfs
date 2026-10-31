// project.list_sandboxes: the project's running Sandboxes of every platform,
// lowest number first, each { sandbox, name, image, platform, workspaces }
// (`workspaces`: how many it holds; `image` null for a mac or native linux
// one). A record from before #529 names no platform and reads as the one its
// runtime and the home imply (../platform.js). It reads their records and
// starts and changes nothing: a stopped Sandbox is left out, its records kept
// for the next sandbox.create or runtime.sandbox.reclaim.
import { invokeClient, page, pageOf } from "../client.js";
import { homeOf, imageOf, platformOf } from "../platform.js";

export async function list_sandboxes(client, pageSize = 50, pagePos = 0) {
  const at = page(pageSize, pagePos);
  const value = await invokeClient(client, "project.list_sandboxes", {});
  const home = homeOf(client);
  return pageOf(value.items.map((item) => ({ sandbox: item.sandbox, name: item.name, image: imageOf(item.image), platform: platformOf(item, home), workspaces: item.workspaces })), at);
}

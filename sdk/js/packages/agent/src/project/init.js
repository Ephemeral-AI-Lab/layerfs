// project.init: one Init of the client's project from `sourcePath`, called by
// LayerFS.init before the client is open.
import { invoke, projectArgs, storeArgs } from "../client.js";
export function init(client, sourcePath) {
  const policy = client.options.storePolicy;
  return invoke("project.init", { ...projectArgs(client), ...storeArgs(client), path: sourcePath, name: client.projectName, ...(policy ? { policy } : {}) }, client.options);
}

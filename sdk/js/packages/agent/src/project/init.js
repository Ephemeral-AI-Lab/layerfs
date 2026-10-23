// project.init: one Init of the client's project from `sourcePath`, called by
// LayerFS.init before the client is open. `timeout` (milliseconds, a positive
// whole number) is sent as `timeoutMs`; the native code where Init runs starts
// its clock. Without it no field is sent and Init has no time limit (#431).
import { invoke, projectArgs, storeArgs, transportOf } from "../client.js";
import { LayerFSError } from "../errors.js";
export function init(client, sourcePath, timeout) {
  if (timeout !== undefined && !(Number.isSafeInteger(timeout) && timeout > 0)) {
    throw new LayerFSError("InvalidInput", "timeout must be a positive whole number of milliseconds", null);
  }
  const policy = client.options.storePolicy;
  return invoke("project.init", { ...projectArgs(client), ...storeArgs(client), path: sourcePath, name: client.projectName, ...(policy ? { policy } : {}), ...(timeout !== undefined ? { timeoutMs: timeout } : {}) }, client.options, transportOf(client));
}

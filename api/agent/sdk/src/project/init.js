// project.init: one Init of the client's project from `sourcePath`, called by
// LayerFS.init before the client is open. `timeout` (milliseconds, a positive
// whole number; default INIT_TIMEOUT_MS) is always sent as `timeoutMs`; the
// native code where Init runs starts its clock (#431). There is no unlimited
// option. LAYERFS_STORE_POLICY (captured by the client), a JSON object
// { smallFileThreshold, wholeFileDepth, chunkDepth, metadataDepth }, is sent
// as `policy`: JSON that is not such an object is refused here, and the
// native code's refusal of a field is named after the variable too.
import { invoke, projectArgs, storeArgs, transportOf } from "../client.js";
import { LayerFSError } from "../errors.js";

/** Init's default timeout, in milliseconds: 300 s. */
export const INIT_TIMEOUT_MS = 300_000;

export async function init(client, sourcePath, timeout = INIT_TIMEOUT_MS) {
  if (!(Number.isSafeInteger(timeout) && timeout > 0)) {
    throw new LayerFSError("InvalidInput", "timeout must be a positive whole number of milliseconds", null);
  }
  const policy = storePolicy(client.options.storePolicy);
  try {
    return await invoke("project.init", { ...projectArgs(client), ...storeArgs(client), path: sourcePath, name: client.projectName, ...(policy ? { policy } : {}), timeoutMs: timeout }, client.options, transportOf(client));
  } catch (error) {
    // the native check runs before Init makes anything: `policy.<field> …` or `policy: …`
    const field = policy && error?.code === "InvalidInput" && /^InvalidInput: (policy[.:].*)$/s.exec(error.message);
    if (field) throw new LayerFSError("InvalidInput", `LAYERFS_STORE_POLICY ${field[1]}`, error.detail);
    throw error;
  }
}

/** The policy in LAYERFS_STORE_POLICY's text, or none when it is unset. */
function storePolicy(text) {
  if (text === undefined) return undefined;
  let policy;
  try { policy = JSON.parse(text); } catch { policy = null; }
  if (policy === null || typeof policy !== "object" || Array.isArray(policy)) {
    throw new LayerFSError("InvalidInput", "LAYERFS_STORE_POLICY must be a JSON object { smallFileThreshold, wholeFileDepth, chunkDepth, metadataDepth }", null);
  }
  return policy;
}

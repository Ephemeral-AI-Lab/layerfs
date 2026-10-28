// sandbox.create: a new Sandbox of the project, the lowest free number, on
// `image` (default LAYERFS_IMAGE), ready when this returns: { sandbox, image }.
// The only call that starts a Sandbox (no placement since 2026-10-23); it
// first reclaims the project's Sandboxes that closed themselves. The new one
// closes itself two minutes after it has no Workspace and no process, like
// every Sandbox; `sandbox.close` stops it now.
import { deployment, invokeClient } from "../client.js";
import { LayerFSError } from "../errors.js";
export async function create(client, options = {}) {
  if (options === null || typeof options !== "object" || (options.image !== undefined && typeof options.image !== "string")) {
    throw new LayerFSError("InvalidInput", "options are { image? }, an image digest", null);
  }
  const deploy = deployment({ ...client.options.deploy, ...(options.image ? { image: options.image } : {}) });
  const value = await invokeClient(client, "sandbox.create", { deploy });
  return { sandbox: value.sandbox, image: value.image };
}

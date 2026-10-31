// sandbox.create: a new Sandbox of the project, the lowest free number, on
// `image` (default LAYERFS_IMAGE) and `platform` (#529; default the home's,
// see ../platform.js), ready when this returns: { sandbox, image, platform }.
// `image` is null for a mac or native linux Sandbox, and an image given for
// one is ignored. A platform the home does not allow is refused Unsupported,
// any other value InvalidInput, both before anything is made. The only call
// that starts a Sandbox; it first reclaims the project's Sandboxes that
// closed themselves. The new one closes itself two minutes after it has no
// Workspace and no process, like every Sandbox; `sandbox.close` stops it now.
import { invokeClient } from "../client.js";
import { LayerFSError } from "../errors.js";
import { checkedPlatform, imageOf, platformDeployment } from "../platform.js";

export async function create(client, options = {}) {
  if (options === null || typeof options !== "object" || (options.image !== undefined && typeof options.image !== "string")) {
    throw new LayerFSError("InvalidInput", "options are { image?, platform? }: an image digest and a platform name", null);
  }
  const { home, platform } = checkedPlatform(client, options.platform);
  const deploy = platformDeployment(client, home, platform, options.image);
  const value = await invokeClient(client, "sandbox.create", { deploy, platform });
  return { sandbox: value.sandbox, image: imageOf(value.image), platform: value.platform ?? platform };
}

// workspace.mount: a new Workspace of a Branch. Without `options.sandbox`
// LayerFS places it in one of the project's Sandboxes of its image (the
// lowest-numbered running one with a free slot, else a new one, started with
// the client's deployment); with it, in that Sandbox only, refused
// NoSuchSandbox when it is not running, Capacity when it holds 16 Workspaces
// and InvalidInput when `options.image` differs from its image. The image is
// `options.image`, else the deployment's. The Branch is found by name in the
// project's Store; the daemon binds its id. Returns the Workspace,
// `{ sandbox, path }`, and its Sandbox's `image`.
import { deployment, invokeClient, sandboxArg } from "../client.js";
import { LayerFSError } from "../errors.js";
export async function mount(client, branch, options = {}) {
  if (options === null || typeof options !== "object" || (options.image !== undefined && typeof options.image !== "string")) {
    throw new LayerFSError("InvalidInput", "options are { sandbox?, image? }", null);
  }
  const chosen = options.sandbox === undefined ? {} : { sandbox: sandboxArg(options.sandbox), ...(options.image ? { image: options.image } : {}) };
  const deploy = deployment({ ...client.options.deploy, ...(options.image ? { image: options.image } : {}) });
  const value = await invokeClient(client, "workspace.mount", { branch, deploy, ...chosen });
  return { sandbox: value.sandbox, path: value.path, image: value.image };
}

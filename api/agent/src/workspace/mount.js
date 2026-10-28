// workspace.mount(sandbox, branch): a new Workspace of a Branch in the
// running Sandbox `sandbox` names (its number, as sandbox.create returns it),
// and nowhere else: there is no placement (owner 2026-10-23), and a mount
// never starts a Sandbox. Refused NoSuchSandbox when it is not running (or
// closed itself) and Capacity when it holds 16 Workspaces; each refusal's
// message names the calls to make instead. The Sandbox fixes the image. The
// Branch is found by name in the project's Store; the daemon binds its id.
// The native call gets { sandbox, branch } with the client's project, home,
// deploy and storePath. Returns the Workspace, `{ sandbox, path }`, and its
// Sandbox's `image`.
import { invokeClient } from "../client.js";
import { LayerFSError } from "../errors.js";
export async function mount(client, sandbox, branch, ...rest) {
  if (typeof sandbox !== "string" || !/^[1-9][0-9]{0,9}$/.test(sandbox) || typeof branch !== "string" || rest.some((value) => value !== undefined)) {
    throw new LayerFSError("InvalidInput", "workspace.mount(sandbox, branch) takes a Sandbox number such as \"1\" (from sandbox.create() or project.list_sandboxes()) and a Branch name, and no options", null);
  }
  const value = await invokeClient(client, "workspace.mount", { sandbox, branch });
  return { sandbox: value.sandbox, path: value.path, image: value.image };
}

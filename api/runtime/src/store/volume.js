// Removing a Store's volume: what uses it first. A running container (a
// Sandbox) refuses Busy unless `stopSandboxes`; stopped containers left by
// failed starts and idle Sandboxes go with it. Moved from project/delete.js
// (#506), where it was a project's own volume. The four Store hooks are the
// platform's: Docker volumes and containers on a local placement, the
// project's VM, its disk and its namespace Sandboxes under `vm:` (#515).
// One attempt per step; a failed step leaves the rest in place.
import { LayerFSError } from "@layerfs/agent";
import { STOPPED } from "../platform/shared.js";

/** `owner` is `{ name, volume }`: the project or Store the volume holds. A
 * `vm:` placement's Store is the project's own VM and disk, so the hooks
 * name the project as well as its volume. */
export async function removeVolume(runtime, owner, options = {}) {
  const { settings, platform } = runtime;
  const holders = await platform.storeHolders(runtime, owner);
  const users = holders.filter((holder) => !STOPPED.has(holder.state)).length;
  if (users && !options.stopSandboxes) throw new LayerFSError("Busy", `${users} running container(s) use ${owner.volume}; a Sandbox closes 2 minutes after its last Workspace is unmounted, or pass stopSandboxes`, null);
  await platform.releaseStoreHolders(runtime, owner, holders, { force: users > 0 });
  await platform.releaseStore(settings, owner);
  await platform.removeStore(runtime, owner);
}

// Removing a Store's volume: what uses it first. A running container (a
// Sandbox) refuses Busy unless `stopSandboxes`; stopped containers left by
// failed starts and idle Sandboxes go with it. Moved from project/delete.js
// (#506), where it was a project's own volume. The four Store hooks are the
// platform's, keyed by Store: Docker volumes and containers on a local
// placement (place/docker/stores.js; on Linux also the running native and
// namespaces Sandboxes the records name, which only their own backend
// closes, #527 step 12), the project's VM, its disk and its Sandboxes under
// `vm:` (#515), which holds only a project's own Store. One attempt per step; a failed step leaves the rest in place.
import { LayerFSError } from "@layerfs/agent";
import { STOPPED } from "../place/docker/sandboxes.js";

/** `store` is `{ name, volume, legacy }`: `legacy` for the Store a project's own Init made, named after
 * that project (a `vm:` placement's Store is that project's VM and disk), else a Store of the home. */
export async function removeVolume(runtime, store, options = {}) {
  const { settings, platform } = runtime;
  const holders = await platform.storeHolders(runtime, store);
  const users = holders.filter((holder) => !STOPPED.has(holder.state)).length;
  if (users && !options.stopSandboxes) throw new LayerFSError("Busy", `${users} running Sandbox(es) use ${store.volume}; a Sandbox closes 2 minutes after its last Workspace is unmounted, or pass stopSandboxes`, null);
  await platform.releaseStoreHolders(runtime, store, holders, { force: users > 0 });
  await platform.releaseStore(settings, store);
  await platform.removeStore(runtime, store);
}

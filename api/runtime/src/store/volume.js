// Removing a Store's volume: what uses it first. A running container (a
// Sandbox) refuses Busy unless `stopSandboxes`; stopped containers left by
// failed starts and idle Sandboxes go with it. Moved from project/remove.js
// (#506), where it was a project's own volume. One attempt per step; a
// failed step leaves the rest in place.
import { LayerFSError } from "@layerfs/agent";

/** Container states that hold the volume but run nothing. */
const STOPPED = new Set(["created", "exited", "dead"]);

export async function removeVolume(runtime, volume, options = {}) {
  const { settings, platform } = runtime;
  const rows = await platform.docker(settings, ["ps", "-a", "--filter", `volume=${volume}`, "--format", "{{.ID}} {{.State}}"]);
  const containers = rows.split("\n").filter(Boolean).map((line) => line.split(" "));
  const users = containers.filter(([, state]) => !STOPPED.has(state)).length;
  if (users && !options.stopSandboxes) throw new LayerFSError("Busy", `${users} running container(s) use ${volume}; a Sandbox closes 2 minutes after its last Workspace is unmounted, or pass stopSandboxes`, null);
  if (containers.length) await platform.docker(settings, ["rm", ...(users ? ["-f"] : []), ...containers.map(([id]) => id)], { timeout: 30_000 });
  await platform.releaseStore(settings, volume);
  await platform.docker(settings, ["volume", "rm", volume], { timeout: 30_000 });
}

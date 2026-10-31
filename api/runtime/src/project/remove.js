// runtime.project.remove(name, { confirm, stopSandboxes, volume }): what Init
// made, in reverse. Copied from the dsh Host's remove.js (#428). `confirm`
// must equal the name. It refuses Busy while a running container (a
// Sandbox) uses the project's volume, unless `stopSandboxes`; stopped
// containers on the volume, left by failed starts and idle Sandboxes, go
// with it. The volume is the runtime's record's, else `volume`. The home
// entry is the project, so deleting it frees the name at once. One attempt
// each, no retry; a failed step leaves the rest in place. The four Store hooks
// are the platform's: Docker volumes and containers on a local placement, the
// VM's own directory and namespace Sandboxes under `vm:`.
import { LayerFSError } from "@layerfs/agent";
import { STOPPED } from "../platform/shared.js";
import { NAME, dropRecord, record } from "./list.js";

const refuse = (code, message) => new LayerFSError(code, message, null);

export async function remove(runtime, name, options = {}) {
  const { settings, platform } = runtime;
  if (typeof name !== "string" || !NAME.test(name)) throw refuse("InvalidInput", "a project name is required");
  if (options?.confirm !== name) throw refuse("InvalidInput", `confirm with the project's name, ${name}`);
  const volume = record(settings, name)?.volume ?? options.volume;
  if (typeof volume !== "string" || !/^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(volume)) throw refuse("InvalidInput", `the runtime has no record of ${name}'s volume; name it as { volume }`);
  if (!record(settings, name) && !(await platform.homeNames(settings)).includes(name)) throw refuse("NoSuchProject", `no LayerFS project named ${name}`);
  await runtime.clients.forget(name);
  // the hooks name the project as well as its volume: a `vm:` placement's Store is
  // the project's own VM and disk, a local placement's is a Docker volume
  const project = { name, volume };
  const holders = await platform.storeHolders(runtime, project);
  const users = holders.filter((holder) => !STOPPED.has(holder.state)).length;
  if (users && !options.stopSandboxes) throw refuse("Busy", `${users} running container(s) use ${volume}; a Sandbox closes 2 minutes after its last Workspace is unmounted, or pass stopSandboxes`);
  await platform.releaseStoreHolders(runtime, project, holders, { force: users > 0 });
  await platform.releaseStore(settings, project);
  await platform.removeStore(runtime, project);
  await platform.removeHome(settings, name);
  dropRecord(settings, name);
  return { kind: "Removed", project: name, volume };
}

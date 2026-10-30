// runtime.project.delete(name, { store, confirm, stopSandboxes, volume, url }).
// Copied from the dsh Host's remove.js (#428); Store-first since #506.
// `confirm` must equal the name; `store` names the project's Store when the
// name is in several.
// - A project alone in a Store of its own (made by its own Init): what Init
//   made, in reverse. It refuses Busy while a running container (a Sandbox)
//   uses the volume, unless `stopSandboxes`; stopped containers on it go
//   with it. The volume is the runtime's record's, else `volume`. The home
//   entry is the project, so deleting it frees the name at once.
// - A project of a Store that holds others: its Sandboxes go (Busy while one
//   runs, unless `stopSandboxes`), then its stack is dropped from the Store
//   (the binding's runtime.store.remove_project: its Layers, Branches and
//   Commits); the volume and the other projects stay. Its packs stay in the
//   Store until reclamation (A8, #411). A PostgreSQL Store's `url` is
//   passed for the call.
// One attempt each, no retry; a failed step leaves the rest in place.
import { LayerFSError } from "@layerfs/agent";
import { invokeBackend } from "@layerfs/agent/internal";
import { NAME, dropRecord, keyOf, list, record } from "./list.js";
import { drop, entries, named, put, storeFields, update } from "../store/registry.js";
import { removeVolume } from "../store/volume.js";
import { containers } from "../sandbox/leftovers.js";

const refuse = (code, message) => new LayerFSError(code, message, null);

/** Removes a project's home entry and the runtime's record of it. */
export async function forgetOwn(runtime, name) {
  await runtime.platform.removeHome(runtime.settings, name);
  dropRecord(runtime.settings, name);
}

export async function deleteProject(runtime, name, options = {}) {
  const { settings, platform } = runtime;
  if (typeof name !== "string" || !NAME.test(name)) throw refuse("InvalidInput", "a project name is required");
  if (options?.confirm !== name) throw refuse("InvalidInput", `confirm with the project's name, ${name}`);
  const found = (await list(runtime)).filter((p) => p.name === name && (options.store === undefined || p.store === options.store));
  if (found.length > 1) throw refuse("InvalidInput", `project ${name} is in Stores ${found.map((p) => p.store).join(", ")}: name its { store }`);
  const project = found[0];
  const others = project && (await named(runtime, project.store)).projects.some((p) => p.name !== name);
  if (project && (project.shared || others)) return dropStack(runtime, project, options);
  const volume = record(settings, name)?.volume ?? options.volume;
  if (typeof volume !== "string" || !/^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(volume)) throw refuse("InvalidInput", `the runtime has no record of ${name}'s volume; name it as { volume }`);
  if (!project) throw refuse("NoSuchProject", `no LayerFS project named ${name}`);
  await runtime.clients.forget(name);
  await removeVolume(runtime, volume, options);
  await forgetOwn(runtime, name);
  if (entries(settings).some((s) => s.name === name)) drop(settings, name);
  return { kind: "Deleted", project: name, volume };
}

/** Container states that run nothing. */
const STOPPED = new Set(["created", "exited", "dead"]);

/** Drops one project's stack from a Store that holds others: its Sandboxes
 * first (Busy while one runs, unless `stopSandboxes`), then the stack. The
 * Store, its volume and its other projects stay; when the project was the
 * one its Store was made by, the Store keeps an entry of its own. */
async function dropStack(runtime, project, options) {
  const { settings, platform } = runtime;
  const store = await named(runtime, project.store);
  const own = store.legacy && project.name === store.name;
  const sandboxes = (await containers(runtime)).filter((c) => c.key === keyOf(project));
  const running = sandboxes.filter((c) => !STOPPED.has(c.state)).length;
  if (running && !options.stopSandboxes) throw refuse("Busy", `${running} running Sandbox(es) of ${project.name} use Store ${store.name}; a Sandbox closes 2 minutes after its last Workspace is unmounted, or pass stopSandboxes`);
  if (sandboxes.length) await platform.docker(settings, ["rm", ...(running ? ["-f"] : []), ...sandboxes.map((c) => c.name)], { timeout: 30_000 });
  await runtime.clients.forget(keyOf(project));
  // a PostgreSQL Store's URL is passed per call
  if (store.kind === "postgresql" && (typeof options.url !== "string" || !options.url)) throw refuse("InvalidInput", `Store ${store.name} is a PostgreSQL schema: pass its { url } for this call`);
  const postgresql = store.kind === "postgresql" ? { postgresql: { url: options.url } } : {};
  const answer = await invokeBackend("runtime.store.remove_project", { ...storeFields(runtime, store), ...postgresql, project: project.name }, {});
  const rest = store.projects.filter((p) => p.name !== project.name);
  if (own) {
    put(settings, { ...store, legacy: false, projects: rest });
    dropRecord(settings, project.name);
  } else if (entries(settings).some((s) => s.name === store.name)) {
    update(settings, store.name, (s) => ({ ...s, projects: s.projects.filter((p) => p.name !== project.name) }));
  }
  return { kind: "Deleted", project: project.name, store: store.name, ...answer };
}

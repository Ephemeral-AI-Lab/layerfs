// The Sandboxes a project's records name, of every backend (#527 step 12,
// F6). The binding's runtime.sandbox.records lists each record with its
// `runtime` (docker, native, namespaces) and `state` (running, stopped,
// gone, refused with its `refusal`, unknown) without matching a deployment's backend, and reopens none,
// so a native or namespaces Sandbox is found beside Docker ones. One backend
// call on the project's binding (in this process, or its worker); no client
// and no Store is opened. A `vm:` home's Store holders and a Linux `local`
// home's (platform `localSandboxes`) are read through it.
import { invokeBackend } from "@layerfs/agent/internal";
import { callHome } from "../settings.js";

/** `[{ sandbox, name, runtime, platform, image, state, workspaces, message?, refusal? }]` of `project`
 * ({ name, store?, shared? }); `platform` is null in a record from before #529. */
export async function recorded(runtime, project) {
  // the home's name prefix and, on a Linux local home, its LAYERFS_LOCAL_DIR:
  // a local record is checked against them (#529), as its clients check it
  const { containerPrefix, localDir } = runtime.settings;
  const deploy = { ...(containerPrefix ? { containerPrefix } : {}), ...(localDir ? { localDir } : {}) };
  const args = { project: project.name, home: callHome(runtime.settings), ...(project.shared ? { store: project.store } : {}), deploy };
  return (await invokeBackend("runtime.sandbox.records", args, {})).items;
}

/** The projects of this home whose records can name a Sandbox of a host backend: SQLite ones with a home entry. */
export async function recordingProjects(runtime) {
  return (await runtime.project.list()).filter((p) => p.initialized && p.kind !== "postgresql");
}

/** A record that may still hold its project's Store: running, refused when read (#529 finding 6:
 * what it names is not checked, so it may run), or of unknown state (a record of the former `local`
 * runtime, never used, holds nothing). */
export const holding = (item) => item.state === "running" || item.state === "refused" || (item.state === "unknown" && item.runtime !== "local");

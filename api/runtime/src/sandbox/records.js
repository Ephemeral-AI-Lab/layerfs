// The Sandboxes a project's records name, of every backend (#527 step 12,
// F6). The binding's runtime.sandbox.records lists each record with its
// `runtime` (docker, native, namespaces) and `state` (running, stopped,
// gone, unknown) without matching a deployment's backend, and reopens none,
// so a native or namespaces Sandbox is found beside Docker ones. One backend
// call on the project's binding (in this process, or its worker); no client
// and no Store is opened. A `vm:` home's Store holders and a Linux `local`
// home's (platform `localSandboxes`) are read through it.
import { invokeBackend } from "@layerfs/agent/internal";
import { callHome } from "../settings.js";

/** `[{ sandbox, name, runtime, image, state, workspaces, message? }]` of `project` ({ name, store?, shared? }). */
export async function recorded(runtime, project) {
  const args = { project: project.name, home: callHome(runtime.settings), ...(project.shared ? { store: project.store } : {}) };
  return (await invokeBackend("runtime.sandbox.records", args, {})).items;
}

/** The projects of this home whose records can name a Sandbox of a host backend: SQLite ones with a home entry. */
export async function recordingProjects(runtime) {
  return (await runtime.project.list()).filter((p) => p.initialized && p.kind !== "postgresql");
}

/** A record that may still hold its project's Store: running, or of unknown state (a record of the
 * former `local` runtime, never used, holds nothing). */
export const holding = (item) => item.state === "running" || (item.state === "unknown" && item.runtime !== "local");

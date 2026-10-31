// Sandbox containers of this home that no project owns, or that were left
// stopped, and their removal. From part of the dsh Host's runtime.js (#428).
// The containers are the Docker group's (place/docker/sandboxes.js).
import { LayerFSError } from "@layerfs/agent";
import { keyOf } from "../project/list.js";
import { dockerOf, VM_SANDBOXES } from "../place/shared.js";

/** This home's Sandbox containers, running or not; refused where the placement has no Docker group. */
export async function containers(runtime) {
  return dockerOf(runtime, "runtime.sandbox", VM_SANDBOXES).sandboxes.containers(runtime);
}

/** Containers of no project of this home, or stopped: `[{ name, project, running, status }]`. */
export async function leftovers(runtime) {
  const known = new Set((await runtime.project.list()).map(keyOf));
  return (await containers(runtime)).filter((c) => !known.has(c.key) || !c.running).map(({ name, project, running, status }) => ({ name, project, running, status }));
}

/** runtime.sandbox.clean_leftovers(names): one `docker rm` per named stopped leftover; a running one is refused. */
export async function clean_leftovers(runtime, names) {
  if (!Array.isArray(names) || !names.length || names.some((n) => typeof n !== "string")) throw new LayerFSError("InvalidInput", "names lists the leftover containers to remove", null);
  const left = new Map((await leftovers(runtime)).map((c) => [c.name, c]));
  const results = [];
  for (const name of names) {
    const c = left.get(name);
    if (!c) { results.push({ name, ok: false, message: "not a leftover of this home" }); continue; }
    if (c.running) { results.push({ name, ok: false, message: "still running; stop it first" }); continue; }
    try {
      await runtime.platform.docker.sandboxes.remove(runtime, [name]);
      results.push({ name, ok: true });
    } catch (error) {
      results.push({ name, ok: false, message: error.stderr?.trim() || error.message });
    }
  }
  return { results };
}

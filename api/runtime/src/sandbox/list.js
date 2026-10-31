// runtime.sandbox.list(): this home's Sandbox containers, running or not,
// with what each holds outside its Workspaces (#423). `scratch` is the
// container's own writable layer in bytes: whatever commands wrote outside a
// Workspace (/tmp, which is also HOME, and the npm, node and cargo caches
// there). It goes only when the container is deleted (sandbox.close, an idle
// close followed by a reclaim, or runtime.sandbox.sweep). One `docker ps`
// and one `docker inspect --size` (platform/docker/sandboxes.js); it changes nothing.
import { keyOf } from "../project/list.js";
import { dockerOf, VM_SANDBOXES } from "../platform/shared.js";

/** `[{ name, project, sandbox, state, running, startedAt, finishedAt, image, ports, scratch, owned }]`, by name. */
export async function list(runtime) {
  const docker = dockerOf(runtime, "runtime.sandbox.list", VM_SANDBOXES);
  const found = await docker.sandboxes.details(runtime);
  if (!found.length) return [];
  const known = new Set((await runtime.project.list()).map(keyOf));
  return found.map((c) => ({ ...c, owned: c.project !== null && known.has(c.store ? `${c.store}/${c.project}` : c.project) }));
}

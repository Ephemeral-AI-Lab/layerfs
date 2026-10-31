// runtime.sandbox.list(): this home's Sandbox containers, running or not,
// with what each holds outside its Workspaces (#423). `scratch` is the
// container's own writable layer in bytes: whatever commands wrote outside a
// Workspace (/tmp, which is also HOME, and the npm, node and cargo caches
// there). It goes only when the container is deleted (sandbox.close, an idle
// close followed by a reclaim, or runtime.sandbox.sweep). One `docker ps`
// and one `docker inspect --size` (platform/docker/sandboxes.js), plus on Linux one
// runtime.sandbox.records per project (sandbox/records.js); it changes nothing.
import { keyOf } from "../project/list.js";
import { dockerOf, VM_SANDBOXES } from "../platform/shared.js";
import { recorded, recordingProjects } from "./records.js";

/** `[{ name, project, sandbox, runtime, platform, state, running, startedAt, finishedAt, image, ports, scratch, owned }]`,
 * by name. Where native and namespaces Sandboxes can run beside Docker's (`localSandboxes`, Linux),
 * each project's records add those (#527 step 12, F6): named by their directory, with no container
 * facts (`startedAt`, `finishedAt`, `scratch` null, no ports), `state` from the binding's
 * runtime.sandbox.records (running, stopped or unknown). `platform` (#529) is `docker` for a
 * container and the record's own for the others (`linux` in a record from before it). */
export async function list(runtime) {
  const docker = dockerOf(runtime, "runtime.sandbox.list", VM_SANDBOXES);
  const found = (await docker.sandboxes.details(runtime)).map((c) => ({ ...c, runtime: "docker", platform: "docker" }));
  if (runtime.platform.localSandboxes) {
    for (const project of await recordingProjects(runtime)) {
      for (const item of (await recorded(runtime, project)).filter((r) => r.runtime !== "docker")) {
        found.push({
          name: item.name, project: project.name, store: project.shared ? project.store : null, sandbox: item.sandbox, runtime: item.runtime, platform: item.platform ?? "linux",
          state: item.state, running: item.state === "running", startedAt: null, finishedAt: null, image: item.image || null, ports: [], scratch: null,
        });
      }
    }
    found.sort((a, b) => a.name.localeCompare(b.name));
  }
  if (!found.length) return [];
  const known = new Set((await runtime.project.list()).map(keyOf));
  return found.map((c) => ({ ...c, owned: c.project !== null && known.has(c.store ? `${c.store}/${c.project}` : c.project) }));
}

// runtime.sandbox.sweep(): deletes the stopped Sandboxes of every project of
// this home, so their scratch (#423) does not wait for that project's next
// sandbox.create. It finds projects with an exited or dead Sandbox container,
// by the container's labels (layerfs.project, layerfs.store), not by its name
// (the name format is the binding's, #527 step 5), and runs
// runtime.sandbox.reclaim on each: the project's own records and
// guards decide what goes, one unforced Delete per Sandbox, so a container
// of another home or one starting now is never touched. Containers no
// project of this home records stay for list_leftovers / clean_leftovers.
// The runtime runs it once when it opens (settings.sweep, default true);
// status().sweep reports that run. One attempt per project, no retry.
// A `vm:<provider>` home keeps no host containers (no Docker group), so its
// sweep reads none and says so: `{ reclaimed: {}, failed: [], host: false }`.
import { keyOf } from "../project/list.js";

/** States of a container whose daemon has ended and that nothing restarts. */
const ENDED = new Set(["exited", "dead"]);

/** `{ reclaimed: { project: [ids] }, failed: [{ project, code, message }] }` */
export async function sweep(runtime) {
  if (!runtime.platform.docker) return { reclaimed: {}, failed: [], host: false };
  const known = new Map((await runtime.project.list()).filter((p) => p.volume && p.kind !== "postgresql").map((p) => [keyOf(p), p]));
  const ended = (await runtime.platform.docker.sandboxes.containers(runtime)).filter((c) => ENDED.has(c.state) && known.has(c.key));
  const reclaimed = {}, failed = [];
  for (const key of [...new Set(ended.map((c) => c.key))].sort()) {
    const project = known.get(key);
    try {
      const { reclaimed: ids } = await runtime.sandbox.reclaim(project.name, project.shared ? { store: project.store } : undefined);
      if (ids?.length) reclaimed[key] = ids;
    } catch (error) {
      failed.push({ project: key, code: error?.code ?? "Unknown", message: error?.message ?? String(error) });
    }
  }
  return { reclaimed, failed };
}

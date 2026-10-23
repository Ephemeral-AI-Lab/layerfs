// runtime.sandbox.sweep(): deletes the stopped Sandboxes of every project of
// this home, so their scratch (#423) does not wait for that project's next
// sandbox.create. It finds projects with an exited or dead Sandbox container
// and runs runtime.sandbox.reclaim on each: the project's own records and
// guards decide what goes, one unforced Delete per Sandbox, so a container
// of another home or one starting now is never touched. Containers no
// project of this home records stay for list_leftovers / clean_leftovers.
// The runtime runs it once when it opens (settings.sweep, default true);
// status().sweep reports that run. One attempt per project, no retry.
import { containers } from "./leftovers.js";

/** States of a container whose daemon has ended and that nothing restarts. */
const ENDED = new Set(["exited", "dead"]);

/** `{ reclaimed: { project: [ids] }, failed: [{ project, code, message }] }` */
export async function sweep(runtime) {
  const { settings } = runtime;
  const known = new Set((await runtime.project.list()).filter((p) => p.volume).map((p) => p.name));
  const ended = (await containers(runtime)).filter((c) => ENDED.has(c.state) && known.has(c.project) && c.name.startsWith(`${settings.containerPrefix}${c.project}-`));
  const reclaimed = {}, failed = [];
  for (const project of [...new Set(ended.map((c) => c.project))].sort()) {
    try {
      const { reclaimed: ids } = await runtime.sandbox.reclaim(project);
      if (ids?.length) reclaimed[project] = ids;
    } catch (error) {
      failed.push({ project, code: error?.code ?? "Unknown", message: error?.message ?? String(error) });
    }
  }
  return { reclaimed, failed };
}

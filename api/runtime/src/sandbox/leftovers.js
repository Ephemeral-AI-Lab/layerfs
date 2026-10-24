// Sandbox containers of this home that no project owns, or that were left
// stopped, and their removal. From part of the dsh Host's runtime.js (#428).
import { LayerFSError } from "@layerfs/agent";

/** This home's Sandbox containers (`<prefix><project>-<n>`, label layerfs.role=sandbox), running or not. */
export async function containers(runtime) {
  const { settings, platform } = runtime;
  const out = await platform.docker(settings, ["ps", "-a", "--no-trunc", "--filter", "label=layerfs.role=sandbox", "--format", "{{.Names}}\t{{.State}}\t{{.Status}}\t{{.Label \"layerfs.project\"}}\t{{.Label \"layerfs.sandbox\"}}\t{{.Image}}"]);
  return out.split("\n").filter(Boolean).map((row) => {
    const [name, state, status, project, sandbox, image] = row.split("\t");
    return { name, state, running: state === "running", status, project, sandbox, image };
  }).filter((c) => c.name.startsWith(settings.containerPrefix));
}

/** Containers of no project of this home, or stopped: `[{ name, project, running, status }]`. */
export async function leftovers(runtime) {
  const known = new Set((await runtime.project.list()).map((p) => p.name));
  return (await containers(runtime)).filter((c) => !known.has(c.project) || !c.running).map(({ name, project, running, status }) => ({ name, project, running, status }));
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
      await runtime.platform.docker(runtime.settings, ["rm", name]);
      results.push({ name, ok: true });
    } catch (error) {
      results.push({ name, ok: false, message: error.stderr?.trim() || error.message });
    }
  }
  return { results };
}

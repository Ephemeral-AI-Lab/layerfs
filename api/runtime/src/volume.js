// runtime.volume.list() and runtime.volume.remove(names): Docker's volumes
// on this engine, with who owns each, and the removal of this home's
// orphans. A project's Store volume is labelled at Init (layerfs.role=store,
// layerfs.home, layerfs.project); an older one is known by the runtime's
// project record. Docker's own "reclaimable" counts every volume no
// container uses, which includes every idle project's Store, so it is not
// used here.
//   owner "this"    labelled with this home, or recorded by one of its projects
//         "other"   labelled with another home
//         "unknown" neither: made before labels, or by another tool
// `live` is true while this home has the project that owns the volume (an
// uncertain Init, which keeps a home entry and no record, counts as live).
// `size` is Docker's figure, rounded to about four significant digits.
// One `docker system df -v`, plus one `docker volume inspect` of the
// labelled volumes; listing changes nothing.
import { LayerFSError } from "@layerfs/agent";

const UNITS = { B: 1, kB: 1e3, KB: 1e3, MB: 1e6, GB: 1e9, TB: 1e12 };
/** "84.05MB" -> 84050000; anything else -> null. */
export const bytesOf = (text) => {
  const m = /^([0-9.]+)\s*([kKMGT]?B)$/.exec(String(text ?? "").trim());
  return m && UNITS[m[2]] ? Math.round(Number(m[1]) * UNITS[m[2]]) : null;
};

/** `[{ name, size, containers, owner, project, live, removable }]`, by name. */
export async function list(runtime) {
  const { settings, platform } = runtime;
  const df = JSON.parse(await platform.docker(settings, ["system", "df", "-v", "--format", "{{json .}}"], { timeout: 60_000 }));
  const volumes = df.Volumes ?? [];
  const labelled = volumes.filter((v) => v.Labels).map((v) => v.Name);
  const labels = new Map(labelled.length ? JSON.parse(await platform.docker(settings, ["volume", "inspect", ...labelled], { timeout: 30_000 })).map((v) => [v.Name, v.Labels ?? {}]) : []);
  const projects = await runtime.project.list();
  const recorded = new Map(projects.filter((p) => p.volume).map((p) => [p.volume, p.name]));
  const named = new Map(projects.map((p) => [p.name, p]));
  return volumes.map((v) => {
    const l = labels.get(v.Name) ?? {};
    const store = l["layerfs.role"] === "store";
    let owner = "unknown", project = null;
    if (recorded.has(v.Name)) [owner, project] = ["this", recorded.get(v.Name)];
    else if (store && l["layerfs.home"] === settings.home) [owner, project] = ["this", l["layerfs.project"] ?? null];
    else if (store && l["layerfs.home"]) [owner, project] = ["other", l["layerfs.project"] ?? null];
    const live = owner === "this" && (recorded.has(v.Name) || (named.has(project) && !named.get(project).volume));
    const containers = Number(v.Links) || 0;
    return { name: v.Name, size: bytesOf(v.Size), containers, owner, project, live, removable: owner === "this" && !live && containers === 0 };
  }).sort((a, b) => a.name.localeCompare(b.name));
}

/** One `docker volume rm` per named removable volume; anything else is refused by name. `{ results: [{ name, ok, message? }] }` */
export async function remove(runtime, names) {
  if (!Array.isArray(names) || !names.length || names.some((n) => typeof n !== "string")) throw new LayerFSError("InvalidInput", "names lists the volumes to remove", null);
  const found = new Map((await list(runtime)).map((v) => [v.name, v]));
  const results = [];
  for (const name of names) {
    const v = found.get(name);
    const why = !v ? "no such volume"
      : v.owner === "other" ? "it belongs to another LayerFS home"
      : v.owner === "unknown" ? "it is not labelled or recorded as this home's; remove it by hand if it is unused"
      : v.live ? `project ${v.project} still uses it; remove the project instead`
      : v.containers ? `${v.containers} container(s) use it`
      : null;
    if (why) { results.push({ name, ok: false, message: why }); continue; }
    try {
      await runtime.platform.docker(runtime.settings, ["volume", "rm", name], { timeout: 30_000 });
      results.push({ name, ok: true });
    } catch (error) {
      results.push({ name, ok: false, message: error.stderr?.trim() || error.message });
    }
  }
  return { results };
}

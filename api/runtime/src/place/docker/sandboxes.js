// This home's Sandbox containers on its Docker engine (label
// layerfs.role=sandbox, named with the home's containerPrefix), as
// runtime.sandbox.list, list_leftovers, clean_leftovers, sweep and
// project.delete read and remove them. Moved from sandbox/leftovers.js,
// sandbox/list.js and project/delete.js (#527 step 10a).

/** Container states that hold a volume but run nothing (Docker's; a namespace Sandbox's record has none). */
export const STOPPED = new Set(["created", "exited", "dead"]);

const docker = (runtime, args, options = {}) => runtime.platform.docker.run(runtime.settings, args, options);

/** This home's Sandbox containers (`<prefix><project>-<n>`, label layerfs.role=sandbox), running or not:
 * `[{ name, state, running, status, project, sandbox, image, store, key }]`. One `docker ps`. */
export async function containers(runtime) {
  const out = await docker(runtime, ["ps", "-a", "--no-trunc", "--filter", "label=layerfs.role=sandbox", "--format", "{{.Names}}\t{{.State}}\t{{.Status}}\t{{.Label \"layerfs.project\"}}\t{{.Label \"layerfs.sandbox\"}}\t{{.Image}}\t{{.Label \"layerfs.store\"}}"]);
  return out.split("\n").filter(Boolean).map((row) => {
    const [name, state, status, project, sandbox, image, store] = row.split("\t");
    // a Sandbox of a shared Store's project (#506) is `<prefix><store>.<project>-<n>`
    return { name, state, running: state === "running", status, project, sandbox, image, store: store || null, key: store ? `${store}/${project}` : project };
  }).filter((c) => c.name.startsWith(runtime.settings.containerPrefix));
}

/** A container gone between the listing and the inspection is left out: inspect still prints the others. */
async function inspect(runtime, ids) {
  try {
    return JSON.parse(await docker(runtime, ["inspect", "--size", ...ids], { timeout: 30_000 }));
  } catch (error) {
    const out = String(error?.stdout ?? "").trim();
    if (out.startsWith("[")) return JSON.parse(out);
    throw error;
  }
}

/** `[{ name, project, store, sandbox, state, running, startedAt, finishedAt, image, ports, scratch }]`
 * of this home's Sandbox containers, by name. One `docker ps` and one `docker inspect --size`. */
export async function details(runtime) {
  const ids = (await docker(runtime, ["ps", "-a", "-q", "--no-trunc", "--filter", "label=layerfs.role=sandbox"])).split("\n").filter(Boolean);
  if (!ids.length) return [];
  return (await inspect(runtime, ids)).map((c) => {
    const labels = c.Config?.Labels ?? {};
    const running = c.State?.Running === true;
    const finished = c.State?.FinishedAt;
    return {
      name: String(c.Name ?? "").replace(/^\//, ""),
      project: labels["layerfs.project"] ?? null,
      store: labels["layerfs.store"] ?? null,
      sandbox: labels["layerfs.sandbox"] ?? null,
      state: c.State?.Status ?? "unknown",
      running,
      startedAt: c.State?.StartedAt ?? null,
      finishedAt: running || !finished || finished.startsWith("0001-") ? null : finished,
      image: c.Config?.Image ?? null,
      ports: Object.entries(c.NetworkSettings?.Ports ?? {}).flatMap(([port, bound]) => (bound ?? []).map((b) => ({ container: port, host: b.HostIp, port: Number(b.HostPort) }))),
      scratch: typeof c.SizeRw === "number" ? c.SizeRw : null,
    };
  }).filter((c) => c.name.startsWith(runtime.settings.containerPrefix)).sort((a, b) => a.name.localeCompare(b.name));
}

/** One `docker rm` of the named containers; the running ones too only when `force`. */
export async function remove(runtime, names, { force = false, timeout } = {}) {
  await docker(runtime, ["rm", ...(force ? ["-f"] : []), ...names], timeout ? { timeout } : {});
}

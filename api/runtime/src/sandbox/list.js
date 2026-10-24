// runtime.sandbox.list(): this home's Sandbox containers, running or not,
// with what each holds outside its Workspaces (#423). `scratch` is the
// container's own writable layer in bytes: whatever commands wrote outside a
// Workspace (/tmp, which is also HOME, and the npm, node and cargo caches
// there). It goes only when the container is deleted (sandbox.close, an idle
// close followed by a reclaim, or runtime.sandbox.sweep). One `docker ps`
// and one `docker inspect --size`; it changes nothing.

/** A container gone between the listing and the inspection is left out: inspect still prints the others. */
async function inspect(runtime, ids) {
  const { settings, platform } = runtime;
  try {
    return JSON.parse(await platform.docker(settings, ["inspect", "--size", ...ids], { timeout: 30_000 }));
  } catch (error) {
    const out = String(error?.stdout ?? "").trim();
    if (out.startsWith("[")) return JSON.parse(out);
    throw error;
  }
}

/** `[{ name, project, sandbox, state, running, startedAt, finishedAt, image, ports, scratch, owned }]`, by name. */
export async function list(runtime) {
  const { settings, platform } = runtime;
  const ids = (await platform.docker(settings, ["ps", "-a", "-q", "--no-trunc", "--filter", "label=layerfs.role=sandbox"])).split("\n").filter(Boolean);
  if (!ids.length) return [];
  const known = new Set((await runtime.project.list()).map((p) => p.name));
  return (await inspect(runtime, ids)).map((c) => {
    const labels = c.Config?.Labels ?? {};
    const project = labels["layerfs.project"] ?? null;
    const running = c.State?.Running === true;
    const finished = c.State?.FinishedAt;
    return {
      name: String(c.Name ?? "").replace(/^\//, ""),
      project,
      sandbox: labels["layerfs.sandbox"] ?? null,
      state: c.State?.Status ?? "unknown",
      running,
      startedAt: c.State?.StartedAt ?? null,
      finishedAt: running || !finished || finished.startsWith("0001-") ? null : finished,
      image: c.Config?.Image ?? null,
      ports: Object.entries(c.NetworkSettings?.Ports ?? {}).flatMap(([port, bound]) => (bound ?? []).map((b) => ({ container: port, host: b.HostIp, port: Number(b.HostPort) }))),
      scratch: typeof c.SizeRw === "number" ? c.SizeRw : null,
      owned: project !== null && known.has(project),
    };
  }).filter((c) => c.name.startsWith(settings.containerPrefix)).sort((a, b) => a.name.localeCompare(b.name));
}

// The engine's volumes, as runtime.volume.list reads them and
// runtime.volume.remove removes them. Moved from volume.js (#527 step 10a).

const docker = (runtime, args, options = {}) => runtime.platform.docker.run(runtime.settings, args, options);

/** Every volume on the engine: `[{ name, size, links, labels }]`, `size` as Docker prints it.
 * One `docker system df -v`, plus one `docker volume inspect` of the labelled volumes. */
export async function volumes(runtime) {
  const df = JSON.parse(await docker(runtime, ["system", "df", "-v", "--format", "{{json .}}"], { timeout: 60_000 }));
  const found = df.Volumes ?? [];
  const labelled = found.filter((v) => v.Labels).map((v) => v.Name);
  const labels = new Map(labelled.length ? JSON.parse(await docker(runtime, ["volume", "inspect", ...labelled], { timeout: 30_000 })).map((v) => [v.Name, v.Labels ?? {}]) : []);
  return found.map((v) => ({ name: v.Name, size: v.Size, links: v.Links, labels: labels.get(v.Name) ?? {} }));
}

/** One `docker volume rm`. */
export async function remove(runtime, name) {
  await docker(runtime, ["volume", "rm", name], { timeout: 30_000 });
}

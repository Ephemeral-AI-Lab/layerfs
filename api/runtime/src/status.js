// runtime.status(): Docker reachability, the worker's state (null on Linux),
// free disk and missing settings. From part of the dsh Host's runtime.js
// (#428). It starts nothing. A `vm:<provider>` home has no host Docker in its
// data path (#515 §3): its Stores and Sandboxes are inside the VM, so `docker`
// says so instead of reporting the host engine down, and `vm` carries the VM's
// own state.
import { statfsSync } from "node:fs";
import { missing } from "./settings.js";

export async function status(runtime) {
  const { settings, platform } = runtime;
  let docker;
  if (platform.hostDocker === false) {
    docker = { level: "not-used", detail: `this home is placed in ${platform.name}; its Stores and Sandboxes are inside each project's VM` };
  } else {
    try { docker = { level: "ok", version: await platform.docker(settings, ["version", "--format", "{{.Server.Version}}"]) }; } catch (error) {
      docker = { level: "down", error: error.stderr?.trim() || error.message };
    }
  }
  // a `vm:` placement has one VM per project, so it is asked about each project
  const projects = await runtime.project.list().then((all) => all.map((project) => project.name), () => []);
  const reported = await platform.status(settings, projects).catch((error) => ({ worker: { state: "unknown", error: error.message } }));
  let home = null;
  try { const s = statfsSync(settings.home); home = s.bavail * s.bsize; } catch { /* unreadable */ }
  return {
    platform: settings.platform,
    placement: settings.placement.name,
    missing: missing(settings),
    docker,
    worker: reported.worker ?? null,
    vm: reported.vm ?? null,
    disk: { home, volumes: docker.level === "not-used" || docker.level === "ok" ? await platform.free(settings) : null },
  };
}

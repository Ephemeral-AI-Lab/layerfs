// runtime.status(): Docker reachability, the worker's state (null on Linux),
// free disk and missing settings. From part of the dsh Host's runtime.js
// (#428). It starts nothing.
import { statfsSync } from "node:fs";
import { missing } from "./settings.js";

export async function status(runtime) {
  const { settings, platform } = runtime;
  let docker;
  try { docker = { level: "ok", version: await platform.docker(settings, ["version", "--format", "{{.Server.Version}}"]) }; } catch (error) {
    docker = { level: "down", error: error.stderr?.trim() || error.message };
  }
  const { worker } = await platform.status(settings).catch((error) => ({ worker: { state: "unknown", error: error.message } }));
  let home = null;
  try { const s = statfsSync(settings.home); home = s.bavail * s.bsize; } catch { /* unreadable */ }
  return {
    platform: platform.name,
    missing: missing(settings),
    docker,
    worker,
    disk: { home, volumes: docker.level === "ok" ? await platform.free(settings) : null },
  };
}

// The `local` placement (#527 §6, D4): Docker on this host (linux.js), in
// Docker Desktop's VM (macos.js) or in a WSL2 distribution (windows/). Each
// of those platforms carries `platform.docker`, the one Docker group, built
// here from that platform's own CLI call `run`:
//   run(settings, args, options)  one `docker` call: its stdout, trimmed
//   version(runtime)              the engine's version (runtime.status)
//   sandboxes                     this home's Sandbox containers (sandboxes.js)
//   stores                        Store volumes and their hooks (stores.js)
//   disk                          the engine's volumes and their removal (disk.js)
// A `vm:<provider>` home has no group: host Docker holds none of its Stores
// or Sandboxes, so a caller that finds none refuses (shared.js `dockerOf`).
// Every helper reaches Docker through `runtime.platform.docker.run`, so a
// test replaces that one call.
import { run as execute } from "../shared.js";
import * as disk from "./disk.js";
import * as sandboxes from "./sandboxes.js";
import * as stores from "./stores.js";

/** Docker's volumes directory where the engine runs: Docker Desktop's VM, WSL2 and, by default (settings.volumes), Linux. */
export const VOLUMES = "/var/lib/docker/volumes";

/** The Store file inside the Docker volume `volume` under the volumes directory `volumes`. */
export const volumeStore = (volumes, volume) => `${volumes}/${volume}/_data/global/store.sqlite`;

/** One Docker CLI call on the configured engine socket (Linux, macOS): its stdout, trimmed. */
export const socketRun = (settings, args, options = {}) =>
  execute("docker", ["-H", `unix://${settings.docker}`, ...args], { timeout: 5000, ...options }).then((r) => r.stdout.trim());

/** The Docker group of a platform whose Docker CLI call is `run`. */
export function dockerGroup(run) {
  return {
    run,
    version: (runtime) => runtime.platform.docker.run(runtime.settings, ["version", "--format", "{{.Server.Version}}"]),
    sandboxes,
    stores,
    disk,
  };
}

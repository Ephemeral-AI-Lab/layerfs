// Linux: Store calls run in the caller's process, with the real addon, so
// there is no worker (AGENTS.md). Init installs the Store straight into the
// project volume's directory under Docker's volumes directory, which Docker
// keeps root-only: `init` checks this process can write there first.
import { accessSync, constants, existsSync, rmSync, statSync, statfsSync } from "node:fs";
import { join } from "node:path";
import { docker, homeNames, refuse } from "../shared.js";

const NO_WORKER = "Linux runs Store calls in-process; there is no worker";

export default {
  name: "linux",
  endpoint: () => null,
  async status() { return { worker: null }; },
  async start() { throw refuse("InvalidInput", NO_WORKER); },
  async restart() { throw refuse("InvalidInput", NO_WORKER); },
  /** Refuses an Init this process could not finish: Docker's volumes or the home not writable by it. */
  async init(settings) {
    const uid = process.getuid?.();
    try {
      accessSync(settings.volumes, constants.R_OK | constants.W_OK | constants.X_OK);
    } catch (error) {
      if (error.code !== "ENOENT") throw refuse("Unsupported", `this process (uid ${uid}) cannot write Docker's volumes directory ${settings.volumes} (${error.code}), where Init installs the Store; run it as a user that can, such as root`);
    }
    const owner = existsSync(settings.home) ? statSync(settings.home).uid : uid;
    if (uid !== undefined && owner !== uid) throw refuse("Unsupported", `the LayerFS home ${settings.home} belongs to uid ${owner}, not this process's uid ${uid}`);
  },
  path: (settings, hostPath) => hostPath,
  docker,
  storePath: (settings, volume) => `${settings.volumes}/${volume}/_data/global/store.sqlite`,
  homeNames: (settings) => homeNames(settings.home),
  async removeHome(settings, name) { rmSync(join(settings.home, name), { recursive: true, force: true }); },
  async releaseStore() {},
  async free(settings) {
    try { const s = statfsSync(settings.volumes); return s.bavail * s.bsize; } catch { return null; }
  },
};

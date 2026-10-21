// The ported SDK's controls for an adapter's own process: the Store container
// (start it, name it, where it keeps a project's Store and its connection
// file), the hook a call that finds it down tells, and the frames its worker
// speaks. The ported SDK itself is `@layerfs/port/sdk` (and `/operator`).
import { DEFAULT_DOCKER, ensureBox, storeName } from "./box.js";
import { onUnreachable } from "./binding.js";
import { setting } from "./env.js";

export { buildOf, DAEMON_HOST, ensureBox, storeFile, storeName, storePathOf, VOLUMES } from "./box.js";
export { onUnreachable, UNREACHABLE } from "./binding.js";
export { frame, readFrames } from "./frames.mjs";

const DEFAULT_PREFIX = "layerfs-";
const REQUIRED = ["HOME", "BUNDLE", "IMAGE"];
let serving = null;

/**
 * Makes this process's ported SDK calls go to the Store container of the
 * deployment in its LAYERFS_* settings (else DSH_LAYERFS_*), as the dsh Host
 * does for its own: starts the container unless it runs, and starts it again
 * when a call finds it down (one start at a time; the call keeps its own
 * answer, and `report` hears a start that failed). The first call of a
 * process sets this up; later ones wait for the same container.
 * @param {(error: Error) => void} report
 */
export function useStoreContainer(report) {
  if (!serving) {
    const missing = REQUIRED.filter((name) => !setting(name));
    if (missing.length) throw new Error(`the ported SDK needs ${missing.map((name) => `LAYERFS_${name}`).join(", ")}`);
    const prefix = setting("CONTAINER_PREFIX") || DEFAULT_PREFIX;
    serving = { docker: setting("DOCKER") || DEFAULT_DOCKER, home: setting("HOME"), bundle: setting("BUNDLE"), image: setting("IMAGE"), prefix, box: storeName(prefix) };
    // the ported binding finds its Store container by this name
    process.env.LAYERFS_STORE_CONTAINER = serving.box;
    onUnreachable(() => ensureBox(serving).catch(report));
  }
  return ensureBox(serving);
}

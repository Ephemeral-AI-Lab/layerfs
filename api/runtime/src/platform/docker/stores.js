// Store volumes on the `local` placement: a Store is one labelled Docker
// volume, its holders are containers, and removing the Store removes the
// volume. These are the four Store hooks (`hooks`) Init, Store creation and
// Store removal use, which a `vm:` placement implements over the project's
// VM instead (vm/index.js). Each is keyed by Store, `{ name, volume, legacy }`:
// `legacy` marks the Store a project's own Init makes, named after it (#506);
// any other is a Store of the home. Each uses that platform's own Docker CLI
// call (Windows goes through WSL).
import { refuse } from "../shared.js";

const docker = (runtime, args, options = {}) => runtime.platform.docker.run(runtime.settings, args, options);

/**
 * Makes the Store's labelled volume and answers its name: a project's own
 * Store gets the volume its Init named (`store.volume`, labelled
 * layerfs.project), any other `<prefix>store-<name>` (labelled
 * layerfs.store). A volume that exists is refused, not reused (docker
 * volume create would reuse it). Docker sets labels only at create, and they
 * tell runtime.volume.list whose the volume is.
 */
export async function createStore(runtime, store) {
  const { settings } = runtime;
  const volume = store.legacy ? store.volume : `${settings.containerPrefix}store-${store.name}`;
  const exists = await docker(runtime, ["volume", "inspect", "--format", "{{.Name}}", volume]).then(() => true, () => false);
  if (exists) throw refuse("Exists", `the volume ${volume} exists; remove it, or name the ${store.legacy ? "project" : "Store"} differently`);
  const owner = store.legacy ? `layerfs.project=${store.name}` : `layerfs.store=${store.name}`;
  await docker(runtime, ["volume", "create", "--label", "layerfs.role=store", "--label", `layerfs.home=${settings.home}`, "--label", owner, volume], { timeout: 30_000 });
  return volume;
}

/** The containers holding the Store's volume now: [{ id, state }], running or not. */
export async function storeHolders(runtime, store) {
  const rows = await docker(runtime, ["ps", "-a", "--filter", `volume=${store.volume}`, "--format", "{{.ID}} {{.State}}"]);
  return rows.split("\n").filter(Boolean).map((line) => { const [id, state] = line.split(" "); return { id, state }; });
}

/** Removes the volume's holders: the running ones only when `force`. */
export async function releaseStoreHolders(runtime, store, holders, { force = false } = {}) {
  if (!holders.length) return;
  await docker(runtime, ["rm", ...(force ? ["-f"] : []), ...holders.map((holder) => holder.id)], { timeout: 30_000 });
}

/** The Store's volume itself, once nothing holds it. */
export async function removeStore(runtime, store) {
  await docker(runtime, ["volume", "rm", store.volume], { timeout: 30_000 });
}

/** The platform-level Store hooks, as every Docker placement exposes them. */
export const hooks = { createStore, storeHolders, releaseStoreHolders, removeStore };

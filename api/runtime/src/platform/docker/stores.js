// Store volumes on the `local` placement: a Store is one labelled Docker
// volume, its holders are containers, and removing the Store removes the
// volume. The four Store hooks Init and Store removal use (`hooks`), which a
// `vm:` placement implements over the project's VM instead (vm/index.js),
// and the volume a Store-level create makes. Each hook takes the runtime and
// the owner `{ name, volume }`, and uses that platform's own Docker CLI call
// (Windows goes through WSL). Moved from platform/shared.js and
// store/calls.js (#527 step 10a).
import { refuse } from "../shared.js";

const docker = (runtime, args, options = {}) => runtime.platform.docker.run(runtime.settings, args, options);

/** Init makes the project's Store volume before installing into it. */
export async function makeStore(runtime, project) {
  const { settings } = runtime;
  await docker(runtime, ["volume", "create", "--label", "layerfs.role=store", "--label", `layerfs.home=${settings.home}`, "--label", `layerfs.project=${project.name}`, project.volume], { timeout: 30_000 });
}

/** The containers holding the volume now: [{ id, state }], running or not. */
export async function storeHolders(runtime, project) {
  const rows = await docker(runtime, ["ps", "-a", "--filter", `volume=${project.volume}`, "--format", "{{.ID}} {{.State}}"]);
  return rows.split("\n").filter(Boolean).map((line) => { const [id, state] = line.split(" "); return { id, state }; });
}

/** Removes the volume's holders: the running ones only when `force`. */
export async function releaseStoreHolders(runtime, project, holders, { force = false } = {}) {
  if (!holders.length) return;
  await docker(runtime, ["rm", ...(force ? ["-f"] : []), ...holders.map((holder) => holder.id)], { timeout: 30_000 });
}

/** The volume itself, once nothing holds it. */
export async function removeStore(runtime, project) {
  await docker(runtime, ["volume", "rm", project.volume], { timeout: 30_000 });
}

/** The platform-level Store hooks, as every Docker placement exposes them. */
export const hooks = { makeStore, storeHolders, releaseStoreHolders, removeStore };

/** Makes a Store's labelled volume, refusing one that exists (docker volume create would reuse it). */
export async function makeVolume(runtime, name) {
  const { settings } = runtime;
  const volume = `${settings.containerPrefix}store-${name}`;
  const exists = await docker(runtime, ["volume", "inspect", "--format", "{{.Name}}", volume]).then(() => true, () => false);
  if (exists) throw refuse("Exists", `the volume ${volume} exists; remove it, or name the Store differently`);
  await docker(runtime, ["volume", "create", "--label", "layerfs.role=store", "--label", `layerfs.home=${settings.home}`, "--label", `layerfs.store=${name}`, volume], { timeout: 30_000 });
  return volume;
}

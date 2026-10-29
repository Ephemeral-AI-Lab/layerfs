// The home's Stores (#506): `<home>/stores.json`, one entry per Store,
//   { name, kind: sqlite | postgresql, volume?, connection?, schema?,
//     synchronousCommit?, remote, legacy?, created,
//     projects: [{ name, source?, created? }], copies: [transfer] }
// where a transfer is the Store's last push or pull with one remote,
// { remote, direction, at, sha256, bytes }. A Store made by its project's own
// Init (before #506, or start_init with no `store`) is listed too, from that
// project's record, as a Store of the project's name holding it (`legacy`);
// it gets an entry here only once another project is added to it. The file
// holds no secret: a PostgreSQL Store names its connection, whose URL the
// caller passes per call.
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { LayerFSError } from "@layerfs/agent";
import { NAME, legacyProjects } from "../project/list.js";
import { callHome } from "../settings.js";

const file = (settings) => join(settings.home, "stores.json");
export const refuse = (code, message) => new LayerFSError(code, message, null);

/** The registered entries, by name. */
export function entries(settings) {
  if (!existsSync(file(settings))) return [];
  const doc = JSON.parse(readFileSync(file(settings), "utf8"));
  return Array.isArray(doc?.stores) ? doc.stores : [];
}

function write(settings, stores) {
  const path = file(settings);
  writeFileSync(`${path}.new`, JSON.stringify({ version: 1, stores: [...stores].sort((a, b) => a.name.localeCompare(b.name)) }, null, 2), { mode: 0o600 });
  renameSync(`${path}.new`, path);
}

/** Adds or replaces the entry `store.name`. */
export function put(settings, store) {
  write(settings, [...entries(settings).filter((s) => s.name !== store.name), store]);
  return store;
}

/** Drops the entry `name`, if any. */
export function drop(settings, name) {
  write(settings, entries(settings).filter((s) => s.name !== name));
}

/** Changes the entry `name` with `change(entry)`. */
export function update(settings, name, change) {
  const found = entries(settings).find((s) => s.name === name);
  if (!found) throw refuse("NoSuchStore", `no LayerFS Store named ${name}`);
  return put(settings, change({ ...found }));
}

/** Records a completed transfer: one per remote, the latest; the Store is bound to that remote. */
export const withCopy = (store, transfer) => ({
  ...store,
  remote: transfer.remote,
  copies: [...(store.copies ?? []).filter((c) => c.remote !== transfer.remote), transfer],
});

/** Every Store of the home: registered ones, and each legacy project's own. */
export async function all(runtime) {
  const { settings } = runtime;
  const registered = new Map(entries(settings).map((s) => [s.name, s]));
  for (const project of await legacyProjects(runtime)) {
    const own = { name: project.name, source: project.source, created: project.created };
    const found = registered.get(project.name);
    if (found?.legacy) registered.set(project.name, { ...found, volume: found.volume ?? project.volume, projects: [own, ...found.projects.filter((p) => p.name !== project.name)] });
    else if (!found) registered.set(project.name, { name: project.name, kind: "sqlite", volume: project.volume, remote: null, legacy: true, created: project.created, projects: [own], copies: [] });
  }
  return [...registered.values()].sort((a, b) => a.name.localeCompare(b.name));
}

/** The Store `name`, else NoSuchStore. */
export async function named(runtime, name) {
  if (typeof name !== "string" || !NAME.test(name)) throw refuse("InvalidInput", "a Store name is 1-63 lowercase letters, digits, _ . - and starts and ends alphanumeric");
  const found = (await all(runtime)).find((s) => s.name === name);
  if (!found) throw refuse("NoSuchStore", `no LayerFS Store named ${name} in ${runtime.settings.home}`);
  return found;
}

/** The call fields that name a Store to the binding: home, store, storePath. */
export function storeFields(runtime, store) {
  const { settings, platform } = runtime;
  const home = callHome(settings);
  if (store.kind === "postgresql") return { home, store: store.name, storePath: `postgresql:${store.name}` };
  if (!store.volume) throw refuse("InvalidInput", `the runtime has no record of Store ${store.name}'s volume (made outside this runtime)`);
  return { home, store: store.name, storePath: platform.storePath(settings, store.volume) };
}

// The home's Stores (#506): `<home>/stores.json`, one entry per Store,
//   { name, kind: sqlite | postgresql, volume?, connection?, schema?,
//     synchronousCommit?, link?, legacy?, created,
//     projects: [{ name, source?, created? }] }
// A SQLite Store links to at most one remote file (#509, version 2):
//   link: { remote, file, linked, last } | null
// where `remote` names a connection, `file` the remote file (the object
// `<prefix>/<file>/store.sqlite`), `linked` when the link was made (ms) and
// `last` its latest transfer, { direction: push | pull, at (ms), bytes,
// sha256, heads } (heads, the Store's history heads digest, may be null).
// One to one per home: a (remote, file) pair is linked by at most one Store,
// checked under the registry lock by every call that makes a link. The
// binding keeps the link's native twin in HOME/remote-state/<store>.
// Version 1 entries (`remote`, `copies`: one per remote) are read with no
// link; the first version 2 write keeps the version 1 file beside it as
// stores.v1.json. A PostgreSQL Store is never linked; one made by an import
// records `importedFrom: { store, at, bytes }` (#506 E6). A Store made by its
// project's own Init (before #506, or start_init with no `store`) is listed
// too, from that project's record, as a Store of the project's name holding
// it (`legacy`); it gets an entry here only once another project is added to
// it or it is linked. The file holds no secret: a PostgreSQL Store names its
// connection, whose URL the caller passes per call.
import { closeSync, copyFileSync, existsSync, openSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { LayerFSError } from "@layerfs/agent";
import { NAME, legacyProjects } from "../project/list.js";
import { callHome } from "../settings.js";

const file = (settings) => join(settings.home, "stores.json");
export const refuse = (code, message) => new LayerFSError(code, message, null);
/** A remote file's name: the object `<prefix>/<file>/store.sqlite`. */
export const FILE = /^[A-Za-z0-9_-][A-Za-z0-9._-]{0,127}$/;

/** A version 1 entry without its copies: no link (#509 settles them apart). */
const fromV1 = ({ remote, copies, ...entry }) => (entry.kind === "sqlite" ? { ...entry, link: null } : entry);

function read(settings) {
  if (!existsSync(file(settings))) return { version: 2, stores: [] };
  const doc = JSON.parse(readFileSync(file(settings), "utf8"));
  const stores = Array.isArray(doc?.stores) ? doc.stores : [];
  return doc?.version === 2 ? { version: 2, stores } : { version: 1, stores: stores.map(fromV1) };
}

/** The registered entries, by name. */
export function entries(settings) {
  return read(settings).stores;
}

/** Runs `change()` (synchronous) holding the home's registry lock: one
 * process at a time reads, checks and writes stores.json. A lock held by
 * another process is refused Busy, never waited for. */
function locked(settings, change) {
  const lock = `${file(settings)}.lock`;
  let fd;
  try {
    fd = openSync(lock, "wx", 0o600);
  } catch (error) {
    if (error?.code === "EEXIST") throw refuse("Busy", `another process is changing ${file(settings)} (its lock ${lock} exists; if no process is, remove it)`);
    throw error;
  }
  try {
    return change();
  } finally {
    closeSync(fd);
    rmSync(lock, { force: true });
  }
}

function write(settings, stores) {
  const path = file(settings);
  if (existsSync(path) && read(settings).version === 1 && !existsSync(join(settings.home, "stores.v1.json"))) copyFileSync(path, join(settings.home, "stores.v1.json"));
  writeFileSync(`${path}.new`, JSON.stringify({ version: 2, stores: [...stores].sort((a, b) => a.name.localeCompare(b.name)) }, null, 2), { mode: 0o600 });
  renameSync(`${path}.new`, path);
}

/** Adds or replaces the entry `store.name`. */
export function put(settings, store) {
  locked(settings, () => write(settings, [...entries(settings).filter((s) => s.name !== store.name), store]));
  return store;
}

/** Drops the entry `name`, if any. */
export function drop(settings, name) {
  locked(settings, () => write(settings, entries(settings).filter((s) => s.name !== name)));
}

/** Changes the entry `name` with `change(entry)`, which may refuse. */
export function update(settings, name, change) {
  return locked(settings, () => {
    const all = entries(settings);
    const found = all.find((s) => s.name === name);
    if (!found) throw refuse("NoSuchStore", `no LayerFS Store named ${name}`);
    const changed = change({ ...found });
    write(settings, [...all.filter((s) => s.name !== name), changed]);
    return changed;
  });
}

/** The Store of the home linked to `(remote, file)`, else null. */
export function linkedBy(settings, remote, file) {
  return entries(settings).find((s) => s.link?.remote === remote && s.link?.file === file)?.name ?? null;
}

/** `(remote, file)` pairs a running job of this process is about to link or create, by key. */
const claims = new Map();
const claimKey = (settings, remote, file) => `${settings.home}\0${remote}\0${file}`;

/** Reserves `(remote, file)` for `store` until `release()`: refused `Linked`
 * (naming the Store) when a Store of the home links to it, `Exists` when
 * another job of this process is creating or linking it. */
export function claim(settings, remote, file, store, linkedCode = "Exists") {
  return locked(settings, () => {
    const holder = linkedBy(settings, remote, file);
    if (holder) throw refuse(linkedCode, `${remote} / ${file} is linked to the Store ${holder}`);
    const key = claimKey(settings, remote, file);
    if (claims.has(key)) throw refuse("Exists", `${remote} / ${file} is being made by a running job for ${claims.get(key)}`);
    claims.set(key, store);
    return () => claims.delete(key);
  });
}

/** The Store with its link `(transfer.remote, transfer.file)` and that
 * transfer as its last; a link made now when it is new. */
export function withLink(store, transfer) {
  const { remote, file, direction, at, bytes, sha256, heads = null } = transfer;
  const same = store.link?.remote === remote && store.link?.file === file;
  return { ...store, link: { remote, file, linked: same ? store.link.linked : Date.now(), last: { direction, at, bytes, sha256, heads } } };
}

/** Links the entry `name` by `transfer` under the lock, refusing `Exists`
 * when another Store of the home links to that file meanwhile. */
export function link(settings, name, transfer, change = (s) => s) {
  return update(settings, name, (s) => {
    const holder = linkedBy(settings, transfer.remote, transfer.file);
    if (holder && holder !== name) throw refuse("Exists", `${transfer.remote} / ${transfer.file} is linked to the Store ${holder}`);
    return withLink(change(s), transfer);
  });
}

/** Every Store of the home: registered ones, and each legacy project's own. */
export async function all(runtime) {
  const { settings } = runtime;
  const registered = new Map(entries(settings).map((s) => [s.name, s]));
  for (const project of await legacyProjects(runtime)) {
    const own = { name: project.name, source: project.source, created: project.created };
    const found = registered.get(project.name);
    if (found?.legacy) registered.set(project.name, { ...found, volume: found.volume ?? project.volume, projects: [own, ...found.projects.filter((p) => p.name !== project.name)] });
    else if (!found) registered.set(project.name, { name: project.name, kind: "sqlite", volume: project.volume, link: null, legacy: true, created: project.created, projects: [own] });
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

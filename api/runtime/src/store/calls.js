// runtime.store.list/create/remove/attach_postgresql/check/unlink,
// runtime.remote.* and runtime.postgresql.status (#506, doc section 6).
// store.list adds each local SQLite Store's sizes, open state and history
// heads (one runtime.store.stat call, E1/E2); store.check is one HEAD of the
// Store's linked file (E3, #509), remote.list_stores one listing with the
// home's Store linked to each file (E9, #509), remote.providers the
// build's providers (E8) and postgresql.status one connection (E5/E7). A local SQLite Store is one Docker volume
// (`<prefix>store-<name>`, labelled layerfs.role=store, layerfs.home,
// layerfs.store) whose `global/store.sqlite` the binding creates; a
// PostgreSQL Store is a schema behind a named connection. Every call is one
// attempt; destructive calls name what they destroy (`confirm`).
import { invokeBackend } from "@layerfs/agent/internal";
import { deploymentEnv, withEnv } from "../clients.js";
import { NAME } from "../project/list.js";
import { forgetOwn } from "../project/delete.js";
import { checkVmUrl } from "../platform/vm/index.js";
import { callHome } from "../settings.js";
import { all, drop, entries, named, put, refuse, storeFields, update } from "./registry.js";
import { removeVolume } from "./volume.js";

const SCHEMA = /^[a-z_][a-z0-9_]{0,62}$/;
const SYNC = new Set(["off", "local", "remote_write", "on", "remote_apply"]);
const call = (op, args) => invokeBackend(op, args, {});
/** Runs a Store backend call through that Store's VM worker on a `vm:` home, else in the home's binding. */
export const storeCall = (runtime, store, op, args) => {
  const target = typeof store === "object" && store !== null ? { ...store, store: store.store ?? store.name } : { name: store, store };
  return runtime.platform.hostDocker === false
    ? withEnv(deploymentEnv(runtime.settings, runtime.platform, target), () => call(op, args))
    : call(op, args);
};

/** What store.list shows of one Store; `local` is its runtime.store.stat entry (#506 E1, E2). */
export const shown = (s, local = null) => ({
  name: s.name, kind: s.kind,
  ...(s.kind === "sqlite" ? { volume: s.volume ?? null } : { connection: s.connection, schema: s.schema, synchronousCommit: s.synchronousCommit }),
  link: s.kind === "sqlite" ? s.link ?? null : null, legacy: !!s.legacy, created: s.created ?? null,
  projects: (s.projects ?? []).map((p) => p.name),
  bytes: local?.bytes ?? null, open: local?.open ?? null, heads: local?.heads ?? null,
  importedFrom: s.importedFrom ?? null,
});

/** Each local SQLite Store's sizes, open state and heads, from the process that sees its volume
 * (this one on Linux, the worker on macOS and Windows). One call for all; a call that fails
 * (no worker yet, a volume this process cannot read) leaves them null. */
async function locals(runtime, stores) {
  const asked = stores.filter((s) => s.kind === "sqlite" && s.volume).map((s) => ({ store: s.name, storePath: storeFields(runtime, s).storePath }));
  if (!asked.length) return new Map();
  if (runtime.platform.hostDocker === false) {
    const rows = await Promise.all(asked.map((s) => storeCall(runtime, s.store, "runtime.store.stat", { home: callHome(runtime.settings), stores: [s] }).then((r) => r[0], () => null)));
    return new Map(rows.filter(Boolean).map((entry) => [entry.store, entry]));
  }
  try {
    const answer = await call("runtime.store.stat", { home: callHome(runtime.settings), stores: asked });
    return new Map(answer.map((entry) => [entry.store, entry]));
  } catch {
    return new Map();
  }
}

export async function list(runtime) {
  const stores = await all(runtime);
  const local = await locals(runtime, stores);
  return stores.map((s) => shown(s, local.get(s.name)));
}

/** runtime.store.check(store, { credentials }): one HEAD of the Store's linked file (#506 E3, #509):
 * `{ store, remote, file, present, bytes, sha256, heads, modified, checkedAt }`. Refused NotLinked. */
export async function check(runtime, name, options = {}) {
  const store = await named(runtime, name);
  if (!store.link) throw refuse("NotLinked", `Store ${name} is linked to no remote file`);
  const credentials = credentialsOf(options);
  if (runtime.platform.hostDocker === false) await runtime.platform.init(runtime.settings, { name, store: name, kind: store.kind });
  return storeCall(runtime, name, "runtime.store.check", { home: callHome(runtime.settings), store: name, remote: store.link.remote, file: store.link.file, ...credentials });
}

/** runtime.store.unlink(store, { confirm }): drops the Store's link and its
 * native record; the remote file stays, loadable. `confirm` is the Store's name. */
export async function unlink(runtime, name, options = {}) {
  if (options?.confirm !== name) throw refuse("InvalidInput", `confirm with the Store's name, ${name}`);
  const store = await named(runtime, name);
  if (!store.link) throw refuse("NotLinked", `Store ${name} is linked to no remote file`);
  if (runtime.platform.hostDocker === false) await runtime.platform.init(runtime.settings, { name, store: name, kind: store.kind });
  await storeCall(runtime, name, "runtime.store.unlink", { home: callHome(runtime.settings), store: name });
  update(runtime.settings, name, (s) => ({ ...s, link: null }));
  return { kind: "Unlinked", store: name, remote: store.link.remote, file: store.link.file };
}

/** runtime.remote.list_stores(remote, { credentials }): every Store file in the remote (#506 E9),
 * `[{ file, bytes, sha256, heads, modified, linkedBy }]`; `linkedBy` is the home's Store linked to it, or null. */
export async function listStores(runtime, remote, options = {}) {
  if (typeof remote !== "string" || !remote) throw refuse("InvalidInput", "remote names one of the home's remotes");
  const files = await call("runtime.remote.list_stores", { home: callHome(runtime.settings), remote, ...credentialsOf(options) });
  const linked = new Map(entries(runtime.settings).filter((s) => s.link?.remote === remote).map((s) => [s.link.file, s.name]));
  return files.map((f) => ({ ...f, linkedBy: linked.get(f.file) ?? null }));
}

/** runtime.remote.providers(): the providers this LayerFS build can add (#506 E8). */
export async function providers(runtime) {
  return call("runtime.remote.providers", { home: callHome(runtime.settings) });
}

/** runtime.postgresql.status({ url, schemas }): server version, reachability and latency, schema sizes (#506 E5, E7). */
export async function postgresqlStatus(runtime, given = {}) {
  const { url, schemas = [] } = given ?? {};
  if (typeof url !== "string" || !url) throw refuse("InvalidInput", "url is the PostgreSQL URL, passed per call and never stored");
  if (runtime.settings.placement.kind === "vm") checkVmUrl(url);
  if (!Array.isArray(schemas) || !schemas.every((s) => typeof s === "string" && SCHEMA.test(s))) throw refuse("InvalidInput", "schemas is a list of PostgreSQL schema names");
  return call("runtime.postgresql.status", { home: callHome(runtime.settings), url, schemas });
}

/** A call's per-call keys: `{ credentials? }`. */
export function credentialsOf(options) {
  const credentials = options?.credentials;
  if (credentials !== undefined && credentials !== null && (typeof credentials.accessKey !== "string" || typeof credentials.secretKey !== "string")) {
    throw refuse("InvalidInput", "credentials is { accessKey, secretKey }, used for this call only");
  }
  return credentials ? { credentials } : {};
}

/** Refuses a Store name in use, or not a name. */
export async function unused(runtime, name) {
  if (typeof name !== "string" || !NAME.test(name)) throw refuse("InvalidInput", "a Store name is 1-63 lowercase letters, digits, _ . - and starts and ends alphanumeric");
  if ((await all(runtime)).some((s) => s.name === name)) throw refuse("Exists", `a LayerFS Store named ${name} exists`);
}

/** Makes a Store's labelled volume, refusing one that exists (docker volume create would reuse it). */
export async function makeVolume(runtime, name) {
  const { settings, platform } = runtime;
  const volume = `${settings.containerPrefix}store-${name}`;
  if (platform.hostDocker === false) {
    const store = { name, store: name, volume };
    await platform.init(settings, store);
    await platform.makeStore(runtime, store);
    return volume;
  }
  const exists = await platform.docker(settings, ["volume", "inspect", "--format", "{{.Name}}", volume]).then(() => true, () => false);
  if (exists) throw refuse("Exists", `the volume ${volume} exists; remove it, or name the Store differently`);
  await platform.docker(settings, ["volume", "create", "--label", "layerfs.role=store", "--label", `layerfs.home=${settings.home}`, "--label", `layerfs.store=${name}`, volume], { timeout: 30_000 });
  return volume;
}

/** runtime.store.create(name): a new empty local SQLite Store in its own volume. */
export async function create(runtime, name) {
  const { settings, platform } = runtime;
  await unused(runtime, name);
  await platform.init(settings);
  const volume = await makeVolume(runtime, name);
  const store = { name, kind: "sqlite", volume, link: null, created: Date.now(), projects: [] };
  try {
    await storeCall(runtime, name, "runtime.store.create", storeFields(runtime, store));
  } catch (error) {
    // a refused or failed create removed its own files; an uncertain one keeps the volume
    if (error?.code !== "Unknown") await removeVolume(runtime, { name, volume }).catch(() => {});
    throw error;
  }
  return shown(put(settings, store));
}

/** runtime.store.remove(name, { confirm, stopSandboxes }): deletes a local Store's volume (every project in it), or forgets a PostgreSQL Store. */
export async function remove(runtime, name, options = {}) {
  const { settings, platform } = runtime;
  if (options?.confirm !== name) throw refuse("InvalidInput", `confirm with the Store's name, ${name}`);
  const store = await named(runtime, name);
  await runtime.clients.forgetStore(name);
  if (store.kind === "sqlite") {
    if (!store.volume) throw refuse("InvalidInput", `the runtime has no record of Store ${name}'s volume`);
    await removeVolume(runtime, { name, volume: store.volume }, options);
  } else if (platform.hostDocker === false) {
    await platform.releaseStore(settings, { name, store: name, kind: "postgresql" });
    await platform.removeStore(runtime, { name, store: name, kind: "postgresql" });
  }
  if (platform.hostDocker !== false) await call("runtime.store.forget", { home: callHome(settings), store: name });
  if (store.legacy) await forgetOwn(runtime, name);
  drop(settings, name);
  return { kind: "Removed", store: name, ...(store.volume ? { volume: store.volume } : {}) };
}

/** Checks a PostgreSQL target: `{ connection, schema, synchronousCommit, url }`. */
export function postgresqlTarget(given = {}, settings = undefined) {
  const { connection, schema, synchronousCommit = "off", url } = given;
  if (typeof connection !== "string" || !NAME.test(connection)) throw refuse("InvalidInput", "connection names the PostgreSQL connection (1-63 lowercase letters, digits, _ . -)");
  if (typeof schema !== "string" || !SCHEMA.test(schema)) throw refuse("InvalidInput", "schema is a PostgreSQL schema name, [a-z_][a-z0-9_]*, at most 63 bytes");
  if (!SYNC.has(synchronousCommit)) throw refuse("InvalidInput", `synchronousCommit is one of ${[...SYNC].join(", ")}`);
  if (typeof url !== "string" || !url) throw refuse("InvalidInput", "url is the PostgreSQL URL, passed per call and never stored");
  if (settings?.placement?.kind === "vm") checkVmUrl(url);
  return { connection, schema, synchronousCommit, url };
}

/** runtime.store.attach_postgresql({ name, connection, schema, synchronousCommit, url }):
 * registers a PostgreSQL Store after checking it opens: one this home
 * imported keeps its identity; any other LayerFS schema is read for the
 * binding it records. */
export async function attach_postgresql(runtime, given = {}) {
  const { name } = given ?? {};
  const target = postgresqlTarget(given, runtime.settings);
  const known = (await all(runtime)).find((s) => s.name === name);
  if (known && known.kind !== "postgresql") throw refuse("Exists", `Store ${name} is a SQLite Store`);
  const store = { name, store: name, kind: "postgresql", connection: target.connection, schema: target.schema, synchronousCommit: target.synchronousCommit, created: known?.created ?? Date.now() };
  if (runtime.platform.hostDocker === false) await runtime.platform.init(runtime.settings, store);
  let answer;
  try {
    answer = await storeCall(runtime, store, "runtime.store.attach_postgresql", { ...storeFields(runtime, store), postgresql: { url: target.url, schema: target.schema, synchronousCommit: target.synchronousCommit } });
  } catch (error) {
    if (!known && runtime.platform.hostDocker === false && error?.code !== "Unknown") {
      await runtime.platform.removeStore(runtime, store).catch(() => {});
    }
    throw error;
  }
  const { store: _, ...entry } = store;
  entry.projects = answer.projects.map((p) => ({ name: p.name }));
  return shown(put(runtime.settings, entry));
}

/** runtime.remote.list(): the home's remotes, with credential references and no secret. */
export async function remotes(runtime) {
  return call("runtime.remote.list", { home: callHome(runtime.settings) });
}

/** runtime.remote.add({ name, provider: "minio", endpoint, bucket, prefix?, region?, credentials?, partSize? } | { name, provider: "r2", account, jurisdiction?, bucket, prefix?, credentials?, partSize? }). */
export async function addRemote(runtime, remote) {
  if (remote === null || typeof remote !== "object") throw refuse("InvalidInput", "a remote is { name, provider, … }");
  if (typeof remote.credentials === "object" && remote.credentials !== null) throw refuse("InvalidInput", "credentials is a reference (env:LAYERFS_… or file:/path), never the keys; pass keys per call to publish, push, pull, send and load");
  return call("runtime.remote.add", { home: callHome(runtime.settings), remote });
}

/** runtime.remote.remove(name, { confirm }): forgets the remote; its files stay on it.
 * Refused InUse, naming them, while any Store links to it (#509). */
export async function removeRemote(runtime, name, options = {}) {
  if (options?.confirm !== name) throw refuse("InvalidInput", `confirm with the remote's name, ${name}`);
  const linked = entries(runtime.settings).filter((s) => s.link?.remote === name).map((s) => s.name);
  if (linked.length) throw refuse("InUse", `Stores link to remote ${name}: ${linked.join(", ")}; unlink them first`);
  return call("runtime.remote.remove", { home: callHome(runtime.settings), name });
}

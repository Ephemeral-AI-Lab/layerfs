// Whole-Store jobs (#506): runtime.store.start_publish, start_push,
// start_pull, start_send, start_load and start_import_postgresql answer a
// job at once, and runtime.sync.get(id) reads it until it ends:
//   { id, kind: publish | push | pull | send | load | import, store, remote,
//     file, step, phase, bytes, total, pieces, error, started, ended, result }
// #509: a SQLite Store links to at most one remote file (registry.js).
// Publish uploads an unlinked Store to a new file and links it; push and
// pull move the Store through its link; send uploads a copy of any SQLite
// Store to a new file and links nothing; load makes a new Store linked to a
// file. A (remote, file) is claimed under the registry lock before a job
// that creates or links it starts, and linked under the lock when it ends.
// `step` is running, done or failed. While it runs, `phase` (check,
// snapshot, hash, upload, download, verify, install, copy, record) and the
// phase's `bytes` of `total` (and `pieces: { done, of }` for a pieced file)
// come from the native call's progress cell (#506 E4), read by sync.get;
// once it is done, `bytes` is what moved. A job runs
// one binding call, made once: an uncertain one fails with code Unknown and
// keeps what it made. Per-call `credentials` ({ accessKey, secretKey }) and
// PostgreSQL URLs reach the binding for that call only and are never kept
// in the job. A job is kept for KEEP_MS after it ends.
import { randomUUID } from "node:crypto";
import { statSync } from "node:fs";
import { invokeBackend } from "@layerfs/agent/internal";
import { callHome } from "../settings.js";
import { credentialsOf, makeVolume, postgresqlTarget, shown, unused } from "./calls.js";
import { FILE, claim, link, named, put, refuse, storeFields } from "./registry.js";
import { removeVolume } from "./volume.js";

const KEEP_MS = 5 * 60_000;
const call = (op, args) => invokeBackend(op, args, {});

export class SyncJobs {
  constructor() {
    this.jobs = new Map();
  }

  /** Starts `run(id)` as a job; answers the job. */
  begin(fields, run) {
    const job = { id: randomUUID(), remote: null, file: null, step: "running", phase: null, bytes: null, total: null, pieces: null, error: null, started: Date.now(), ended: null, result: null, ...fields };
    this.jobs.set(job.id, job);
    const end = (more) => {
      Object.assign(job, more, { ended: Date.now() });
      setTimeout(() => this.jobs.delete(job.id), KEEP_MS).unref?.();
    };
    run(job.id).then(
      (result) => end({ step: "done", phase: null, pieces: null, result, bytes: result?.transfer?.bytes ?? result?.packBytes ?? null }),
      (error) => end({ step: "failed", error: { code: error?.code || "Failed", message: error?.message ?? String(error) } }),
    );
    return { ...job };
  }

  /** The job, with a running one's latest progress sample from `sample(id)` when it answers. */
  async get(id, sample = null) {
    const job = this.jobs.get(id?.id ?? id);
    if (!job) return null;
    if (job.step === "running" && sample) {
      const now = await sample(job.id).catch(() => null);
      // a job may have ended meanwhile: its result stands
      if (now && job.step === "running") Object.assign(job, { phase: now.phase, bytes: now.bytes, total: now.total ?? job.total, pieces: now.pieces });
    }
    return { ...job };
  }
}

/** One running job's sample from the binding that runs it: `{ phase, bytes, total, pieces }` or null. */
export const progressOf = (runtime) => (id) => call("runtime.sync.progress", { home: callHome(runtime.settings), job: id });

/** The Store file's size, when this process can see it (Linux). */
function size(fields) {
  try { return statSync(fields.storePath).size; } catch { return null; }
}

/** A local SQLite Store, else refused. */
async function sqlite(runtime, name, what) {
  const store = await named(runtime, name);
  if (store.kind !== "sqlite") throw refuse("Unsupported", `${what} works on a SQLite Store; ${name} is a PostgreSQL schema`);
  return store;
}

/** The Store's link, else NotLinked. */
export function linkOf(store) {
  if (!store.link) throw refuse("NotLinked", `Store ${store.name} is linked to no remote file; publish it, or load a remote file`);
  return store.link;
}

/** A new remote file `(remote, file)` named by a publish, send or load. */
function newFile(options, what) {
  if (typeof options?.remote !== "string" || !options.remote) throw refuse("InvalidInput", `remote names the connection to ${what}`);
  if (typeof options.file !== "string" || !FILE.test(options.file)) throw refuse("InvalidInput", "file names the remote file: 1-128 of A-Z a-z 0-9 . _ -, not starting with a dot");
  return { remote: options.remote, file: options.file };
}

/** Refuses a `remote` argument where the Store's link decides (#509). */
function byLink(options, what) {
  if (options?.remote !== undefined) throw refuse("InvalidInput", `${what} goes through the Store's link and takes no remote; publish or send to choose a remote file`);
}

/** Keeps a Store's entry, making one for a project's own Store first. */
function entryOf(runtime, store) {
  return store.legacy ? put(runtime.settings, { ...store, projects: store.projects.filter((p) => p.name !== store.name) }) : store;
}

/** runtime.store.start_publish(store, { remote, file = store, credentials }):
 * uploads an unlinked Store to a new file and links it. Refused `Linked`
 * when the Store has a link, `Exists` when a Store of the home links to the
 * file or an object is there (one HEAD before the upload; no overwrite). */
export async function start_publish(runtime, jobs, name, options = {}) {
  const store = await sqlite(runtime, name, "publish");
  if (store.link) throw refuse("Linked", `Store ${name} is linked to ${store.link.remote} / ${store.link.file}; push to it, or unlink it first`);
  const to = newFile({ ...options, file: options?.file ?? name }, "publish to");
  const fields = storeFields(runtime, store);
  const credentials = credentialsOf(options);
  const release = claim(runtime.settings, to.remote, to.file, name);
  try {
    return jobs.begin({ kind: "publish", store: name, ...to, total: size(fields) }, async (job) => {
      try {
        const answer = await call("runtime.store.publish", { ...fields, ...to, ...credentials, job });
        entryOf(runtime, store);
        link(runtime.settings, name, answer.transfer, (s) => {
          if (s.link) throw refuse("Linked", `Store ${name} was linked to ${s.link.remote} / ${s.link.file} meanwhile`);
          return s;
        });
        return answer;
      } finally {
        release();
      }
    });
  } catch (error) {
    release();
    throw error;
  }
}

/** runtime.store.start_push(store, { credentials, force }): uploads the whole
 * Store to its link. `Conflict` when the file is not what the link's last
 * transfer recorded (either direction), unless `force`. */
export async function start_push(runtime, jobs, name, options = {}) {
  byLink(options, "a push");
  const store = await sqlite(runtime, name, "push");
  const to = linkOf(store);
  const fields = storeFields(runtime, store);
  return jobs.begin({ kind: "push", store: name, remote: to.remote, file: to.file, total: size(fields) }, async (job) => {
    const answer = await call("runtime.store.push", { ...fields, remote: to.remote, file: to.file, ...credentialsOf(options), job, ...(options.force ? { force: true } : {}) });
    link(runtime.settings, name, answer.transfer);
    return answer;
  });
}

/** runtime.store.start_pull(store, { credentials, confirm }): replaces the Store, every project in it, by its linked file. */
export async function start_pull(runtime, jobs, name, options = {}) {
  byLink(options, "a pull");
  if (options?.confirm !== name) throw refuse("InvalidInput", `a pull replaces every project of Store ${name}; confirm with its name`);
  const store = await sqlite(runtime, name, "pull");
  const from = linkOf(store);
  const fields = storeFields(runtime, store);
  return jobs.begin({ kind: "pull", store: name, remote: from.remote, file: from.file }, async (job) => {
    // this process's clients hold the Store open; a pull refuses an open Store
    await runtime.clients.forgetStore(name);
    const answer = await call("runtime.store.pull", { ...fields, remote: from.remote, file: from.file, ...credentialsOf(options), job });
    link(runtime.settings, name, answer.transfer, (s) => ({ ...s, projects: [...s.projects, ...answer.added.map((p) => ({ name: p }))] }));
    return answer;
  });
}

/** runtime.store.start_send(store, { remote, file, credentials }): uploads a
 * copy of any SQLite Store to a new file, linked to nothing; the Store's
 * link and records do not change. Refused `Exists` when a Store of the home
 * links to the file or an object is there. The result names `{ remote,
 * file, bytes, sha256, heads }`. */
export async function start_send(runtime, jobs, name, options = {}) {
  const store = await sqlite(runtime, name, "send");
  const to = newFile(options, "send to");
  const fields = storeFields(runtime, store);
  const release = claim(runtime.settings, to.remote, to.file, name);
  try {
    return jobs.begin({ kind: "send", store: name, ...to, total: size(fields) }, async (job) => {
      try {
        const answer = await call("runtime.store.send", { ...fields, ...to, ...credentialsOf(options), job });
        const { bytes, sha256, heads } = answer.transfer;
        return { ...answer, ...to, bytes, sha256, heads: heads ?? null };
      } finally {
        release();
      }
    });
  } catch (error) {
    release();
    throw error;
  }
}

/** runtime.store.start_load(remote, { file, store = file, credentials }): a
 * new local Store `store` from the remote file, linked to it. Replaces
 * start_clone. Refused `Linked` (naming the Store) when a Store of the home
 * links to the file, `Exists` for a Store name in use. */
export async function start_load(runtime, jobs, remoteName, options = {}) {
  const from = newFile({ ...options, remote: remoteName }, "load from");
  const name = options?.store ?? from.file;
  const release = claim(runtime.settings, from.remote, from.file, name, "Linked");
  try {
    await unused(runtime, name);
    return jobs.begin({ kind: "load", store: name, ...from }, async (job) => {
      try {
        await runtime.platform.init(runtime.settings);
        const volume = await makeVolume(runtime, name);
        const store = { name, kind: "sqlite", volume, link: null, created: Date.now(), projects: [] };
        let answer;
        try {
          answer = await call("runtime.store.load", { ...storeFields(runtime, store), ...from, ...credentialsOf(options), job });
        } catch (error) {
          if (error?.code !== "Unknown") await removeVolume(runtime, { name, volume }).catch(() => {});
          throw error;
        }
        put(runtime.settings, { ...store, projects: answer.projects.map((p) => ({ name: p })) });
        link(runtime.settings, name, answer.transfer);
        return answer;
      } finally {
        release();
      }
    });
  } catch (error) {
    release();
    throw error;
  }
}

/** runtime.store.start_import_postgresql(store, { name, connection, url, schema, synchronousCommit }):
 * copies the SQLite Store into a new schema (every project, the same ids)
 * and registers it as the PostgreSQL Store `name` (default `<store>-pg`). */
export async function start_import_postgresql(runtime, jobs, name, options = {}) {
  const source = await sqlite(runtime, name, "an import into PostgreSQL");
  const target = postgresqlTarget(options);
  const pgName = options.name ?? `${name}-pg`;
  await unused(runtime, pgName);
  const fields = storeFields(runtime, source);
  return jobs.begin({ kind: "import", store: name, target: pgName, total: size(fields) }, async (job) => {
    const postgresql = { url: target.url, schema: target.schema, synchronousCommit: target.synchronousCommit };
    const answer = await call("runtime.store.import_postgresql", { ...fields, target: pgName, postgresql, job });
    // #506 E6: the PostgreSQL Store records the SQLite Store it came from
    const importedFrom = { store: name, at: Date.now(), bytes: answer.sourceBytes ?? null };
    const store = { name: pgName, kind: "postgresql", connection: target.connection, schema: target.schema, synchronousCommit: target.synchronousCommit, created: Date.now(), importedFrom };
    const projects = await call("runtime.store.list_projects", { home: callHome(runtime.settings), store: pgName, storePath: `postgresql:${pgName}`, postgresql: { url: target.url } });
    put(runtime.settings, { ...store, projects: projects.map((p) => ({ name: p.name })) });
    return { ...answer, created: shown({ ...store, projects }) };
  });
}

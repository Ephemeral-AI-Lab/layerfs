// Whole-Store jobs (#506): runtime.store.start_push, start_pull,
// start_clone and start_import_postgresql answer a job at once, and
// runtime.sync.get(id) reads it until it ends:
//   { id, kind: push | pull | clone | import, store, remote, step, phase,
//     bytes, total, pieces, error, started, ended, result }
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
import { makeVolume, postgresqlTarget, remoteOf, shown, unused } from "./calls.js";
import { named, put, refuse, storeFields, update, withCopy } from "./registry.js";
import { removeVolume } from "./volume.js";

const KEEP_MS = 5 * 60_000;
const call = (op, args) => invokeBackend(op, args, {});

export class SyncJobs {
  constructor() {
    this.jobs = new Map();
  }

  /** Starts `run(id)` as a job; answers the job. */
  begin(fields, run) {
    const job = { id: randomUUID(), remote: null, step: "running", phase: null, bytes: null, total: null, pieces: null, error: null, started: Date.now(), ended: null, result: null, ...fields };
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

/** runtime.store.start_push(store, { remote, credentials, force }): uploads the whole Store. */
export async function start_push(runtime, jobs, name, options = {}) {
  const store = await sqlite(runtime, name, "push");
  const fields = storeFields(runtime, store);
  const remote = remoteOf(options);
  return jobs.begin({ kind: "push", store: name, remote: remote.remote, total: size(fields) }, async (job) => {
    const answer = await call("runtime.store.push", { ...fields, ...remote, job, ...(options.force ? { force: true } : {}) });
    update(runtime.settings, name, (s) => withCopy(s, answer.transfer));
    return answer;
  });
}

/** Keeps a Store's entry, making one for a project's own Store first. */
function entryOf(runtime, store) {
  return store.legacy ? put(runtime.settings, { ...store, projects: store.projects.filter((p) => p.name !== store.name) }) : store;
}

/** runtime.store.start_pull(store, { remote, credentials, confirm }): replaces the Store, every project in it, by its remote copy. */
export async function start_pull(runtime, jobs, name, options = {}) {
  if (options?.confirm !== name) throw refuse("InvalidInput", `a pull replaces every project of Store ${name}; confirm with its name`);
  const store = await sqlite(runtime, name, "pull");
  const fields = storeFields(runtime, store);
  const remote = remoteOf(options);
  return jobs.begin({ kind: "pull", store: name, remote: remote.remote }, async (job) => {
    // this process's clients hold the Store open; a pull refuses an open Store
    await runtime.clients.forgetStore(name);
    const answer = await call("runtime.store.pull", { ...fields, ...remote, job });
    entryOf(runtime, store);
    update(runtime.settings, name, (s) => withCopy({ ...s, projects: [...s.projects, ...answer.added.map((p) => ({ name: p }))] }, answer.transfer));
    return answer;
  });
}

/** runtime.store.start_clone(remote, { from, store, credentials }): a new local
 * Store `store` from the remote copy of `from` (default `store`). The older
 * `{ store, as }` (`store` on the remote, `as` here) is accepted too. */
export async function start_clone(runtime, jobs, remoteName, options = {}) {
  const from = options?.from ?? options?.store;
  const name = options?.as ?? (options?.from !== undefined ? options?.store ?? options.from : from);
  if (typeof from !== "string" || !from) throw refuse("InvalidInput", "store names the Store on the remote");
  await unused(runtime, name);
  const remote = remoteOf({ ...options, remote: remoteName });
  return jobs.begin({ kind: "clone", store: name, remote: remote.remote }, async (job) => {
    await runtime.platform.init(runtime.settings);
    const volume = await makeVolume(runtime, name);
    const store = { name, kind: "sqlite", volume, remote: remote.remote, created: Date.now(), projects: [], copies: [] };
    let answer;
    try {
      answer = await call("runtime.store.clone", { ...storeFields(runtime, store), from, ...remote, job });
    } catch (error) {
      if (error?.code !== "Unknown") await removeVolume(runtime, volume).catch(() => {});
      throw error;
    }
    put(runtime.settings, withCopy({ ...store, projects: answer.projects.map((p) => ({ name: p })) }, answer.transfer));
    return answer;
  });
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
    const store = { name: pgName, kind: "postgresql", connection: target.connection, schema: target.schema, synchronousCommit: target.synchronousCommit, remote: null, created: Date.now(), copies: [], importedFrom };
    const projects = await call("runtime.store.list_projects", { home: callHome(runtime.settings), store: pgName, storePath: `postgresql:${pgName}`, postgresql: { url: target.url } });
    put(runtime.settings, { ...store, projects: projects.map((p) => ({ name: p.name })) });
    return { ...answer, created: shown({ ...store, projects }) };
  });
}

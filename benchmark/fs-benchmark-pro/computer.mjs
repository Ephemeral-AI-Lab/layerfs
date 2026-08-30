#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { once } from "node:events";
import {
  closeSync,
  createReadStream,
  existsSync,
  fsyncSync,
  mkdirSync,
  openSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { registerHooks } from "node:module";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const SCHEMA = "fs-benchmark-pro-sample-v2";
export const CANDIDATE = "computer-upstream";
export const COMPUTER_COMMIT = "de87919a4fd37242e960e13b7b3ba802d1eef0a0";
export const COMPUTER_TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190";
export const COMPUTER_ARCHIVE_SHA256 =
  "c7d7d1b2e63c97006aee744cf0ece5af529d28a39752467b5da1247370a38b84";
export const COMPUTER_LOCK_SHA256 =
  "3484114c272ea8903963ff464e0a695baf3645659b50b2220f125103b10d505c";
export const FILE_BYTES = 32 * 1024 * 1024;
export const EDIT_COUNT = 16;
export const INITIAL_SHA256 =
  "3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c";
export const AFTER_EDITS_SHA256 =
  "30e8b6c71ab635057c32f0e509e6e0037b5781f94bf1b4c88fb438f41d76ca26";
export const PREPEND = "PREPEND010";
export const FINAL_BYTES = FILE_BYTES + PREPEND.length;
export const FINAL_SHA256 =
  "7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3";

const PRODUCT_ROOT = process.env.CLOUDFLARE_COMPUTER_ROOT ?? "/opt/cloudflare-computer";
const SCRIPT = fileURLToPath(import.meta.url);
const MOUNT = "/workspace";
const BENCH_DIR = `${MOUNT}/fs-benchmark-pro`;
const TARGET = `${BENCH_DIR}/payload.bin`;
const WORKLOAD = "/benchmark/fs-benchmark-workload";
const PORT = 45678;
const COMPUTERD = resolve(PRODUCT_ROOT, "packages/computerd/dist/cli/computerd.cjs");
const INTERNAL_VERIFY = "--verify-authority";

// @cloudflare/computer's official barrel exports Workers-only proxy classes.
// This Node harness uses Workspace and TestBackend only; the upstream project
// uses the same loader shim in script/lib/cloudflare-workers-stub.mjs.
const STUB_URL = "cloudflare-workers-stub:fs-benchmark-pro";
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === "cloudflare:workers") return { url: STUB_URL, shortCircuit: true };
    return next(specifier, context);
  },
  load(url, context, next) {
    if (url === STUB_URL) {
      return {
        format: "module",
        shortCircuit: true,
        source: `
          export class RpcTarget {}
          class Entrypoint { constructor(ctx, env) { this.ctx = ctx; this.env = env; } }
          export class WorkerEntrypoint extends Entrypoint {}
          export class DurableObject extends Entrypoint {}
          export const tracing = { enterSpan(_name, callback) { return callback(); } };
        `,
      };
    }
    return next(url, context);
  },
});

let DatabaseSync;

class Cursor {
  constructor(rows) {
    this.rows = rows;
  }

  toArray() {
    return this.rows;
  }
}

function sqliteValue(value) {
  if (value === undefined || value === null) return null;
  if (typeof value === "boolean") return value ? 1 : 0;
  if (
    value instanceof Uint8Array ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "bigint"
  ) {
    return value;
  }
  throw new TypeError(`unsupported SQLite binding: ${typeof value}`);
}

class FileSQLiteStorage {
  constructor(filename, { readOnly = false } = {}) {
    if (DatabaseSync === undefined) throw new Error("node:sqlite was not initialized");
    this.filename = filename;
    this.raw = new DatabaseSync(filename, readOnly ? { readOnly: true } : {});
    this.cache = new Map();
    if (!readOnly) {
      this.raw.exec("PRAGMA journal_mode=WAL");
      this.raw.exec("PRAGMA synchronous=FULL");
      this.raw.exec("PRAGMA wal_autocheckpoint=0");
    }
    this.sql = {
      exec: (query, ...bindings) => {
        let statement = this.cache.get(query);
        if (statement === undefined) {
          statement = this.raw.prepare(query);
          this.cache.set(query, statement);
        }
        return new Cursor(statement.all(...bindings.map(sqliteValue)) ?? []);
      },
    };
  }

  transactionSync(closure) {
    this.raw.exec("BEGIN IMMEDIATE");
    try {
      const result = closure();
      this.raw.exec("COMMIT");
      return result;
    } catch (error) {
      this.raw.exec("ROLLBACK");
      throw error;
    }
  }

  one(query) {
    const row = this.raw.prepare(query).get();
    if (row === undefined) throw new Error(`query returned no row: ${query}`);
    return row;
  }

  durableBarrier() {
    const checkpoint = this.raw.prepare("PRAGMA wal_checkpoint(TRUNCATE)").get();
    if (checkpoint.busy !== 0 || checkpoint.log !== 0 || checkpoint.checkpointed !== 0) {
      throw new Error(`incomplete WAL checkpoint: ${JSON.stringify(checkpoint)}`);
    }
    fsyncPath(this.filename, false);
    const directory = fsyncPath(dirname(this.filename), true);
    return { checkpoint, database_fsync: true, directory_fsync: directory };
  }

  close() {
    this.cache.clear();
    this.raw.close();
  }
}

function fsyncPath(path, allowUnsupported) {
  const fd = openSync(path, "r");
  try {
    fsyncSync(fd);
    return { supported: true, reason: null };
  } catch (error) {
    if (allowUnsupported && ["EINVAL", "ENOTSUP", "EOPNOTSUPP"].includes(error?.code)) {
      return { supported: false, reason: `${error.code}: ${error.message}` };
    }
    throw error;
  } finally {
    closeSync(fd);
  }
}

async function productModules() {
  const [computer, dofs] = await Promise.all([
    import(pathToFileURL(resolve(PRODUCT_ROOT, "packages/computer/dist/index.js")).href),
    import(pathToFileURL(resolve(PRODUCT_ROOT, "packages/dofs/dist/index.js")).href),
  ]);
  return { ...computer, ...dofs };
}

export function editPlan(size = FILE_BYTES, count = EDIT_COUNT) {
  if (!Number.isSafeInteger(size) || size <= 10) throw new Error("size must be an integer > 10");
  if (!Number.isSafeInteger(count) || count <= 0) throw new Error("count must be positive");
  return Array.from({ length: count }, (_, index) => ({
    id: `edit-${String(index + 1).padStart(2, "0")}`,
    offset: Number((BigInt(index + 1) * 2_654_435_761n) % BigInt(size - 10)),
    marker: `E${String(index + 1).padStart(9, "0")}`,
  }));
}

export function applyEdits(bytes, plan = editPlan(bytes.length)) {
  const result = Buffer.from(bytes);
  for (const edit of plan) result.write(edit.marker, edit.offset, "utf8");
  return result;
}

export function prependBytes(bytes, marker = PREPEND) {
  return Buffer.concat([Buffer.from(marker), Buffer.from(bytes)]);
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function aggregateOperations(operations) {
  const byId = new Map(operations.map((operation) => [operation.id, operation]));
  const required = ["create", ...editPlan().map(({ id }) => id), "prepend", "read"];
  if (operations.length !== required.length || required.some((id) => !byId.has(id))) {
    throw new Error(`operation matrix mismatch: expected ${required.join(",")}`);
  }
  const value = (id) => byId.get(id).comparable_ns;
  return {
    create_ns: value("create"),
    sixteen_edits_sum_ns: editPlan().reduce((sum, { id }) => sum + value(id), 0),
    prepend_ns: value("prepend"),
    read_ns: value("read"),
  };
}

export function validateSummaryShape(summary) {
  if (summary?.schema !== SCHEMA || summary?.candidate !== CANDIDATE) {
    throw new Error("summary identity mismatch");
  }
  if (summary.status !== "PASS") throw new Error("summary did not pass");
  const aggregate = aggregateOperations(summary.operations);
  if (JSON.stringify(aggregate) !== JSON.stringify(summary.aggregates)) {
    throw new Error("summary aggregates are not derived from operations");
  }
  if (
    summary.verification?.final_bytes !== FINAL_BYTES ||
    summary.verification?.final_sha256 !== FINAL_SHA256 ||
    summary.verification?.reopen_passed !== true
  ) {
    throw new Error("summary final oracle mismatch");
  }
  return true;
}

function shellQuote(value) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

async function sha256File(path) {
  const digest = createHash("sha256");
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest("hex");
}

function fileMetric(path) {
  try {
    const stat = statSync(path);
    return { bytes: stat.size, allocated_bytes: stat.blocks * 512 };
  } catch (error) {
    if (error?.code === "ENOENT") return { bytes: 0, allocated_bytes: 0 };
    throw error;
  }
}

function storageSnapshot(storage) {
  const files = storage.one(
    "SELECT COUNT(*) AS count, COALESCE(SUM(size), 0) AS bytes FROM vfs_nodes WHERE type = 'file'",
  );
  const blobs = storage.one(
    "SELECT COUNT(*) AS count, COALESCE(SUM(size), 0) AS bytes FROM vfs_blobs",
  );
  const chunks = storage.one("SELECT COUNT(*) AS count FROM vfs_chunks");
  const reachable = storage.one(
    `SELECT COALESCE(SUM(size), 0) AS bytes FROM vfs_blobs AS blob
       WHERE EXISTS (SELECT 1 FROM vfs_chunks AS chunk WHERE chunk.hash = blob.hash)`,
  );
  const database = fileMetric(storage.filename);
  const wal = fileMetric(`${storage.filename}-wal`);
  const shm = fileMetric(`${storage.filename}-shm`);
  return {
    logical_bytes: Number(files.bytes),
    database_bytes: database.bytes,
    wal_bytes: wal.bytes,
    shm_bytes: shm.bytes,
    durable_allocated_bytes:
      database.allocated_bytes + wal.allocated_bytes + shm.allocated_bytes,
    semantic_payload_bytes: Number(blobs.bytes),
    wire_bytes: null,
    file_count: Number(files.count),
    unique_blob_count: Number(blobs.count),
    chunk_reference_count: Number(chunks.count),
    reachable_blob_bytes: Number(reachable.bytes),
    orphaned_blob_bytes: Math.max(0, Number(blobs.bytes) - Number(reachable.bytes)),
    unavailable: {
      wire_bytes:
        "The official Workspace result exposes pushed/pulled entry counts, not serialized socket bytes.",
    },
  };
}

async function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return { code: child.exitCode, signal: child.signalCode };
  }
  return Promise.race([
    once(child, "exit").then(([code, signal]) => ({ code, signal })),
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error(`process ${child.pid} did not exit in ${timeoutMs} ms`)), timeoutMs),
    ),
  ]);
}

function fuseMountLine() {
  return readFileSync("/proc/self/mountinfo", "utf8")
    .split("\n")
    .find((line) => {
      const fields = line.split(" ");
      const separator = fields.indexOf("-");
      return fields[4] === MOUNT && separator >= 0 && fields[separator + 1]?.startsWith("fuse");
    });
}

async function startComputerd(logPath) {
  if (fuseMountLine() !== undefined) throw new Error(`${MOUNT} is already a FUSE mount`);
  const logFd = openSync(logPath, "wx");
  const child = spawn(process.execPath, [COMPUTERD], {
    env: {
      ...process.env,
      FUSE_MOUNT: "fuse",
      MOUNT_POINT: MOUNT,
      PORT: String(PORT),
    },
    stdio: ["ignore", logFd, logFd],
  });
  closeSync(logFd);
  const deadline = Date.now() + 60_000;
  let lastError;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`computerd ${child.pid} exited early with ${child.exitCode}`);
    }
    try {
      const response = await fetch(`http://127.0.0.1:${PORT}/__computerd/info`);
      if (response.ok) {
        const info = await response.json();
        const mountinfo = fuseMountLine();
        if (info?.backend?.kind !== "fuse" || info.mountPoint !== MOUNT || mountinfo === undefined) {
          throw new Error(`computerd did not select real FUSE: ${JSON.stringify(info)}`);
        }
        return { child, pid: child.pid, info, mountinfo, log: logPath };
      }
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((done) => setTimeout(done, 100));
  }
  child.kill("SIGTERM");
  await waitForExit(child, 10_000).catch(() => undefined);
  throw new Error(`computerd not ready: ${lastError?.message ?? "timeout"}`);
}

async function stopComputerd(daemon) {
  if (!daemon) return null;
  daemon.child.kill("SIGTERM");
  const exit = await waitForExit(daemon.child, 20_000);
  if (exit.code !== 143 && exit.signal !== "SIGTERM") {
    throw new Error(`computerd ${daemon.pid} stopped unexpectedly: ${JSON.stringify(exit)}`);
  }
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline && fuseMountLine() !== undefined) {
    await new Promise((done) => setTimeout(done, 50));
  }
  if (fuseMountLine() !== undefined) throw new Error(`${MOUNT} remained mounted after computerd exit`);
  return exit;
}

function checkExecResult(id, result) {
  if (
    result.status !== "completed" ||
    result.exitCode !== 0 ||
    result.sync?.status !== "complete" ||
    result.skipped?.length !== 0 ||
    result.sync?.skipped?.length !== 0
  ) {
    throw new Error(
      `${id} failed: ${JSON.stringify({
        status: result.status,
        exitCode: result.exitCode,
        sync: result.sync,
        skipped: result.skipped,
        stderr: result.stderr,
      })}`,
    );
  }
}

async function execOperation(workspace, storage, id, command) {
  const started = process.hrtime.bigint();
  const handle = await workspace.runtime.exec(command, {
    backend: "test",
    encoding: "utf8",
    sync: "wait",
  });
  let result;
  try {
    result = await handle.result();
  } finally {
    handle[Symbol.dispose]();
  }
  const apiDone = process.hrtime.bigint();
  checkExecResult(id, result);
  const barrier = storage.durableBarrier();
  const durableDone = process.hrtime.bigint();
  const apiNs = Number(apiDone - started);
  const persistenceNs = Number(durableDone - apiDone);
  const toDurableNs = Number(durableDone - started);
  if (apiNs + persistenceNs !== toDurableNs) throw new Error(`${id}: non-additive timers`);
  return completeOperation({
    id,
    api_ns: apiNs,
    persistence_ns: persistenceNs,
    to_durable_ns: toDurableNs,
    phases: { workspace_runtime_exec_sync_wait_ns: apiNs, sqlite_barrier_ns: persistenceNs },
    pushed: result.pushed,
    pulled: result.pulled,
    skipped: result.skipped.length,
    sync: result.sync,
    stdout: result.stdout,
    barrier,
    storage_before: null,
    storage_after: null,
  });
}

function completeOperation(
  operation,
  { workspaceCreateNs = 0, workspaceEndNs = 0, storageBefore = null, storageAfter = null } = {},
) {
  operation.workspace_create_ns = workspaceCreateNs;
  operation.workspace_end_ns = workspaceEndNs;
  operation.complete_turn_ns = workspaceCreateNs + operation.to_durable_ns + workspaceEndNs;
  operation.comparable_ns = operation.complete_turn_ns;
  operation.storage_before = storageBefore;
  operation.storage_after = storageAfter;
  return operation;
}

async function closeWorkspace(workspace) {
  const started = process.hrtime.bigint();
  await workspace.close();
  return Number(process.hrtime.bigint() - started);
}

async function verifyProvider(dbFile, expectedHash, expectedBytes) {
  const { Database, SQLiteWorkspaceProvider } = await productModules();
  const storage = new FileSQLiteStorage(dbFile, { readOnly: true });
  try {
    const provider = new SQLiteWorkspaceProvider(new Database(storage), { now: Date.now });
    const stat = provider.statSync(TARGET);
    const bytes = provider.readFileSync(TARGET);
    const digest = sha256(bytes);
    if (stat.size !== expectedBytes || bytes.length !== expectedBytes || digest !== expectedHash) {
      throw new Error(
        `authority oracle mismatch: ${JSON.stringify({ stat: stat.size, bytes: bytes.length, digest })}`,
      );
    }
    return { pid: process.pid, bytes: stat.size, sha256: digest };
  } finally {
    storage.close();
  }
}

async function freshProcessAuthorityProof(dbFile) {
  const child = spawn(
    process.execPath,
    ["--no-warnings", SCRIPT, INTERNAL_VERIFY, dbFile, FINAL_SHA256, String(FINAL_BYTES)],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8").on("data", (chunk) => (stdout += chunk));
  child.stderr.setEncoding("utf8").on("data", (chunk) => (stderr += chunk));
  const exit = await waitForExit(child, 30_000);
  if (exit.code !== 0) throw new Error(`fresh authority verifier failed: ${stderr || stdout}`);
  const proof = JSON.parse(stdout);
  if (proof.pid === process.pid) throw new Error("authority verifier did not use a fresh process");
  return proof;
}

async function openWorkspace(dbFile, daemonUrl) {
  const { Database, TestBackend, Workspace, initializeSchema } = await productModules();
  const storage = new FileSQLiteStorage(dbFile);
  const db = new Database(storage);
  initializeSchema(db, Date.now);
  const workspace = new Workspace({
    storage,
    backends: [new TestBackend({ url: daemonUrl })],
  });
  await workspace.ready();
  return { workspace, storage };
}

function parseArgs(argv) {
  if (argv[0] === INTERNAL_VERIFY) {
    if (argv.length !== 4) throw new Error("internal verifier requires DB HASH BYTES");
    return { verify: true, db: argv[1], hash: argv[2], bytes: Number(argv[3]) };
  }
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index];
    const value = argv[index + 1];
    if (!["--fixture", "--output"].includes(name) || value === undefined) {
      throw new Error("usage: computer.mjs --fixture ABSOLUTE_PATH --output ABSOLUTE_PATH");
    }
    if (values[name] !== undefined) throw new Error(`duplicate option: ${name}`);
    values[name] = value;
  }
  const fixture = values["--fixture"];
  const output = values["--output"];
  if (!isAbsolute(fixture ?? "") || !isAbsolute(output ?? "")) {
    throw new Error("--fixture and --output must be absolute paths");
  }
  return { verify: false, fixture, output };
}

async function runBenchmark(args) {
  const fixtureStat = statSync(args.fixture);
  const fixtureHash = await sha256File(args.fixture);
  if (fixtureStat.size !== FILE_BYTES || fixtureHash !== INITIAL_SHA256) {
    throw new Error(
      `neutral fixture mismatch: ${JSON.stringify({ bytes: fixtureStat.size, sha256: fixtureHash })}`,
    );
  }
  if (existsSync(args.output)) throw new Error(`refusing to overwrite ${args.output}`);
  mkdirSync(dirname(args.output), { recursive: true });
  const coldDb = resolve(dirname(args.output), "computer-cold-create.sqlite");
  const editDb = resolve(dirname(args.output), "computer-reference-edit.sqlite");
  if (existsSync(coldDb) || existsSync(editDb)) {
    throw new Error("refusing to reuse Computer authority databases");
  }

  const receipt = {
    schema: SCHEMA,
    candidate: CANDIDATE,
    status: "FAIL",
    started_utc: new Date().toISOString(),
    provenance: {
      repository: "https://github.com/cloudflare/computer",
      commit: COMPUTER_COMMIT,
      tree: COMPUTER_TREE,
      archive_sha256: COMPUTER_ARCHIVE_SHA256,
      package_lock_sha256: COMPUTER_LOCK_SHA256,
      product_patches: [],
      node: process.version,
      platform: `${process.platform}/${process.arch}`,
    },
    workload: {
      initial_bytes: fixtureStat.size,
      initial_sha256: fixtureHash,
      edit_size_bytes: 10,
      prepend_bytes: PREPEND.length,
      fixture: { bytes: fixtureStat.size, sha256: fixtureHash, source: "shared-read-only" },
      edit_count: EDIT_COUNT,
      edits: editPlan(),
      prepend: PREPEND,
      mount: "real FUSE",
      pipeline:
        "authority SQLite -> Workspace.runtime.exec(sync=wait) -> computerd/FUSE -> pull -> authority SQLite -> WAL checkpoint/fsync",
    },
    operations: [],
    unavailable: {
      wire_bytes:
        "The official Workspace result exposes pushed/pulled entry counts, not serialized socket bytes.",
      device_write_bytes: "No portable per-process device-write counter is exposed inside Docker Desktop.",
      executor_database_bytes: "Pinned upstream computerd stores its executor VFS in memory.",
    },
  };

  let daemon;
  let workspace;
  let storage;
  try {
    daemon = await startComputerd(resolve(dirname(args.output), "computerd-cold-create.log"));
    ({ workspace, storage } = await openWorkspace(coldDb, `http://127.0.0.1:${PORT}`));

    const setup = await execOperation(
      workspace,
      storage,
      "setup-unmeasured",
      `mkdir -p -- ${shellQuote(BENCH_DIR)}`,
    );
    receipt.setup = { cold_create: { ...setup, comparable_ns: null }, reference_seed: null };
    await workspace.close();
    workspace = undefined;
    storage.close();
    storage = undefined;

    const createOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(coldDb, `http://127.0.0.1:${PORT}`));
    const createOpenNs = Number(process.hrtime.bigint() - createOpenStarted);
    const createStorageBefore = storageSnapshot(storage);
    receipt.storage = { initial: createStorageBefore, final: null };
    const create = await execOperation(
      workspace,
      storage,
      "create",
      `${shellQuote(WORKLOAD)} create ${shellQuote(args.fixture)} ${shellQuote(TARGET)}`,
    );
    const createStorageAfter = storageSnapshot(storage);
    const createCloseNs = await closeWorkspace(workspace);
    workspace = undefined;
    completeOperation(create, {
      workspaceCreateNs: createOpenNs,
      workspaceEndNs: createCloseNs,
      storageBefore: createStorageBefore,
      storageAfter: createStorageAfter,
    });
    receipt.operations.push(create);
    receipt.verification = {
      initial_bytes: FILE_BYTES,
      initial_sha256: (await verifyProvider(coldDb, INITIAL_SHA256, FILE_BYTES)).sha256,
      after_edits_sha256: null,
      final_bytes: null,
      final_sha256: null,
      reopen_passed: false,
    };

    storage.close();
    storage = undefined;
    const coldStop = await stopComputerd(daemon);
    receipt.executor_cold_create = {
      pid: daemon.pid,
      info: daemon.info,
      mountinfo: daemon.mountinfo,
      stop: coldStop,
    };
    daemon = undefined;

    daemon = await startComputerd(resolve(dirname(args.output), "computerd-reference-seed.log"));
    ({ workspace, storage } = await openWorkspace(editDb, `http://127.0.0.1:${PORT}`));
    const editSetup = await execOperation(
      workspace,
      storage,
      "reference-setup-unmeasured",
      `mkdir -p -- ${shellQuote(BENCH_DIR)}`,
    );
    const seed = await execOperation(
      workspace,
      storage,
      "reference-seed-unmeasured",
      `${shellQuote(WORKLOAD)} create ${shellQuote(args.fixture)} ${shellQuote(TARGET)}`,
    );
    receipt.setup.reference_seed = {
      directory: { ...editSetup, comparable_ns: null },
      file: { ...seed, comparable_ns: null },
    };
    const seedProof = await verifyProvider(editDb, INITIAL_SHA256, FILE_BYTES);
    if (seedProof.sha256 !== INITIAL_SHA256) throw new Error("Computer seed oracle mismatch");
    await workspace.close();
    workspace = undefined;
    storage.close();
    storage = undefined;
    const seedStop = await stopComputerd(daemon);
    receipt.executor_reference_seed = {
      pid: daemon.pid,
      info: daemon.info,
      mountinfo: daemon.mountinfo,
      stop: seedStop,
    };
    daemon = undefined;

    daemon = await startComputerd(resolve(dirname(args.output), "computerd-edit.log"));
    const editOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(editDb, `http://127.0.0.1:${PORT}`));
    const editOpenNs = Number(process.hrtime.bigint() - editOpenStarted);
    receipt.storage.seeded_reopen = storageSnapshot(storage);
    const editStorageBefore = storageSnapshot(storage);
    const editOperations = [];

    for (const [index, edit] of editPlan().entries()) {
      editOperations.push(
        await execOperation(
          workspace,
          storage,
          edit.id,
          `/bin/bash -lc ${shellQuote('"$@"')} fs-bench-shell ${shellQuote(WORKLOAD)} edit ${shellQuote(TARGET)} ${index} ${FILE_BYTES}`,
        ),
      );
    }
    const editStorageAfter = storageSnapshot(storage);
    const editCloseNs = await closeWorkspace(workspace);
    workspace = undefined;
    storage.close();
    storage = undefined;
    completeOperation(editOperations[0], {
      workspaceCreateNs: editOpenNs,
      storageBefore: editStorageBefore,
    });
    completeOperation(editOperations.at(-1), {
      workspaceEndNs: editCloseNs,
      storageAfter: editStorageAfter,
    });
    receipt.operations.push(...editOperations);
    receipt.verification.after_edits_sha256 = (
      await verifyProvider(editDb, AFTER_EDITS_SHA256, FILE_BYTES)
    ).sha256;

    const prependOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(editDb, `http://127.0.0.1:${PORT}`));
    const prependOpenNs = Number(process.hrtime.bigint() - prependOpenStarted);
    const prependBefore = storageSnapshot(storage);
    const prepend = await execOperation(
      workspace,
      storage,
      "prepend",
      `${shellQuote(WORKLOAD)} prepend ${shellQuote(TARGET)}`,
    );
    const prependAfter = storageSnapshot(storage);
    const prependCloseNs = await closeWorkspace(workspace);
    workspace = undefined;
    storage.close();
    storage = undefined;
    completeOperation(prepend, {
      workspaceCreateNs: prependOpenNs,
      workspaceEndNs: prependCloseNs,
      storageBefore: prependBefore,
      storageAfter: prependAfter,
    });
    receipt.operations.push(prepend);

    const readOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(editDb, `http://127.0.0.1:${PORT}`));
    const readOpenNs = Number(process.hrtime.bigint() - readOpenStarted);
    const readBefore = storageSnapshot(storage);
    const read = await execOperation(
      workspace,
      storage,
      "read",
      `${shellQuote(WORKLOAD)} read ${shellQuote(TARGET)}`,
    );
    if (read.stdout.trim().split(/\s+/).at(-1) !== FINAL_SHA256) {
      throw new Error(`read operation digest mismatch: ${JSON.stringify(read.stdout)}`);
    }
    const readAfter = storageSnapshot(storage);
    const readCloseNs = await closeWorkspace(workspace);
    workspace = undefined;
    completeOperation(read, {
      workspaceCreateNs: readOpenNs,
      workspaceEndNs: readCloseNs,
      storageBefore: readBefore,
      storageAfter: readAfter,
    });
    receipt.operations.push(read);
    const finalOracle = await verifyProvider(editDb, FINAL_SHA256, FINAL_BYTES);
    receipt.verification.final_bytes = finalOracle.bytes;
    receipt.verification.final_sha256 = finalOracle.sha256;
    receipt.storage.final = readAfter;
    receipt.aggregates = aggregateOperations(receipt.operations);

    storage.close();
    storage = undefined;
    const firstStop = await stopComputerd(daemon);
    receipt.executor_initial = { pid: daemon.pid, info: daemon.info, mountinfo: daemon.mountinfo, stop: firstStop };
    daemon = undefined;

    const authorityProof = await freshProcessAuthorityProof(editDb);
    const reopenStarted = process.hrtime.bigint();
    daemon = await startComputerd(resolve(dirname(args.output), "computerd-reopen.log"));
    ({ workspace, storage } = await openWorkspace(editDb, `http://127.0.0.1:${PORT}`));
    const reopened = await execOperation(
      workspace,
      storage,
      "reopen-verify-unmeasured",
      `${shellQuote(WORKLOAD)} verify ${shellQuote(TARGET)} ${FINAL_BYTES} ${FINAL_SHA256}`,
    );
    const reopenDigest = reopened.stdout.trim().split(/\s+/).at(-1);
    const reopenedAuthority = await verifyProvider(editDb, FINAL_SHA256, FINAL_BYTES);
    const reopenDone = process.hrtime.bigint();
    if (daemon.pid === receipt.executor_initial.pid || reopenDigest !== FINAL_SHA256) {
      throw new Error("fresh executor reopen proof failed");
    }
    receipt.reopen = {
      elapsed_ns: Number(reopenDone - reopenStarted),
      authority_fresh_process: authorityProof,
      executor: { pid: daemon.pid, info: daemon.info, mountinfo: daemon.mountinfo },
      operation: { ...reopened, comparable_ns: null },
      authority: reopenedAuthority,
    };
    receipt.verification.reopen_passed = true;
    receipt.status = "PASS";
    receipt.finished_utc = new Date().toISOString();
    validateSummaryShape(receipt);
    return receipt;
  } finally {
    if (workspace !== undefined) await workspace.close().catch(() => undefined);
    if (storage !== undefined) storage.close();
    if (daemon !== undefined) await stopComputerd(daemon);
  }
}

async function cli() {
  ({ DatabaseSync } = await import("node:sqlite"));
  const args = parseArgs(process.argv.slice(2));
  if (args.verify) {
    console.log(JSON.stringify(await verifyProvider(args.db, args.hash, args.bytes)));
    return;
  }
  let receipt;
  try {
    receipt = await runBenchmark(args);
  } catch (error) {
    receipt = {
      schema: SCHEMA,
      candidate: CANDIDATE,
      status: "FAIL",
      finished_utc: new Date().toISOString(),
      error: error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error),
    };
  }
  writeFileSync(args.output, `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx" });
  if (receipt.status !== "PASS") throw new Error(receipt.error?.message ?? "benchmark failed");
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT) {
  cli().catch((error) => {
    console.error(error instanceof Error ? error.stack ?? error.message : error);
    process.exitCode = 1;
  });
}

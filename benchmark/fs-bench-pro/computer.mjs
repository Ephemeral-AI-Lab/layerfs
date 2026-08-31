#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { once } from "node:events";
import {
  createReadStream,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { registerHooks } from "node:module";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const SCHEMA = "fs-benchmark-pro-computer-v3";
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
export const PREPEND_ONLY_SHA256 =
  "d5f0fb52a686e6f912a56d3fcc26da2111a8bdf48679fab77cb0921be97163ba";

const PRODUCT_ROOT = process.env.CLOUDFLARE_COMPUTER_ROOT ?? "/opt/cloudflare-computer";
const SCRIPT = fileURLToPath(import.meta.url);
const MOUNT = "/workspace";
const BENCH_DIR = `${MOUNT}/fs-benchmark-pro`;
const TARGET = `${BENCH_DIR}/payload.bin`;
const WORKLOAD = process.env.COMPUTER_BENCH_WORKLOAD ?? "/benchmark/fs-benchmark-workload";
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
      this.raw.exec("PRAGMA foreign_keys=ON");
      this.raw.exec("PRAGMA journal_mode=MEMORY");
      this.raw.exec("PRAGMA synchronous=OFF");
      this.raw.exec("PRAGMA temp_store=MEMORY");
      this.raw.exec("PRAGMA cache_size=-32768");
      this.raw.exec("PRAGMA cache_spill=OFF");
      this.raw.exec("PRAGMA mmap_size=0");
      this.raw.exec("PRAGMA threads=0");
      this.raw.exec("PRAGMA busy_timeout=5000");
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

  acknowledgementProfile() {
    const value = (name) => this.raw.prepare(`PRAGMA ${name}`).get()[name];
    const profile = {
      journal_mode: String(value("journal_mode")).toLowerCase(),
      synchronous: Number(value("synchronous")),
      temp_store: Number(value("temp_store")),
      cache_size: Number(value("cache_size")),
      mmap_size: Number(value("mmap_size")),
    };
    if (
      profile.journal_mode !== "memory" ||
      profile.synchronous !== 0 ||
      profile.temp_store !== 2 ||
      profile.cache_size !== -32768 ||
      profile.mmap_size !== 0
    ) {
      throw new Error(`unmatched SQLite acknowledgement profile: ${JSON.stringify(profile)}`);
    }
    return {
      contract: "transaction-committed-and-readable-from-live-local-process",
      crash_durable: false,
      database_fsync: false,
      directory_fsync: false,
      checkpoint: false,
      ...profile,
    };
  }

  close() {
    this.cache.clear();
    this.raw.close();
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
  const setups = Object.values(summary.setup ?? {});
  if (
    summary.workload?.container_prewarm !== false ||
    setups.length !== 4 ||
    setups.some((setup) => setup.helper_invocations !== 0 || setup.shell_invocations !== 0)
  ) {
    throw new Error("summary edit scenario was prewarmed");
  }
  const aggregate = aggregateOperations(summary.operations);
  if (JSON.stringify(aggregate) !== JSON.stringify(summary.aggregates)) {
    throw new Error("summary aggregates are not derived from operations");
  }
  if (
    summary.verification?.cold_create_sha256 !== INITIAL_SHA256 ||
    summary.verification?.edit16_sha256 !== AFTER_EDITS_SHA256 ||
    summary.verification?.prepend_sha256 !== PREPEND_ONLY_SHA256 ||
    summary.verification?.read_sha256 !== INITIAL_SHA256 ||
    summary.verification?.reopen_passed !== true
  ) {
    throw new Error("summary isolated-scenario oracle mismatch");
  }
  if (
    summary.operations.some(
      (operation) =>
        operation.acknowledgement?.crash_durable !== false ||
        operation.acknowledgement?.journal_mode !== "memory" ||
        operation.acknowledgement?.synchronous !== 0,
    )
  ) {
    throw new Error("summary acknowledgement profile mismatch");
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
    allocated_bytes: database.allocated_bytes + wal.allocated_bytes + shm.allocated_bytes,
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

async function connectComputerd(label) {
  const envName = `COMPUTERD_${label.toUpperCase()}_URL`;
  const url = process.env[envName];
  if (!url?.startsWith("http://127.0.0.1:")) {
    throw new Error(`${envName} must be an http://127.0.0.1 URL`);
  }
  const deadline = Date.now() + 60_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${url}/__computerd/info`);
      if (response.ok) {
        const info = await response.json();
        if (info?.backend?.kind !== "fuse" || info.mountPoint !== MOUNT) {
          throw new Error(`computerd did not select real FUSE: ${JSON.stringify(info)}`);
        }
        return { label, url, info };
      }
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`${label} computerd not ready: ${lastError?.message ?? "timeout"}`);
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
  const visibleDone = process.hrtime.bigint();
  checkExecResult(id, result);
  const apiNs = Number(visibleDone - started);
  return completeOperation({
    id,
    api_ns: apiNs,
    persistence_ns: 0,
    to_visible_ns: apiNs,
    phases: { workspace_runtime_exec_sync_wait_ns: apiNs, extra_durability_barrier_ns: 0 },
    pushed: result.pushed,
    pulled: result.pulled,
    skipped: result.skipped.length,
    sync: result.sync,
    stdout: result.stdout,
    acknowledgement: storage.acknowledgementProfile(),
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
  operation.complete_turn_ns = workspaceCreateNs + operation.to_visible_ns + workspaceEndNs;
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

async function freshProcessAuthorityProof(dbFile, expectedHash, expectedBytes) {
  const child = spawn(
    process.execPath,
    ["--no-warnings", SCRIPT, INTERNAL_VERIFY, dbFile, expectedHash, String(expectedBytes)],
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

async function prepareAuthority(dbFile, fixture) {
  const started = process.hrtime.bigint();
  const { Database, SQLiteWorkspaceProvider, initializeSchema } = await productModules();
  const storage = new FileSQLiteStorage(dbFile);
  try {
    const database = new Database(storage);
    initializeSchema(database, Date.now);
    const provider = new SQLiteWorkspaceProvider(database, { now: Date.now });
    provider.mkdirSync(BENCH_DIR, { recursive: true, mode: 0o755 });
    if (fixture !== undefined) {
      provider.writeFileSync(TARGET, readFileSync(fixture), { mode: 0o644 });
    }
    const acknowledgement = storage.acknowledgementProfile();
    return {
      mode: "direct-authority-setup",
      elapsed_ns: Number(process.hrtime.bigint() - started),
      helper_invocations: 0,
      shell_invocations: 0,
      target_present: fixture !== undefined,
      acknowledgement,
      storage: storageSnapshot(storage),
    };
  } finally {
    storage.close();
  }
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
    if (!["--fixture", "--container-fixture", "--output"].includes(name) || value === undefined) {
      throw new Error(
        "usage: computer.mjs --fixture HOST_PATH --container-fixture CONTAINER_PATH --output PATH",
      );
    }
    if (values[name] !== undefined) throw new Error(`duplicate option: ${name}`);
    values[name] = value;
  }
  const fixture = values["--fixture"];
  const containerFixture = values["--container-fixture"];
  const output = values["--output"];
  if (
    !isAbsolute(fixture ?? "") ||
    !isAbsolute(containerFixture ?? "") ||
    !isAbsolute(output ?? "")
  ) {
    throw new Error("fixture, container fixture, and output must be absolute paths");
  }
  return { verify: false, fixture, containerFixture, output };
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
  const databases = {
    cold: resolve(dirname(args.output), "computer-cold-create.sqlite"),
    edit: resolve(dirname(args.output), "computer-edit16.sqlite"),
    prepend: resolve(dirname(args.output), "computer-prepend.sqlite"),
    read: resolve(dirname(args.output), "computer-read.sqlite"),
  };
  if (Object.values(databases).some((path) => existsSync(path))) {
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
      container_prewarm: false,
      registered_edit_argv: [
        "/bin/sh",
        "-c",
        WORKLOAD,
        "edit",
        TARGET,
        "<index>",
        String(FILE_BYTES),
      ],
      pipeline:
        "native authority SQLite MEMORY/OFF -> Workspace.runtime.exec(sync=wait) -> prepared computerd/FUSE -> pull -> authority-visible transaction",
      acknowledgement:
        "transaction committed and readable from the live local process; no crash or power-loss durability",
    },
    operations: [],
    unavailable: {
      wire_bytes:
        "The official Workspace result exposes pushed/pulled entry counts, not serialized socket bytes.",
      device_write_bytes: "No portable per-process device-write counter is exposed inside Docker Desktop.",
      executor_database_bytes: "Pinned upstream computerd stores its executor VFS in memory.",
    },
  };

  let workspace;
  let storage;
  try {
    receipt.setup = {
      cold_create: await prepareAuthority(databases.cold),
      edit16: await prepareAuthority(databases.edit, args.fixture),
      prepend: await prepareAuthority(databases.prepend, args.fixture),
      read: await prepareAuthority(databases.read, args.fixture),
    };
    receipt.storage = { cold_create: null, edit16: null, prepend: null, read: null };
    receipt.verification = {
      cold_create_sha256: null,
      edit16_sha256: null,
      prepend_sha256: null,
      read_sha256: null,
      reopen_passed: false,
    };
    const daemons = {
      cold: await connectComputerd("cold"),
      edit: await connectComputerd("edit"),
      prepend: await connectComputerd("prepend"),
      read: await connectComputerd("read"),
    };
    receipt.executors = daemons;

    const editOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(databases.edit, daemons.edit.url));
    const editOpenNs = Number(process.hrtime.bigint() - editOpenStarted);
    const editStorageBefore = storageSnapshot(storage);
    const editOperations = [];

    for (const [index, edit] of editPlan().entries()) {
      editOperations.push(
        await execOperation(
          workspace,
          storage,
          edit.id,
          `${shellQuote(WORKLOAD)} edit ${shellQuote(TARGET)} ${index} ${FILE_BYTES}`,
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
    receipt.storage.edit16 = { before: editStorageBefore, after: editStorageAfter };
    receipt.verification.edit16_sha256 = (
      await verifyProvider(databases.edit, AFTER_EDITS_SHA256, FILE_BYTES)
    ).sha256;

    const prependOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(databases.prepend, daemons.prepend.url));
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
    receipt.storage.prepend = { before: prependBefore, after: prependAfter };
    receipt.verification.prepend_sha256 = (
      await verifyProvider(databases.prepend, PREPEND_ONLY_SHA256, FINAL_BYTES)
    ).sha256;

    const readOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(databases.read, daemons.read.url));
    const readOpenNs = Number(process.hrtime.bigint() - readOpenStarted);
    const readBefore = storageSnapshot(storage);
    const read = await execOperation(
      workspace,
      storage,
      "read",
      `${shellQuote(WORKLOAD)} read ${shellQuote(TARGET)}`,
    );
    if (read.stdout.trim() !== `read_bytes=${FILE_BYTES}`) {
      throw new Error(`read operation size mismatch: ${JSON.stringify(read.stdout)}`);
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
    receipt.storage.read = { before: readBefore, after: readAfter };
    receipt.verification.read_sha256 = (
      await verifyProvider(databases.read, INITIAL_SHA256, FILE_BYTES)
    ).sha256;
    storage.close();
    storage = undefined;

    const createOpenStarted = process.hrtime.bigint();
    ({ workspace, storage } = await openWorkspace(databases.cold, daemons.cold.url));
    const createOpenNs = Number(process.hrtime.bigint() - createOpenStarted);
    const createStorageBefore = storageSnapshot(storage);
    const create = await execOperation(
      workspace,
      storage,
      "create",
      `${shellQuote(WORKLOAD)} create ${shellQuote(args.containerFixture)} ${shellQuote(TARGET)}`,
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
    receipt.operations.unshift(create);
    receipt.storage.cold_create = { before: createStorageBefore, after: createStorageAfter };
    storage.close();
    storage = undefined;
    const coldProof = await verifyProvider(databases.cold, INITIAL_SHA256, FILE_BYTES);
    if (coldProof.sha256 !== INITIAL_SHA256) throw new Error("Computer cold-create oracle mismatch");
    receipt.verification.cold_create_sha256 = coldProof.sha256;
    receipt.aggregates = aggregateOperations(receipt.operations);

    const reopenStarted = process.hrtime.bigint();
    const reopenCases = [
      ["cold", databases.cold, daemons.cold.url, INITIAL_SHA256, FILE_BYTES],
      ["edit", databases.edit, daemons.edit.url, AFTER_EDITS_SHA256, FILE_BYTES],
      ["prepend", databases.prepend, daemons.prepend.url, PREPEND_ONLY_SHA256, FINAL_BYTES],
      ["read", databases.read, daemons.read.url, INITIAL_SHA256, FILE_BYTES],
    ];
    const reopened = [];
    for (const [name, database, url, hash, bytes] of reopenCases) {
      const authority = await freshProcessAuthorityProof(database, hash, bytes);
      ({ workspace, storage } = await openWorkspace(database, url));
      const operation = await execOperation(
        workspace,
        storage,
        `reopen-${name}-unmeasured`,
        `${shellQuote(WORKLOAD)} verify ${shellQuote(TARGET)} ${bytes} ${hash}`,
      );
      const digest = operation.stdout.trim().split(/\s+/).at(-1);
      await closeWorkspace(workspace);
      workspace = undefined;
      storage.close();
      storage = undefined;
      if (digest !== hash) throw new Error(`${name} executor reopen proof failed`);
      reopened.push({ name, authority, operation: { ...operation, comparable_ns: null } });
    }
    const reopenDone = process.hrtime.bigint();
    receipt.reopen = {
      elapsed_ns: Number(reopenDone - reopenStarted),
      cases: reopened,
    };
    receipt.verification.reopen_passed = true;
    receipt.status = "PASS";
    receipt.finished_utc = new Date().toISOString();
    validateSummaryShape(receipt);
    return receipt;
  } finally {
    if (workspace !== undefined) await workspace.close().catch(() => undefined);
    if (storage !== undefined) storage.close();
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

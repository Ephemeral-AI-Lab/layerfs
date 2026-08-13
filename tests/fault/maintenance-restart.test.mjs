import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import { constrainStorageLimits } from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { MaintenanceRepository } from "../../packages/fs/dist/sqlite/maintenance-repository.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import {
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { maintenanceFaultInjector } from "./maintenance-fault-injector.mjs";

const storageOptions = Object.freeze({
  maxGcBatchSize: 1,
  maxQueryBatchSize: 1,
  maxFinalTransactionRows: 64,
  maxMaintenanceBytes: 32 * 1024 * 1024,
  maintenanceReserveBytes: 32 * 1024 * 1024,
});

function stateSnapshot(database, runId) {
  return database.transaction("read", (tx) => {
    const rows = (sql, bindings = [], maxRows = 10_000) =>
      tx.all(sql, bindings, { maxRows, maxBytes: 8 * 1024 * 1024 });
    return Object.freeze({
      meta: rows(
        "SELECT root_mutation_generation,last_root_removal_generation,next_allocation_sequence FROM efs_meta WHERE singleton=1",
        [],
        1,
      ),
      usage: rows("SELECT * FROM efs_usage WHERE singleton=1", [], 1),
      snapshot: rows("SELECT * FROM efs_storage_snapshots WHERE singleton=1", [], 1),
      snapshotMarks: rows(
        "SELECT kind,hash,edge_cursor,processed,accounted,scope_mask FROM efs_storage_marks ORDER BY kind,hash",
      ),
      gcRuns: rows(
        "SELECT * FROM efs_gc_runs WHERE id=? OR id='prior-terminal' ORDER BY id",
        [runId],
      ),
      gcMarks: rows(
        "SELECT run_id,kind,hash,edge_cursor,processed FROM efs_gc_marks WHERE run_id=? OR run_id='prior-terminal' ORDER BY run_id,kind,hash",
        [runId],
      ),
      rootJournal: rows(
        "SELECT generation,kind,root_id FROM efs_root_journal ORDER BY generation",
      ),
      leaseCleanups: rows("SELECT * FROM efs_lease_cleanups ORDER BY lease_id"),
      leases: rows("SELECT id,state,expires_at_ms FROM efs_leases ORDER BY id"),
      objects: rows(
        "SELECT hash,size,allocation_sequence FROM efs_cas_objects ORDER BY hash",
      ),
      manifestRoots: rows(
        "SELECT hash,allocation_sequence FROM efs_manifest_roots ORDER BY hash",
      ),
      manifestNodes: rows(
        "SELECT hash,allocation_sequence FROM efs_manifest_nodes ORDER BY hash",
      ),
    });
  });
}

function verifyUsage(database) {
  const limits = constrainStorageLimits(
    { ...storageOptions, maxQueryBatchSize: 1024 },
    database.capabilities,
  );
  database.transaction("read", (tx) =>
    new UsageRepository(tx, limits).verifyDerivedUsage(),
  );
}

async function driveSnapshot(filesystem) {
  for (let batches = 0; batches < 1000; batches += 1) {
    const result = await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
    if (result.state === "complete") return result;
  }
  throw new Error("snapshot did not finish within bounded batches");
}

async function driveCollection(filesystem, runId) {
  let totalBatches = 0;
  for (let calls = 0; calls < 2000; calls += 1) {
    const result = await filesystem.maintenance.collectGarbage({
      runId,
      maxBatches: 1,
    });
    assert.ok(result.committedBatches <= 1);
    totalBatches += result.committedBatches;
    if (result.state === "complete") return { result, totalBatches };
  }
  throw new Error("collection did not finish within bounded batches");
}

function snapshotCounters(result) {
  return Object.freeze({
    mainLogicalBytes: result.mainLogicalBytes,
    storedObjectPayloadBytes: result.storedObjectPayloadBytes,
    storedManifestPayloadBytes: result.storedManifestPayloadBytes,
    reachableObjectPayloadBytes: result.reachableObjectPayloadBytes,
    reachableManifestPayloadBytes: result.reachableManifestPayloadBytes,
    reclaimablePayloadBytes: result.reclaimablePayloadBytes,
    branchPageBytes: result.branchPageBytes,
    branchPatchBytes: result.branchPatchBytes,
    branchExclusivePayloadBytes: result.branchExclusivePayloadBytes,
    operationResultPayloadBytes: result.operationResultPayloadBytes,
    objectCount: result.objectCount,
    manifestRootCount: result.manifestRootCount,
    manifestNodeCount: result.manifestNodeCount,
    chargedMetadataBytes: result.chargedMetadataBytes,
    revisionCount: result.revisionCount,
  });
}

function collectionCounters(result) {
  return Object.freeze({
    examinedManifestRootCount: result.examinedManifestRootCount,
    deletedManifestRootCount: result.deletedManifestRootCount,
    examinedManifestNodeCount: result.examinedManifestNodeCount,
    deletedManifestNodeCount: result.deletedManifestNodeCount,
    examinedObjectCount: result.examinedObjectCount,
    deletedObjectCount: result.deletedObjectCount,
    reclaimedObjectPayloadBytes: result.reclaimedObjectPayloadBytes,
    reclaimedManifestPayloadBytes: result.reclaimedManifestPayloadBytes,
    reclaimedBranchOverlayPayloadBytes: result.reclaimedBranchOverlayPayloadBytes,
  });
}

async function setupSnapshot(filename) {
  const database = await openNodeSqlite({ filename });
  const filesystem = await EphemeralFS.open({
    database,
    storage: storageOptions,
  });
  await filesystem.writeFile("/main", "main-value");
  const branch = await filesystem.branches.create("fault-branch");
  await branch.writeFile("/branch", "branch-only-value");
  await branch.close();
  await filesystem.close();
  database.close();
}

async function setupCollection(filename) {
  const database = await openNodeSqlite({ filename });
  let now = 10;
  const filesystem = await EphemeralFS.open({
    database,
    storage: storageOptions,
    clock: () => now,
  });
  await filesystem.writeFile("/kept", "reachable-value");
  const limits = constrainStorageLimits(storageOptions, database.capabilities);
  const orphan = buildManifest(new TextEncoder().encode("orphan-value"), {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  database.transaction("write", (tx) => {
    const content = new ContentRepository(tx, limits);
    for (const [hash, bytes] of orphan.objects)
      content.putObject(Buffer.from(hash, "hex"), bytes);
    for (const node of orphan.nodes.values())
      content.putManifestNode(node.hash, node.encoded);
    content.putManifestRoot(orphan.rootHash, orphan.root);

    const staging = new StagingRepository(tx, limits);
    const nonce = new Uint8Array(16).fill(7);
    const staged = Uint8Array.of(8, 9, 10);
    const stagedHash = sha256(staged);
    staging.begin({
      leaseId: "expired-fault-lease",
      ownerId: "fault-owner",
      ownerNonce: nonce,
      now: 1,
      expiresAt: 2,
    });
    content.putObject(stagedHash, staged);
    staging.appendBatch("expired-fault-lease", nonce, [
      { kind: "object", hash: stagedHash, size: staged.length },
    ]);

    const maintenance = new MaintenanceRepository(tx, limits);
    maintenance.beginRun("prior-terminal", 1);
    tx.run("UPDATE efs_gc_runs SET state=7 WHERE id='prior-terminal'");
    const generation = tx.all(
      "SELECT root_mutation_generation generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0].generation;
    for (let index = 1; index <= 2; index += 1)
      tx.run("INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)", [
        generation + index,
        0,
        Uint8Array.of(index),
      ]);
    tx.run("UPDATE efs_meta SET root_mutation_generation=? WHERE singleton=1", [
      generation + 2,
    ]);
    new UsageRepository(tx, limits).apply({
      maintenance_bytes: 2 * (CHARGED_ROW_BYTES + 1),
    });
  });
  now = 10;
  await filesystem.close();
  database.close();
}

async function setupAbandoned(filename) {
  const database = await openNodeSqlite({ filename });
  const filesystem = await EphemeralFS.open({
    database,
    storage: storageOptions,
  });
  await filesystem.writeFile("/kept", "abandoned-kept-value");
  const limits = constrainStorageLimits(storageOptions, database.capabilities);
  database.transaction("write", (tx) => {
    const maintenance = new MaintenanceRepository(tx, limits);
    maintenance.beginRun("abandoned-fault", 1);
    for (let index = 1; index <= 3; index += 1)
      maintenance.addMark("abandoned-fault", 2, new Uint8Array(32).fill(index));
    maintenance.abandonRun("abandoned-fault", 7, 8);
  });
  await filesystem.close();
  database.close();
}

async function snapshotScenario(fault, { mutateAfterFault = false } = {}) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-snapshot-fault-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    await setupSnapshot(filename);
    database = await openNodeSqlite({ filename, create: false });
    const injector = maintenanceFaultInjector(database);
    filesystem = await EphemeralFS.open({
      database: injector.driver,
      storage: storageOptions,
    });
    injector.arm(fault ?? { afterStatement: Number.MAX_SAFE_INTEGER });
    let interrupted = false;
    try {
      await driveSnapshot(filesystem);
    } catch (error) {
      interrupted = true;
      assert.equal(error.name, "AbortError");
    }
    const metrics = injector.metrics();
    if (fault) assert.equal(interrupted, true);
    injector.disarm();
    const faultOrdinal = fault?.afterStatement ?? fault?.afterBatch;
    const replacedMain =
      mutateAfterFault && faultOrdinal !== undefined && faultOrdinal % 2 === 0;
    if (fault && mutateAfterFault)
      await filesystem.writeFile(
        replacedMain ? "/main" : "/post-fault",
        replacedMain ? "replacement-main-value" : "post-fault-value",
      );
    const before = stateSnapshot(database, "snapshot-unused");
    verifyUsage(database);
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    const reopened = stateSnapshot(database, "snapshot-unused");
    assert.deepEqual(reopened, before);
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    const completed = await driveSnapshot(filesystem);
    assert.equal(completed.remainingWork, 0);
    assert.equal(completed.phase, "complete");
    assert.equal(
      await filesystem.readFile("/main", { encoding: "utf8" }),
      replacedMain ? "replacement-main-value" : "main-value",
    );
    if (fault && mutateAfterFault && !replacedMain)
      assert.equal(
        await filesystem.readFile("/post-fault", { encoding: "utf8" }),
        "post-fault-value",
      );
    const branch = await filesystem.branches.open("fault-branch");
    assert.equal(
      await branch.readFile("/branch", { encoding: "utf8" }),
      "branch-only-value",
    );
    await branch.close();
    verifyUsage(database);
    return Object.freeze({
      ...metrics,
      resultCounters: snapshotCounters(completed),
    });
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
}

async function collectionScenario(fault, { mutateAfterFault = false } = {}) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-gc-fault-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    await setupCollection(filename);
    database = await openNodeSqlite({ filename, create: false });
    const injector = maintenanceFaultInjector(database);
    filesystem = await EphemeralFS.open({
      database: injector.driver,
      storage: storageOptions,
      clock: () => 10,
    });
    injector.arm(fault ?? { afterStatement: Number.MAX_SAFE_INTEGER });
    let interrupted = false;
    try {
      await driveCollection(filesystem, "fault-gc");
    } catch (error) {
      interrupted = true;
      assert.equal(error.name, "AbortError");
    }
    const metrics = injector.metrics();
    if (fault) assert.equal(interrupted, true);
    injector.disarm();
    const faultOrdinal = fault?.afterStatement ?? fault?.afterBatch;
    const replacedKept =
      mutateAfterFault && faultOrdinal !== undefined && faultOrdinal % 2 === 0;
    if (fault && mutateAfterFault)
      await filesystem.writeFile(
        replacedKept ? "/kept" : "/post-fault",
        replacedKept ? "replacement-kept-value" : "post-fault-value",
      );
    const before = stateSnapshot(database, "fault-gc");
    verifyUsage(database);
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    const reopened = stateSnapshot(database, "fault-gc");
    assert.deepEqual(reopened, before);
    filesystem = await EphemeralFS.open({
      database,
      storage: storageOptions,
      clock: () => 10,
    });
    const { result, totalBatches } = await driveCollection(filesystem, "fault-gc");
    assert.equal(result.state, "complete");
    assert.ok(totalBatches < 2000);
    // A fault may land on the terminal commit. The deliberate post-fault root
    // mutation then belongs to the next generation, so a fresh run must clean
    // its journal entry rather than rewriting the already-terminal run.
    if (fault && mutateAfterFault) {
      const followup = await driveCollection(filesystem, "fault-gc-followup");
      assert.equal(followup.result.state, "complete");
      assert.ok(followup.totalBatches < 2000);
    }
    assert.equal(
      await filesystem.readFile("/kept", { encoding: "utf8" }),
      replacedKept ? "replacement-kept-value" : "reachable-value",
    );
    if (fault && mutateAfterFault && !replacedKept)
      assert.equal(
        await filesystem.readFile("/post-fault", { encoding: "utf8" }),
        "post-fault-value",
      );
    const terminal = database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_gc_marks) marks,(SELECT count(*) FROM efs_root_journal) journals,(SELECT count(*) FROM efs_gc_runs WHERE id='prior-terminal') prior,(SELECT count(*) FROM efs_lease_cleanups) lease_cleanups",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    assert.deepEqual(terminal, {
      marks: 0,
      journals: 0,
      prior: 0,
      lease_cleanups: 0,
    });
    verifyUsage(database);
    return Object.freeze({
      ...metrics,
      resultCounters: collectionCounters(result),
    });
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
}

async function abandonedScenario(fault) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-abandoned-fault-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    await setupAbandoned(filename);
    database = await openNodeSqlite({ filename, create: false });
    const injector = maintenanceFaultInjector(database);
    filesystem = await EphemeralFS.open({
      database: injector.driver,
      storage: storageOptions,
    });
    injector.arm(fault ?? { afterStatement: Number.MAX_SAFE_INTEGER });
    let interrupted = false;
    try {
      await driveCollection(filesystem, "abandoned-fault");
    } catch (error) {
      interrupted = true;
      assert.equal(error.name, "AbortError");
    }
    const metrics = injector.metrics();
    if (fault) assert.equal(interrupted, true);
    injector.disarm();
    const before = stateSnapshot(database, "abandoned-fault");
    verifyUsage(database);
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    assert.deepEqual(stateSnapshot(database, "abandoned-fault"), before);
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    const { result, totalBatches } = await driveCollection(
      filesystem,
      "abandoned-fault",
    );
    assert.equal(result.state, "complete");
    assert.ok(totalBatches < 100);
    assert.equal(
      await filesystem.readFile("/kept", { encoding: "utf8" }),
      "abandoned-kept-value",
    );
    assert.equal(
      database.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT count(*) count FROM efs_gc_marks WHERE run_id='abandoned-fault'",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0].count,
      ),
      0,
    );
    verifyUsage(database);
    return metrics;
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
}

test(
  "storage snapshots physically reopen and resume after every durable statement and batch",
  { timeout: 600_000 },
  async (t) => {
    const probe = await snapshotScenario();
    t.diagnostic(
      `snapshot fault positions: ${probe.durableStatements} statements, ${probe.committedBatches} batches, max ${probe.maxBatchStatements} statements/batch`,
    );
    assert.ok(probe.durableStatements > 0);
    assert.ok(probe.committedBatches > 0);
    assert.equal(probe.durableStatements, 110);
    assert.equal(probe.committedBatches, 42);
    assert.ok(probe.trace.some(({ sql }) => sql.includes("efs_storage_marks")));
    assert.deepEqual(
      probe.trace.map(({ statement }) => statement),
      Array.from({ length: probe.durableStatements }, (_, index) => index + 1),
    );
    for (let ordinal = 1; ordinal <= probe.durableStatements; ordinal += 1) {
      const resumed = await snapshotScenario({ afterStatement: ordinal });
      assert.deepEqual(resumed.resultCounters, probe.resultCounters);
    }
    for (let ordinal = 1; ordinal <= probe.committedBatches; ordinal += 1) {
      const resumed = await snapshotScenario({ afterBatch: ordinal });
      assert.deepEqual(resumed.resultCounters, probe.resultCounters);
    }
    await snapshotScenario({ afterBatch: 1 }, { mutateAfterFault: true });
  },
);

test(
  "collection and cleanup physically reopen after every durable statement and batch",
  { timeout: 600_000 },
  async (t) => {
    const probe = await collectionScenario();
    t.diagnostic(
      `collection fault positions: ${probe.durableStatements} statements, ${probe.committedBatches} batches, max ${probe.maxBatchStatements} statements/batch`,
    );
    assert.ok(probe.durableStatements > 0);
    assert.ok(probe.committedBatches > 0);
    assert.equal(probe.durableStatements, 154);
    assert.equal(probe.committedBatches, 72);
    for (const fragment of [
      "efs_gc_marks",
      "efs_root_journal",
      "efs_lease_cleanups",
      "efs_gc_runs",
    ])
      assert.ok(
        probe.trace.some(({ sql }) => sql.includes(fragment)),
        fragment,
      );
    assert.deepEqual(
      probe.trace.map(({ statement }) => statement),
      Array.from({ length: probe.durableStatements }, (_, index) => index + 1),
    );
    for (let ordinal = 1; ordinal <= probe.durableStatements; ordinal += 1) {
      const resumed = await collectionScenario({ afterStatement: ordinal });
      assert.deepEqual(resumed.resultCounters, probe.resultCounters);
    }
    for (let ordinal = 1; ordinal <= probe.committedBatches; ordinal += 1) {
      const resumed = await collectionScenario({ afterBatch: ordinal });
      assert.deepEqual(resumed.resultCounters, probe.resultCounters);
    }
    await collectionScenario({ afterBatch: 1 }, { mutateAfterFault: true });
  },
);

test(
  "abandoned-run reclamation reopens at every durable cleanup boundary",
  { timeout: 300_000 },
  async (t) => {
    const probe = await abandonedScenario();
    t.diagnostic(
      `abandoned-run fault positions: ${probe.durableStatements} statements, ${probe.committedBatches} batches`,
    );
    assert.deepEqual(
      probe.trace.map(({ statement }) => statement),
      Array.from({ length: probe.durableStatements }, (_, index) => index + 1),
    );
    assert.equal(probe.durableStatements, 61);
    assert.equal(probe.committedBatches, 33);
    for (let ordinal = 1; ordinal <= probe.durableStatements; ordinal += 1)
      await abandonedScenario({ afterStatement: ordinal });
    for (let ordinal = 1; ordinal <= probe.committedBatches; ordinal += 1)
      await abandonedScenario({ afterBatch: ordinal });
  },
);

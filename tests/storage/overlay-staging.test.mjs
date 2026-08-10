import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import {
  AdmissionController,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentCache } from "../../packages/fs/dist/cache/content-cache.js";
import { prepareContent } from "../../packages/fs/dist/operations/manifest-io.js";
import { MaintenanceManager } from "../../packages/fs/dist/operations/maintenance.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { OverlayRepository } from "../../packages/fs/dist/sqlite/overlay-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";

function limits(driver) {
  return constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
      maxBranchOverlayBytes: 32 * 1024 * 1024,
    },
    driver.capabilities,
  );
}
function createBranch(driver, id = "branch") {
  driver.transaction("write", (tx) => {
    tx.run("INSERT INTO efs_branch_ids(id,created_at_ms) VALUES(?,0)", [id]);
    tx.run(
      "INSERT INTO efs_branches(id,base_revision,state,generation,created_at_ms,terminal_at_ms) VALUES(?,0,0,0,0,NULL)",
      [id],
    );
  });
}

test("immutable COW heads retain one current page and atomically cross boundaries at every page size", async () => {
  for (const pageBytes of [4096, 8192, 16384]) {
    const driver = await openNodeSqlite({ filename: ":memory:" });
    initializeOrValidateSchema(driver, { cowPageBytes: pageBytes });
    createBranch(driver);
    const storage = limits(driver);
    for (let iteration = 0; iteration < 1000; iteration += 1)
      driver.transaction("write", (tx) =>
        new OverlayRepository(tx, storage, pageBytes).writePages(
          "branch",
          "inode",
          pageBytes * 2,
          [{ index: 0, bytes: new Uint8Array(pageBytes).fill(iteration & 0xff) }],
          iteration,
        ),
      );
    let state = driver.transaction("read", (tx) => ({
      versions: tx.all("SELECT count(*) count FROM efs_cow_page_versions", [], {
        maxRows: 1,
        maxBytes: 100,
      })[0].count,
      heads: tx.all("SELECT count(*) count FROM efs_cow_page_heads", [], {
        maxRows: 1,
        maxBytes: 100,
      })[0].count,
      usage: tx.all("SELECT page_count,page_bytes FROM efs_usage", [], {
        maxRows: 1,
        maxBytes: 100,
      })[0],
    }));
    assert.equal(state.versions, 1);
    assert.equal(state.heads, 1);
    assert.deepEqual(state.usage, { page_count: 1, page_bytes: pageBytes });
    driver.transaction("write", (tx) =>
      new OverlayRepository(tx, storage, pageBytes).writePages(
        "branch",
        "crossing",
        pageBytes + 17,
        [
          { index: 0, bytes: new Uint8Array(pageBytes).fill(1) },
          { index: 1, bytes: new Uint8Array(17).fill(2) },
        ],
        1001,
      ),
    );
    state = driver.transaction("read", (tx) => ({
      versions: tx.all(
        "SELECT count(*) count FROM efs_cow_page_versions WHERE inode_id='crossing'",
        [],
        { maxRows: 1, maxBytes: 100 },
      )[0].count,
      heads: tx.all(
        "SELECT count(*) count FROM efs_cow_page_heads WHERE inode_id='crossing'",
        [],
        { maxRows: 1, maxBytes: 100 },
      )[0].count,
    }));
    assert.deepEqual(state, { versions: 2, heads: 2 });
    assert.throws(
      () =>
        driver.transaction("write", (tx) =>
          new OverlayRepository(tx, storage, pageBytes).writePages(
            "branch",
            "bad",
            pageBytes + 1,
            [{ index: 1, bytes: new Uint8Array(pageBytes) }],
            1002,
          ),
        ),
      /exact logical length/,
    );
    driver.close();
  }
});

test("structural patches are segmented, ordered, bounded, and exact", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const metadata = initializeOrValidateSchema(driver);
  createBranch(driver);
  const storage = limits(driver);
  driver.transaction("write", (tx) =>
    new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch(
      "branch",
      "inode",
      10,
      4,
      2,
      [Uint8Array.of(1, 2), Uint8Array.of(3)],
    ),
  );
  driver.transaction("write", (tx) =>
    new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch(
      "branch",
      "inode",
      11,
      11,
      0,
      [],
    ),
  );
  const patches = driver.transaction("read", (tx) =>
    new OverlayRepository(tx, storage, metadata.cowPageBytes).patches(
      "branch",
      "inode",
    ),
  );
  assert.deepEqual(
    patches.map((patch) => ({
      sequence: patch.sequence,
      offset: patch.offset,
      deleteLength: patch.deleteLength,
      insertLength: patch.insertLength,
      segments: patch.segments.map((value) => [...value]),
    })),
    [
      {
        sequence: 0,
        offset: 4,
        deleteLength: 2,
        insertLength: 3,
        segments: [[1, 2], [3]],
      },
      { sequence: 1, offset: 11, deleteLength: 0, insertLength: 0, segments: [] },
    ],
  );
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch(
          "branch",
          "inode",
          11,
          12,
          0,
          [],
        ),
      ),
    /outside/,
  );
  driver.close();
});

test("byte-weighted cache verifies once, remains bounded, and eviction preserves integrity checks", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const admission = new AdmissionController(1024 * 1024);
  const cache = new ContentCache(128 * 1024, admission);
  const bytes = Uint8Array.from({ length: 32 * 1024 }, (_, index) => index & 0xff);
  const hash = sha256(bytes);
  driver.transaction("write", (tx) =>
    assert.equal(
      new ContentRepository(tx, storage, cache).putObject(hash, bytes),
      true,
    ),
  );
  assert.deepEqual(
    driver.transaction("read", (tx) =>
      new ContentRepository(tx, storage, cache).getObject(hash, bytes.length),
    ),
    bytes,
  );
  assert.deepEqual(
    driver.transaction("read", (tx) =>
      new ContentRepository(tx, storage, cache).getObject(hash, bytes.length),
    ),
    bytes,
  );
  assert.ok(cache.metrics().hits >= 1);
  assert.ok(cache.metrics().bytes <= 128 * 1024);
  cache.clear();
  assert.equal(admission.usedBytes, 0);
  driver.transaction("write", (tx) =>
    tx.run("UPDATE efs_cas_objects SET bytes=zeroblob(size) WHERE hash=?", [hash]),
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        new ContentRepository(tx, storage, cache).getObject(hash, bytes.length),
      ),
    /digest mismatch/,
  );
  cache.clear();
  driver.close();
});

test("partial write-admission failure removes its staging lease and releases every reservation", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const releasePressure = admission.reserve(120 * 1024 * 1024);
  await assert.rejects(
    prepareContent(port, Uint8Array.of(1), storage, DEFAULT_RUNTIME_LIMITS, admission),
    /managed resident memory limit/,
  );
  releasePressure();
  assert.equal(admission.usedBytes, 0);
  const active = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) active,(SELECT count(*) FROM efs_leases WHERE state=2) tombstoned,(SELECT count(*) FROM efs_lease_cleanups) cleanups",
        [],
        {
          maxRows: 1,
          maxBytes: 128,
        },
      )[0],
  );
  assert.deepEqual(active, { active: 0, tombstoned: 1, cleanups: 1 });
  driver.close();
});

test("a huge upstream stream chunk is admitted at its full size, rejected before processing, and cancelled cleanly", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const runtime = {
    ...DEFAULT_RUNTIME_LIMITS,
    maxManagedResidentBytes: 4 * 1024 * 1024,
    maxPendingWriteBytes: 1024 * 1024,
    maxWriteSessionBytes: 256 * 1024,
    maxQueryBatchBytes: 128 * 1024,
  };
  const admission = new AdmissionController(runtime.maxManagedResidentBytes);
  let cancelled = false;
  let cancellationReason;
  const stream = new ReadableStream({
    pull(controller) {
      controller.enqueue(new Uint8Array(3 * 1024 * 1024));
    },
    cancel(reason) {
      cancelled = true;
      cancellationReason = reason;
    },
  });
  await assert.rejects(
    prepareContent(
      port,
      stream,
      storage,
      runtime,
      admission,
      undefined,
      undefined,
      () => 7,
    ),
    /managed resident memory limit/,
  );
  assert.equal(cancelled, true);
  assert.ok(cancellationReason instanceof RangeError);
  assert.equal(admission.usedBytes, 0);
  const state = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) active,(SELECT count(*) FROM efs_leases WHERE state=2) tombstoned,(SELECT count(*) FROM efs_lease_cleanups) cleanups,staging_bytes FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  assert.deepEqual(state, {
    active: 0,
    tombstoned: 1,
    cleanups: 1,
    staging_bytes: 0,
  });
  driver.close();
});

test("staging payload quota is exact across rollback, release, and reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-stage-quota-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    const storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 16 * 1024 * 1024,
        maintenanceReserveBytes: 1024,
        maxStagingPayloadBytes: 8,
      },
      driver.capabilities,
    );
    const nonce = new Uint8Array(16).fill(1);
    const first = new Uint8Array(8).fill(2);
    const firstHash = sha256(first);
    driver.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "quota",
        ownerId: "owner",
        ownerNonce: nonce,
        now: 1,
        expiresAt: 100,
      });
      new ContentRepository(tx, storage).putObject(firstHash, first);
      staging.appendBatch("quota", nonce, [
        { kind: "object", hash: firstHash, size: first.length },
      ]);
    });
    const second = new Uint8Array(1).fill(3);
    const secondHash = sha256(second);
    assert.throws(
      () =>
        driver.transaction("write", (tx) => {
          new ContentRepository(tx, storage).putObject(secondHash, second);
          new StagingRepository(tx, storage).appendBatch("quota", nonce, [
            { kind: "object", hash: secondHash, size: second.length },
          ]);
        }),
      /staging payload quota/,
    );
    let state = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT object_count,staging_bytes,(SELECT count(*) FROM efs_lease_objects WHERE lease_id='quota') members FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    assert.deepEqual(state, { object_count: 1, staging_bytes: 8, members: 1 });
    driver.transaction("write", (tx) =>
      assert.equal(
        new StagingRepository(tx, storage).release("quota", nonce, false),
        true,
      ),
    );
    driver.close();
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    state = driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT staging_bytes FROM efs_usage", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0],
    );
    assert.equal(state.staging_bytes, 0);
    driver.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("maintenance expiry atomically releases partial and sealed staging charges after reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-stage-expiry-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    let port = createSqliteOperationsStorage(driver);
    initializeOrValidateSchema(driver);
    const storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 32 * 1024 * 1024,
        maintenanceReserveBytes: 1024,
        maxStagingPayloadBytes: 1024 * 1024,
        stagingLeaseMs: 10,
      },
      driver.capabilities,
    );
    const partialNonce = new Uint8Array(16).fill(4);
    const partial = Uint8Array.of(1, 2, 3, 4);
    const partialHash = sha256(partial);
    driver.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "partial",
        ownerId: "owner",
        ownerNonce: partialNonce,
        now: 1,
        expiresAt: 11,
      });
      new ContentRepository(tx, storage).putObject(partialHash, partial);
      staging.appendBatch("partial", partialNonce, [
        { kind: "object", hash: partialHash, size: partial.length },
      ]);
    });
    const admission = new AdmissionController(
      DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
    );
    await prepareContent(
      port,
      Uint8Array.of(5, 6, 7),
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      undefined,
      undefined,
      () => 1,
    );
    assert.equal(admission.usedBytes, 0);
    const before = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT staging_bytes,(SELECT count(*) FROM efs_leases) leases FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    assert.equal(before.leases, 2);
    assert.ok(before.staging_bytes > partial.length);
    driver.close();
    driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    port = createSqliteOperationsStorage(driver);
    initializeOrValidateSchema(driver);
    const maintenance = new MaintenanceManager(
      port,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      () => 100,
    );
    const zero = await maintenance.collectGarbage({
      runId: "expiry-accounting",
      maxBatches: 0,
    });
    assert.equal(zero.committedBatches, 0);
    const untouched = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT staging_bytes,(SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_lease_cleanups) cleanups,(SELECT count(*) FROM efs_gc_runs) runs FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    assert.deepEqual(untouched, {
      staging_bytes: before.staging_bytes,
      leases: before.leases,
      cleanups: 0,
      runs: 0,
    });
    const collected = await maintenance.collectGarbage({
      runId: "expiry-accounting",
      maxBatches: 100,
    });
    assert.equal(collected.state, "complete");
    const after = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT staging_bytes,(SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_staging_certificates) certificates,(SELECT count(*) FROM efs_lease_objects) object_members,(SELECT count(*) FROM efs_lease_staged_manifests) manifest_members,(SELECT count(*) FROM efs_staging_reconciliation_queue) queued FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    assert.deepEqual(after, {
      staging_bytes: 0,
      leases: 0,
      certificates: 0,
      object_members: 0,
      manifest_members: 0,
      queued: 0,
    });
    driver.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("every expired-lease tombstone statement fault rolls back lease state and usage", async () => {
  async function fixture(failAt, counter) {
    const base = await openNodeSqlite({ filename: ":memory:" });
    initializeOrValidateSchema(base);
    const storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 16 * 1024 * 1024,
        maintenanceReserveBytes: 1024,
        maxStagingPayloadBytes: 1024,
      },
      base.capabilities,
    );
    const nonce = new Uint8Array(16).fill(8);
    const bytes = Uint8Array.of(9, 9, 9);
    const hash = sha256(bytes);
    base.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "expired",
        ownerId: "owner",
        ownerNonce: nonce,
        now: 1,
        expiresAt: 2,
      });
      new ContentRepository(tx, storage).putObject(hash, bytes);
      staging.appendBatch("expired", nonce, [
        { kind: "object", hash, size: bytes.length },
      ]);
    });
    const wrapped = {
      kind: base.kind,
      readOnly: base.readOnly,
      capabilities: base.capabilities,
      close: () => base.close(),
      transaction(mode, callback) {
        return base.transaction(mode, (tx) =>
          callback({
            scope: tx.scope,
            run(...args) {
              counter.value += 1;
              if (counter.value === failAt) throw new Error(`expiry fault ${failAt}`);
              return tx.run(...args);
            },
            all(...args) {
              counter.value += 1;
              if (counter.value === failAt) throw new Error(`expiry fault ${failAt}`);
              return tx.all(...args);
            },
          }),
        );
      },
    };
    return { base, wrapped, storage };
  }
  const probeCount = { value: 0 };
  const probe = await fixture(Number.POSITIVE_INFINITY, probeCount);
  runUnitOfWork(
    probe.wrapped,
    "write",
    { maxRows: 1000, maxBytes: 1024 * 1024 },
    (tx) => new StagingRepository(tx, probe.storage).expireBatch(3, 10),
  );
  probe.base.close();
  assert.ok(probeCount.value >= 6);
  for (let failAt = 1; failAt <= probeCount.value; failAt += 1) {
    const count = { value: 0 };
    const { base, wrapped, storage } = await fixture(failAt, count);
    assert.throws(
      () =>
        runUnitOfWork(
          wrapped,
          "write",
          { maxRows: 1000, maxBytes: 1024 * 1024 },
          (tx) => new StagingRepository(tx, storage).expireBatch(3, 10),
        ),
      new RegExp(`expiry fault ${failAt}`),
    );
    const state = base.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT staging_bytes,(SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_lease_objects) members FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    assert.deepEqual(state, { staging_bytes: 3, leases: 1, members: 1 });
    base.close();
  }
});

test("every keyset cleanup statement fault rolls back its child deletion and cursor", async () => {
  async function fixture(failAt, counter) {
    const base = await openNodeSqlite({ filename: ":memory:" });
    initializeOrValidateSchema(base);
    const storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 16 * 1024 * 1024,
        maintenanceReserveBytes: 1024,
        maxStagingPayloadBytes: 1024,
        maxGcBatchSize: 2,
        maxQueryBatchSize: 2,
      },
      base.capabilities,
    );
    const nonce = new Uint8Array(16).fill(5);
    const bytes = Uint8Array.of(7);
    const hash = sha256(bytes);
    base.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "cleanup-fault",
        ownerId: "owner",
        ownerNonce: nonce,
        now: 1,
        expiresAt: 100,
      });
      new ContentRepository(tx, storage).putObject(hash, bytes);
      staging.putEntry("cleanup-fault", 0, hash, 1);
      staging.appendBatch("cleanup-fault", nonce, [{ kind: "object", hash, size: 1 }]);
      staging.release("cleanup-fault", nonce, false);
    });
    const wrapped = {
      kind: base.kind,
      readOnly: base.readOnly,
      capabilities: base.capabilities,
      close: () => base.close(),
      transaction(mode, callback) {
        return base.transaction(mode, (tx) =>
          callback({
            scope: tx.scope,
            run(...args) {
              counter.value += 1;
              if (counter.value === failAt) throw new Error(`cleanup fault ${failAt}`);
              return tx.run(...args);
            },
            all(...args) {
              counter.value += 1;
              if (counter.value === failAt) throw new Error(`cleanup fault ${failAt}`);
              return tx.all(...args);
            },
          }),
        );
      },
    };
    return { base, wrapped, storage };
  }
  const probeCount = { value: 0 };
  const probe = await fixture(Number.POSITIVE_INFINITY, probeCount);
  runUnitOfWork(probe.wrapped, "write", { maxRows: 100, maxBytes: 1024 * 1024 }, (tx) =>
    new StagingRepository(tx, probe.storage).cleanupBatch(2),
  );
  probe.base.close();
  assert.ok(probeCount.value >= 3);
  for (let failAt = 1; failAt <= probeCount.value; failAt += 1) {
    const count = { value: 0 };
    const { base, wrapped, storage } = await fixture(failAt, count);
    assert.throws(
      () =>
        runUnitOfWork(wrapped, "write", { maxRows: 100, maxBytes: 1024 * 1024 }, (tx) =>
          new StagingRepository(tx, storage).cleanupBatch(2),
        ),
      new RegExp(`cleanup fault ${failAt}`),
    );
    const state = base.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_leases WHERE id='cleanup-fault') leases,(SELECT count(*) FROM efs_staging_entries WHERE lease_id='cleanup-fault') entries,(SELECT count(*) FROM efs_lease_objects WHERE lease_id='cleanup-fault') members,(SELECT phase FROM efs_lease_cleanups WHERE lease_id='cleanup-fault') phase",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    assert.deepEqual(state, { leases: 1, entries: 1, members: 1, phase: 0 });
    base.close();
  }
});

test("tombstoned leases clean up through resumable keyset-sized child batches", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 16 * 1024 * 1024,
      maintenanceReserveBytes: 1024,
      maxStagingPayloadBytes: 1024,
      maxGcBatchSize: 2,
      maxQueryBatchSize: 2,
    },
    driver.capabilities,
  );
  const nonce = new Uint8Array(16).fill(6);
  driver.transaction("write", (tx) =>
    new StagingRepository(tx, storage).begin({
      leaseId: "bounded-cleanup",
      ownerId: "owner",
      ownerNonce: nonce,
      now: 1,
      expiresAt: 100,
    }),
  );
  for (let index = 0; index < 5; index += 1) {
    const bytes = Uint8Array.of(index + 1);
    const hash = sha256(bytes);
    driver.transaction("write", (tx) => {
      new ContentRepository(tx, storage).putObject(hash, bytes);
      const staging = new StagingRepository(tx, storage);
      staging.putEntry("bounded-cleanup", index, hash, 1);
      staging.appendBatch("bounded-cleanup", nonce, [
        { kind: "object", hash, size: 1 },
      ]);
    });
  }
  driver.transaction("write", (tx) =>
    assert.equal(
      new StagingRepository(tx, storage).release("bounded-cleanup", nonce, false),
      true,
    ),
  );
  let batches = 0;
  while (true) {
    const before = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_leases WHERE id='bounded-cleanup') leases,(SELECT count(*) FROM efs_staging_entries WHERE lease_id='bounded-cleanup') entries,(SELECT count(*) FROM efs_lease_objects WHERE lease_id='bounded-cleanup') members,staging_bytes FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    if (!before.leases) {
      assert.equal(before.entries, 0);
      assert.equal(before.members, 0);
      assert.equal(before.staging_bytes, 0);
      break;
    }
    const progress = driver.transaction("write", (tx) =>
      new StagingRepository(tx, storage).cleanupBatch(2),
    );
    assert.equal(progress.worked, true);
    assert.ok(progress.deletedRows <= 2);
    batches += 1;
    assert.ok(batches < 32, "cleanup did not make bounded progress");
  }
  assert.ok(batches > 2, "fixture should require resumable cleanup");
  driver.close();
});

test("lease maintenance observes aborts between bounded committed batches", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 16 * 1024 * 1024,
      maintenanceReserveBytes: 1024,
      maxStagingPayloadBytes: 1024,
      maxGcBatchSize: 1,
      maxQueryBatchSize: 2,
    },
    driver.capabilities,
  );
  driver.transaction("write", (tx) => {
    const staging = new StagingRepository(tx, storage);
    for (let index = 0; index < 3; index += 1)
      staging.begin({
        leaseId: `abort-${index}`,
        ownerId: "owner",
        ownerNonce: new Uint8Array(16).fill(index + 1),
        now: 1,
        expiresAt: 2,
      });
  });
  const maintenance = new MaintenanceManager(
    port,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    () => 10,
  );
  let checks = 0;
  const signal = {
    get aborted() {
      checks += 1;
      return checks >= 3;
    },
  };
  await assert.rejects(
    maintenance.collectGarbage({
      runId: "bounded-abort",
      maxBatches: 100,
      signal,
    }),
    /aborted/i,
  );
  const state = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) active,(SELECT count(*) FROM efs_leases WHERE state=2) tombstoned,(SELECT count(*) FROM efs_lease_cleanups) cleanups,(SELECT count(*) FROM efs_gc_runs) runs",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  assert.deepEqual(state, { active: 2, tombstoned: 1, cleanups: 1, runs: 0 });
  driver.close();
});

test(
  "a genuine 100001-entry manifest closure reconciles durably and final-validates with constant-row work",
  { timeout: 120_000 },
  async (t) => {
    const driver = await openNodeSqlite({
      filename: ":memory:",
      durability: "relaxed-test",
    });
    initializeOrValidateSchema(driver);
    const storage = limits(driver);
    const leaseId = "large-stage";
    const nonce = Uint8Array.from({ length: 16 }, (_, index) => index + 1);
    const budget = {
      maxRows: storage.maxFinalTransactionRows,
      maxBytes: storage.maxFinalTransactionBytes,
    };
    runUnitOfWork(driver, "write", budget, (tx) =>
      new StagingRepository(tx, storage).begin({
        leaseId,
        ownerId: "test",
        ownerNonce: nonce,
        now: 1,
        expiresAt: 1_000_000,
      }),
    );
    const total = 100_001;
    const batchSize = 128;
    for (let start = 0; start < total; start += batchSize) {
      const batch = [];
      for (let index = start; index < Math.min(total, start + batchSize); index += 1) {
        const bytes = new Uint8Array(8);
        new DataView(bytes.buffer).setBigUint64(0, BigInt(index), true);
        batch.push({ hash: sha256(bytes), bytes });
      }
      runUnitOfWork(driver, "write", budget, (tx) => {
        const staging = new StagingRepository(tx, storage);
        new ContentRepository(tx, storage).putObjectsBatch(batch);
        for (let index = 0; index < batch.length; index += 1)
          staging.putEntry(
            leaseId,
            start + index,
            batch[index].hash,
            batch[index].bytes.length,
          );
        staging.appendBatch(
          leaseId,
          nonce,
          batch.map((item) => ({
            kind: "object",
            hash: item.hash,
            size: item.bytes.length,
          })),
        );
      });
    }
    const workspace = {
      writeNode(record) {
        runUnitOfWork(driver, "write", budget, (tx) => {
          const staging = new StagingRepository(tx, storage);
          new ContentRepository(tx, storage).putManifestNode(
            record.value.hash,
            record.value.encoded,
          );
          staging.putLevelRecord(
            leaseId,
            record.level,
            record.index,
            record.value.hash,
            record.child.span,
            record.child.entryCount,
          );
          staging.appendBatch(leaseId, nonce, [
            {
              kind: "manifest-node",
              hash: record.value.hash,
              size: record.value.encoded.length,
            },
          ]);
        });
      },
      readLevel(level, afterIndex, limit) {
        return runUnitOfWork(driver, "read", budget, (tx) =>
          new StagingRepository(tx, storage)
            .levelRecordsAfter(leaseId, level, afterIndex, limit, 1024 * 1024)
            .map((row) => ({
              index: row.record_index,
              child: {
                hash: row.node_hash,
                span: row.span,
                entryCount: row.entry_count,
              },
            })),
        );
      },
    };
    function* entries() {
      let cursor = -1;
      while (true) {
        const rows = runUnitOfWork(driver, "read", budget, (tx) =>
          new StagingRepository(tx, storage).entriesAfter(
            leaseId,
            cursor,
            batchSize,
            64 * 1024,
          ),
        );
        if (!rows.length) return;
        for (const row of rows) {
          cursor = row.entry_index;
          yield { hash: row.object_hash, length: row.length };
        }
      }
    }
    const built = buildManifestFromEntries(
      entries(),
      { minimum: 8, average: 8, maximum: 8 },
      workspace,
      { readBatchRecords: 31, maxDepth: storage.maxManifestDepth },
    );
    let reconciliationStatements = 0;
    const counted = {
      kind: driver.kind,
      readOnly: driver.readOnly,
      capabilities: driver.capabilities,
      close: () => {},
      transaction(mode, callback) {
        return driver.transaction(mode, (tx) =>
          callback({
            scope: tx.scope,
            run(...args) {
              reconciliationStatements += 1;
              return tx.run(...args);
            },
            all(...args) {
              reconciliationStatements += 1;
              return tx.all(...args);
            },
          }),
        );
      },
    };
    const certificate = runUnitOfWork(counted, "write", budget, (tx) => {
      const content = new ContentRepository(tx, storage);
      content.putManifestRoot(built.rootHash, built.root);
      const staging = new StagingRepository(tx, storage);
      staging.appendBatch(leaseId, nonce, [
        { kind: "manifest-root", hash: built.rootHash, size: built.root.length },
      ]);
      staging.beginReconciliation(leaseId, nonce, built.rootHash);
      return { ...staging.snapshot(leaseId, nonce), manifestHash: built.rootHash };
    });
    let complete = false;
    while (!complete)
      complete = runUnitOfWork(
        counted,
        "write",
        budget,
        (tx) =>
          new StagingRepository(tx, storage).reconcileBatch(
            leaseId,
            nonce,
            storage.maxQueryBatchSize,
          ).complete,
      );
    runUnitOfWork(counted, "write", budget, (tx) =>
      new StagingRepository(tx, storage).seal(certificate),
    );
    const reconciliation = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT object_count,node_count,membership_count,complete FROM efs_staging_reconciliations WHERE lease_id=?",
          [leaseId],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    let finalStatements = 0;
    const finalCounted = {
      ...counted,
      transaction(mode, callback) {
        return driver.transaction(mode, (tx) =>
          callback({
            scope: tx.scope,
            run(...args) {
              finalStatements += 1;
              return tx.run(...args);
            },
            all(...args) {
              finalStatements += 1;
              return tx.all(...args);
            },
          }),
        );
      },
    };
    runUnitOfWork(finalCounted, "read", budget, (tx) =>
      new StagingRepository(tx, storage).validateSealed(certificate, 2),
    );
    assert.equal(built.entryCount, total);
    assert.equal(certificate.objectCount, total);
    assert.deepEqual(reconciliation, {
      object_count: total,
      node_count: certificate.nodeCount,
      membership_count: certificate.membershipCount,
      complete: 1,
    });
    assert.equal(finalStatements, 1);
    assert.ok(
      reconciliationStatements < certificate.membershipCount * 8,
      `unexpected reconciliation SQL amplification: ${reconciliationStatements}`,
    );
    t.diagnostic(
      JSON.stringify({
        manifestEntries: total,
        uniqueClosureMembers: certificate.membershipCount,
        reconciliationStatements,
        statementsPerClosureMember:
          reconciliationStatements / certificate.membershipCount,
        finalValidationStatements: finalStatements,
      }),
    );
    driver.close();
  },
);

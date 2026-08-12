import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
} from "../../packages/fs/dist/manifests/codec.js";
import {
  AdmissionController,
  DEFAULT_FASTCDC_MAXIMUM_BYTES,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentCache } from "../../packages/fs/dist/cache/content-cache.js";
import { prepareContent } from "../../packages/fs/dist/operations/manifest-io.js";
import { prepareContentEntriesStreaming } from "../../packages/fs/dist/operations/streaming-prepare.js";
import { MaintenanceManager } from "../../packages/fs/dist/operations/maintenance.js";
import { BranchRepository } from "../../packages/fs/dist/sqlite/branch-repository.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { ManifestTreeRepository } from "../../packages/fs/dist/sqlite/manifest-tree-repository.js";
import { OverlayRepository } from "../../packages/fs/dist/sqlite/overlay-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import {
  CHARGED_ROW_BYTES,
  DIRECT_STAGING_BYTES_SQL,
  USAGE_COUNTER_COLUMNS,
  USAGE_RECOUNT_PHASE_COUNT,
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";

function limits(driver, overrides = {}) {
  return constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
      maxBranchOverlayBytes: 32 * 1024 * 1024,
      ...overrides,
    },
    driver.capabilities,
  );
}
function readObject(repository, hash, size) {
  const output = new Uint8Array(size);
  assert.equal(repository.readObjectInto(hash, size, 0, output, 0, size), true);
  return output;
}
function maintenanceCache() {
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  return new ContentCache(DEFAULT_RUNTIME_LIMITS.maxCacheBytes, admission);
}
function createBranch(driver, id = "branch") {
  driver.transaction("write", (tx) =>
    new BranchRepository(tx, limits(driver)).create(id, 0, 0),
  );
}
function verifyKeysetUsage(tx, storage) {
  const repository = new UsageRepository(tx, storage);
  const totals = USAGE_COUNTER_COLUMNS.map(() => 0);
  for (let phase = 0; phase < USAGE_RECOUNT_PHASE_COUNT; phase += 1) {
    let afterKey = null;
    for (;;) {
      const batch = repository.recountBatch(phase, afterKey, 7, 64 * 1024);
      for (let index = 0; index < totals.length; index += 1)
        totals[index] += batch.deltas[index];
      if (batch.complete) break;
      afterKey = batch.nextKey;
    }
  }
  const snapshot = repository.snapshot();
  assert.deepEqual(
    Object.fromEntries(
      USAGE_COUNTER_COLUMNS.map((column, index) => [column, totals[index]]),
    ),
    Object.fromEntries(
      USAGE_COUNTER_COLUMNS.map((column) => [column, snapshot[column]]),
    ),
  );
}

test("local fresh appends reject duplicates while generic appends retain probes", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  try {
    initializeOrValidateSchema(driver);
    const storage = limits(driver);
    const bytes = Uint8Array.of(1, 2, 3, 4);
    const hash = sha256(bytes);
    driver.transaction("write", (tx) =>
      new ContentRepository(tx, storage).putObject(hash, bytes),
    );

    const freshNonce = Uint8Array.from({ length: 16 }, (_, index) => index + 1);
    driver.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "fresh-append",
        ownerId: "fresh-append-owner",
        ownerNonce: freshNonce,
        now: 1,
        expiresAt: 100,
      });
      const member = { kind: "object", hash, size: bytes.length };
      staging.appendFreshBatch("fresh-append", freshNonce, [member]);
      assert.throws(
        () => staging.appendFreshBatch("fresh-append", freshNonce, [member]),
        /staging membership changed during batched insert/,
      );
      assert.throws(
        () =>
          staging.appendFreshBatch("fresh-append", freshNonce, [
            { ...member, counted: true },
          ]),
        /fresh local batch cannot contain count-only members/,
      );
    });

    const genericNonce = Uint8Array.from({ length: 16 }, (_, index) => index + 20);
    driver.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "generic-append",
        ownerId: "generic-append-owner",
        ownerNonce: genericNonce,
        now: 1,
        expiresAt: 100,
      });
      const member = { kind: "object", hash, size: bytes.length };
      staging.appendBatch("generic-append", genericNonce, [member]);
      staging.appendBatch("generic-append", genericNonce, [member]);
    });
    assert.equal(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT count(*) count FROM efs_lease_objects WHERE lease_id=?",
            ["generic-append"],
            { maxRows: 1, maxBytes: 128 },
          )[0].count,
      ),
      1,
    );

    const invalidNonce = Uint8Array.from({ length: 16 }, (_, index) => index + 40);
    driver.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "generic-invalid",
        ownerId: "generic-invalid-owner",
        ownerNonce: invalidNonce,
        now: 1,
        expiresAt: 100,
      });
      assert.throws(
        () =>
          staging.appendBatch("generic-invalid", invalidNonce, [
            { kind: "object", hash, size: bytes.length + 1 },
          ]),
        /staged membership does not match immutable content/,
      );
    });
  } finally {
    driver.close();
  }
});

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
    driver.transaction("read", (tx) => verifyKeysetUsage(tx, storage));
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
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        tx.run(
          "INSERT INTO efs_patches(branch_id,inode_id,sequence,generation,offset,delete_length,insert_length) VALUES('branch','inode',3,3,0,0,0)",
        ),
      ),
    /contiguous/,
  );
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        tx.run(
          "UPDATE efs_patches SET sequence=4 WHERE branch_id='branch' AND inode_id='inode' AND sequence=0",
        ),
      ),
    /immutable/,
  );
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        tx.run(
          "DELETE FROM efs_patches WHERE branch_id='branch' AND inode_id='inode' AND sequence=0",
        ),
      ),
    /immutable/,
  );
  assert.equal(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_patches WHERE branch_id='branch' AND inode_id='inode'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    2,
  );
  driver.transaction("read", (tx) => verifyKeysetUsage(tx, storage));
  driver.close();
});

test("structural patch segment envelopes persist exactly and reject plus one before writes", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-patch-segments-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    const metadata = initializeOrValidateSchema(driver);
    createBranch(driver);
    const storage = limits(driver, { maxPatchesPerFile: 2 });
    const exact = Array.from({ length: 64 }, () => Uint8Array.of(1));
    driver.transaction("write", (tx) =>
      new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch(
        "branch",
        "inode",
        0,
        0,
        0,
        exact,
      ),
    );
    assert.throws(
      () =>
        driver.transaction("write", (tx) =>
          new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch(
            "branch",
            "other-inode",
            0,
            0,
            0,
            [...exact, Uint8Array.of(2)],
          ),
        ),
      /segment limit/,
    );
    assert.equal(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT count(*) count FROM efs_patches WHERE inode_id='other-inode'",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0].count,
      ),
      0,
    );
    driver.close();
    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver);
    const reopened = driver.transaction("read", (tx) =>
      new OverlayRepository(tx, storage, metadata.cowPageBytes).patches(
        "branch",
        "inode",
      ),
    );
    assert.equal(reopened.length, 1);
    assert.equal(reopened[0].segments.length, 64);
    assert.equal(reopened[0].insertLength, 64);
    driver.close();
    driver = undefined;
  } finally {
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("tight row profiles persist only patch sets their bounded reader can materialize", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-patch-row-envelope-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  let port;
  try {
    driver = await openNodeSqlite({ filename });
    const storage = limits(driver, {
      maxFinalTransactionRows: 64,
      maxPatchesPerFile: 40,
    });
    port = createSqliteOperationsStorage(driver);
    const metadata = port.initialize();
    port.transaction(
      "write",
      { maxRows: 64, maxBytes: storage.maxFinalTransactionBytes },
      (tx) => tx.branches(storage).create("tight", 0, 0),
    );
    for (let index = 0; index < 32; index += 1)
      port.transaction(
        "write",
        { maxRows: 64, maxBytes: storage.maxFinalTransactionBytes },
        (tx) =>
          tx
            .overlay(storage, metadata.cowPageBytes)
            .appendPatch("tight", "inode", index, index, 0, [Uint8Array.of(index)]),
      );
    assert.equal(
      port.transaction(
        "read",
        { maxRows: 64, maxBytes: storage.maxFinalTransactionBytes },
        (tx) =>
          tx.overlay(storage, metadata.cowPageBytes).patches("tight", "inode").length,
      ),
      32,
    );
    assert.throws(
      () =>
        port.transaction(
          "write",
          { maxRows: 64, maxBytes: storage.maxFinalTransactionBytes },
          (tx) =>
            tx
              .overlay(storage, metadata.cowPageBytes)
              .appendPatch("tight", "inode", 32, 32, 0, [Uint8Array.of(33)]),
        ),
      /requires materialization/,
    );
    await port.close();
    port = undefined;
    driver = await openNodeSqlite({ filename, create: false });
    port = createSqliteOperationsStorage(driver);
    port.initialize();
    assert.equal(
      port.transaction(
        "read",
        { maxRows: 64, maxBytes: storage.maxFinalTransactionBytes },
        (tx) =>
          tx.overlay(storage, metadata.cowPageBytes).patches("tight", "inode").length,
      ),
      32,
    );
  } finally {
    try {
      await port?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("patch payload plus row and binding overhead is exact across reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-patch-byte-envelope-"));
  const filename = path.join(directory, "filesystem.db");
  const finalBytes = DEFAULT_FASTCDC_MAXIMUM_BYTES + 16 * 1024;
  const segmentCount = 32;
  const payloadBytes = finalBytes - 16 * 1024 - segmentCount * 272;
  const segmented = (total) => {
    const base = Math.floor(total / segmentCount);
    let remaining = total;
    return Array.from({ length: segmentCount }, () => {
      const size = Math.min(base + (remaining % segmentCount ? 1 : 0), remaining);
      remaining -= size;
      return new Uint8Array(size).fill(23);
    });
  };
  let driver;
  let port;
  try {
    driver = await openNodeSqlite({ filename });
    const storage = limits(driver, {
      maxFinalTransactionBytes: finalBytes,
      maxPatchBytesPerFile: payloadBytes + 1,
      maxPatchesPerFile: 1,
    });
    port = createSqliteOperationsStorage(driver);
    const metadata = port.initialize();
    port.transaction(
      "write",
      { maxRows: storage.maxFinalTransactionRows, maxBytes: finalBytes },
      (tx) => tx.branches(storage).create("bytes", 0, 0),
    );
    const exact = segmented(payloadBytes);
    port.transaction(
      "write",
      { maxRows: storage.maxFinalTransactionRows, maxBytes: finalBytes },
      (tx) =>
        tx
          .overlay(storage, metadata.cowPageBytes)
          .appendPatch("bytes", "inode", 0, 0, 0, exact),
    );
    const plusOne = segmented(payloadBytes + 1);
    assert.throws(
      () =>
        port.transaction(
          "write",
          { maxRows: storage.maxFinalTransactionRows, maxBytes: finalBytes },
          (tx) =>
            tx
              .overlay(storage, metadata.cowPageBytes)
              .appendPatch("bytes", "other", 0, 0, 0, plusOne),
        ),
      /requires materialization/,
    );
    assert.equal(
      port.transaction(
        "read",
        { maxRows: storage.maxFinalTransactionRows, maxBytes: finalBytes },
        (tx) =>
          tx.overlay(storage, metadata.cowPageBytes).patches("bytes", "inode")[0]
            .insertLength,
      ),
      payloadBytes,
    );
    await port.close();
    port = undefined;
    driver = await openNodeSqlite({ filename, create: false });
    port = createSqliteOperationsStorage(driver);
    port.initialize();
    assert.equal(
      port.transaction(
        "read",
        { maxRows: storage.maxFinalTransactionRows, maxBytes: finalBytes },
        (tx) =>
          tx.overlay(storage, metadata.cowPageBytes).patches("bytes", "inode")[0]
            .segments.length,
      ),
      segmentCount,
    );
  } finally {
    try {
      await port?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("bounded usage recount derives patch bytes from physical segments after reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-patch-recount-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    const metadata = initializeOrValidateSchema(driver);
    createBranch(driver);
    let storage = limits(driver);
    driver.transaction("write", (tx) =>
      new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch(
        "branch",
        "inode",
        8,
        4,
        0,
        [Uint8Array.of(1, 2, 3)],
      ),
    );
    driver.close();
    driver = undefined;

    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver);
    driver.transaction("write", (tx) =>
      tx.run(
        "UPDATE efs_patch_segments SET bytes=? WHERE branch_id='branch' AND inode_id='inode' AND sequence=0 AND segment_index=0",
        [Uint8Array.of(1, 2, 3, 4)],
      ),
    );
    driver.close();
    driver = undefined;

    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver);
    storage = limits(driver);
    assert.throws(
      () =>
        driver.transaction("read", (tx) =>
          new UsageRepository(tx, storage).verifyDerivedUsage(),
        ),
      /patch_bytes differs from bounded direct recount/,
    );
  } finally {
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
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
  const first = driver.transaction("read", (tx) =>
    readObject(new ContentRepository(tx, storage, cache), hash, bytes.length),
  );
  assert.deepEqual(first, bytes);
  first.fill(0);
  assert.deepEqual(
    driver.transaction("read", (tx) =>
      readObject(new ContentRepository(tx, storage, cache), hash, bytes.length),
    ),
    bytes,
  );
  assert.ok(cache.metrics().hits >= 1);
  assert.ok(cache.metrics().bytes <= 128 * 1024);
  cache.clear();
  assert.equal(admission.usedBytes, 0);
  driver.transaction("write", (tx) =>
    tx.run("UPDATE efs_cas_objects SET bytes=? WHERE hash=?", [
      new Uint8Array(bytes.length),
      hash,
    ]),
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readObject(new ContentRepository(tx, storage, cache), hash, bytes.length),
      ),
    /digest mismatch/,
  );
  cache.clear();
  driver.close();
});

test("content cache owns Buffer and subclass inputs and detaches every outward hit", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const admission = new AdmissionController(1024 * 1024);
  const cache = new ContentCache(128 * 1024, admission);
  class HostileBytes extends Uint8Array {
    get byteLength() {
      throw new Error("subclass byteLength must not be observed");
    }
    slice() {
      throw new Error("subclass slice must not be called");
    }
    subarray() {
      throw new Error("subclass subarray must not be called");
    }
  }
  for (const borrowed of [Buffer.from([1, 2, 3, 4]), new HostileBytes([5, 6, 7])]) {
    const expected = Uint8Array.from(borrowed);
    const hash = sha256(borrowed);
    driver.transaction("write", (tx) =>
      new ContentRepository(tx, storage, cache).putObject(hash, borrowed),
    );
    const first = driver.transaction("read", (tx) =>
      readObject(new ContentRepository(tx, storage, cache), hash, expected.length),
    );
    assert.equal(Object.getPrototypeOf(first), Uint8Array.prototype);
    first.fill(255);
    const second = driver.transaction("read", (tx) =>
      readObject(new ContentRepository(tx, storage, cache), hash, expected.length),
    );
    assert.equal(Object.getPrototypeOf(second), Uint8Array.prototype);
    assert.deepEqual(second, expected);
  }
  cache.clear();
  const nodeBytes = encodeManifestNode({
    kind: "leaf",
    span: 0,
    entryCount: 0,
    entries: [],
  });
  const nodeHash = sha256(nodeBytes);
  const rootBytes = encodeManifestRoot({
    parameters: { minimum: 32_768, average: 131_072, maximum: 524_288 },
    fileSize: 0,
    entryCount: 0,
    rootNodeHash: nodeHash,
  });
  const rootHash = sha256(rootBytes);
  driver.transaction("write", (tx) => {
    const content = new ContentRepository(tx, storage, cache);
    content.putManifestNode(nodeHash, new HostileBytes(nodeBytes));
    content.putManifestRoot(rootHash, Buffer.from(rootBytes));
  });
  for (const [kind, hash, expected] of [
    ["node", nodeHash, nodeBytes],
    ["root", rootHash, rootBytes],
  ]) {
    const consume = (repository, callback) =>
      kind === "node"
        ? repository.withManifestNode(hash, callback)
        : repository.withManifestRoot(hash, callback);
    driver.transaction("read", (tx) =>
      consume(new ContentRepository(tx, storage, cache), (encoded) => {
        assert.equal(Object.getPrototypeOf(encoded), Uint8Array.prototype);
        encoded.fill(255);
      }),
    );
    const reread = driver.transaction("read", (tx) =>
      consume(new ContentRepository(tx, storage, cache), (encoded) =>
        Uint8Array.from(encoded),
      ),
    );
    assert.deepEqual(reread, expected);
  }
  cache.clear();
  assert.equal(admission.usedBytes, 0);
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
  assert.deepEqual(active, { active: 0, tombstoned: 0, cleanups: 0 });
  driver.close();
});

test("an oversized hostile stream chunk is intrinsically preflighted and cancelled before copy or processing", async () => {
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
  class HostileChunk extends Uint8Array {
    get byteLength() {
      return 1;
    }
    slice() {
      throw new Error("subclass slice must not be called");
    }
    subarray() {
      throw new Error("subclass subarray must not be called");
    }
  }
  let pulls = 0;
  const stream = new ReadableStream({
    pull(controller) {
      pulls += 1;
      controller.enqueue(new HostileChunk(3 * 1024 * 1024));
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
      3 * 1024 * 1024,
    ),
    /maxWriteSessionBytes/,
  );
  assert.equal(pulls, 1);
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

test("declared streamed-ingest quota is reserved before the first producer pull", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 2 * 1024 * 1024,
      maintenanceReserveBytes: 4096,
    },
    driver.capabilities,
  );
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  let pulls = 0;
  const stream = new ReadableStream(
    {
      pull(controller) {
        pulls += 1;
        controller.enqueue(Uint8Array.of(1));
      },
    },
    { highWaterMark: 0 },
  );
  await assert.rejects(
    prepareContent(
      port,
      stream,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      undefined,
      undefined,
      () => 8,
      2 * 1024 * 1024,
    ),
    /aggregate managed payload quota/,
  );
  assert.equal(pulls, 0);
  assert.deepEqual(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT ingest_reservation_bytes,(SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_cas_objects) objects FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    { ingest_reservation_bytes: 0, leases: 0, objects: 0 },
  );
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("declared entry-stream quota is reserved before iterable work or durable batches", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 128 * 1024,
      maintenanceReserveBytes: 4096,
      maxQueryBatchSize: 1,
    },
    driver.capabilities,
  );
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  let entriesRead = 0;
  function* entries() {
    for (let index = 0; index < 60; index += 1) {
      entriesRead += 1;
      const bytes = Uint8Array.of(index);
      yield { hash: sha256(bytes), length: 1, bytes };
    }
  }
  await assert.rejects(
    prepareContentEntriesStreaming(
      port,
      entries(),
      { minimum: 1, average: 1, maximum: 1 },
      60,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
    ),
    /aggregate managed payload quota/,
  );
  assert.equal(entriesRead, 0);
  assert.deepEqual(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT ingest_reservation_bytes,(SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_cas_objects) objects FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    { ingest_reservation_bytes: 0, leases: 0, objects: 0 },
  );
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("borrowed entry streams reject intrinsic oversized views before detached copies", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  class HostileBytes extends Uint8Array {
    get byteLength() {
      return 1;
    }
    slice() {
      throw new Error("subclass slice must not be called");
    }
    subarray() {
      throw new Error("subclass subarray must not be called");
    }
  }
  for (const entry of [
    {
      hash: new HostileBytes(2 * 1024 * 1024),
      length: 1,
      bytes: Uint8Array.of(1),
    },
    {
      hash: new Uint8Array(32),
      length: 1,
      bytes: new HostileBytes(2 * 1024 * 1024),
    },
  ])
    await assert.rejects(
      prepareContentEntriesStreaming(
        port,
        [entry],
        { minimum: 1, average: 1, maximum: 1 },
        1,
        storage,
        DEFAULT_RUNTIME_LIMITS,
        admission,
      ),
      /invalid staged manifest entry/,
    );
  assert.equal(admission.usedBytes, 0);
  assert.equal(
    driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM efs_cas_objects", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].count,
    ),
    0,
  );
  driver.close();
});

test("a 100 MiB streamed write stays chunk-bounded and a buffered peer rejects before copy", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-100m-stream-"));
  const filename = path.join(directory, "filesystem.db");
  let driver = await openNodeSqlite({ filename });
  let port = createSqliteOperationsStorage(driver);
  t.after(async () => {
    try {
      await port.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  });
  port.initialize();
  let storage = limits(driver);
  const runtime = {
    ...DEFAULT_RUNTIME_LIMITS,
    maxWriteSessionBytes: 1024 * 1024,
  };
  const admission = new AdmissionController(runtime.maxManagedResidentBytes);
  const producerChunkBytes = 1024 * 1024;
  let pulls = 0;
  const stream = new ReadableStream({
    pull(controller) {
      if (pulls === 100) {
        controller.close();
        return;
      }
      let state = (pulls + 1) * 0x9e3779b1;
      const producerChunk = new Uint8Array(producerChunkBytes);
      for (let index = 0; index < producerChunk.length; index += 1) {
        state ^= state << 13;
        state ^= state >>> 17;
        state ^= state << 5;
        producerChunk[index] = state;
      }
      pulls += 1;
      controller.enqueue(producerChunk);
    },
  });
  const prepared = await prepareContent(
    port,
    stream,
    storage,
    runtime,
    admission,
    undefined,
    undefined,
    () => 20,
    100 * 1024 * 1024,
  );
  assert.equal(prepared.size, 100 * 1024 * 1024);
  assert.equal(pulls, 100);
  assert.ok(admission.peakBytes < 16 * 1024 * 1024);
  assert.equal(admission.usedBytes, 0);
  const durable = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT object_bytes,staging_bytes,ingest_reservation_bytes FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  assert.ok(durable.object_bytes > 90 * 1024 * 1024);
  assert.ok(durable.staging_bytes > 90 * 1024 * 1024);
  assert.equal(durable.ingest_reservation_bytes, 0);
  const pinned = await new MaintenanceManager(
    port,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    () => 21,
    maintenanceCache(),
  ).collectGarbage({ runId: "pinned-100m" });
  assert.equal(pinned.deletedObjectCount, 0);
  const physicalBeforeReopen = port.physicalStorage();
  class HostileBuffered extends Uint8Array {
    get byteLength() {
      return 1;
    }
    slice() {
      throw new Error("oversized buffered input must not be sliced");
    }
    subarray() {
      throw new Error("oversized buffered input must not be viewed");
    }
  }
  await assert.rejects(
    prepareContent(
      port,
      new HostileBuffered(100 * 1024 * 1024),
      storage,
      runtime,
      admission,
    ),
    /buffered write exceeds maxWriteBytes/,
  );
  assert.equal(admission.usedBytes, 0);
  await port.close();
  driver = await openNodeSqlite({ filename, create: false });
  port = createSqliteOperationsStorage(driver);
  port.initialize();
  storage = limits(driver);
  port.transaction("read", { maxRows: 64, maxBytes: 64 * 1024 }, (tx) =>
    tx.staging(storage).validateSealed(prepared.certificate, 22),
  );
  port.transaction("write", { maxRows: 1024, maxBytes: 1024 * 1024 }, (tx) =>
    assert.equal(
      tx
        .staging(storage)
        .release(prepared.certificate.leaseId, prepared.certificate.ownerNonce, true),
      true,
    ),
  );
  const reclaimed = await new MaintenanceManager(
    port,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    () => 23,
    maintenanceCache(),
  ).collectGarbage({ runId: "reclaim-100m" });
  assert.ok(reclaimed.deletedObjectCount > 0);
  const afterGc = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT object_count,object_bytes,staging_bytes,ingest_reservation_bytes FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  assert.deepEqual(afterGc, {
    object_count: 0,
    object_bytes: 0,
    staging_bytes: 0,
    ingest_reservation_bytes: 0,
  });
  t.diagnostic(
    JSON.stringify({
      streamedBytes: prepared.size,
      producerOwnedChunkBytes: producerChunkBytes,
      managedPeakBytes: admission.peakBytes,
      callerOwnedInputExcluded: true,
      physicalBeforeReopen,
      pinnedDeletedObjects: pinned.deletedObjectCount,
      reclaimedObjects: reclaimed.deletedObjectCount,
    }),
  );
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
        maintenanceReserveBytes: 4096,
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

test("staging row metadata is exact at limit, rolls back at plus one, recounts, and releases", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-stage-metadata-"));
  const filename = path.join(directory, "filesystem.db");
  const nonce = new Uint8Array(16).fill(14);
  const bytes = Uint8Array.of(9);
  const hash = sha256(bytes);
  let metadataLimit;
  let baselineMetadata;
  let driver;
  try {
    driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    initializeOrValidateSchema(driver);
    const defaults = constrainStorageLimits(undefined, driver.capabilities);
    baselineMetadata = driver.transaction(
      "read",
      (tx) => new UsageRepository(tx, defaults).snapshot().charged_metadata_bytes,
    );
    metadataLimit = baselineMetadata + 4 * CHARGED_ROW_BYTES;
    let storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 16 * 1024 * 1024,
        maintenanceReserveBytes: 4096,
        maxChargedMetadataBytes: metadataLimit,
      },
      driver.capabilities,
    );
    driver.transaction("write", (tx) =>
      new ContentRepository(tx, storage).putObject(hash, bytes),
    );
    driver.transaction("write", (tx) => {
      const staging = new StagingRepository(tx, storage);
      staging.begin({
        leaseId: "metadata",
        ownerId: "owner",
        ownerNonce: nonce,
        now: 1,
        expiresAt: 100,
      });
      staging.putEntry("metadata", 0, hash, 1);
    });
    assert.throws(
      () =>
        driver.transaction("write", (tx) =>
          new StagingRepository(tx, storage).appendBatch("metadata", nonce, [
            { kind: "object", hash, size: 1 },
          ]),
        ),
      /charged metadata quota/,
    );
    driver.close();

    driver = await openNodeSqlite({
      filename,
      create: false,
      durability: "relaxed-test",
    });
    initializeOrValidateSchema(driver);
    storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 16 * 1024 * 1024,
        maintenanceReserveBytes: 4096,
        maxChargedMetadataBytes: metadataLimit,
      },
      driver.capabilities,
    );
    const recounted = driver.transaction("read", (tx) => {
      new UsageRepository(tx, storage).verifyDerivedUsage();
      verifyKeysetUsage(tx, storage);
      const usage = tx.all(
        "SELECT charged_metadata_bytes,(SELECT count(*) FROM efs_lease_objects) members,staging_bytes FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      return {
        ...usage,
        direct: new UsageRepository(tx, storage).directChargedMetadataBytes(),
      };
    });
    assert.deepEqual(recounted, {
      charged_metadata_bytes: metadataLimit,
      direct: metadataLimit,
      members: 0,
      staging_bytes: 0,
    });
    driver.transaction("write", (tx) =>
      new StagingRepository(tx, storage).release("metadata", nonce, false),
    );
    for (let batches = 0; batches < 16; batches += 1) {
      const exists = driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT id FROM efs_leases WHERE id='metadata'", [], {
            maxRows: 1,
            maxBytes: 128,
          }).length,
      );
      if (!exists) break;
      driver.transaction("write", (tx) =>
        new StagingRepository(tx, storage).cleanupBatch(8),
      );
    }
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT charged_metadata_bytes,(SELECT count(*) FROM efs_leases) leases FROM efs_usage",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0],
      ),
      {
        charged_metadata_bytes: baselineMetadata + CHARGED_ROW_BYTES,
        leases: 0,
      },
    );
    driver.close();
    driver = undefined;
  } finally {
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("maintenance expiry atomically releases partial and sealed staging charges after reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-stage-expiry-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  let port;
  try {
    driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    port = createSqliteOperationsStorage(driver);
    initializeOrValidateSchema(driver);
    const storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 32 * 1024 * 1024,
        maintenanceReserveBytes: 4096,
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
    await port.close();
    port = undefined;
    driver = undefined;
    driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    port = createSqliteOperationsStorage(driver);
    initializeOrValidateSchema(driver);
    const maintenance = new MaintenanceManager(
      port,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      () => 100,
      maintenanceCache(),
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
    await port.close();
    port = undefined;
    driver = undefined;
  } finally {
    try {
      await port?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
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
        maintenanceReserveBytes: 4096,
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
        maintenanceReserveBytes: 4096,
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
      maintenanceReserveBytes: 4096,
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
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        tx.run("DELETE FROM efs_leases WHERE id='bounded-cleanup'"),
      ),
    /lease deletion requires completed bounded cleanup/,
  );
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
      maintenanceReserveBytes: 4096,
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
    maintenanceCache(),
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

test("sealed recovery rows reject raw mutation until tombstoned cleanup", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sealed-reopen-"));
  const filename = path.join(directory, "filesystem.db");
  let driver = await openNodeSqlite({ filename });
  t.after(async () => {
    try {
      driver.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  });
  let port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  let storage = limits(driver);
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const prepared = await prepareContent(
    port,
    Uint8Array.of(1, 2, 3, 4),
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    undefined,
    () => 10,
  );
  const leaseId = prepared.certificate.leaseId;
  const mutations = [
    "UPDATE efs_staging_certificates SET verified=verified WHERE lease_id=?",
    "DELETE FROM efs_staging_certificates WHERE lease_id=?",
    "UPDATE efs_staging_reconciliations SET complete=complete WHERE lease_id=?",
    "DELETE FROM efs_staging_reconciliations WHERE lease_id=?",
    "UPDATE efs_staging_reconciliation_queue SET processed=processed WHERE lease_id=?",
    "DELETE FROM efs_staging_reconciliation_queue WHERE lease_id=?",
    "UPDATE efs_lease_objects SET size=size WHERE lease_id=?",
    "DELETE FROM efs_lease_objects WHERE lease_id=?",
    "UPDATE efs_lease_staged_manifests SET size=size WHERE lease_id=?",
    "DELETE FROM efs_lease_staged_manifests WHERE lease_id=?",
    "DELETE FROM efs_lease_manifests WHERE lease_id=?",
    "INSERT OR IGNORE INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)",
  ];
  for (const sql of mutations)
    assert.throws(
      () => driver.transaction("write", (tx) => tx.run(sql, [leaseId])),
      /sealed staging/,
      sql,
    );
  assert.throws(
    () =>
      driver.transaction("write", (tx) => {
        tx.run("UPDATE efs_leases SET state=2 WHERE id=?", [leaseId]);
        tx.run("DELETE FROM efs_staging_certificates WHERE lease_id=?", [leaseId]);
      }),
    /lease tombstone requires bounded cleanup state/,
    "a raw tombstone without its authenticated cleanup authority bypassed sealing",
  );
  driver.close();
  driver = await openNodeSqlite({ filename });
  port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  storage = limits(driver);
  let recoveryStatements = 0;
  const counted = {
    ...driver,
    transaction(mode, callback) {
      return driver.transaction(mode, (tx) =>
        callback({
          scope: tx.scope,
          run(...args) {
            recoveryStatements += 1;
            return tx.run(...args);
          },
          all(...args) {
            recoveryStatements += 1;
            return tx.all(...args);
          },
        }),
      );
    },
  };
  runUnitOfWork(counted, "read", { maxRows: 8, maxBytes: 4096 }, (tx) =>
    new StagingRepository(tx, storage).validateSealed(prepared.certificate, 10),
  );
  assert.equal(recoveryStatements, 1);
  driver.transaction("write", (tx) => {
    assert.equal(
      new StagingRepository(tx, storage).release(
        leaseId,
        prepared.certificate.ownerNonce,
        true,
      ),
      true,
    );
  });
  let batches = 0;
  while (
    driver.transaction("read", (tx) =>
      tx.all("SELECT id FROM efs_leases WHERE id=?", [leaseId], {
        maxRows: 1,
        maxBytes: 128,
      }),
    ).length
  ) {
    driver.transaction("write", (tx) =>
      new StagingRepository(tx, storage).cleanupBatch(8),
    );
    batches += 1;
    assert.ok(batches < 32);
  }
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("count-only closure members seal across shared leaves, survive GC, and release exactly", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const budget = {
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
  };
  const cache = maintenanceCache();
  const sharedBytes = Uint8Array.of(23);
  const sharedHash = sha256(sharedBytes);
  const levels = new Map();
  const workspace = {
    writeNode(record) {
      const rows = levels.get(record.level) ?? [];
      rows.push(record);
      levels.set(record.level, rows);
    },
    readLevel(level, afterIndex, limit) {
      return (levels.get(level) ?? [])
        .filter((record) => record.index > afterIndex)
        .slice(0, limit);
    },
  };
  const built = buildManifestFromEntries(
    Array.from({ length: 300 }, () => ({ hash: sharedHash, length: 1 })),
    { minimum: 1, average: 1, maximum: 1 },
    workspace,
    { maxDepth: 8, readBatchRecords: 17 },
  );
  const nodeValues = [...levels.values()].flatMap((records) =>
    records.map((record) => record.value),
  );
  assert.ok(nodeValues.filter((node) => node.node.kind === "leaf").length > 1);
  driver.transaction("write", (tx) => {
    const content = new ContentRepository(tx, storage);
    content.putObject(sharedHash, sharedBytes);
    for (const node of nodeValues) content.putManifestNode(node.hash, node.encoded);
    content.putManifestRoot(built.rootHash, built.root);
    tx.run(
      "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
      [built.rootHash, built.depth],
    );
    new UsageRepository(tx, storage).apply(
      { charged_metadata_bytes: CHARGED_ROW_BYTES },
      "count-only fixture validation certificate",
    );
  });
  const stagingBytes = () =>
    driver.transaction("read", (tx) => ({
      counter: tx.all("SELECT staging_bytes FROM efs_usage", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].staging_bytes,
      direct: tx.all(DIRECT_STAGING_BYTES_SQL, [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
    }));
  const assertStagingExact = () => {
    const state = stagingBytes();
    assert.equal(state.counter, state.direct);
  };
  const cleanupLease = (leaseId) => {
    let batches = 0;
    while (
      driver.transaction("read", (tx) =>
        tx.all("SELECT 1 FROM efs_leases WHERE id=?", [leaseId], {
          maxRows: 1,
          maxBytes: 128,
        }),
      ).length
    ) {
      driver.transaction("write", (tx) =>
        new StagingRepository(tx, storage).cleanupBatch(64),
      );
      assert.ok(++batches < 32, `${leaseId} cleanup made no progress`);
    }
  };
  const appendManifestLease = (
    leaseId,
    nonce,
    counted,
    expiresAt = 100_000,
    complete = true,
  ) => {
    let certificate;
    runUnitOfWork(driver, "write", budget, (tx) => {
      const staging = new StagingRepository(tx, storage, cache);
      staging.begin({
        leaseId,
        ownerId: `${leaseId}-owner`,
        ownerNonce: nonce,
        now: 1,
        expiresAt,
      });
      new ManifestTreeRepository(tx, storage, cache).protectSourceManifest(
        leaseId,
        nonce,
        built.rootHash,
      );
      if (counted)
        staging.appendCountedBatch(leaseId, nonce, [
          { kind: "object", hash: sharedHash, size: sharedBytes.length, counted: true },
        ]);
      for (const node of nodeValues)
        staging.appendBatch(leaseId, nonce, [
          { kind: "manifest-node", hash: node.hash, size: node.encoded.length },
        ]);
      staging.appendBatch(leaseId, nonce, [
        { kind: "manifest-root", hash: built.rootHash, size: built.root.length },
      ]);
      if (complete) staging.beginReconciliation(leaseId, nonce, built.rootHash);
      certificate = {
        ...staging.snapshot(leaseId, nonce),
        manifestHash: built.rootHash,
      };
    });
    if (!complete) return certificate;
    let reconciled = false;
    while (!reconciled)
      reconciled = runUnitOfWork(
        driver,
        "write",
        budget,
        (tx) =>
          new StagingRepository(tx, storage, cache).reconcileBatch(
            leaseId,
            nonce,
            storage.maxQueryBatchSize,
          ).complete,
      );
    return certificate;
  };

  const baseline = stagingBytes();
  const nonce = Uint8Array.from({ length: 16 }, (_, index) => index + 1);
  const certificate = appendManifestLease("count-only", nonce, true);
  assert.equal(certificate.objectCount, 1);
  assert.equal(certificate.objectBytes, sharedBytes.length);
  assert.equal(certificate.membershipCount, certificate.nodeCount + 1);
  runUnitOfWork(driver, "write", budget, (tx) =>
    new StagingRepository(tx, storage).seal(certificate),
  );
  runUnitOfWork(driver, "read", budget, (tx) =>
    new StagingRepository(tx, storage).validateSealed(certificate, 2),
  );
  const rows = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT (SELECT count(*) FROM efs_lease_objects WHERE lease_id=?) object_rows,(SELECT count(*) FROM efs_lease_staged_manifests WHERE lease_id=?) node_rows",
        [certificate.leaseId, certificate.leaseId],
        { maxRows: 1, maxBytes: 128 },
      )[0],
  );
  assert.deepEqual(rows, { object_rows: 0, node_rows: certificate.nodeCount });
  assertStagingExact();
  assert.equal(
    stagingBytes().counter,
    baseline.counter +
      nodeValues.reduce((sum, node) => sum + node.encoded.length, 0) +
      built.root.length,
  );
  driver.transaction("read", (tx) => verifyKeysetUsage(tx, storage));

  const gcWhileCounted = await new MaintenanceManager(
    port,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    () => 3,
    maintenanceCache(),
  ).collectGarbage({ runId: "count-only-protected" });
  assert.equal(gcWhileCounted.deletedObjectCount, 0);
  assert.equal(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_cas_objects WHERE hash=?",
          [sharedHash],
          {
            maxRows: 1,
            maxBytes: 128,
          },
        )[0].count,
    ),
    1,
  );

  const expiringNonce = Uint8Array.from({ length: 16 }, (_, index) => index + 20);
  runUnitOfWork(driver, "write", budget, (tx) => {
    const staging = new StagingRepository(tx, storage);
    staging.begin({
      leaseId: "count-only-expiring",
      ownerId: "count-only-expiring-owner",
      ownerNonce: expiringNonce,
      now: 1,
      expiresAt: 2,
    });
    staging.appendCountedBatch("count-only-expiring", expiringNonce, [
      { kind: "object", hash: sharedHash, size: sharedBytes.length, counted: true },
    ]);
  });
  const beforeExpiry = stagingBytes();
  runUnitOfWork(driver, "write", budget, (tx) =>
    new StagingRepository(tx, storage).expireBatch(3, 10),
  );
  cleanupLease("count-only-expiring");
  assert.deepEqual(stagingBytes(), beforeExpiry);
  assertStagingExact();

  const duplicateNonce = Uint8Array.from({ length: 16 }, (_, index) => index + 40);
  const duplicateCertificate = appendManifestLease(
    "count-only-duplicate",
    duplicateNonce,
    true,
    100_000,
    false,
  );
  runUnitOfWork(driver, "write", budget, (tx) => {
    const staging = new StagingRepository(tx, storage, cache);
    staging.appendBatch("count-only-duplicate", duplicateNonce, [
      { kind: "object", hash: sharedHash, size: sharedBytes.length },
    ]);
    staging.beginReconciliation("count-only-duplicate", duplicateNonce, built.rootHash);
  });
  assert.throws(() => {
    let complete = false;
    while (!complete)
      complete = runUnitOfWork(
        driver,
        "write",
        budget,
        (tx) =>
          new StagingRepository(tx, storage, cache).reconcileBatch(
            "count-only-duplicate",
            duplicateNonce,
            storage.maxQueryBatchSize,
          ).complete,
      );
  }, /complete manifest closure differs/);
  runUnitOfWork(driver, "write", budget, (tx) =>
    new StagingRepository(tx, storage).release(
      duplicateCertificate.leaseId,
      duplicateNonce,
      false,
    ),
  );
  cleanupLease(duplicateCertificate.leaseId);

  const fullFirstNonce = Uint8Array.from({ length: 16 }, (_, index) => index + 60);
  runUnitOfWork(driver, "write", budget, (tx) => {
    const staging = new StagingRepository(tx, storage);
    staging.begin({
      leaseId: "count-only-full-first",
      ownerId: "count-only-full-first-owner",
      ownerNonce: fullFirstNonce,
      now: 1,
      expiresAt: 100_000,
    });
    staging.appendBatch("count-only-full-first", fullFirstNonce, [
      { kind: "object", hash: sharedHash, size: sharedBytes.length },
    ]);
    assert.throws(
      () =>
        staging.appendBatch("count-only-full-first", fullFirstNonce, [
          { kind: "object", hash: sharedHash, size: sharedBytes.length, counted: true },
        ]),
      /counted closure member is already a full staged member/,
    );
    assert.throws(
      () =>
        staging.appendBatch("count-only-full-first", fullFirstNonce, [
          { kind: "object", hash: sharedHash, size: sharedBytes.length, counted: true },
          { kind: "object", hash: sharedHash, size: sharedBytes.length, counted: true },
        ]),
      /duplicate staging member/,
    );
  });
  runUnitOfWork(driver, "write", budget, (tx) =>
    new StagingRepository(tx, storage).release(
      "count-only-full-first",
      fullFirstNonce,
      false,
    ),
  );
  cleanupLease("count-only-full-first");

  runUnitOfWork(driver, "write", budget, (tx) =>
    new StagingRepository(tx, storage).release(certificate.leaseId, nonce, true),
  );
  cleanupLease(certificate.leaseId);
  assertStagingExact();
  driver.transaction("read", (tx) => verifyKeysetUsage(tx, storage));
  const gcAfterRelease = await new MaintenanceManager(
    port,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    () => 4,
    maintenanceCache(),
  ).collectGarbage({ runId: "count-only-released" });
  assert.ok(gcAfterRelease.deletedObjectCount >= 1);
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
    const admission = new AdmissionController(
      DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
    );
    const cache = new ContentCache(1, admission);
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
    const sharedBytes = new Uint8Array(8).fill(23);
    const sharedHash = sha256(sharedBytes);
    runUnitOfWork(driver, "write", budget, (tx) => {
      new ContentRepository(tx, storage).putObject(sharedHash, sharedBytes);
      new StagingRepository(tx, storage).appendBatch(leaseId, nonce, [
        { kind: "object", hash: sharedHash, size: sharedBytes.length },
      ]);
    });
    for (let start = 0; start < total; start += batchSize) {
      const end = Math.min(total, start + batchSize);
      runUnitOfWork(driver, "write", budget, (tx) => {
        const staging = new StagingRepository(tx, storage);
        for (let index = start; index < end; index += 1)
          staging.putEntry(leaseId, index, sharedHash, sharedBytes.length);
      });
    }
    const workspace = {
      writeNode(record) {
        runUnitOfWork(driver, "write", budget, (tx) => {
          const staging = new StagingRepository(tx, storage);
          new ContentRepository(tx, storage, cache).putManifestNode(
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
      const content = new ContentRepository(tx, storage, cache);
      content.putManifestRoot(built.rootHash, built.root);
      const staging = new StagingRepository(tx, storage, cache);
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
          new StagingRepository(tx, storage, cache).reconcileBatch(
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
    assert.equal(certificate.objectCount, 1);
    assert.deepEqual(reconciliation, {
      object_count: 1,
      node_count: certificate.nodeCount,
      membership_count: certificate.membershipCount,
      complete: 1,
    });
    assert.equal(finalStatements, 1);
    assert.ok(
      reconciliationStatements < total * 8,
      `unexpected reconciliation SQL amplification: ${reconciliationStatements}`,
    );
    const metadata = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT charged_metadata_bytes,(SELECT count(*) FROM efs_staging_entries WHERE lease_id=?) entries FROM efs_usage",
          [leaseId],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    assert.equal(metadata.entries, total);
    assert.ok(metadata.charged_metadata_bytes >= (3 + total) * CHARGED_ROW_BYTES);
    t.diagnostic(
      JSON.stringify({
        manifestEntries: total,
        uniqueClosureMembers: certificate.membershipCount,
        reconciliationStatements,
        statementsPerManifestEntry: reconciliationStatements / total,
        finalValidationStatements: finalStatements,
      }),
    );
    driver.close();
  },
);

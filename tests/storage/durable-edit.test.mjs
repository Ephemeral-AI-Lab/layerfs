import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { bytesToHex } from "../../packages/fs/dist/cas/bytes.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import { decodeManifestNode } from "../../packages/fs/dist/manifests/codec.js";
import { prepareDurableEditedContent } from "../../packages/fs/dist/operations/durable-edit-prepare.js";
import { readManifestRange as readManifestRangeUnadmitted } from "../../packages/fs/dist/operations/manifest-io.js";
import {
  AdmissionController,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

function readManifestRange(repository, hash, offset, length) {
  return readManifestRangeUnadmitted(
    repository,
    hash,
    offset,
    length,
    new AdmissionController(DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes),
  );
}

class MemoryManifestWorkspace {
  constructor() {
    this.levels = new Map();
    this.nodes = new Map();
  }
  writeNode(record) {
    const level = this.levels.get(record.level) ?? [];
    level.push(record);
    this.levels.set(record.level, level);
    this.nodes.set(bytesToHex(record.value.hash), {
      hash: record.value.hash,
      encoded: record.value.encoded,
    });
  }
  readLevel(level, afterIndex, limit) {
    return (this.levels.get(level) ?? [])
      .filter((record) => record.index > afterIndex)
      .slice(0, limit);
  }
}

function repeatedEntries(count, ordinaryHash, changedIndex = -1, changedHash) {
  return {
    *[Symbol.iterator]() {
      for (let index = 0; index < count; index += 1)
        yield {
          hash: index === changedIndex ? changedHash : ordinaryHash,
          length: 1,
        };
    },
  };
}

function countedDriver(driver, observed) {
  return {
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    physicalStorage: () => driver.physicalStorage?.() ?? {},
    checkpoint: (mode) => driver.checkpoint?.(mode),
    close: () => driver.close(),
    transaction(mode, callback) {
      observed.transactions += 1;
      return driver.transaction(mode, (tx) =>
        callback({
          scope: tx.scope,
          run: (sql, bindings) => tx.run(sql, bindings),
          all(sql, bindings, budget) {
            const rows = tx.all(sql, bindings, budget);
            if (sql.includes("efs_manifest_nodes")) {
              for (const row of rows) {
                if (!(row.encoded instanceof Uint8Array)) continue;
                observed.manifestNodeRows += 1;
                const node = decodeManifestNode(row.encoded);
                if (node.kind === "leaf")
                  observed.manifestEntriesDecoded += node.entries.length;
              }
            }
            return rows;
          },
        }),
      );
    },
  };
}

test("durable path-copy is authenticated and bounded on a 65,537-entry, three-level manifest", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-path-copy-"));
  const filename = path.join(directory, "filesystem.db");
  let rawDriver = await openNodeSqlite({ filename });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  let port = createSqliteOperationsStorage(countedDriver(rawDriver, observed));
  t.after(async () => {
    try {
      await port.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  });
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    rawDriver.capabilities,
  );
  port.initialize({ now: 1000 });
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const parameters = Object.freeze({ minimum: 1, average: 1, maximum: 1 });
  const entryCount = 65_537;
  const originalObject = Uint8Array.of(0);
  const replacementObject = Uint8Array.of(1);
  const originalHash = sha256(originalObject);
  const replacementHash = sha256(replacementObject);
  const workspace = new MemoryManifestWorkspace();
  const old = buildManifestFromEntries(
    repeatedEntries(entryCount, originalHash),
    parameters,
    workspace,
    { maxDepth: storage.maxManifestDepth },
  );
  assert.ok(old.entryCount > 16_384);
  assert.equal(old.depth, 3);
  port.transaction("write", { maxRows: 10_000, maxBytes: 16 * 1024 * 1024 }, (tx) => {
    const content = tx.content(storage);
    content.putObject(originalHash, originalObject);
    content.putManifestNodesBatch([...workspace.nodes.values()]);
    content.putManifestRoot(old.rootHash, old.root);
  });

  let sourceBytesRead = 0;
  let sourceReadCalls = 0;
  const source = Object.freeze({
    manifestHash: old.rootHash,
    size: old.fileSize,
    parameters,
    read(offset, length) {
      sourceReadCalls += 1;
      sourceBytesRead += length;
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
        (tx) => readManifestRange(tx.content(storage), old.rootHash, offset, length),
      );
    },
  });
  const editOffset = 30_000;
  const edit = Object.freeze({
    offset: editOffset,
    deleteLength: 1,
    insertLength: 1,
    readInsert(offset, length) {
      return replacementObject.slice(offset, offset + length);
    },
  });
  const expectedWorkspace = new MemoryManifestWorkspace();
  const expected = buildManifestFromEntries(
    repeatedEntries(entryCount, originalHash, editOffset, replacementHash),
    parameters,
    expectedWorkspace,
    { maxDepth: storage.maxManifestDepth },
  );
  observed.transactions = 0;
  observed.manifestNodeRows = 0;
  observed.manifestEntriesDecoded = 0;
  const prepared = await prepareDurableEditedContent(
    port,
    source,
    edit,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 1002,
  );
  assert.equal(prepared.mode, "durable-path-copy");
  assert.deepEqual(prepared.hash, expected.rootHash);
  assert.equal(prepared.size, entryCount);
  assert.equal(prepared.pathCopyMetrics.authenticatedNodesRead, old.depth);
  assert.equal(prepared.pathCopyMetrics.emittedNodes, old.depth);
  assert.ok(prepared.pathCopyMetrics.reusedSubtrees > 0);
  assert.ok(prepared.pathCopyMetrics.reusedSubtrees <= old.depth * 128);
  assert.ok(sourceReadCalls <= 2, `path-copy made ${sourceReadCalls} source reads`);
  assert.ok(sourceBytesRead <= 256, `path-copy read ${sourceBytesRead} source bytes`);
  assert.equal(prepared.pathCopyMetrics.sourceBytesRead, sourceBytesRead);
  assert.ok(
    observed.manifestNodeRows < 64,
    `path-copy materialized ${observed.manifestNodeRows} manifest node rows`,
  );
  assert.ok(
    observed.manifestEntriesDecoded < 4096,
    `path-copy decoded ${observed.manifestEntriesDecoded} manifest entries`,
  );
  assert.ok(
    observed.transactions < 32,
    `path-copy used ${observed.transactions} storage transactions`,
  );
  port.transaction("read", { maxRows: 100, maxBytes: 64 * 1024 }, (tx) =>
    tx.staging(storage).validateSealed(prepared.certificate, 1002),
  );
  assert.ok(
    rawDriver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT node_hash FROM efs_staging_reused_subtrees WHERE lease_id=? LIMIT 1",
          [prepared.certificate.leaseId],
          { maxRows: 1, maxBytes: 128 },
        ).length,
    ),
    "path-copy sealed without an authenticated reused-subtree claim",
  );
  for (const sql of [
    "UPDATE efs_staging_reused_subtrees SET span=span WHERE lease_id=?",
    "DELETE FROM efs_staging_reused_subtrees WHERE lease_id=?",
  ])
    assert.throws(
      () =>
        rawDriver.transaction("write", (tx) =>
          tx.run(sql, [prepared.certificate.leaseId]),
        ),
      /sealed reused subtree is immutable/,
      sql,
    );
  const actual = port.transaction(
    "read",
    { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
    (tx) => readManifestRange(tx.content(storage), prepared.hash, editOffset - 16, 33),
  );
  const expectedRange = new Uint8Array(33);
  expectedRange[16] = 1;
  assert.deepEqual(actual, expectedRange);

  let corruptSourceReads = 0;
  const corruptSource = Object.freeze({
    ...source,
    manifestHash: sha256(Uint8Array.of(99)),
    read(offset, length) {
      corruptSourceReads += 1;
      return source.read(offset, length);
    },
  });
  await assert.rejects(
    prepareDurableEditedContent(
      port,
      corruptSource,
      edit,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      undefined,
      () => 1003,
    ),
    /ECORRUPT: missing manifest root/,
  );
  assert.equal(corruptSourceReads, 0, "corrupt identity exposed source bytes");

  const inserted = Uint8Array.of(7);
  const insertion = Object.freeze({
    offset: editOffset,
    deleteLength: 0,
    insertLength: 1,
    readInsert(offset, length) {
      return inserted.slice(offset, offset + length);
    },
  });
  const fallback = await prepareDurableEditedContent(
    port,
    source,
    insertion,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 1004,
  );
  assert.equal(fallback.mode, "streamed-fallback");
  assert.match(fallback.pathCopyReason, /equal-length replacement/);
  assert.deepEqual(
    port.transaction("read", { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 }, (tx) =>
      readManifestRange(tx.content(storage), fallback.hash, editOffset - 16, 33),
    ),
    Uint8Array.from({ length: 33 }, (_, index) => (index === 16 ? 7 : 0)),
  );
  assert.equal(admission.usedBytes, 0);
  await port.close();

  rawDriver = await openNodeSqlite({ filename, create: false });
  port = createSqliteOperationsStorage(countedDriver(rawDriver, observed));
  port.initialize();
  observed.transactions = 0;
  observed.manifestNodeRows = 0;
  port.transaction("read", { maxRows: 1, maxBytes: 4096 }, (tx) =>
    tx.staging(storage).validateSealed(prepared.certificate, 1002),
  );
  assert.equal(observed.transactions, 1);
  assert.equal(observed.manifestNodeRows, 0);
  assert.deepEqual(
    port.transaction("read", { maxRows: 1024, maxBytes: 64 * 1024 }, (tx) =>
      readManifestRange(tx.content(storage), prepared.hash, editOffset - 1, 3),
    ),
    Uint8Array.of(0, 1, 0),
  );
  await port.close();
});

test("durable edit reserves its concurrent read windows before source or insertion work", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 16 * 1024 * 1024,
      maintenanceReserveBytes: 1024,
    },
    driver.capabilities,
  );
  port.initialize();
  const parameters = Object.freeze({ minimum: 1, average: 1, maximum: 1 });
  const object = Uint8Array.of(0);
  const objectHash = sha256(object);
  const workspace = new MemoryManifestWorkspace();
  const built = buildManifestFromEntries(
    repeatedEntries(257, objectHash),
    parameters,
    workspace,
  );
  port.transaction("write", { maxRows: 1000, maxBytes: 1024 * 1024 }, (tx) => {
    const content = tx.content(storage);
    content.putObject(objectHash, object);
    content.putManifestNodesBatch([...workspace.nodes.values()]);
    content.putManifestRoot(built.rootHash, built.root);
  });
  const runtime = Object.freeze({
    ...DEFAULT_RUNTIME_LIMITS,
    maxManagedResidentBytes: 128 * 1024,
    maxCacheBytes: 32 * 1024,
    maxPendingWriteBytes: 1024,
    maxWriteSessionBytes: 1024,
    maxPrefetchBytes: 16 * 1024,
    maxQueryBatchBytes: 32 * 1024,
    maxPreparedResultBytes: 32 * 1024,
  });
  const admission = new AdmissionController(runtime.maxManagedResidentBytes);
  const residentPressure = 127 * 1024;
  const releasePressure = admission.reserve(residentPressure);
  let sourceReads = 0;
  let insertionReads = 0;
  try {
    await assert.rejects(
      prepareDurableEditedContent(
        port,
        Object.freeze({
          manifestHash: built.rootHash,
          size: built.fileSize,
          parameters,
          read() {
            sourceReads += 1;
            throw new Error("source must not run before admission");
          },
        }),
        Object.freeze({
          offset: 100,
          deleteLength: 1,
          insertLength: 1,
          readInsert() {
            insertionReads += 1;
            throw new Error("insertion must not run before admission");
          },
        }),
        storage,
        runtime,
        admission,
      ),
      /managed resident memory limit/,
    );
    assert.equal(sourceReads, 0);
    assert.equal(insertionReads, 0);
    assert.equal(admission.usedBytes, residentPressure);
  } finally {
    releasePressure();
    await port.close();
  }
});

test("filesystem range mutations and streamed preparation own hostile byte views", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({ database: driver });
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
  try {
    const initial = new HostileBytes([97, 98, 99, 100, 101, 102]);
    await filesystem.writeFile("/data", initial);
    initial.fill(0);
    await filesystem.writeRange("/data", 8, new HostileBytes([88]));
    assert.deepEqual(
      await filesystem.readFile("/data"),
      Uint8Array.of(97, 98, 99, 100, 101, 102, 0, 0, 88),
    );
    await filesystem.replaceRange("/data", 2, 4, new HostileBytes([49, 50, 51]));
    assert.deepEqual(
      await filesystem.readFile("/data"),
      Uint8Array.of(97, 98, 49, 50, 51, 0, 0, 88),
    );
    await filesystem.truncate("/data", 3);
    assert.deepEqual(await filesystem.readFile("/data"), Uint8Array.of(97, 98, 49));
    await filesystem.truncate("/data", 1024 * 1024);
    assert.deepEqual(
      await filesystem.readRange("/data", { offset: 1024 * 1024 - 4, length: 4 }),
      new Uint8Array(4),
    );

    const streamParts = [
      new HostileBytes([1, 2]),
      new HostileBytes([3]),
      new HostileBytes([4, 5, 6]),
    ];
    await filesystem.writeFile(
      "/stream",
      new ReadableStream({
        pull(controller) {
          const part = streamParts.shift();
          if (part) controller.enqueue(part);
          else controller.close();
        },
      }),
    );
    assert.deepEqual(
      await filesystem.readFile("/stream"),
      Uint8Array.of(1, 2, 3, 4, 5, 6),
    );
  } finally {
    await filesystem.close();
    driver.close();
  }
});

test("public range edits admit intrinsic exact-bound bytes before ownership copy or source work", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({
    database: driver,
    storage: { maxWriteBytes: 4 },
  });
  class HostileBytes extends Uint8Array {
    get byteLength() {
      return 0;
    }
    slice() {
      throw new Error("subclass slice must not be called");
    }
    subarray() {
      throw new Error("subclass subarray must not be called");
    }
  }
  try {
    await filesystem.writeFile("/data", Uint8Array.of(1, 2, 3, 4));
    await filesystem.writeRange("/data", 0, new HostileBytes([5, 6, 7, 8]));
    assert.deepEqual(await filesystem.readFile("/data"), Uint8Array.of(5, 6, 7, 8));
    await assert.rejects(
      filesystem.replaceRange("/missing", 0, 0, new HostileBytes(5)),
      (error) => error?.code === "EFBIG",
    );
    assert.equal(filesystem.capabilities.runtime.maxManagedResidentBytes >= 4, true);
  } finally {
    await filesystem.close();
  }
});

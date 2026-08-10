import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { bytesToHex } from "../../packages/fs/dist/cas/bytes.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import {
  DEFAULT_FASTCDC,
  StreamingFastCdc,
  fastCdcGearTableV1,
} from "../../packages/fs/dist/cdc/fastcdc.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
} from "../../packages/fs/dist/manifests/codec.js";
import { prepareDurableEditedContent } from "../../packages/fs/dist/operations/durable-edit-prepare.js";
import { readManifestRange as readManifestRangeUnadmitted } from "../../packages/fs/dist/operations/manifest-io.js";
import {
  AdmissionController,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";
import { ContentCache } from "../../packages/fs/dist/cache/content-cache.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";

function certifyRoot(driver, storage, manifestHash, depth) {
  driver.transaction("write", (tx) => {
    tx.run(
      "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
      [manifestHash, depth],
    );
    new UsageRepository(tx, storage).apply(
      { charged_metadata_bytes: CHARGED_ROW_BYTES },
      "test validation certificate",
    );
  });
}

function readManifestRange(tx, storage, hash, offset, length) {
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(1, admission);
  return readManifestRangeUnadmitted(
    tx.content(storage, cache),
    hash,
    offset,
    length,
    admission,
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

function chunkedContent(bytes, parameters) {
  const entries = [];
  const objects = new Map();
  new StreamingFastCdc(parameters).drain(
    bytes,
    (chunk) => {
      const hash = sha256(chunk);
      entries.push({ hash, length: chunk.length });
      objects.set(bytesToHex(hash), { hash, bytes: chunk });
    },
    true,
  );
  return { entries, objects };
}

function deterministicRange(offset, length) {
  const bytes = new Uint8Array(length);
  for (let index = 0; index < length; index += 1) {
    const position = offset + index;
    const wordIndex = Math.floor(position / 4);
    let value = Math.imul(wordIndex ^ 0x7f4a7c15, 0x45d9f3b);
    value = Math.imul(value ^ (value >>> 16), 0x45d9f3b);
    value ^= value >>> 16;
    bytes[index] = (value >>> ((position & 3) * 8)) & 0xff;
  }
  return bytes;
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
      maxFinalTransactionRows: 64,
      maxQueryBatchSize: 1,
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
    for (const node of workspace.nodes.values())
      content.putManifestNode(node.hash, node.encoded);
    content.putManifestRoot(old.rootHash, old.root);
  });
  certifyRoot(rawDriver, storage, old.rootHash, old.depth);

  let sourceBytesRead = 0;
  let sourceReadCalls = 0;
  const source = Object.freeze({
    manifestHash: old.rootHash,
    size: old.fileSize,
    parameters,
    readStorageTransactions: 1,
    maxReadWindowBytes: 4_000,
    read(offset, length) {
      sourceReadCalls += 1;
      sourceBytesRead += length;
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
        (tx) => readManifestRange(tx, storage, old.rootHash, offset, length),
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
  assert.equal(prepared.mode, "durable-path-copy", prepared.pathCopyReason);
  assert.deepEqual(prepared.hash, expected.rootHash);
  assert.equal(prepared.size, entryCount);
  assert.ok(prepared.pathCopyMetrics.authenticatedNodesRead >= old.depth);
  assert.ok(
    observed.manifestNodeRows <= prepared.pathCopyMetrics.manifestRecordsRead,
    `${observed.manifestNodeRows} rows exceeded metric ${prepared.pathCopyMetrics.manifestRecordsRead}`,
  );
  assert.equal(prepared.pathCopyMetrics.emittedNodes, old.depth);
  assert.ok(prepared.pathCopyMetrics.emittedEntries <= 256);
  assert.ok(prepared.pathCopyMetrics.emittedObjectBytes <= 256);
  assert.ok(prepared.pathCopyMetrics.reusedSubtrees > 0);
  assert.ok(prepared.pathCopyMetrics.reusedSubtrees <= old.depth * 128);
  assert.ok(sourceReadCalls <= 2, `path-copy made ${sourceReadCalls} source reads`);
  assert.ok(sourceBytesRead <= 256, `path-copy read ${sourceBytesRead} source bytes`);
  assert.equal(prepared.pathCopyMetrics.sourceBytesRead, sourceBytesRead);
  assert.equal(prepared.pathCopyMetrics.sourceReadCalls, sourceReadCalls);
  assert.equal(prepared.pathCopyMetrics.sourceReadTransactions, sourceReadCalls);
  assert.ok(
    observed.manifestNodeRows < 64,
    `path-copy materialized ${observed.manifestNodeRows} manifest node rows`,
  );
  assert.ok(
    observed.manifestEntriesDecoded < 4096,
    `path-copy decoded ${observed.manifestEntriesDecoded} manifest entries`,
  );
  assert.ok(
    observed.transactions < 64,
    `path-copy used ${observed.transactions} storage transactions`,
  );
  assert.equal(prepared.pathCopyMetrics.storageTransactions, observed.transactions);
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
    (tx) => readManifestRange(tx, storage, prepared.hash, editOffset - 16, 33),
  );
  const expectedRange = new Uint8Array(33);
  expectedRange[16] = 1;
  assert.deepEqual(actual, expectedRange);

  const noOp = await prepareDurableEditedContent(
    port,
    source,
    Object.freeze({
      offset: editOffset,
      deleteLength: 1,
      insertLength: 1,
      readInsert: (_offset, length) => new Uint8Array(length),
    }),
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 1003,
  );
  assert.equal(noOp.mode, "durable-path-copy");
  assert.deepEqual(noOp.hash, old.rootHash);

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
    /ECORRUPT: (?:missing manifest root|manifest lacks a durable validation certificate)/,
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
      readManifestRange(tx, storage, fallback.hash, editOffset - 16, 33),
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
      readManifestRange(tx, storage, prepared.hash, editOffset - 1, 3),
    ),
    Uint8Array.of(0, 1, 0),
  );
  await port.close();
});

test("repeated reused hashes retain the stronger non-final authenticated source path", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-path-copy-context-"));
  const filename = path.join(directory, "filesystem.db");
  const rawDriver = await openNodeSqlite({ filename });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  const port = createSqliteOperationsStorage(countedDriver(rawDriver, observed));
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
      maxFinalTransactionRows: 64,
    },
    rawDriver.capabilities,
  );
  port.initialize({ now: 1100 });
  const parameters = Object.freeze({ minimum: 1, average: 1, maximum: 1 });
  const entryCount = 98_304;
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
  assert.equal(old.depth, 3);
  const decodedRoot = decodeManifestRoot(old.root, old.rootHash);
  const top = decodeManifestNode(
    workspace.nodes.get(bytesToHex(decodedRoot.rootNodeHash)).encoded,
    decodedRoot.rootNodeHash,
  );
  assert.equal(top.kind, "internal");
  assert.equal(top.children.length, 3);
  assert.deepEqual(top.children[1].hash, top.children[2].hash);
  const repeatedChildHash = top.children[1].hash;
  port.transaction("write", { maxRows: 10_000, maxBytes: 16 * 1024 * 1024 }, (tx) => {
    const content = tx.content(storage);
    content.putObject(originalHash, originalObject);
    content.putManifestNodesBatch([...workspace.nodes.values()]);
    content.putManifestRoot(old.rootHash, old.root);
  });
  certifyRoot(rawDriver, storage, old.rootHash, old.depth);
  let sourceBytesRead = 0;
  const source = Object.freeze({
    manifestHash: old.rootHash,
    size: old.fileSize,
    parameters,
    readStorageTransactions: 1,
    maxReadWindowBytes: 4_000,
    read(offset, length) {
      sourceBytesRead += length;
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
        (tx) => readManifestRange(tx, storage, old.rootHash, offset, length),
      );
    },
  });
  const editOffset = 17;
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
    Object.freeze({
      offset: editOffset,
      deleteLength: 1,
      insertLength: 1,
      readInsert: () => replacementObject,
    }),
    storage,
    DEFAULT_RUNTIME_LIMITS,
    new AdmissionController(DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes),
    undefined,
    () => 1101,
  );
  assert.equal(prepared.mode, "durable-path-copy");
  assert.deepEqual(prepared.hash, expected.rootHash);
  assert.ok(sourceBytesRead <= 256);
  assert.ok(observed.manifestNodeRows < 64);
  assert.ok(observed.manifestEntriesDecoded < 4096);
  assert.ok(observed.transactions < 64);
  assert.equal(prepared.pathCopyMetrics.storageTransactions, observed.transactions);
  assert.deepEqual(
    rawDriver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT source_path FROM efs_staging_reused_subtrees WHERE lease_id=? AND node_hash=?",
          [prepared.certificate.leaseId, repeatedChildHash],
          { maxRows: 1, maxBytes: 128 },
        )[0].source_path,
    ),
    Uint8Array.of(1),
  );
});

test("nondegenerate multi-height CDC replacement copies one authenticated path", async () => {
  const raw = await openNodeSqlite({ filename: ":memory:" });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  const port = createSqliteOperationsStorage(countedDriver(raw, observed));
  port.initialize();
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
      maxFinalTransactionRows: 64,
    },
    raw.capabilities,
  );
  const parameters = Object.freeze({ minimum: 2, average: 4, maximum: 8 });
  const original = Uint8Array.from(
    { length: 200_000 },
    (_, index) => (index * 17 + Math.floor(index / 257)) & 0xff,
  );
  const chunked = chunkedContent(original, parameters);
  assert.ok(chunked.entries.length > 16_384);
  const workspace = new MemoryManifestWorkspace();
  const built = buildManifestFromEntries(chunked.entries, parameters, workspace, {
    maxDepth: storage.maxManifestDepth,
  });
  assert.ok(built.depth >= 3);
  for (
    let start = 0;
    start < chunked.objects.size;
    start += storage.maxQueryBatchSize
  ) {
    const batch = [...chunked.objects.values()].slice(
      start,
      start + storage.maxQueryBatchSize,
    );
    port.transaction(
      "write",
      { maxRows: 1024, maxBytes: storage.maxFinalTransactionBytes },
      (tx) => tx.content(storage).putObjectsBatch(batch),
    );
  }
  for (
    let start = 0;
    start < workspace.nodes.size;
    start += storage.maxQueryBatchSize
  ) {
    const batch = [...workspace.nodes.values()].slice(
      start,
      start + storage.maxQueryBatchSize,
    );
    port.transaction(
      "write",
      { maxRows: 1024, maxBytes: storage.maxFinalTransactionBytes },
      (tx) => tx.content(storage).putManifestNodesBatch(batch),
    );
  }
  port.transaction("write", { maxRows: 16, maxBytes: 64 * 1024 }, (tx) =>
    tx.content(storage).putManifestRoot(built.rootHash, built.root),
  );
  certifyRoot(raw, storage, built.rootHash, built.depth);
  const editOffset = Math.floor(original.length / 2);
  const replacement = original[editOffset] ^ 0xff;
  const expectedBytes = original.slice();
  expectedBytes[editOffset] = replacement;
  const expectedChunks = chunkedContent(expectedBytes, parameters);
  const expectedWorkspace = new MemoryManifestWorkspace();
  const expected = buildManifestFromEntries(
    expectedChunks.entries,
    parameters,
    expectedWorkspace,
    { maxDepth: storage.maxManifestDepth },
  );
  observed.transactions = 0;
  observed.manifestNodeRows = 0;
  observed.manifestEntriesDecoded = 0;
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const prepared = await prepareDurableEditedContent(
    port,
    Object.freeze({
      manifestHash: built.rootHash,
      size: built.fileSize,
      parameters,
      read(offset, length) {
        return original.slice(offset, offset + length);
      },
    }),
    Object.freeze({
      offset: editOffset,
      deleteLength: 1,
      insertLength: 1,
      readInsert: () => Uint8Array.of(replacement),
    }),
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 1500,
  );
  assert.equal(prepared.mode, "durable-path-copy");
  assert.deepEqual(prepared.hash, expected.rootHash);
  assert.equal(prepared.pathCopyMetrics.emittedNodes, built.depth);
  assert.ok(prepared.pathCopyMetrics.emittedEntries <= 256);
  assert.ok(prepared.pathCopyMetrics.authenticatedNodesRead < 1024);
  assert.ok(observed.manifestNodeRows <= prepared.pathCopyMetrics.manifestRecordsRead);
  assert.equal(prepared.pathCopyMetrics.storageTransactions, observed.transactions);
  assert.ok(prepared.pathCopyMetrics.storageTransactions < 64);
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("path-copy caps hostile nondegenerate rechunk output before retaining entry 257", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  port.initialize();
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    driver.capabilities,
  );
  const parameters = Object.freeze({
    minimum: 1,
    average: 1,
    maximum: 64 * 1024,
  });
  const boundaryByte = fastCdcGearTableV1().findIndex((value) => (value & 1) === 0);
  assert.ok(boundaryByte >= 0);
  const object = new Uint8Array(parameters.maximum).fill(boundaryByte);
  const objectHash = sha256(object);
  const leafValue = Object.freeze({
    kind: "leaf",
    span: parameters.maximum * 256,
    entryCount: 256,
    entries: Object.freeze(
      Array.from({ length: 256 }, () =>
        Object.freeze({ hash: objectHash, length: parameters.maximum }),
      ),
    ),
  });
  const leaf = encodeManifestNode(leafValue);
  const leafHash = sha256(leaf);
  const root = encodeManifestRoot({
    parameters,
    fileSize: leafValue.span,
    entryCount: leafValue.entryCount,
    rootNodeHash: leafHash,
  });
  const rootHash = sha256(root);
  port.transaction(
    "write",
    { maxRows: 1024, maxBytes: storage.maxFinalTransactionBytes },
    (tx) => {
      const content = tx.content(storage);
      content.putObject(objectHash, object);
      content.putManifestNode(leafHash, leaf);
      content.putManifestRoot(rootHash, root);
    },
  );
  certifyRoot(driver, storage, rootHash, 1);
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(DEFAULT_RUNTIME_LIMITS.maxCacheBytes, admission);
  let sourceBytesRead = 0;
  let sourceReadCalls = 0;
  const source = Object.freeze({
    manifestHash: rootHash,
    size: leafValue.span,
    parameters,
    readStorageTransactions: 1,
    read(offset, length) {
      sourceReadCalls += 1;
      sourceBytesRead += length;
      if (sourceReadCalls > 2)
        throw new Error("streamed fallback sentinel after bounded path-copy");
      return port.transaction(
        "read",
        {
          maxRows: storage.maxFinalTransactionRows,
          maxBytes: storage.maxFinalTransactionBytes,
        },
        (tx) =>
          readManifestRangeUnadmitted(
            tx.content(storage, cache),
            rootHash,
            offset,
            length,
            admission,
            cache,
          ),
      );
    },
  });
  const editOffset = Math.floor(leafValue.span / 2);
  await assert.rejects(
    prepareDurableEditedContent(
      port,
      source,
      Object.freeze({
        offset: editOffset,
        deleteLength: 1,
        insertLength: 1,
        readInsert: () => Uint8Array.of(1),
      }),
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      cache,
      () => 2000,
    ),
    /streamed fallback sentinel after bounded path-copy/,
  );
  assert.equal(sourceReadCalls, 3);
  assert.ok(sourceBytesRead <= leafValue.span + 1024 * 1024);
  assert.ok(admission.peakBytes <= admission.limitBytes);
  assert.equal(admission.usedBytes, cache.metrics().bytes);
  cache.clear();
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("a 100 MiB fallback reports bounded windows and its full source-transaction cost", async (t) => {
  const raw = await openNodeSqlite({ filename: ":memory:" });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  const port = createSqliteOperationsStorage(countedDriver(raw, observed));
  port.initialize();
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    raw.capabilities,
  );
  const parameters = Object.freeze({
    minimum: 1024 * 1024,
    average: 1024 * 1024,
    maximum: 1024 * 1024,
  });
  const objectHash = sha256(Uint8Array.of(0x41));
  const workspace = new MemoryManifestWorkspace();
  const built = buildManifestFromEntries(
    {
      *[Symbol.iterator]() {
        for (let index = 0; index < 100; index += 1)
          yield { hash: objectHash, length: 1024 * 1024 };
      },
    },
    parameters,
    workspace,
    { maxDepth: storage.maxManifestDepth },
  );
  assert.equal(built.fileSize, 100 * 1024 * 1024);
  port.transaction(
    "write",
    { maxRows: 1024, maxBytes: storage.maxFinalTransactionBytes },
    (tx) => {
      const content = tx.content(storage);
      content.putManifestNodesBatch([...workspace.nodes.values()]);
      content.putManifestRoot(built.rootHash, built.root);
    },
  );
  certifyRoot(raw, storage, built.rootHash, built.depth);
  observed.transactions = 0;
  observed.manifestNodeRows = 0;
  observed.manifestEntriesDecoded = 0;
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  let reads = 0;
  let requestedBytes = 0;
  let largestRead = 0;
  const editOffset = 50 * 1024 * 1024;
  const prepared = await prepareDurableEditedContent(
    port,
    Object.freeze({
      manifestHash: built.rootHash,
      size: built.fileSize,
      parameters,
      readStorageTransactions: 1,
      maxReadWindowBytes: 32 * 1024,
      read(offset, length) {
        reads += 1;
        requestedBytes += length;
        largestRead = Math.max(largestRead, length);
        return deterministicRange(offset, length);
      },
    }),
    Object.freeze({
      offset: editOffset,
      deleteLength: 1,
      insertLength: 1,
      readInsert: () => Uint8Array.of(0x42),
    }),
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 3000,
  );
  assert.equal(prepared.mode, "streamed-fallback");
  assert.match(prepared.pathCopyReason, /authenticated leaf exceeds/);
  assert.equal(requestedBytes, built.fileSize - 1);
  assert.ok(reads <= 3201, `fallback made ${reads} source reads`);
  assert.ok(largestRead <= 32 * 1024, `fallback requested ${largestRead} bytes`);
  assert.ok(
    observed.manifestNodeRows < 4096,
    `fallback materialized ${observed.manifestNodeRows} manifest node rows`,
  );
  assert.ok(
    observed.transactions < 64,
    `fallback persistence used ${observed.transactions} transactions`,
  );
  assert.equal(prepared.fallbackMetrics.sourceReadCalls, reads);
  assert.equal(prepared.fallbackMetrics.sourceReadTransactions, reads);
  assert.equal(
    prepared.fallbackMetrics.storageTransactions,
    observed.transactions + reads,
  );
  const reportedTransactions = prepared.fallbackMetrics.storageTransactions;
  const expected = deterministicRange(editOffset - 16, 33);
  expected[16] = 0x42;
  assert.deepEqual(
    port.transaction("read", { maxRows: 1024, maxBytes: 1024 * 1024 }, (tx) =>
      readManifestRange(tx, storage, prepared.hash, editOffset - 16, 33),
    ),
    expected,
  );
  assert.ok(admission.peakBytes < 32 * 1024 * 1024);
  assert.equal(admission.usedBytes, 0);
  t.diagnostic(
    JSON.stringify({
      sourceReadCalls: reads,
      sourceBytesRead: requestedBytes,
      largestSourceReadBytes: largestRead,
      repositoryPersistenceTransactions: observed.transactions,
      reportedStorageTransactions: reportedTransactions,
      managedPeakBytes: admission.peakBytes,
    }),
  );
  await port.close();
});

test("durable edits authenticate empty, singleton, and every manifest height before fallback", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  const port = createSqliteOperationsStorage(countedDriver(driver, observed));
  port.initialize();
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    driver.capabilities,
  );
  const parameters = Object.freeze({ minimum: 1, average: 1, maximum: 1 });
  const object = Uint8Array.of(0);
  const objectHash = sha256(object);
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(1, admission);
  const shapes = [
    { count: 0, depth: 1, label: "empty" },
    { count: 1, depth: 1, label: "singleton" },
    { count: 257, depth: 2, label: "two-level" },
    { count: 65_537, depth: 3, label: "three-level" },
  ];
  for (let shapeIndex = 0; shapeIndex < shapes.length; shapeIndex += 1) {
    const shape = shapes[shapeIndex];
    const workspace = new MemoryManifestWorkspace();
    const built = buildManifestFromEntries(
      repeatedEntries(shape.count, objectHash),
      parameters,
      workspace,
      { maxDepth: storage.maxManifestDepth },
    );
    assert.equal(built.depth, shape.depth, shape.label);
    port.transaction(
      "write",
      { maxRows: 1024, maxBytes: storage.maxFinalTransactionBytes },
      (tx) => {
        const content = tx.content(storage, cache);
        if (shape.count) content.putObject(objectHash, object);
        content.putManifestNodesBatch([...workspace.nodes.values()]);
        content.putManifestRoot(built.rootHash, built.root);
      },
    );
    certifyRoot(driver, storage, built.rootHash, built.depth);
    const sourceBytes = new Uint8Array(shape.count);
    const editOffset = Math.floor(shape.count / 2);
    let sourceReads = 0;
    observed.transactions = 0;
    observed.manifestNodeRows = 0;
    observed.manifestEntriesDecoded = 0;
    const marker = 0x30 + shapeIndex;
    const prepared = await prepareDurableEditedContent(
      port,
      Object.freeze({
        manifestHash: built.rootHash,
        size: built.fileSize,
        parameters,
        maxReadWindowBytes: 32 * 1024,
        read(offset, length) {
          sourceReads += 1;
          return sourceBytes.slice(offset, offset + length);
        },
      }),
      Object.freeze({
        offset: editOffset,
        deleteLength: 0,
        insertLength: 1,
        readInsert: () => Uint8Array.of(marker),
      }),
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      cache,
      () => 4000 + shapeIndex,
    );
    assert.equal(prepared.mode, "streamed-fallback", shape.label);
    assert.match(prepared.pathCopyReason, /equal-length replacement/, shape.label);
    assert.ok(
      sourceReads <= Math.ceil(shape.count / (32 * 1024)) + 1,
      `${shape.label} fallback made ${sourceReads} source reads`,
    );
    const expected = new Uint8Array(shape.count + 1);
    expected[editOffset] = marker;
    assert.deepEqual(
      port.transaction("read", { maxRows: 1024, maxBytes: 1024 * 1024 }, (tx) =>
        readManifestRange(tx, storage, prepared.hash, 0, expected.length),
      ),
      expected,
      shape.label,
    );
    assert.ok(observed.transactions < 32, shape.label);
  }
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("a singleton leaf expands through fallback into a canonical multi-leaf tree", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  const port = createSqliteOperationsStorage(countedDriver(driver, observed));
  port.initialize();
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    driver.capabilities,
  );
  const original = Uint8Array.of(0);
  const originalHash = sha256(original);
  const workspace = new MemoryManifestWorkspace();
  const built = buildManifestFromEntries(
    [{ hash: originalHash, length: 1 }],
    DEFAULT_FASTCDC,
    workspace,
    { maxDepth: storage.maxManifestDepth },
  );
  port.transaction(
    "write",
    { maxRows: 1024, maxBytes: storage.maxFinalTransactionBytes },
    (tx) => {
      const content = tx.content(storage);
      content.putObject(originalHash, original);
      content.putManifestNodesBatch([...workspace.nodes.values()]);
      content.putManifestRoot(built.rootHash, built.root);
    },
  );
  certifyRoot(driver, storage, built.rootHash, built.depth);
  const insertedBytes = 64 * 1024 * 1024;
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(1, admission);
  observed.transactions = 0;
  const prepared = await prepareDurableEditedContent(
    port,
    Object.freeze({
      manifestHash: built.rootHash,
      size: 1,
      parameters: DEFAULT_FASTCDC,
      read: (offset, length) => original.slice(offset, offset + length),
    }),
    Object.freeze({
      offset: 1,
      deleteLength: 0,
      insertLength: insertedBytes,
      readInsert: deterministicRange,
    }),
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    cache,
    () => 5000,
  );
  assert.equal(prepared.mode, "streamed-fallback");
  assert.match(prepared.pathCopyReason, /equal-length replacement/);
  const resultPath = port.transaction(
    "read",
    { maxRows: 64, maxBytes: 1024 * 1024 },
    (tx) =>
      tx.manifestTree(storage, cache).pathAtOffset(prepared.hash, insertedBytes / 2),
  );
  assert.ok(resultPath.nodes.length >= 2);
  assert.ok(resultPath.entryCount > 256);
  assert.deepEqual(
    port.transaction("read", { maxRows: 1024, maxBytes: 1024 * 1024 }, (tx) =>
      readManifestRange(tx, storage, prepared.hash, 1 + 10 * 1024 * 1024, 64),
    ),
    deterministicRange(10 * 1024 * 1024, 64),
  );
  assert.ok(observed.transactions < 64);
  assert.ok(admission.peakBytes <= admission.limitBytes);
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("durable edit reserves its concurrent read windows before source or insertion work", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const observed = {
    transactions: 0,
    manifestNodeRows: 0,
    manifestEntriesDecoded: 0,
  };
  const port = createSqliteOperationsStorage(countedDriver(driver, observed));
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 16 * 1024 * 1024,
      maintenanceReserveBytes: 4096,
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
  certifyRoot(driver, storage, built.rootHash, built.depth);
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
  observed.transactions = 0;
  observed.manifestNodeRows = 0;
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
    assert.equal(observed.transactions, 0);
    assert.equal(observed.manifestNodeRows, 0);
    assert.equal(admission.usedBytes, residentPressure);
  } finally {
    releasePressure();
    await port.close();
  }
});

test("direct durable edits account retained insertion ownership before storage or source work", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 16 * 1024 * 1024,
      maintenanceReserveBytes: 4096,
    },
    driver.capabilities,
  );
  const admission = new AdmissionController(1024 * 1024);
  let storageCalls = 0;
  let sourceReads = 0;
  let insertionReads = 0;
  try {
    await assert.rejects(
      prepareDurableEditedContent(
        {
          transaction() {
            storageCalls += 1;
            throw new Error("storage must not run before retained admission");
          },
        },
        Object.freeze({
          manifestHash: new Uint8Array(32),
          size: 1,
          parameters: DEFAULT_FASTCDC,
          read() {
            sourceReads += 1;
            throw new Error("source must not run before retained admission");
          },
        }),
        Object.freeze({
          offset: 0,
          deleteLength: 0,
          insertLength: 768 * 1024,
          retainedBytes: 768 * 1024,
          readInsert() {
            insertionReads += 1;
            throw new Error("insertion must not run before retained admission");
          },
        }),
        storage,
        DEFAULT_RUNTIME_LIMITS,
        admission,
      ),
      /managed resident memory limit/,
    );
    assert.equal(storageCalls, 0);
    assert.equal(sourceReads, 0);
    assert.equal(insertionReads, 0);
    assert.equal(admission.usedBytes, 0);
  } finally {
    driver.close();
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
      { maxBytes: 6 },
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

test("string write preflight failures leave admission at its baseline", async () => {
  const originalReserve = AdmissionController.prototype.reserve;
  let outstanding = 0;
  AdmissionController.prototype.reserve = function (bytes) {
    const release = originalReserve.call(this, bytes);
    outstanding += bytes;
    let active = true;
    return () => {
      if (active) {
        active = false;
        outstanding -= bytes;
      }
      release();
    };
  };
  const driver = await openNodeSqlite({ filename: ":memory:" });
  let filesystem;
  try {
    filesystem = await EphemeralFS.open({ database: driver });
    await assert.rejects(filesystem.writeFile("/", "x".repeat(1024 * 1024)), {
      code: "EISDIR",
    });
    assert.equal(outstanding, 0);
    await filesystem.writeFile("/exists", "seed");
    const baseline = outstanding;
    await assert.rejects(
      filesystem.writeFile("/exists", "x".repeat(1024 * 1024), {
        exclusive: true,
      }),
      { code: "EEXIST" },
    );
    assert.equal(outstanding, baseline);
  } finally {
    try {
      await filesystem?.close();
    } finally {
      driver.close();
      AdmissionController.prototype.reserve = originalReserve;
    }
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
    const originalReserve = AdmissionController.prototype.reserve;
    let reserveCalls = 0;
    AdmissionController.prototype.reserve = function (...args) {
      reserveCalls += 1;
      return originalReserve.apply(this, args);
    };
    try {
      await assert.rejects(
        filesystem.writeRange("/data", -1, new HostileBytes([1, 2, 3, 4])),
        /offset/,
      );
      await assert.rejects(
        filesystem.replaceRange(
          "/data",
          0,
          Number.MAX_SAFE_INTEGER + 1,
          new HostileBytes([1, 2, 3, 4]),
        ),
        /deleteLength/,
      );
      assert.equal(reserveCalls, 0);
    } finally {
      AdmissionController.prototype.reserve = originalReserve;
    }
    await assert.rejects(
      filesystem.replaceRange("/missing", 0, 0, new HostileBytes(5)),
      (error) => error?.code === "EFBIG",
    );
    assert.equal(filesystem.capabilities.runtime.maxManagedResidentBytes >= 4, true);
  } finally {
    await filesystem.close();
  }
});

test("Node storage prerequisite bounds 64 MiB materialization while public snapshot pinning remains M3", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-materialized-read-"));
  const filename = path.join(directory, "filesystem.db");
  let driver = await openNodeSqlite({ filename });
  let filesystem = await EphemeralFS.open({ database: driver });
  t.after(async () => {
    try {
      await filesystem?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  });
  const bytes = Uint8Array.from(
    { length: 20 * 1024 * 1024 },
    (_, index) => (index * 131 + (index >>> 7) * 17) & 0xff,
  );
  await filesystem.writeFile("/large", bytes);
  await filesystem.close();
  driver.close();
  driver = await openNodeSqlite({ filename, create: false });
  filesystem = await EphemeralFS.open({ database: driver });
  assert.deepEqual(await filesystem.readFile("/large"), bytes);

  const rejectedDriver = await openNodeSqlite({ filename: ":memory:" });
  await assert.rejects(
    EphemeralFS.open({
      database: rejectedDriver,
      filesystem: { maxMaterializedBytes: 64 * 1024 * 1024 + 1 },
      runtime: { maxPreparedResultBytes: 64 * 1024 * 1024 + 1 },
    }),
    /Node storage-prerequisite materialization profile is capped at 64 MiB/,
  );
  rejectedDriver.close();
});

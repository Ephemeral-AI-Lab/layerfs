import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { bytesToHex, copyBytes } from "../../packages/fs/dist/cas/bytes.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { StreamingFastCdc } from "../../packages/fs/dist/cdc/fastcdc.js";
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
import { ContentCache } from "../../packages/fs/dist/cache/content-cache.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";

const MIB = 1024 * 1024;

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

function readManifestRange(tx, storage, cache, hash, offset, length) {
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  return readManifestRangeUnadmitted(
    tx.content(storage, cache),
    hash,
    offset,
    length,
    admission,
  );
}

function deterministicBytes(seed, length) {
  let state = seed >>> 0;
  const bytes = new Uint8Array(length);
  for (let offset = 0; offset < length; offset += 4) {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    const word = (value ^ (value >>> 14)) >>> 0;
    const end = Math.min(length, offset + 4);
    for (let index = offset; index < end; index += 1)
      bytes[index] = (word >>> ((index - offset) * 8)) & 0xff;
  }
  return bytes;
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

async function openFixture(filename, bytes, parameters, storage) {
  const driver = await openNodeSqlite({ filename });
  const port = createSqliteOperationsStorage(driver);
  port.initialize({ now: 1000 });
  const { entries, objects } = chunkedContent(bytes, parameters);
  const workspace = new MemoryManifestWorkspace();
  const built = buildManifestFromEntries(entries, parameters, workspace, {
    maxDepth: storage.maxManifestDepth,
  });
  port.transaction("write", { maxRows: 10_000, maxBytes: 64 * 1024 * 1024 }, (tx) => {
    const content = tx.content(storage);
    for (const object of objects.values()) content.putObject(object.hash, object.bytes);
    for (const node of workspace.nodes.values())
      content.putManifestNode(node.hash, node.encoded);
    content.putManifestRoot(built.rootHash, built.root);
  });
  certifyRoot(driver, storage, built.rootHash, built.depth);
  return { driver, port, built, entries, objects };
}

function readAll(port, storage, cache, manifestHash, fileSize) {
  const output = new Uint8Array(fileSize);
  let position = 0;
  while (position < fileSize) {
    const length = Math.min(1024 * 1024, fileSize - position);
    port.transaction("read", { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 }, (tx) => {
      const bytes = readManifestRange(
        tx,
        storage,
        cache,
        manifestHash,
        position,
        length,
      );
      output.set(bytes, position);
    });
    position += length;
  }
  return output;
}

test("durable local rebuild reconnects a size-changing edit and persists byte-identical content", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-local-rebuild-"));
  const filename = path.join(directory, "filesystem.db");
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    (await openNodeSqlite({ filename: ":memory:" })).capabilities,
  );
  const { driver, port, built } = await openFixture(
    filename,
    deterministicBytes(0x5eed, 40 * MIB),
    { minimum: 32_768, average: 131_072, maximum: 524_288 },
    storage,
  );
  t.after(async () => {
    try {
      await port.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  });
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(DEFAULT_RUNTIME_LIMITS.maxCacheBytes, admission);
  const replacement = deterministicBytes(0xbeef, 4096);
  const editOffset = Math.floor(built.fileSize / 2);
  const edit = Object.freeze({
    offset: editOffset,
    deleteLength: 17,
    insertLength: replacement.length,
    readInsert(offset, length) {
      return replacement.slice(offset, offset + length);
    },
  });
  const source = Object.freeze({
    manifestHash: built.rootHash,
    size: built.fileSize,
    parameters: { minimum: 32_768, average: 131_072, maximum: 524_288 },
    readStorageTransactions: 1,
    read(offset, length) {
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
        (tx) => readManifestRange(tx, storage, cache, built.rootHash, offset, length),
      );
    },
  });
  const prepared = await prepareDurableEditedContent(
    port,
    source,
    edit,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    cache,
    () => 1002,
  );
  assert.equal(prepared.mode, "local-rebuild");
  assert.equal(prepared.size, built.fileSize - 17 + replacement.length);
  assert.ok(prepared.localRebuildMetrics.reusedSubtrees > 0);
  assert.ok(prepared.localRebuildMetrics.affectedEntries > 0);
  assert.ok(prepared.localRebuildMetrics.newObjectCount > 0);
  assert.ok(prepared.localRebuildMetrics.storageTransactions < 64);
  port.transaction("read", { maxRows: 100, maxBytes: 64 * 1024 }, (tx) =>
    tx.staging(storage).validateSealed(prepared.certificate, 1002),
  );
  const actual = readAll(port, storage, cache, prepared.hash, prepared.size);
  const expected = new Uint8Array(built.fileSize - 17 + replacement.length);
  expected.set(deterministicBytes(0x5eed, built.fileSize).subarray(0, editOffset));
  expected.set(replacement, editOffset);
  expected.set(
    deterministicBytes(0x5eed, built.fileSize).subarray(editOffset + 17),
    editOffset + replacement.length,
  );
  assert.deepEqual(actual, expected);
});

test("durable local rebuild handles append, prepend, and truncate byte-identically", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-local-rebuild-edges-"));
  const filename = path.join(directory, "filesystem.db");
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    (await openNodeSqlite({ filename: ":memory:" })).capabilities,
  );
  const original = deterministicBytes(0x1234, 4 * MIB);
  const { driver, port, built } = await openFixture(
    filename,
    original,
    { minimum: 32_768, average: 131_072, maximum: 524_288 },
    storage,
  );
  t.after(async () => {
    try {
      await port.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  });
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(DEFAULT_RUNTIME_LIMITS.maxCacheBytes, admission);
  const cases = [
    {
      label: "append",
      edit: {
        offset: built.fileSize,
        deleteLength: 0,
        insertLength: 3,
        readInsert: (offset, length) =>
          Uint8Array.of(1, 2, 3).slice(offset, offset + length),
      },
      apply: (bytes) => {
        const out = new Uint8Array(bytes.length + 3);
        out.set(bytes);
        out.set([1, 2, 3], bytes.length);
        return out;
      },
    },
    {
      label: "prepend",
      edit: {
        offset: 0,
        deleteLength: 0,
        insertLength: 5,
        readInsert: (offset, length) =>
          Uint8Array.of(9, 8, 7, 6, 5).slice(offset, offset + length),
      },
      apply: (bytes) => {
        const out = new Uint8Array(bytes.length + 5);
        out.set([9, 8, 7, 6, 5]);
        out.set(bytes, 5);
        return out;
      },
    },
    {
      label: "truncate-middle",
      edit: {
        offset: 4096,
        deleteLength: 8192,
        insertLength: 0,
        readInsert: (offset, length) => new Uint8Array(length),
      },
      apply: (bytes) => {
        const out = new Uint8Array(bytes.length - 8192);
        out.set(bytes.subarray(0, 4096));
        out.set(bytes.subarray(4096 + 8192), 4096);
        return out;
      },
    },
    {
      label: "replace-larger",
      edit: {
        offset: 1024,
        deleteLength: 1,
        insertLength: 2,
        readInsert: (offset, length) =>
          Uint8Array.of(7, 7).slice(offset, offset + length),
      },
      apply: (bytes) => {
        const out = new Uint8Array(bytes.length + 1);
        out.set(bytes.subarray(0, 1024));
        out.set([7, 7], 1024);
        out.set(bytes.subarray(1025), 1026);
        return out;
      },
    },
  ];
  let currentHash = built.rootHash;
  let currentSize = built.fileSize;
  let currentExpected = original;
  for (const entry of cases) {
    const source = Object.freeze({
      manifestHash: currentHash,
      size: currentSize,
      parameters: { minimum: 32_768, average: 131_072, maximum: 524_288 },
      readStorageTransactions: 1,
      read(offset, length) {
        return port.transaction(
          "read",
          { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
          (tx) => readManifestRange(tx, storage, cache, currentHash, offset, length),
        );
      },
    });
    const prepared = await prepareDurableEditedContent(
      port,
      source,
      entry.edit,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      cache,
      () => 1002,
    );
    assert.equal(prepared.mode, "local-rebuild", entry.label);
    const actual = readAll(port, storage, cache, prepared.hash, prepared.size);
    const expected = entry.apply(currentExpected);
    assert.deepEqual(actual, expected, entry.label);
    currentHash = copyBytes(prepared.hash);
    currentSize = prepared.size;
    currentExpected = expected;
  }
});

test("every durable local rebuild persistence statement fault leaves the old state intact", async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-local-rebuild-fault-"));
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    (await openNodeSqlite({ filename: ":memory:" })).capabilities,
  );
  const bytes = deterministicBytes(0x4242, 2 * MIB);
  const parameters = { minimum: 32_768, average: 131_072, maximum: 524_288 };
  for (let occurrence = 1; occurrence <= 12; occurrence += 1) {
    const filename = path.join(directory, `fault-${occurrence}.db`);
    const { driver, port, built } = await openFixture(
      filename,
      bytes,
      parameters,
      storage,
    );
    let fired = false;
    const failingPort = Object.freeze({
      ...port,
      transaction(mode, budget, callback) {
        if (mode !== "write") return port.transaction(mode, budget, callback);
        return port.transaction(mode, budget, (tx) => {
          let txStatements = 0;
          const invoke =
            (fn) =>
            (...args) => {
              txStatements += 1;
              if (txStatements === occurrence) {
                fired = true;
                throw new Error(`local rebuild fault after statement ${occurrence}`);
              }
              return fn(...args);
            };
          return callback(
            Object.freeze({ ...tx, run: invoke(tx.run), all: invoke(tx.all) }),
          );
        });
      },
    });
    const admission = new AdmissionController(
      DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
    );
    const cache = new ContentCache(DEFAULT_RUNTIME_LIMITS.maxCacheBytes, admission);
    const source = Object.freeze({
      manifestHash: built.rootHash,
      size: built.fileSize,
      parameters,
      readStorageTransactions: 1,
      read(offset, length) {
        return port.transaction(
          "read",
          { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
          (tx) => readManifestRange(tx, storage, cache, built.rootHash, offset, length),
        );
      },
    });
    const edit = Object.freeze({
      offset: 1024,
      deleteLength: 1,
      insertLength: 2,
      readInsert: (offset, length) =>
        Uint8Array.of(7, 7).slice(offset, offset + length),
    });
    const snapshot = (driverHandle) =>
      driverHandle.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT count(*) FROM efs_cas_objects) objects,(SELECT count(*) FROM efs_manifest_nodes) nodes,(SELECT count(*) FROM efs_manifest_roots) roots,(SELECT count(*) FROM efs_manifest_validations) validations,(SELECT count(*) FROM efs_leases WHERE state=0) leases",
            [],
            { maxRows: 1, maxBytes: 1024 },
          )[0],
      );
    const before = snapshot(driver);
    assert.equal(before.roots, 1);
    assert.equal(before.validations, 1);
    assert.equal(before.leases, 0);
    try {
      const prepared = await prepareDurableEditedContent(
        failingPort,
        source,
        edit,
        storage,
        DEFAULT_RUNTIME_LIMITS,
        admission,
        cache,
        () => 1002,
      );
      assert.equal(
        fired,
        false,
        `fault ${occurrence} fired during a run that still succeeded`,
      );
      assert.equal(prepared.mode, "local-rebuild");
      port.transaction("read", { maxRows: 100, maxBytes: 64 * 1024 }, (tx) =>
        tx.staging(storage).validateSealed(prepared.certificate, 1002),
      );
    } catch (error) {
      assert.ok(fired, `fault ${occurrence} rejected without firing`);
      assert.match(error.message, /local rebuild fault/);
      const after = snapshot(driver);
      assert.deepEqual(
        after,
        before,
        `fault ${occurrence} changed the durable old state`,
      );
    }
    driver.close();
  }
  await rm(directory, { recursive: true, force: true });
});

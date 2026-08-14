import { bytesToHex } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/cas/bytes.js";
import { IncrementalSha256, sha256 } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/cas/sha256.js";
import { StreamingFastCdc } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/cdc/fastcdc.js";
import { buildManifestFromEntries } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/manifests/builder.js";
import { prepareDurableEditedContent } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/operations/durable-edit-prepare.js";
import { readManifestRange } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/operations/manifest-io.js";
import {
  AdmissionController,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/resources/limits.js";
import { ContentCache } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/cache/content-cache.js";
import { createSqliteOperationsStorage } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/sqlite/operations-storage.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/fs/dist/sqlite/usage-repository.js";
import { openNodeSqlite } from "file:///Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify/packages/sqlite-node/dist/index.js";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const MIB = 1024 * 1024;
const parameters = { minimum: 32_768, average: 131_072, maximum: 524_288 };
const SOURCE_WINDOW_BYTES = 2 * MIB;

function deterministicBytes(seed, length) {
  let state = seed >>> 0;
  const bytes = new Uint8Array(length);
  for (let offset = 0; offset < length; offset += 4) {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    const word = (value ^ (value >>> 14)) >>> 0;
    for (let index = offset; index < Math.min(length, offset + 4); index += 1)
      bytes[index] = (word >>> ((index - offset) * 8)) & 0xff;
  }
  return bytes;
}

class W {
  constructor() {
    this.levels = new Map();
    this.nodes = new Map();
  }
  writeNode(record) {
    const level = this.levels.get(record.level) ?? [];
    level.push(record);
    this.levels.set(record.level, level);
    this.nodes.set(bytesToHex(record.value.hash), record.value);
  }
  readLevel(level, afterIndex, limit) {
    return (this.levels.get(level) ?? [])
      .filter((record) => record.index > afterIndex)
      .slice(0, limit);
  }
}

function fixture(size) {
  const original = deterministicBytes(0x5eed, size);
  const entries = [];
  const objects = [];
  new StreamingFastCdc(parameters).drain(
    original,
    (chunk) => {
      const hash = sha256(chunk);
      entries.push({ hash, length: chunk.length });
      objects.push({ hash, bytes: chunk });
    },
    true,
  );
  const w = new W();
  const built = buildManifestFromEntries(entries, parameters, w, { maxDepth: 8 });
  return {
    original,
    entries,
    objects,
    nodes: w.nodes,
    rootHash: built.rootHash,
    root: built.root,
    depth: built.depth,
    fileSize: size,
  };
}

function publishHead(tx, storage, hash) {
  tx.staging(storage).bumpRoot(0, bytesToHex(hash));
}

function digestManifest(port, storage, hash, size) {
  const admission = new AdmissionController(64 * MIB);
  const cache = new ContentCache(16 * MIB, admission);
  return port.transaction(
    "read",
    { maxRows: 10_000, maxBytes: 256 * MIB },
    (tx) => {
      const repository = tx.content(storage, cache);
      const digest = new IncrementalSha256();
      for (let offset = 0; offset < size; offset += SOURCE_WINDOW_BYTES)
        digest.update(
          readManifestRange(
            repository,
            hash,
            offset,
            Math.min(SOURCE_WINDOW_BYTES, size - offset),
            admission,
            cache,
          ),
        );
      return digest.digest();
    },
  );
}

async function runSize(size, cacheMb) {
  const started = performance.now();
  const built = fixture(size);
  const directory = await mkdtemp(path.join(tmpdir(), "efs-m8-node-"));
  const driver = await openNodeSqlite({
    filename: path.join(directory, "fs.db"),
    cacheTargetBytes: cacheMb * MIB,
  });
  const port = createSqliteOperationsStorage(driver);
  port.initialize({ now: 1000 });
  const pragma = (name) =>
    driver.transaction("read", (tx) =>
      tx.all(`SELECT ${name} FROM pragma_${name}`, [], { maxRows: 1, maxBytes: 1024 })[0][name],
    );
  const settings = {
    cache_size: pragma("cache_size"),
    foreign_keys: pragma("foreign_keys"),
    journal_mode: pragma("journal_mode"),
    mmap_size: driver.capabilities.mmapLimitBytes,
    synchronous: pragma("synchronous"),
    wal_autocheckpoint: Math.floor(1024 ** 3 / (4096 + 24) / 2),
  };
  const sqlite = driver.transaction("read", (tx) =>
    tx.all("SELECT sqlite_version() AS version", [], { maxRows: 1, maxBytes: 1024 })[0]
      .version,
  );
  const storage = constrainStorageLimits(
    { maxManagedPayloadBytes: 256 * MIB, maintenanceReserveBytes: MIB },
    driver.capabilities,
  );
  port.transaction("write", { maxRows: 10_000, maxBytes: 256 * MIB }, (tx) => {
    const content = tx.content(storage);
    for (const object of built.objects) content.putObject(object.hash, object.bytes);
    for (const node of built.nodes.values()) content.putManifestNode(node.hash, node.encoded);
    tx.manifestTree(storage).recordSubtreeSummaries(
      [...built.nodes.values()].map((node) => ({ hash: node.hash, encoded: node.encoded })),
    );
    content.putManifestRoot(built.rootHash, built.root);
    tx.staging(storage).bumpRoot(0, bytesToHex(built.rootHash));
  });
  driver.transaction("write", (tx) => {
    tx.run(
      "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
      [built.rootHash, built.depth],
    );
    new UsageRepository(tx, storage).apply({ charged_metadata_bytes: CHARGED_ROW_BYTES }, "m8-rust-pair");
  });
  const admission = new AdmissionController(DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes);
  const cache = new ContentCache(DEFAULT_RUNTIME_LIMITS.maxCacheBytes, admission);
  const head = () =>
    new TextDecoder().decode(
      driver.transaction("read", (tx) =>
        tx.all(
          "SELECT root_id FROM efs_root_journal WHERE kind=0 ORDER BY generation DESC LIMIT 1",
          [],
          { maxRows: 1, maxBytes: 4096 },
        )[0].root_id,
      ),
    );
  const baseHead = head();
  const probe = built.objects[0];
  let sameIdReuse = false;
  port.transaction("write", { maxRows: 10_000, maxBytes: 256 * MIB }, (tx) => {
    sameIdReuse = !tx.content(storage, cache).putObject(probe.hash, probe.bytes);
  });
  let collisionRejected = false;
  try {
    port.transaction("write", { maxRows: 10_000, maxBytes: 256 * MIB }, (tx) => {
      const different = probe.bytes.slice();
      different[0] ^= 1;
      tx.content(storage, cache).putObject(probe.hash, different);
    });
  } catch {
    collisionRejected = true;
  }
  const casCount = () =>
    Number(
      driver.transaction("read", (tx) =>
        tx.all("SELECT count(*) AS count FROM efs_cas_objects", [], { maxRows: 1, maxBytes: 1024 })[0]
          .count,
      ),
    );
  const beforeRollbackCount = casCount();
  try {
    port.transaction("write", { maxRows: 10_000, maxBytes: 256 * MIB }, (tx) => {
      const bytes = Uint8Array.of(7, 8, 9);
      tx.content(storage, cache).putObject(sha256(bytes), bytes);
      throw new Error("intentional termination");
    });
  } catch {
    // The transaction must roll back the staged CAS insert.
  }
  const guards = {
    sameIdReuse,
    collisionRejected,
    rollback: casCount() === beforeRollbackCount,
    rootUnchanged: head() === baseHead,
  };
  if (!guards.sameIdReuse || !guards.collisionRejected || !guards.rollback || !guards.rootUnchanged)
    throw new Error("CAS identity or rollback guard failed");
  const source = Object.freeze({
    manifestHash: built.rootHash,
    size: built.fileSize,
    parameters,
    readStorageTransactions: 1,
    maxReadWindowBytes: SOURCE_WINDOW_BYTES,
    read(offset, length) {
      if (length > SOURCE_WINDOW_BYTES) throw new RangeError("source read exceeded 2 MiB bound");
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 256 * MIB },
        (tx) =>
          readManifestRange(
            tx.content(storage, cache),
            built.rootHash,
            offset,
            length,
            admission,
            cache,
          ),
      );
    },
  });
  const edits = [];
  for (const offset of [0, Math.floor(size / 2), size - 1]) {
    const editStarted = performance.now();
    const prepared = await prepareDurableEditedContent(
      port,
      source,
      {
        offset,
        deleteLength: 1,
        insertLength: 1,
        readInsert: (start, length) => Uint8Array.of(1).slice(start, start + length),
      },
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      cache,
      () => 2000,
      false,
      (tx, certificate, hash, editedSize) => {
        if (certificate.manifestHash.length !== 32 || editedSize !== size)
          throw new Error("prepared certificate shape mismatch");
        publishHead(tx, storage, hash);
      },
    );
    const prepareMs = performance.now() - editStarted;
    const digest = digestManifest(port, storage, prepared.hash, prepared.size);
    const expected = built.original.slice();
    expected[offset] = 1;
    const expectedDigest = sha256(expected);
    if (bytesToHex(digest) !== bytesToHex(expectedDigest))
      throw new Error(`reopened logical digest mismatch at ${offset}`);
    const metrics = prepared.localRebuildMetrics;
    edits.push({
      offset,
      root: bytesToHex(prepared.hash),
      digest: bytesToHex(digest),
      prepare_ms: Math.round(prepareMs * 1000) / 1000,
      mode: prepared.mode,
      loadedEntries: metrics?.loadedEntries,
      affectedEntries: metrics?.affectedEntries,
      newObjectCount: metrics?.newObjectCount,
      newManifestNodeCount: metrics?.newManifestNodeCount,
      reusedSubtrees: metrics?.reusedSubtrees,
      storageTransactions: metrics?.storageTransactions,
      sourceReadTransactions: metrics?.sourceReadTransactions,
      persistenceRows: metrics?.persistenceRows,
      persistenceBytes: metrics?.persistenceBytes,
      persistenceUnits: metrics?.persistenceUnits,
      reconnectOldOffset: metrics?.reconnectOldOffset,
      reconnectNewOffset: metrics?.reconnectNewOffset,
      phaseMs: metrics?.phaseMs,
    });
  }
  const finalRoot = driver.transaction("read", (tx) =>
    tx.all(
      "SELECT root_id FROM efs_root_journal WHERE kind=0 ORDER BY generation DESC LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 4096 },
    )[0].root_id,
  );
  const finalRootHex = new TextDecoder().decode(finalRoot);
  if (finalRootHex !== edits.at(-1).root) throw new Error("published head mismatch");
  const physical = driver.physicalStorage();
  const checkpoint = driver.checkpoint("truncate");
  await port.close();
  await rm(directory, { recursive: true, force: true });
  return {
    candidate: "node",
    size,
    size_mib: size / MIB,
    base_root: bytesToHex(built.rootHash),
    base_digest: bytesToHex(sha256(built.original)),
    final_root: finalRootHex,
    manifest_entries: built.entries.length,
    manifest_nodes: built.nodes.size,
    sqlite,
    settings,
    guards,
    cache_mib: cacheMb,
    checkpoint,
    physical,
    elapsed_ms: Math.round((performance.now() - started) * 1000) / 1000,
    edits,
  };
}

const cacheMb = Number(process.env.SWEEP_CACHE_MB ?? 64);
const sizes = (process.env.M8_SIZES ?? "1,10,20,100")
  .split(",")
  .map((value) => Number(value.trim()) * MIB);
for (const size of sizes) console.log(JSON.stringify(await runSize(size, cacheMb)));

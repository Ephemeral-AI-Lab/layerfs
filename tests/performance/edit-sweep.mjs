// Size-agnostic durable edit sweep: per-edit cost at 1/20/100 MiB on the
// 100 MiB fixture profile. Mirrors the mini-bench A5 cell but sweeps the
// file size. Not milestone-owned (tests/performance is harness territory).
import { bytesToHex } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/cas/bytes.js";
import { sha256 } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/cas/sha256.js";
import { StreamingFastCdc } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/cdc/fastcdc.js";
import { buildManifestFromEntries } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/manifests/builder.js";
import { prepareDurableEditedContent } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/operations/durable-edit-prepare.js";
import { readManifestRange } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/operations/manifest-io.js";
import {
  AdmissionController,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/resources/limits.js";
import { createSqliteOperationsStorage } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/sqlite/operations-storage.js";
import { ContentCache } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/cache/content-cache.js";
import { openNodeSqlite } from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/sqlite-node/dist/index.js";
import {
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "file:///C:/Users/yifan/code/Ephemeral-AI-Lab/ephemeral-ai-fs/packages/fs/dist/sqlite/usage-repository.js";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const MIB = 1024 * 1024;
const parameters = { minimum: 32_768, average: 131_072, maximum: 524_288 };

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

class W {
  constructor() {
    this.levels = new Map();
    this.nodes = new Map();
  }
  writeNode(r) {
    const l = this.levels.get(r.level) ?? [];
    l.push(r);
    this.levels.set(r.level, l);
    this.nodes.set(bytesToHex(r.value.hash), r.value);
  }
  readLevel(level, afterIndex, limit) {
    return (this.levels.get(level) ?? [])
      .filter((r) => r.index > afterIndex)
      .slice(0, limit);
  }
}

async function runSweep(size, cacheMb) {
  const original = deterministicBytes(0x5eed, size);
  const entries = [];
  const objects = [];
  new StreamingFastCdc(parameters).drain(
    original,
    (c) => {
      entries.push({ hash: sha256(c), length: c.length });
      objects.push({ hash: sha256(c), bytes: c });
    },
    true,
  );
  const w = new W();
  const b = buildManifestFromEntries(entries, parameters, w, { maxDepth: 8 });
  const built = {
    id: bytesToHex(b.rootHash),
    rootHash: b.rootHash,
    root: b.root,
    nodes: w.nodes,
    entries,
    fileSize: entries.reduce((s, e) => s + e.length, 0),
  };
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sweep-"));
  const driver = await openNodeSqlite({
    filename: path.join(directory, "fs.db"),
    cacheTargetBytes: cacheMb * MIB,
  });
  const port = createSqliteOperationsStorage(driver);
  port.initialize({ now: 1000 });
  const storage = constrainStorageLimits(
    { maxManagedPayloadBytes: 256 * 1024 * 1024, maintenanceReserveBytes: 1024 * 1024 },
    driver.capabilities,
  );
  port.transaction("write", { maxRows: 10000, maxBytes: 256 * 1024 * 1024 }, (tx) => {
    const content = tx.content(storage);
    for (const object of objects) content.putObject(object.hash, object.bytes);
    for (const node of w.nodes.values())
      content.putManifestNode(node.hash, node.encoded);
    tx.manifestTree(storage).recordSubtreeSummaries(
      [...w.nodes.values()].map((node) => ({
        hash: node.hash,
        encoded: node.encoded,
      })),
    );
    content.putManifestRoot(built.rootHash, built.root);
  });
  driver.transaction("write", (tx) => {
    tx.run(
      "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
      [built.rootHash, b.depth],
    );
    new UsageRepository(tx, storage).apply(
      { charged_metadata_bytes: CHARGED_ROW_BYTES },
      "sweep",
    );
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
    maxReadWindowBytes: 2 * 1024 * 1024,
    read(offset, length) {
      return port.transaction(
        "read",
        { maxRows: 10000, maxBytes: 4 * 1024 * 1024 },
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
  const result = [];
  for (const offset of [0, Math.floor(size / 2), size - 1]) {
    const started = performance.now();
    const prepared = await prepareDurableEditedContent(
      port,
      source,
      {
        offset,
        deleteLength: 1,
        insertLength: 1,
        readInsert: (o, l) => Uint8Array.of(1).slice(o, o + l),
      },
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
      cache,
      () => 2000,
    );
    const ms = performance.now() - started;
    const m = prepared.localRebuildMetrics;
    result.push({
      offset,
      ms: Math.round(ms * 10) / 10,
      mode: prepared.mode,
      loaded: m ? m.loadedEntries : undefined,
      counted: m ? m.loadedEntries - m.newObjectCount : undefined,
      newObj: m ? m.newObjectCount : undefined,
      storageTransactions: m?.storageTransactions,
      sourceReadTransactions: m?.sourceReadTransactions,
      persistenceMerged: m?.persistenceMerged,
      persistenceRows: m?.persistenceRows,
      persistenceBytes: m?.persistenceBytes,
      persistenceUnits: m?.persistenceUnits,
      reusedSubtrees: m?.reusedSubtrees,
      affectedEntries: m?.affectedEntries,
      reconnectOldOffset: m?.reconnectOldOffset,
      reconnectNewOffset: m?.reconnectNewOffset,
      phaseMs: m?.phaseMs,
    });
  }
  await port.close();
  await rm(directory, { recursive: true, force: true });
  return result;
}

const cacheMb = Number(process.env.SWEEP_CACHE_MB ?? 64);
console.log(
  `sweep (SQLite cache ${cacheMb} MiB): size -> [edit@0, edit@mid, edit@EOF] per-edit ms (loaded entries / count-only objects / new objects)`,
);
for (const size of [1 * MIB, 20 * MIB, 100 * MIB]) {
  const result = await runSweep(size, cacheMb);
  console.log(
    `${size / MIB} MiB:`,
    result
      .map(
        (r) =>
          `${r.ms}ms (${r.mode}, loaded=${r.loaded}, affected=${r.affectedEntries}, reconnect=${r.reconnectOldOffset}->${r.reconnectNewOffset}, counted=${r.counted}, new=${r.newObj}, tx=${r.storageTransactions}, readTx=${r.sourceReadTransactions}, reused=${r.reusedSubtrees}, merged=${r.persistenceMerged}, forecast=${r.persistenceRows}/${r.persistenceBytes}/${r.persistenceUnits}, phases=${JSON.stringify(r.phaseMs)})`,
      )
      .join(" | "),
  );
}

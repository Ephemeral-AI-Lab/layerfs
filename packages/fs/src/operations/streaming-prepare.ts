import { sha256 } from "../cas/sha256.js";
import { DEFAULT_FASTCDC, FASTCDC_GEAR_V1, StreamingFastCdc } from "../cdc/fastcdc.js";
import { encodeManifestNode, encodeManifestRoot, type ManifestChild, type ManifestEntry, type ManifestInternal, type ManifestLeaf } from "../manifests/codec.js";
import { AdmissionController, type RuntimeLimits, type StorageLimits } from "../resources/limits.js";
import type { ContentCache } from "../resources/content-cache.js";
import { ContentRepository, type ContentObjectInput } from "../sqlite/content-repository.js";
import { StagingRepository, type ClosureCertificate } from "../sqlite/staging-repository.js";
import { runUnitOfWork } from "../sqlite/unit-of-work.js";
import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SqliteRow } from "../sqlite-driver.js";
import { bytesToHex, checkedAdd, utf8 } from "../utils/bytes.js";

interface EntryRow extends SqliteRow { entry_index: number; object_hash: Uint8Array; length: number }
interface LevelRow extends SqliteRow { record_index: number; node_hash: Uint8Array; span: number; entry_count: number }
interface GenerationRow extends SqliteRow { root_mutation_generation: number }
interface PreparedNode { readonly hash: Uint8Array; readonly encoded: Uint8Array; readonly span: number; readonly entryCount: number }
export interface StreamPreparedManifest { readonly hash: Uint8Array; readonly size: number; readonly certificate: ClosureCertificate }

function recordBytes(record: ManifestEntry | ManifestChild): Uint8Array {
  if ("length" in record) { const bytes = new Uint8Array(36); bytes.set(record.hash); new DataView(bytes.buffer).setUint32(32, record.length, true); return bytes; }
  const bytes = new Uint8Array(48); const view = new DataView(bytes.buffer); bytes.set(record.hash); view.setBigUint64(32, BigInt(record.span), true); view.setBigUint64(40, BigInt(record.entryCount), true); return bytes;
}
function bumpRoot(tx: FilesystemSQLiteTransaction, leaseId: string): void {
  tx.run("UPDATE efs_meta SET root_mutation_generation=root_mutation_generation+1 WHERE singleton=1");
  const generation = tx.all<GenerationRow>("SELECT root_mutation_generation FROM efs_meta WHERE singleton=1", [], { maxRows: 1, maxBytes: 128 })[0]?.root_mutation_generation;
  if (!Number.isSafeInteger(generation)) throw new Error("ECORRUPT: invalid root mutation generation");
  tx.run("INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,5,?)", [generation!, utf8(leaseId)]);
}
function randomNonce(): Uint8Array { return globalThis.crypto.getRandomValues(new Uint8Array(16)); }

export async function prepareContentStreaming(driver: FilesystemSQLiteDriver, input: Uint8Array | ReadableStream<Uint8Array>, storage: StorageLimits, runtime: RuntimeLimits, admission: AdmissionController, signal?: AbortSignal, cache?: ContentCache): Promise<StreamPreparedManifest> {
  const leaseId = globalThis.crypto.randomUUID(); const ownerId = globalThis.crypto.randomUUID(); const ownerNonce = randomNonce(); const now = Date.now();
  const workBudget = { maxRows: storage.maxFinalTransactionRows, maxBytes: storage.maxFinalTransactionBytes };
  const pendingLimit = Math.max(DEFAULT_FASTCDC.maximum, Math.min(runtime.maxPendingWriteBytes, Math.floor(storage.maxFinalTransactionBytes / 2)));
  const inputBudget = input instanceof Uint8Array ? input.byteLength : 0;
  const builderBudget = Math.min(runtime.maxQueryBatchBytes + storage.maxManifestNodeBytes * 2, runtime.maxManagedResidentBytes - DEFAULT_FASTCDC.maximum - pendingLimit - inputBudget);
  if (builderBudget <= 0) throw new RangeError("managed resident memory limit cannot admit streaming manifest construction");
  const reservationBytes = DEFAULT_FASTCDC.maximum + pendingLimit + builderBudget + inputBudget;
  const releases: Array<() => void> = []; let leaseBegun = false;
  let chunker!: StreamingFastCdc; let total = 0; let entryIndex = 0; let pendingBytes = 0;
  const pending: ContentObjectInput[] = [];
  const flushObjects = (): void => {
    if (!pending.length) return; const batch = pending.splice(0); pendingBytes = 0;
    runUnitOfWork(driver, "write", workBudget, (tx) => {
      const repository = new ContentRepository(tx, storage); repository.putObjectsBatch(batch);
      for (const item of batch) tx.run("INSERT INTO efs_staging_entries(lease_id,entry_index,object_hash,length) VALUES(?,?,?,?)", [leaseId, entryIndex++, item.hash, item.bytes.byteLength]);
      const unique = [...new Map(batch.map((item) => [bytesToHex(item.hash), item])).values()];
      new StagingRepository(tx).appendBatch(leaseId, ownerNonce, unique.map((item) => Object.freeze({ kind: "object" as const, hash: item.hash, size: item.bytes.byteLength })));
      bumpRoot(tx, leaseId);
    });
  };
  const acceptChunks = (chunks: readonly Uint8Array[]): void => {
    for (const chunk of chunks) {
      total = checkedAdd(total, chunk.byteLength); if (total > storage.maxFileBytes) throw new RangeError("file exceeds maxFileBytes");
      if (pending.length >= storage.maxQueryBatchSize || pendingBytes + chunk.byteLength > pendingLimit) flushObjects();
      pending.push(Object.freeze({ hash: sha256(chunk), bytes: chunk })); pendingBytes += chunk.byteLength;
    }
  };
  const feed = (bytes: Uint8Array): void => { for (let offset = 0; offset < bytes.byteLength; offset += runtime.maxWriteSessionBytes) acceptChunks(chunker.push(bytes.subarray(offset, offset + runtime.maxWriteSessionBytes))); };
  try {
    cache?.makeRoom(reservationBytes);
    runUnitOfWork(driver, "write", workBudget, (tx) => { new StagingRepository(tx).begin({ leaseId, ownerId, ownerNonce, now, expiresAt: now + storage.stagingLeaseMs }); bumpRoot(tx, leaseId); }); leaseBegun = true;
    releases.push(admission.reserve(DEFAULT_FASTCDC.maximum));
    releases.push(admission.reserve(pendingLimit));
    releases.push(admission.reserve(builderBudget));
    if (inputBudget) releases.push(admission.reserve(inputBudget));
    chunker = new StreamingFastCdc(DEFAULT_FASTCDC);
    if (input instanceof Uint8Array) feed(input);
    else {
      const reader = input.getReader();
      try { while (true) { if (signal?.aborted) throw new DOMException("The operation was aborted", "AbortError"); const { done, value } = await reader.read(); if (done) break; if (!(value instanceof Uint8Array)) throw new TypeError("write stream chunks must be Uint8Array values"); feed(value); } }
      finally { reader.releaseLock(); }
    }
    acceptChunks(chunker.finish()); flushObjects();
    const rootNode = buildManifestLevels(driver, storage, runtime, leaseId, ownerNonce, workBudget);
    const root = encodeManifestRoot({ parameters: DEFAULT_FASTCDC, fileSize: total, entryCount: entryIndex, rootNodeHash: rootNode.hash }); const rootHash = sha256(root);
    const certificate = runUnitOfWork(driver, "write", workBudget, (tx) => {
      const repository = new ContentRepository(tx, storage); repository.putManifestRoot(rootHash, root);
      const staging = new StagingRepository(tx); staging.appendBatch(leaseId, ownerNonce, [Object.freeze({ kind: "manifest-root", hash: rootHash, size: root.byteLength })]);
      const snapshot = staging.snapshot(leaseId, ownerNonce); const sealed = Object.freeze({ ...snapshot, manifestHash: rootHash }); staging.seal(sealed); bumpRoot(tx, leaseId); return sealed;
    });
    return Object.freeze({ hash: rootHash, size: total, certificate });
  } catch (error) {
    if (leaseBegun) try { runUnitOfWork(driver, "write", workBudget, (tx) => { const removed = tx.run("DELETE FROM efs_leases WHERE id=? AND owner_nonce=?", [leaseId, ownerNonce]); if (removed.changes) bumpRoot(tx, leaseId); }); } catch {}
    throw error;
  } finally { for (let index = releases.length - 1; index >= 0; index -= 1) releases[index]!(); }
}

function buildManifestLevels(driver: FilesystemSQLiteDriver, storage: StorageLimits, runtime: RuntimeLimits, leaseId: string, ownerNonce: Uint8Array, budget: { readonly maxRows: number; readonly maxBytes: number }): PreparedNode {
  let level = 0; let sourceKind: "entries" | "level" = "entries";
  while (true) {
    let cursor = -1; let state = 0n; let group: Array<ManifestEntry | ManifestChild> = []; let outputIndex = 0; let single: PreparedNode | undefined;
    const pendingNodes: PreparedNode[] = [];
    const flushNodes = (): void => {
      if (!pendingNodes.length) return; const nodes = pendingNodes.splice(0);
      runUnitOfWork(driver, "write", budget, (tx) => {
        const repository = new ContentRepository(tx, storage); repository.putManifestNodesBatch(nodes.map((node) => ({ hash: node.hash, encoded: node.encoded })));
        for (const node of nodes) tx.run("INSERT INTO efs_staging_level_records(lease_id,level,record_index,node_hash,span,entry_count) VALUES(?,?,?,?,?,?)", [leaseId, level, outputIndex++, node.hash, node.span, node.entryCount]);
        const unique = [...new Map(nodes.map((node) => [bytesToHex(node.hash), node])).values()];
        new StagingRepository(tx).appendBatch(leaseId, ownerNonce, unique.map((node) => Object.freeze({ kind: "manifest-node" as const, hash: node.hash, size: node.encoded.byteLength })));
        bumpRoot(tx, leaseId);
      });
    };
    const emit = (): void => {
      const node = level === 0
        ? Object.freeze({ kind: "leaf", span: group.reduce((sum, entry) => checkedAdd(sum, (entry as ManifestEntry).length), 0), entryCount: group.length, entries: Object.freeze(group as ManifestEntry[]) } satisfies ManifestLeaf)
        : Object.freeze({ kind: "internal", span: group.reduce((sum, child) => checkedAdd(sum, (child as ManifestChild).span), 0), entryCount: group.reduce((sum, child) => checkedAdd(sum, (child as ManifestChild).entryCount), 0), children: Object.freeze(group as ManifestChild[]) } satisfies ManifestInternal);
      const encoded = encodeManifestNode(node); const prepared = Object.freeze({ hash: sha256(encoded), encoded, span: node.span, entryCount: node.entryCount }); single = prepared; pendingNodes.push(prepared); group = []; state = 0n;
      if (pendingNodes.length >= Math.min(storage.maxQueryBatchSize, 64) || pendingNodes.reduce((sum, item) => sum + item.encoded.byteLength, 0) >= Math.floor(storage.maxFinalTransactionBytes / 2)) flushNodes();
    };
    const minimum = level === 0 ? 64 : 32; const target = level === 0 ? 128 : 64; const maximum = level === 0 ? 256 : 128;
    while (true) {
      const rows = runUnitOfWork(driver, "read", budget, (tx) => sourceKind === "entries"
        ? tx.all<EntryRow>("SELECT entry_index,object_hash,length FROM efs_staging_entries WHERE lease_id=? AND entry_index>? ORDER BY entry_index LIMIT ?", [leaseId, cursor, storage.maxQueryBatchSize], { maxRows: storage.maxQueryBatchSize, maxBytes: runtime.maxQueryBatchBytes })
        : tx.all<LevelRow>("SELECT record_index,node_hash,span,entry_count FROM efs_staging_level_records WHERE lease_id=? AND level=? AND record_index>? ORDER BY record_index LIMIT ?", [leaseId, level - 1, cursor, storage.maxQueryBatchSize], { maxRows: storage.maxQueryBatchSize, maxBytes: runtime.maxQueryBatchBytes }));
      if (!rows.length) break;
      for (const row of rows) {
        cursor = sourceKind === "entries" ? (row as EntryRow).entry_index : (row as LevelRow).record_index;
        const record: ManifestEntry | ManifestChild = sourceKind === "entries" ? Object.freeze({ hash: (row as EntryRow).object_hash, length: (row as EntryRow).length }) : Object.freeze({ hash: (row as LevelRow).node_hash, span: (row as LevelRow).span, entryCount: (row as LevelRow).entry_count });
        group.push(record); for (const byte of recordBytes(record)) state = ((state << 1n) + BigInt(FASTCDC_GEAR_V1[byte]!)) & 0xffff_ffff_ffff_ffffn;
        if (group.length >= maximum || (group.length >= minimum && (state & BigInt(target - 1)) === 0n)) emit();
      }
      if (rows.length < storage.maxQueryBatchSize) break;
    }
    if (group.length || (level === 0 && outputIndex === 0 && pendingNodes.length === 0)) emit(); flushNodes();
    if (outputIndex === 1 && single) return single;
    if (outputIndex <= 0) throw new Error("ECORRUPT: manifest level produced no node");
    sourceKind = "level"; level += 1; if (level >= storage.maxManifestDepth) throw new RangeError("manifest depth exceeds configured limit");
  }
}

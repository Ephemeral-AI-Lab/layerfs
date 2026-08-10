import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction } from "../sqlite/driver.js";
import type { RuntimeLimits, StorageLimits } from "../resources/limits.js";
import { runUnitOfWork } from "../sqlite/unit-of-work.js";
import { ContentRepository } from "../sqlite/content-repository.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import { bytesToHex, hexToBytes } from "../cas/bytes.js";
import type { FilesystemMaintenance, GarbageCollectionOptions, GarbageCollectionResult, StorageSnapshot, VerificationOptions, VerificationResult, VerificationScope } from "../filesystem/types.js";
import { abortError, fsError } from "../filesystem/errors.js";
import { MaintenanceRepository, type GcRunRow } from "../sqlite/maintenance-repository.js";

const COMPLETE = 4; const ABANDONED = 5;

export class MaintenanceManager implements FilesystemMaintenance {
  readonly #driver: FilesystemSQLiteDriver; readonly #storage: StorageLimits; readonly #runtime: RuntimeLimits; readonly #clock: () => number;
  constructor(driver: FilesystemSQLiteDriver, storage: StorageLimits, runtime: RuntimeLimits, clock: () => number) { this.#driver = driver; this.#storage = storage; this.#runtime = runtime; this.#clock = clock; }

  async collectGarbage(options: GarbageCollectionOptions = {}): Promise<GarbageCollectionResult> {
    if (this.#driver.readOnly) throw fsError("EROFS", "collectGarbage", undefined, "garbage collection requires writable storage"); if (options.signal?.aborted) throw abortError();
    const start = performance.now(); const runId = options.runId ?? globalThis.crypto.randomUUID(); const maxBatches = options.maxBatches ?? 100_000; if (!Number.isSafeInteger(maxBatches) || maxBatches < 0) throw fsError("EINVAL", "collectGarbage", undefined, "maxBatches must be a nonnegative safe integer");
    this.#write((tx) => { new MaintenanceRepository(tx).beginRun(runId, this.#now()); });
    let batches = 0;
    try {
      while (batches < maxBatches) {
        if (options.signal?.aborted) throw abortError(); const state = this.#read((tx) => new MaintenanceRepository(tx).run(runId)?.state); if (state === undefined) throw fsError("ENOENT", "collectGarbage", undefined, "collection run does not exist"); if (state === COMPLETE || state === ABANDONED) break;
        if (state === 0) this.#markBatch(runId); else this.#sweepBatch(runId, state); batches += 1;
      }
    } catch (error) { if (!(error instanceof DOMException && error.name === "AbortError")) { try { this.#write((tx) => new MaintenanceRepository(tx).abandonRun(runId, COMPLETE, ABANDONED)); } catch {} } throw error; }
    const row = this.#read((tx) => new MaintenanceRepository(tx).run(runId)); if (!row) throw fsError("ENOENT", "collectGarbage", undefined, "collection run disappeared");
    return Object.freeze({ runId, state: row.state === COMPLETE ? "complete" : row.state === ABANDONED ? "abandoned" : "paused", examinedManifestRootCount: row.examined_roots, deletedManifestRootCount: row.deleted_roots, examinedManifestNodeCount: row.examined_nodes, deletedManifestNodeCount: row.deleted_nodes, examinedManifestCount: row.examined_roots + row.examined_nodes, deletedManifestCount: row.deleted_roots + row.deleted_nodes, examinedObjectCount: row.examined_objects, deletedObjectCount: row.deleted_objects, reclaimedObjectPayloadBytes: row.reclaimed_object_bytes, reclaimedManifestPayloadBytes: row.reclaimed_manifest_bytes, reclaimedBranchOverlayPayloadBytes: 0, committedBatches: batches, elapsedMs: performance.now() - start });
  }

  async snapshotStorage(): Promise<StorageSnapshot> {
    const row = this.#read((tx) => new MaintenanceRepository(tx).snapshot()); if (!row) throw new Error("ECORRUPT: usage metadata is missing");
    const physical = this.#read((tx) => { const value = new MaintenanceRepository(tx).physical(); return Object.freeze({ mainFileBytes: value.pageCount * value.pageSize, freelistBytes: value.freePages * value.pageSize }); });
    const manifestBytes = row.manifest_root_bytes + row.manifest_node_bytes;
    return Object.freeze({ rootMutationGeneration: row.generation, mainLogicalBytes: row.logical_bytes, storedObjectPayloadBytes: row.object_bytes, storedManifestPayloadBytes: manifestBytes, reachableObjectPayloadBytes: row.object_bytes, reachableManifestPayloadBytes: manifestBytes, reclaimablePayloadBytes: 0, branchPageBytes: row.page_bytes, branchPatchBytes: row.patch_bytes, branchExclusiveObjectBytes: 0, branchExclusiveManifestBytes: 0, branchExclusivePayloadBytes: row.page_bytes + row.patch_bytes, objectCount: row.object_count, manifestRootCount: row.manifest_root_count, manifestNodeCount: row.manifest_node_count, manifestCount: row.manifest_root_count + row.manifest_node_count, chargedMetadataBytes: row.charged_metadata_bytes, revisionCount: row.revisions, includesNamespaceMetadata: true, includesOperationResults: true, physical });
  }

  async verify(options: VerificationOptions = {}): Promise<VerificationResult> {
    if (options.signal?.aborted) throw abortError(); const maximum = options.maxEntities ?? this.#storage.maxGcBatchSize; if (!Number.isSafeInteger(maximum) || maximum <= 0) throw fsError("EINVAL", "verify", undefined, "maxEntities must be a positive safe integer"); const scopes = new Set<VerificationScope>(options.scopes ?? ["metadata", "namespace", "manifests", "objects", "head"]); const phases = ["roots", "nodes", "objects", "inodes"] as const; let cursor = options.cursor ? this.#decodeCursor(options.cursor) : { phase: 0, last: "" }; let checked = 0;
    const generation = this.#read((tx) => {
      const maintenance = new MaintenanceRepository(tx); const result = maintenance.generation(); const repo = new ContentRepository(tx, this.#storage);
      while (checked < maximum && cursor.phase < phases.length) {
        const phase = phases[cursor.phase]!; if ((phase === "roots" || phase === "nodes") && !scopes.has("manifests") || phase === "objects" && !scopes.has("objects") || phase === "inodes" && !scopes.has("namespace") && !scopes.has("head")) { cursor = { phase: cursor.phase + 1, last: "" }; continue; }
        if (phase === "roots" || phase === "nodes") { const rows = maintenance.hashes(phase, cursor.last ? hexToBytes(cursor.last, 32) : new Uint8Array(), maximum - checked, this.#runtime.maxQueryBatchBytes); for (const row of rows) { if (phase === "roots") decodeManifestRoot(row.encoded, row.hash); else decodeManifestNode(row.encoded, row.hash); cursor.last = bytesToHex(row.hash); checked += 1; } if (rows.length < maximum - (checked - rows.length)) cursor = { phase: cursor.phase + 1, last: "" }; else break; }
        else if (phase === "objects") { const rows = maintenance.objects(cursor.last ? hexToBytes(cursor.last, 32) : new Uint8Array(), maximum - checked, this.#runtime.maxQueryBatchBytes); for (const row of rows) { const object = repo.getObject(row.hash); if (!object || object.byteLength !== row.size) throw new Error("ECORRUPT: invalid CAS object"); cursor.last = bytesToHex(row.hash); checked += 1; } if (rows.length < maximum - (checked - rows.length)) cursor = { phase: cursor.phase + 1, last: "" }; else break; }
        else { const rows = maintenance.inodes(cursor.last, maximum - checked, this.#runtime.maxQueryBatchBytes); for (const row of rows) { if (row.type === 0) { if (!row.manifest_hash || row.size === null) throw new Error("ECORRUPT: file inode lacks content"); const rootBytes = repo.getManifestRoot(row.manifest_hash); if (!rootBytes || decodeManifestRoot(rootBytes, row.manifest_hash).fileSize !== row.size) throw new Error("ECORRUPT: inode manifest size mismatch"); if (row.actual_links !== row.nlink) throw new Error("ECORRUPT: hard-link count mismatch"); } cursor.last = row.id; checked += 1; } if (rows.length < maximum - (checked - rows.length)) cursor = { phase: cursor.phase + 1, last: "" }; else break; }
      }
      return result;
    });
    return Object.freeze({ rootMutationGeneration: generation, checkedEntities: checked, complete: cursor.phase >= phases.length, nextCursor: cursor.phase >= phases.length ? null : this.#encodeCursor(cursor) });
  }

  #markBatch(runId: string): void { this.#write((tx) => { const maintenance = new MaintenanceRepository(tx); const perMark = 129; const limit = Math.max(1, Math.min(this.#storage.maxGcBatchSize, Math.floor(this.#storage.maxFinalTransactionRows / perMark))); const rows = maintenance.pendingMarks(runId, limit, this.#runtime.maxQueryBatchBytes); const repo = new ContentRepository(tx, this.#storage); let roots = 0; let nodes = 0; let objects = 0;
      for (const row of rows) { if (row.kind === 0) { const encoded = repo.getManifestRoot(row.hash); if (!encoded) throw new Error("ECORRUPT: reachable manifest root is missing"); const root = decodeManifestRoot(encoded, row.hash); maintenance.addMark(runId, 1, root.rootNodeHash); roots += 1; } else if (row.kind === 1) { const encoded = repo.getManifestNode(row.hash); if (!encoded) throw new Error("ECORRUPT: reachable manifest node is missing"); const node = decodeManifestNode(encoded, row.hash); if (node.kind === "leaf") for (const entry of node.entries) maintenance.addMark(runId, 2, entry.hash); else for (const child of node.children) maintenance.addMark(runId, 1, child.hash); nodes += 1; } else { if (!repo.getObject(row.hash)) throw new Error("ECORRUPT: reachable object is missing"); objects += 1; } maintenance.markProcessed(runId, row.kind, row.hash); }
      maintenance.addExamined(runId, roots, nodes, objects);
      if (!rows.length) maintenance.reconcileRoots(runId);
    }); }
  #sweepBatch(runId: string, state: number): void { this.#write((tx) => { const maintenance = new MaintenanceRepository(tx); const run = maintenance.run(runId)!; const rows = maintenance.sweepCandidates(runId, state, run.high_water, this.#storage.maxGcBatchSize, this.#runtime.maxQueryBatchBytes); maintenance.applySweep(runId, state, rows, COMPLETE);
    }); }
  #read<T>(callback: (tx: FilesystemSQLiteTransaction) => T): T { return runUnitOfWork(this.#driver, "read", { maxRows: this.#storage.maxFinalTransactionRows, maxBytes: this.#storage.maxFinalTransactionBytes }, callback); }
  #write<T>(callback: (tx: FilesystemSQLiteTransaction) => T): T { return runUnitOfWork(this.#driver, "write", { maxRows: this.#storage.maxFinalTransactionRows, maxBytes: this.#storage.maxFinalTransactionBytes }, callback); }
  #now(): number { return this.#clock(); }
  #encodeCursor(cursor: { phase: number; last: string }): string { return btoa(JSON.stringify(cursor)); }
  #decodeCursor(value: string): { phase: number; last: string } { try { const result = JSON.parse(atob(value)); if (!Number.isInteger(result.phase) || typeof result.last !== "string") throw new Error(); return result; } catch { throw fsError("EINVAL", "verify", undefined, "invalid verification cursor"); } }
}

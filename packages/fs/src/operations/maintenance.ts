import type { RuntimeLimits, StorageLimits } from "../resources/limits.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import { bytesToHex, hexToBytes } from "../cas/bytes.js";
import type {
  FilesystemMaintenance,
  GarbageCollectionOptions,
  GarbageCollectionResult,
  StorageSnapshot,
  VerificationOptions,
  VerificationResult,
  VerificationScope,
} from "../filesystem/types.js";
import { abortError, fsError } from "../filesystem/errors.js";
import type {
  GcRunRow,
  OperationsStorage,
  StorageTransactionPorts,
} from "./storage-ports.js";

const COMPLETE = 4;
const ABANDONED = 5;

export class MaintenanceManager implements FilesystemMaintenance {
  readonly #port: OperationsStorage;
  readonly #storage: StorageLimits;
  readonly #runtime: RuntimeLimits;
  readonly #clock: () => number;
  constructor(
    port: OperationsStorage,
    storage: StorageLimits,
    runtime: RuntimeLimits,
    clock: () => number,
  ) {
    this.#port = port;
    this.#storage = storage;
    this.#runtime = runtime;
    this.#clock = clock;
  }

  async collectGarbage(
    options: GarbageCollectionOptions = {},
  ): Promise<GarbageCollectionResult> {
    if (this.#port.readOnly)
      throw fsError(
        "EROFS",
        "collectGarbage",
        undefined,
        "garbage collection requires writable storage",
      );
    if (options.signal?.aborted) throw abortError();
    const start = performance.now();
    const runId = options.runId ?? globalThis.crypto.randomUUID();
    const maxBatches = options.maxBatches ?? 100_000;
    if (!Number.isSafeInteger(maxBatches) || maxBatches < 0)
      throw fsError(
        "EINVAL",
        "collectGarbage",
        undefined,
        "maxBatches must be a nonnegative safe integer",
      );
    if (maxBatches === 0) {
      const existing = this.#read((tx) => tx.maintenance(this.#storage).run(runId));
      return this.#collectionResult(runId, existing, 0, start);
    }
    const now = this.#now();
    const expiryLimit = Math.max(
      1,
      Math.min(
        this.#storage.maxGcBatchSize,
        this.#storage.maxQueryBatchSize,
        Math.floor((this.#storage.maxFinalTransactionRows - 8) / 2),
      ),
    );
    const cleanupLimit = Math.max(
      1,
      Math.min(
        this.#storage.maxGcBatchSize,
        this.#storage.maxQueryBatchSize,
        this.#storage.maxFinalTransactionRows - 8,
      ),
    );
    let batches = 0;
    try {
      while (batches < maxBatches) {
        if (options.signal?.aborted) throw abortError();
        const expired = this.#write((tx) =>
          tx.staging(this.#storage).expireBatch(now, expiryLimit),
        );
        if (expired) {
          batches += 1;
          continue;
        }
        if (options.signal?.aborted) throw abortError();
        const cleanup = this.#write((tx) =>
          tx.staging(this.#storage).cleanupBatch(cleanupLimit),
        );
        if (cleanup.worked) {
          batches += 1;
          continue;
        }
        const run = this.#read((tx) => tx.maintenance(this.#storage).run(runId));
        if (!run) {
          this.#write((tx) => tx.maintenance(this.#storage).beginRun(runId, now));
          batches += 1;
          continue;
        }
        const state = run.state;
        if (state === COMPLETE || state === ABANDONED) break;
        if (state === 0) this.#markBatch(runId);
        else this.#sweepBatch(runId, state);
        batches += 1;
      }
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        try {
          this.#write((tx) =>
            tx.maintenance(this.#storage).abandonRun(runId, COMPLETE, ABANDONED),
          );
        } catch {}
      }
      throw error;
    }
    const row = this.#read((tx) => tx.maintenance(this.#storage).run(runId));
    if (!row)
      throw fsError(
        "ENOENT",
        "collectGarbage",
        undefined,
        "collection run disappeared",
      );
    return this.#collectionResult(runId, row, batches, start);
  }

  async snapshotStorage(): Promise<StorageSnapshot> {
    const row = this.#read((tx) => tx.maintenance(this.#storage).snapshot());
    if (!row) throw new Error("ECORRUPT: usage metadata is missing");
    const physical = this.#read((tx) => {
      const value = tx.maintenance(this.#storage).physical();
      return Object.freeze({
        mainFileBytes: value.pageCount * value.pageSize,
        freelistBytes: value.freePages * value.pageSize,
      });
    });
    const manifestBytes = row.manifest_root_bytes + row.manifest_node_bytes;
    return Object.freeze({
      rootMutationGeneration: row.generation,
      mainLogicalBytes: row.logical_bytes,
      storedObjectPayloadBytes: row.object_bytes,
      storedManifestPayloadBytes: manifestBytes,
      reachableObjectPayloadBytes: row.object_bytes,
      reachableManifestPayloadBytes: manifestBytes,
      reclaimablePayloadBytes: 0,
      branchPageBytes: row.page_bytes,
      branchPatchBytes: row.patch_bytes,
      branchExclusiveObjectBytes: 0,
      branchExclusiveManifestBytes: 0,
      branchExclusivePayloadBytes: row.page_bytes + row.patch_bytes,
      objectCount: row.object_count,
      manifestRootCount: row.manifest_root_count,
      manifestNodeCount: row.manifest_node_count,
      manifestCount: row.manifest_root_count + row.manifest_node_count,
      chargedMetadataBytes: row.charged_metadata_bytes,
      revisionCount: row.revisions,
      includesNamespaceMetadata: true,
      includesOperationResults: true,
      physical,
    });
  }

  async verify(options: VerificationOptions = {}): Promise<VerificationResult> {
    if (options.signal?.aborted) throw abortError();
    const maximum = options.maxEntities ?? this.#storage.maxGcBatchSize;
    if (!Number.isSafeInteger(maximum) || maximum <= 0)
      throw fsError(
        "EINVAL",
        "verify",
        undefined,
        "maxEntities must be a positive safe integer",
      );
    const scopes = new Set<VerificationScope>(
      options.scopes ?? ["metadata", "namespace", "manifests", "objects", "head"],
    );
    const phases = ["roots", "nodes", "objects", "inodes"] as const;
    let cursor = options.cursor
      ? this.#decodeCursor(options.cursor)
      : { phase: 0, last: "" };
    let checked = 0;
    const generation = this.#read((tx) => {
      const maintenance = tx.maintenance(this.#storage);
      const result = maintenance.generation();
      const repo = tx.content(this.#storage);
      while (checked < maximum && cursor.phase < phases.length) {
        const phase = phases[cursor.phase]!;
        if (
          ((phase === "roots" || phase === "nodes") && !scopes.has("manifests")) ||
          (phase === "objects" && !scopes.has("objects")) ||
          (phase === "inodes" && !scopes.has("namespace") && !scopes.has("head"))
        ) {
          cursor = { phase: cursor.phase + 1, last: "" };
          continue;
        }
        if (phase === "roots" || phase === "nodes") {
          const rows = maintenance.hashes(
            phase,
            cursor.last ? hexToBytes(cursor.last, 32) : new Uint8Array(),
            maximum - checked,
            this.#runtime.maxQueryBatchBytes,
          );
          for (const row of rows) {
            if (phase === "roots") decodeManifestRoot(row.encoded, row.hash);
            else decodeManifestNode(row.encoded, row.hash);
            cursor.last = bytesToHex(row.hash);
            checked += 1;
          }
          if (rows.length < maximum - (checked - rows.length))
            cursor = { phase: cursor.phase + 1, last: "" };
          else break;
        } else if (phase === "objects") {
          const rows = maintenance.objects(
            cursor.last ? hexToBytes(cursor.last, 32) : new Uint8Array(),
            maximum - checked,
            this.#runtime.maxQueryBatchBytes,
          );
          for (const row of rows) {
            const object = repo.getObject(row.hash);
            if (!object || object.byteLength !== row.size)
              throw new Error("ECORRUPT: invalid CAS object");
            cursor.last = bytesToHex(row.hash);
            checked += 1;
          }
          if (rows.length < maximum - (checked - rows.length))
            cursor = { phase: cursor.phase + 1, last: "" };
          else break;
        } else {
          const rows = maintenance.inodes(
            cursor.last,
            maximum - checked,
            this.#runtime.maxQueryBatchBytes,
          );
          for (const row of rows) {
            if (row.type === 0) {
              if (!row.manifest_hash || row.size === null)
                throw new Error("ECORRUPT: file inode lacks content");
              const rootBytes = repo.getManifestRoot(row.manifest_hash);
              if (
                !rootBytes ||
                decodeManifestRoot(rootBytes, row.manifest_hash).fileSize !== row.size
              )
                throw new Error("ECORRUPT: inode manifest size mismatch");
              if (row.actual_links !== row.nlink)
                throw new Error("ECORRUPT: hard-link count mismatch");
            }
            cursor.last = row.id;
            checked += 1;
          }
          if (rows.length < maximum - (checked - rows.length))
            cursor = { phase: cursor.phase + 1, last: "" };
          else break;
        }
      }
      return result;
    });
    return Object.freeze({
      rootMutationGeneration: generation,
      checkedEntities: checked,
      complete: cursor.phase >= phases.length,
      nextCursor: cursor.phase >= phases.length ? null : this.#encodeCursor(cursor),
    });
  }

  #markBatch(runId: string): void {
    this.#write((tx) => {
      const maintenance = tx.maintenance(this.#storage);
      const perMark = 129;
      const limit = Math.max(
        1,
        Math.min(
          this.#storage.maxGcBatchSize,
          Math.floor(this.#storage.maxFinalTransactionRows / perMark),
        ),
      );
      const rows = maintenance.pendingMarks(
        runId,
        limit,
        this.#runtime.maxQueryBatchBytes,
      );
      const repo = tx.content(this.#storage);
      let roots = 0;
      let nodes = 0;
      let objects = 0;
      for (const row of rows) {
        if (row.kind === 0) {
          const encoded = repo.getManifestRoot(row.hash);
          if (!encoded) throw new Error("ECORRUPT: reachable manifest root is missing");
          const root = decodeManifestRoot(encoded, row.hash);
          maintenance.addMark(runId, 1, root.rootNodeHash);
          roots += 1;
        } else if (row.kind === 1) {
          const encoded = repo.getManifestNode(row.hash);
          if (!encoded) throw new Error("ECORRUPT: reachable manifest node is missing");
          const node = decodeManifestNode(encoded, row.hash);
          if (node.kind === "leaf")
            for (const entry of node.entries) maintenance.addMark(runId, 2, entry.hash);
          else
            for (const child of node.children)
              maintenance.addMark(runId, 1, child.hash);
          nodes += 1;
        } else {
          if (!repo.getObject(row.hash))
            throw new Error("ECORRUPT: reachable object is missing");
          objects += 1;
        }
        maintenance.markProcessed(runId, row.kind, row.hash);
      }
      maintenance.addExamined(runId, roots, nodes, objects);
      if (!rows.length) maintenance.reconcileRoots(runId);
    });
  }
  #sweepBatch(runId: string, state: number): void {
    this.#write((tx) => {
      const maintenance = tx.maintenance(this.#storage);
      const run = maintenance.run(runId)!;
      const rows = maintenance.sweepCandidates(
        runId,
        state,
        run.high_water,
        this.#storage.maxGcBatchSize,
        this.#runtime.maxQueryBatchBytes,
      );
      maintenance.applySweep(runId, state, rows, COMPLETE);
    });
  }
  #read<T>(callback: (tx: StorageTransactionPorts) => T): T {
    return this.#port.transaction(
      "read",
      {
        maxRows: this.#storage.maxFinalTransactionRows,
        maxBytes: this.#storage.maxFinalTransactionBytes,
        maxElapsedMs: 250,
      },
      callback,
    );
  }
  #write<T>(callback: (tx: StorageTransactionPorts) => T): T {
    return this.#port.transaction(
      "write",
      {
        maxRows: this.#storage.maxFinalTransactionRows,
        maxBytes: this.#storage.maxFinalTransactionBytes,
        maxElapsedMs: 250,
      },
      callback,
    );
  }
  #now(): number {
    return this.#clock();
  }
  #collectionResult(
    runId: string,
    row: GcRunRow | undefined,
    batches: number,
    start: number,
  ): GarbageCollectionResult {
    return Object.freeze({
      runId,
      state:
        row?.state === COMPLETE
          ? "complete"
          : row?.state === ABANDONED
            ? "abandoned"
            : "paused",
      examinedManifestRootCount: row?.examined_roots ?? 0,
      deletedManifestRootCount: row?.deleted_roots ?? 0,
      examinedManifestNodeCount: row?.examined_nodes ?? 0,
      deletedManifestNodeCount: row?.deleted_nodes ?? 0,
      examinedManifestCount: (row?.examined_roots ?? 0) + (row?.examined_nodes ?? 0),
      deletedManifestCount: (row?.deleted_roots ?? 0) + (row?.deleted_nodes ?? 0),
      examinedObjectCount: row?.examined_objects ?? 0,
      deletedObjectCount: row?.deleted_objects ?? 0,
      reclaimedObjectPayloadBytes: row?.reclaimed_object_bytes ?? 0,
      reclaimedManifestPayloadBytes: row?.reclaimed_manifest_bytes ?? 0,
      reclaimedBranchOverlayPayloadBytes: 0,
      committedBatches: batches,
      elapsedMs: performance.now() - start,
    });
  }
  #encodeCursor(cursor: { phase: number; last: string }): string {
    return btoa(JSON.stringify(cursor));
  }
  #decodeCursor(value: string): { phase: number; last: string } {
    try {
      const result = JSON.parse(atob(value));
      if (!Number.isInteger(result.phase) || typeof result.last !== "string")
        throw new Error();
      return result;
    } catch {
      throw fsError("EINVAL", "verify", undefined, "invalid verification cursor");
    }
  }
}

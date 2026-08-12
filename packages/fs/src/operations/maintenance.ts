import {
  maxPersistedContentObjectBytes,
  type BranchConfiguration,
  DEFAULT_BRANCH_CONFIGURATION,
  type RuntimeLimits,
  type StorageLimits,
} from "../resources/limits.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import { bytesToHex, hexToBytes } from "../cas/bytes.js";
import { sha256 } from "../cas/sha256.js";
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
import type { ContentCache } from "../cache/content-cache.js";
import { utf8ByteLength } from "../namespace/utf8.js";
import type { CowPageBytes } from "../cow/pages.js";

const CLEAN_MARKS = 4;
const CLEAN_ROOT_JOURNAL = 5;
const CLEAN_TERMINAL_RUNS = 6;
const COMPLETE = 7;
const ABANDONED = 8;
const MAX_GC_RUN_ID_BYTES = 256;
const MAX_VERIFICATION_CURSOR_BYTES = 64 * 1024;
const MAX_VERIFICATION_CURSOR_PAYLOAD_BYTES = 48 * 1024;
const USAGE_COUNTER_COUNT = 16;

interface UsageVerificationCursor {
  readonly phase: number;
  readonly lastKey: string | null;
  readonly mutationSequence: number;
  readonly totals: readonly number[];
}
interface VerificationCursorState {
  phase: number;
  last: string;
  rootMutationGeneration: number;
  usage?: UsageVerificationCursor;
}

function encodeBase64Text(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function decodeBase64Text(value: string): string {
  const binary = atob(value);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

export class MaintenanceManager implements FilesystemMaintenance {
  readonly #port: OperationsStorage;
  readonly #storage: StorageLimits;
  readonly #runtime: RuntimeLimits;
  readonly #clock: () => number;
  readonly #cache: ContentCache;
  readonly #pageBytes: CowPageBytes;
  readonly #branch: BranchConfiguration;
  readonly #verificationSecret = globalThis.crypto.getRandomValues(new Uint8Array(32));
  constructor(
    port: OperationsStorage,
    storage: StorageLimits,
    runtime: RuntimeLimits,
    clock: () => number,
    cache: ContentCache,
    pageBytes: CowPageBytes,
    branch: BranchConfiguration = DEFAULT_BRANCH_CONFIGURATION,
  ) {
    this.#port = port;
    this.#storage = storage;
    this.#runtime = runtime;
    this.#clock = clock;
    this.#cache = cache;
    this.#pageBytes = pageBytes;
    this.#branch = branch;
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
    if (
      typeof runId !== "string" ||
      runId.length === 0 ||
      runId.includes("\0") ||
      utf8ByteLength(runId) > MAX_GC_RUN_ID_BYTES
    )
      throw fsError(
        "EINVAL",
        "collectGarbage",
        undefined,
        `runId must encode to at most ${MAX_GC_RUN_ID_BYTES} bytes`,
      );
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
        Math.floor((this.#storage.maxFinalTransactionRows - 8) / 4),
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
    const resultLimit = Math.max(
      1,
      Math.min(
        this.#storage.maxGcBatchSize,
        this.#storage.maxQueryBatchSize,
        this.#storage.maxFinalTransactionRows - 8,
      ),
    );
    const retentionLimit = Math.max(
      1,
      Math.min(
        this.#storage.maxGcBatchSize,
        this.#storage.maxQueryBatchSize,
        Math.floor((this.#storage.maxFinalTransactionRows - 16) / 2),
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
        const overlayCleanup = this.#write((tx) =>
          tx.overlay(this.#storage, this.#pageBytes).cleanupUnleased(cleanupLimit),
        );
        if (overlayCleanup) {
          batches += 1;
          continue;
        }
        const expiredResults = this.#write((tx) =>
          tx.branches(this.#storage).pruneExpiredResults(now, resultLimit),
        );
        if (expiredResults) {
          batches += 1;
          continue;
        }
        const terminalBranches = this.#write((tx) =>
          tx
            .branches(this.#storage)
            .pruneTerminalBranches(
              now,
              this.#branch.terminalBranchRetentionMs,
              resultLimit,
            ),
        );
        if (terminalBranches) {
          batches += 1;
          continue;
        }
        const retained = this.#write((tx) =>
          tx
            .branches(this.#storage)
            .maintainRevisionRetention(
              this.#storage.maxRetainedRevisions,
              now,
              retentionLimit,
            ),
        );
        if (retained) {
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
        if (state === COMPLETE) break;
        if (state === ABANDONED) {
          this.#write((tx) =>
            tx
              .maintenance(this.#storage)
              .resumeAbandonedRun(runId, ABANDONED, CLEAN_MARKS),
          );
          batches += 1;
          continue;
        }
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
    return this.#collectionResult(runId, row, batches, start);
  }

  async snapshotStorage(): Promise<StorageSnapshot> {
    const row = this.#read((tx) => tx.maintenance(this.#storage).snapshot());
    if (!row) throw new Error("ECORRUPT: usage metadata is missing");
    const pagePhysical = this.#read((tx) => {
      const value = tx.maintenance(this.#storage).physical();
      return Object.freeze({
        mainFileBytes: value.pageCount * value.pageSize,
        freelistBytes: value.freePages * value.pageSize,
      });
    });
    const files = this.#port.physicalStorage();
    const physical = Object.freeze({
      mainFileBytes: files.mainFileBytes ?? pagePhysical.mainFileBytes,
      ...(files.walBytes === undefined ? {} : { walBytes: files.walBytes }),
      freelistBytes: pagePhysical.freelistBytes,
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
    const requestedMaximum = options.maxEntities ?? this.#storage.maxGcBatchSize;
    if (!Number.isSafeInteger(requestedMaximum) || requestedMaximum <= 0)
      throw fsError(
        "EINVAL",
        "verify",
        undefined,
        "maxEntities must be a positive safe integer",
      );
    const maximum = Math.min(
      requestedMaximum,
      this.#storage.maxGcBatchSize,
      this.#storage.maxQueryBatchSize,
      this.#storage.maxFinalTransactionRows - 4,
    );
    const scopes = new Set<VerificationScope>(
      options.scopes ?? ["metadata", "namespace", "manifests", "objects", "head"],
    );
    const phases = ["roots", "nodes", "objects", "inodes", "usage"] as const;
    let cursor = options.cursor
      ? this.#decodeCursor(options.cursor)
      : { phase: 0, last: "", rootMutationGeneration: -1 };
    let checked = 0;
    const generation = this.#read((tx) => {
      const maintenance = tx.maintenance(this.#storage);
      const result = maintenance.generation();
      if (cursor.rootMutationGeneration < 0)
        cursor = { ...cursor, rootMutationGeneration: result };
      else if (cursor.rootMutationGeneration !== result)
        throw fsError(
          "EBUSY",
          "verify",
          undefined,
          "root mutation generation changed while verification was in progress",
        );
      const repo = tx.content(this.#storage, this.#cache);
      while (checked < maximum && cursor.phase < phases.length) {
        const phase = phases[cursor.phase]!;
        if (
          ((phase === "roots" || phase === "nodes") && !scopes.has("manifests")) ||
          (phase === "objects" && !scopes.has("objects")) ||
          (phase === "inodes" && !scopes.has("namespace") && !scopes.has("head")) ||
          (phase === "usage" && !scopes.has("metadata"))
        ) {
          cursor = {
            phase: cursor.phase + 1,
            last: "",
            rootMutationGeneration: cursor.rootMutationGeneration,
          };
          continue;
        }
        if (phase === "usage") {
          const usageState = maintenance.usageVerificationState();
          let usage = cursor.usage ?? {
            phase: 0,
            lastKey: null,
            mutationSequence: usageState.mutationSequence,
            totals: Object.freeze(Array.from({ length: USAGE_COUNTER_COUNT }, () => 0)),
          };
          if (usage.mutationSequence !== usageState.mutationSequence)
            throw fsError(
              "EBUSY",
              "verify",
              undefined,
              "durable usage changed while verification was in progress",
            );
          const phaseCount = maintenance.usageVerificationPhaseCount();
          while (checked < maximum && usage.phase < phaseCount) {
            const batch = maintenance.usageVerificationBatch(
              usage.phase,
              usage.lastKey,
              maximum - checked,
              this.#runtime.maxQueryBatchBytes,
            );
            const totals = usage.totals.map((value, index) => {
              const delta = batch.deltas[index];
              if (delta === undefined) throw new Error("invalid usage recount result");
              const total = value + delta;
              if (!Number.isSafeInteger(total) || total < 0)
                throw new Error("ECORRUPT: usage recount overflow");
              return total;
            });
            checked += batch.checkedRows;
            usage = {
              phase: batch.complete ? usage.phase + 1 : usage.phase,
              lastKey: batch.complete ? null : batch.nextKey,
              mutationSequence: usage.mutationSequence,
              totals: Object.freeze(totals),
            };
            if (!batch.complete) break;
          }
          if (usage.phase >= phaseCount) {
            const finalState = maintenance.usageVerificationState();
            if (finalState.mutationSequence !== usage.mutationSequence)
              throw fsError(
                "EBUSY",
                "verify",
                undefined,
                "durable usage changed while verification was in progress",
              );
            if (
              finalState.counters.length !== usage.totals.length ||
              finalState.counters.some((value, index) => value !== usage.totals[index])
            )
              throw fsError(
                "ECORRUPT",
                "verify",
                undefined,
                "authoritative usage differs from the bounded durable recount",
              );
            cursor = {
              phase: cursor.phase + 1,
              last: "",
              rootMutationGeneration: cursor.rootMutationGeneration,
            };
            continue;
          }
          cursor = {
            phase: cursor.phase,
            last: "",
            rootMutationGeneration: cursor.rootMutationGeneration,
            usage,
          };
          break;
        }
        if (phase === "roots" || phase === "nodes") {
          const remaining = maximum - checked;
          const rowCapacity = Math.max(
            1,
            Math.floor(
              this.#runtime.maxQueryBatchBytes /
                (phase === "nodes" ? this.#storage.maxManifestNodeBytes + 256 : 512),
            ),
          );
          const rowLimit = Math.min(remaining, rowCapacity);
          const rows = maintenance.hashes(
            phase,
            cursor.last ? hexToBytes(cursor.last, 32) : Uint8Array.of(0),
            rowLimit,
            this.#runtime.maxQueryBatchBytes,
          );
          for (const row of rows) {
            if (phase === "roots") decodeManifestRoot(row.encoded, row.hash);
            else decodeManifestNode(row.encoded, row.hash);
            cursor.last = bytesToHex(row.hash);
            checked += 1;
          }
          if (rows.length < rowLimit)
            cursor = {
              phase: cursor.phase + 1,
              last: "",
              rootMutationGeneration: cursor.rootMutationGeneration,
            };
          else break;
        } else if (phase === "objects") {
          const rowLimit = Math.min(
            maximum - checked,
            Math.max(
              1,
              Math.floor(
                (this.#storage.maxFinalTransactionBytes - 512) /
                  (maxPersistedContentObjectBytes(this.#storage) + 512),
              ),
            ),
          );
          const rows = maintenance.objects(
            cursor.last ? hexToBytes(cursor.last, 32) : Uint8Array.of(0),
            rowLimit,
            this.#runtime.maxQueryBatchBytes,
          );
          for (const row of rows) {
            if (!repo.verifyObject(row.hash, row.size, true))
              throw new Error("ECORRUPT: invalid CAS object");
            cursor.last = bytesToHex(row.hash);
            checked += 1;
          }
          if (rows.length < rowLimit)
            cursor = {
              phase: cursor.phase + 1,
              last: "",
              rootMutationGeneration: cursor.rootMutationGeneration,
            };
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
              const fileSize = repo.withManifestRoot(
                row.manifest_hash,
                (rootBytes) =>
                  decodeManifestRoot(rootBytes, row.manifest_hash!).fileSize,
              );
              if (fileSize === undefined || fileSize !== row.size)
                throw new Error("ECORRUPT: inode manifest size mismatch");
              if (row.actual_links !== row.nlink)
                throw new Error("ECORRUPT: hard-link count mismatch");
            }
            cursor.last = row.id;
            checked += 1;
          }
          if (rows.length < maximum - (checked - rows.length))
            cursor = {
              phase: cursor.phase + 1,
              last: "",
              rootMutationGeneration: cursor.rootMutationGeneration,
            };
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
      const edgeLimit = Math.max(
        1,
        Math.min(128, Math.floor((this.#storage.maxFinalTransactionRows - 4) / 2)),
      );
      const rows = maintenance.pendingMarks(runId, 1, this.#runtime.maxQueryBatchBytes);
      const repo = tx.content(this.#storage, this.#cache);
      let roots = 0;
      let nodes = 0;
      let objects = 0;
      for (const row of rows) {
        if (row.kind === 0) {
          const root = repo.withManifestRoot(row.hash, (encoded) =>
            decodeManifestRoot(encoded, row.hash),
          );
          if (!root) throw new Error("ECORRUPT: reachable manifest root is missing");
          if (row.edge_cursor > 0)
            throw new Error("ECORRUPT: invalid manifest-root GC edge cursor");
          maintenance.addMark(runId, 1, root.rootNodeHash);
          maintenance.advanceMark(runId, row.kind, row.hash, 1, true);
          roots = 1;
        } else if (row.kind === 1) {
          const node = repo.withManifestNode(row.hash, (encoded) =>
            decodeManifestNode(encoded, row.hash),
          );
          if (!node) throw new Error("ECORRUPT: reachable manifest node is missing");
          const edges = node.kind === "leaf" ? node.entries : node.children;
          if (row.edge_cursor > edges.length)
            throw new Error("ECORRUPT: invalid manifest-node GC edge cursor");
          const end = Math.min(edges.length, row.edge_cursor + edgeLimit);
          for (let index = row.edge_cursor; index < end; index += 1)
            maintenance.addMark(
              runId,
              node.kind === "leaf" ? 2 : 1,
              edges[index]!.hash,
            );
          const complete = end === edges.length;
          maintenance.advanceMark(runId, row.kind, row.hash, end, complete);
          if (complete) nodes = 1;
        } else {
          if (row.edge_cursor > 0)
            throw new Error("ECORRUPT: invalid object GC edge cursor");
          if (!repo.verifyObject(row.hash, undefined, true))
            throw new Error("ECORRUPT: reachable object is missing");
          maintenance.advanceMark(runId, row.kind, row.hash, 0, true);
          objects = 1;
        }
      }
      maintenance.addExamined(runId, roots, nodes, objects);
      if (!rows.length)
        maintenance.seedRootsBatch(
          runId,
          Math.max(
            1,
            Math.min(
              this.#storage.maxGcBatchSize,
              this.#storage.maxQueryBatchSize,
              Math.floor((this.#storage.maxFinalTransactionRows - 4) / 2),
            ),
          ),
          this.#runtime.maxQueryBatchBytes,
        );
    });
  }
  #sweepBatch(runId: string, state: number): void {
    this.#write((tx) => {
      const maintenance = tx.maintenance(this.#storage);
      const cleanupLimit = Math.max(
        1,
        Math.min(
          this.#storage.maxGcBatchSize,
          this.#storage.maxQueryBatchSize,
          this.#storage.maxFinalTransactionRows - 4,
        ),
      );
      if (state === CLEAN_MARKS) {
        maintenance.cleanupMarks(runId, cleanupLimit, CLEAN_ROOT_JOURNAL);
        return;
      }
      if (state === CLEAN_ROOT_JOURNAL) {
        maintenance.cleanupRootJournal(runId, cleanupLimit, CLEAN_TERMINAL_RUNS);
        return;
      }
      if (state === CLEAN_TERMINAL_RUNS) {
        maintenance.cleanupTerminalRuns(
          runId,
          cleanupLimit,
          COMPLETE,
          ABANDONED,
          COMPLETE,
        );
        return;
      }
      if (state < 1 || state > 3)
        throw new Error("ECORRUPT: invalid garbage-collection state");
      if (!maintenance.reconcileSweepGeneration(runId, state)) return;
      const run = maintenance.run(runId)!;
      const rowLimit = Math.max(
        1,
        Math.min(
          this.#storage.maxGcBatchSize,
          this.#storage.maxQueryBatchSize,
          state === 1
            ? Math.floor((this.#storage.maxFinalTransactionRows - 8) / 2)
            : this.#storage.maxFinalTransactionRows - 8,
        ),
      );
      const candidates = maintenance.sweepCandidates(
        runId,
        state,
        run.high_water,
        rowLimit,
        this.#runtime.maxQueryBatchBytes,
      );
      const payloadLimit = Math.max(
        1,
        Math.min(
          this.#runtime.maxQueryBatchBytes,
          Math.floor(this.#storage.maxFinalTransactionBytes / 4),
        ),
      );
      const rows: (typeof candidates)[number][] = [];
      let payloadBytes = 0;
      for (const candidate of candidates) {
        if (
          rows.length &&
          (rows.length >= rowLimit || payloadBytes + candidate.size > payloadLimit)
        )
          break;
        rows.push(candidate);
        payloadBytes += candidate.size;
      }
      maintenance.applySweep(runId, state, rows, CLEAN_MARKS);
    });
  }
  #read<T>(callback: (tx: StorageTransactionPorts) => T): T {
    return this.#port.transaction(
      "read",
      {
        maxRows: this.#storage.maxFinalTransactionRows,
        maxBytes: this.#storage.maxFinalTransactionBytes,
        maxElapsedMs: 1_000,
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
        maxElapsedMs: 1_000,
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
  #encodeCursor(cursor: VerificationCursorState): string {
    const payload = JSON.stringify(cursor);
    if (utf8ByteLength(payload) > MAX_VERIFICATION_CURSOR_PAYLOAD_BYTES)
      throw fsError("EFBIG", "verify", undefined, "verification cursor exceeds limit");
    return `${encodeBase64Text(payload)}.${bytesToHex(this.#cursorDigest(payload))}`;
  }
  #decodeCursor(value: string): VerificationCursorState {
    try {
      if (typeof value !== "string" || value.length > MAX_VERIFICATION_CURSOR_BYTES)
        throw new Error();
      const separator = value.lastIndexOf(".");
      if (
        separator <= 0 ||
        separator > MAX_VERIFICATION_CURSOR_PAYLOAD_BYTES * 2 ||
        value.length - separator - 1 !== 64
      )
        throw new Error();
      const encodedPayload = value.slice(0, separator);
      const payload = decodeBase64Text(encodedPayload);
      if (utf8ByteLength(payload) > MAX_VERIFICATION_CURSOR_PAYLOAD_BYTES)
        throw new Error();
      if (bytesToHex(this.#cursorDigest(payload)) !== value.slice(separator + 1))
        throw new Error();
      const result = JSON.parse(payload);
      if (
        !Number.isInteger(result.phase) ||
        result.phase < 0 ||
        typeof result.last !== "string" ||
        !Number.isSafeInteger(result.rootMutationGeneration) ||
        result.rootMutationGeneration < 0
      )
        throw new Error();
      if (result.usage !== undefined) {
        const usage = result.usage;
        if (
          !Number.isInteger(usage.phase) ||
          usage.phase < 0 ||
          (usage.lastKey !== null && typeof usage.lastKey !== "string") ||
          !Number.isSafeInteger(usage.mutationSequence) ||
          usage.mutationSequence < 0 ||
          !Array.isArray(usage.totals) ||
          usage.totals.length !== USAGE_COUNTER_COUNT ||
          usage.totals.some(
            (counter: unknown) =>
              !Number.isSafeInteger(counter) || (counter as number) < 0,
          )
        )
          throw new Error();
      }
      return result;
    } catch {
      throw fsError("EINVAL", "verify", undefined, "invalid verification cursor");
    }
  }
  #cursorDigest(payload: string): Uint8Array {
    const encoded = new TextEncoder().encode(payload);
    const input = new Uint8Array(this.#verificationSecret.length + encoded.length);
    input.set(this.#verificationSecret);
    input.set(encoded, this.#verificationSecret.length);
    return sha256(input);
  }
}

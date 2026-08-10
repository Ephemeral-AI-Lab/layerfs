export interface FilesystemLimits {
  readonly maxPathBytes: number;
  readonly maxNameBytes: number;
  readonly maxSymlinkTargetBytes: number;
  readonly maxSymlinkTraversals: number;
  readonly maxMaterializedBytes: number;
  readonly preferredStreamChunkBytes: number;
  readonly maxAtomicTreeEntries: number;
  readonly maxReaddirEntries: number;
}
export interface StorageLimits {
  readonly maxManifestEntries: number;
  readonly maxManifestNodeBytes: number;
  readonly maxManifestDepth: number;
  readonly maxFileBytes: number;
  readonly maxWriteBytes: number;
  readonly maxManagedPayloadBytes: number;
  readonly maxChargedMetadataBytes: number;
  readonly maxPhysicalDatabaseBytes: number;
  readonly maxJournalBytes: number;
  readonly maxStagingPayloadBytes: number;
  readonly maxBranchOverlayBytes: number;
  readonly maxMaintenanceBytes: number;
  readonly maintenanceReserveBytes: number;
  readonly maxPermanentIdentifiers: number;
  readonly maxFinalTransactionRows: number;
  readonly maxFinalTransactionBytes: number;
  readonly maxRevisionReplaySteps: number;
  readonly maxPatchesPerFile: number;
  readonly maxPatchBytesPerFile: number;
  readonly maxQueryBatchSize: number;
  readonly maxGcBatchSize: number;
  readonly maxRetainedRevisions: number;
  readonly readLeaseMs: number;
  readonly stagingLeaseMs: number;
}
export interface RuntimeLimits {
  readonly maxManagedResidentBytes: number;
  readonly maxCacheBytes: number;
  readonly maxPendingWriteBytes: number;
  readonly maxWriteSessionBytes: number;
  readonly maxPrefetchBytes: number;
  readonly maxQueryBatchBytes: number;
  readonly maxPreparedResultBytes: number;
  readonly maxConcurrentStreams: number;
  readonly maxConcurrentOperations: number;
  readonly maxOpenBranchHandles: number;
  readonly maxOpenNodeVfsSessions: number;
}
export interface BranchConfiguration {
  readonly maxBranchIdBytes: number;
  readonly maxOperationIdBytes: number;
  readonly maxActiveBranches: number;
  readonly maxChangedPathsPerBranch: number;
  readonly maxChangedPathBytes: number;
  readonly maxConflictsPerPublication: number;
  readonly maxConflictResultBytes: number;
  readonly terminalBranchRetentionMs: number;
  readonly publicationResultRetentionMs: number;
}

/** Structural adapter limits consumed by resource policy without depending on SQLite. */
export interface StorageAdapterLimits {
  readonly maxBlobBytes: number;
  readonly maxBindings: number;
  readonly maxPhysicalDatabaseBytes: number;
  readonly maxJournalBytes: number;
}

/** Hard version-0.1 content-object/streaming CDC allocation ceiling. */
export const MAX_CONTENT_OBJECT_BYTES = 16 * 1024 * 1024;
export const DEFAULT_FASTCDC_MINIMUM_BYTES = 32_768;
export const DEFAULT_FASTCDC_MAXIMUM_BYTES = 524_288;
/** Conservative per-object binding/row/index envelope in a durable transaction. */
export const CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES = 16 * 1024;

export function maxPersistedContentObjectBytes(
  storage: Pick<StorageLimits, "maxFinalTransactionBytes">,
): number {
  return Math.max(
    0,
    Math.min(
      MAX_CONTENT_OBJECT_BYTES,
      storage.maxFinalTransactionBytes - CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES,
    ),
  );
}
/** Additional caller input one collecting FastCDC push may return with a prebuffer. */
export const MAX_CONTENT_COLLECTOR_PUSH_BYTES = 1024 * 1024;
/** Maximum retained chunk references returned by one collecting push call. */
export const MAX_CONTENT_COLLECTOR_REFERENCES = 16_384;
/** Conservative allocated-capacity charge for one JavaScript array element slot. */
export const CONTENT_COLLECTOR_REFERENCE_BYTES = 16;
/**
 * Source/carry, chunker, emitted chunk, sink handoff, retained object, and
 * replacement-window copies may coexist in the bounded rebuild pipeline.
 */
export const MAX_CONTENT_WORKING_SET_COPIES = 6;
export const MIN_CANONICAL_MANIFEST_NODE_BYTES = 9248;
export const DURABLE_METADATA_ROW_BYTES = 512;
export const MAX_MAINTENANCE_RUN_ROW_BYTES = 1024;
export const MAX_MAINTENANCE_MARK_ROW_BYTES = 704;
export const MAINTENANCE_CLEANUP_ROW_BYTES = 512;
export const MAINTENANCE_GC_EMERGENCY_BYTES =
  MAX_MAINTENANCE_RUN_ROW_BYTES + MAX_MAINTENANCE_MARK_ROW_BYTES;
export const MAINTENANCE_TOTAL_EMERGENCY_BYTES =
  MAINTENANCE_GC_EMERGENCY_BYTES + MAINTENANCE_CLEANUP_ROW_BYTES;
export const MIN_MAINTENANCE_BYTES =
  MAINTENANCE_TOTAL_EMERGENCY_BYTES + Math.max(640, MAX_MAINTENANCE_MARK_ROW_BYTES);

function validateCowPageBytes(cowPageBytes: number): 4096 | 8192 | 16384 {
  if (cowPageBytes !== 4096 && cowPageBytes !== 8192 && cowPageBytes !== 16384)
    throw new RangeError("cowPageBytes must be exactly 4096, 8192, or 16384");
  return cowPageBytes;
}

export const DEFAULT_FILESYSTEM_LIMITS: FilesystemLimits = Object.freeze({
  maxPathBytes: 4096,
  maxNameBytes: 255,
  maxSymlinkTargetBytes: 4096,
  maxSymlinkTraversals: 40,
  maxMaterializedBytes: 64 * 1024 * 1024,
  preferredStreamChunkBytes: 256 * 1024,
  maxAtomicTreeEntries: 10_000,
  maxReaddirEntries: 10_000,
});
export const DEFAULT_STORAGE_LIMITS: StorageLimits = Object.freeze({
  maxManifestEntries: 0xffff_ffff,
  maxManifestNodeBytes: 16 * 1024,
  maxManifestDepth: 8,
  maxFileBytes: 16 * 1024 ** 3,
  maxWriteBytes: 64 * 1024 ** 2,
  maxManagedPayloadBytes: 8 * 1024 ** 3,
  maxChargedMetadataBytes: 1024 ** 3,
  maxPhysicalDatabaseBytes: 10 * 1024 ** 3,
  maxJournalBytes: 1024 ** 3,
  maxStagingPayloadBytes: 512 * 1024 ** 2,
  maxBranchOverlayBytes: 1024 ** 3,
  maxMaintenanceBytes: 64 * 1024 ** 2,
  maintenanceReserveBytes: 64 * 1024 ** 2,
  maxPermanentIdentifiers: 10_000_000,
  maxFinalTransactionRows: 100_000,
  maxFinalTransactionBytes:
    MAX_CONTENT_OBJECT_BYTES + CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES,
  maxRevisionReplaySteps: 1_000,
  maxPatchesPerFile: 256,
  maxPatchBytesPerFile: 16 * 1024 ** 2,
  maxQueryBatchSize: 256,
  maxGcBatchSize: 1_000,
  maxRetainedRevisions: 1_000,
  readLeaseMs: 300_000,
  stagingLeaseMs: 900_000,
});
export const DEFAULT_RUNTIME_LIMITS: RuntimeLimits = Object.freeze({
  maxManagedResidentBytes: 128 * 1024 ** 2,
  maxCacheBytes: 64 * 1024 ** 2,
  maxPendingWriteBytes: 64 * 1024 ** 2,
  maxWriteSessionBytes: 16 * 1024 ** 2,
  maxPrefetchBytes: 1024 ** 2,
  maxQueryBatchBytes: 2 * 1024 ** 2,
  maxPreparedResultBytes: 64 * 1024 ** 2,
  maxConcurrentStreams: 64,
  maxConcurrentOperations: 256,
  maxOpenBranchHandles: 1_024,
  maxOpenNodeVfsSessions: 256,
});
export const DEFAULT_BRANCH_CONFIGURATION: BranchConfiguration = Object.freeze({
  maxBranchIdBytes: 128,
  maxOperationIdBytes: 128,
  maxActiveBranches: 1_000,
  maxChangedPathsPerBranch: 100_000,
  maxChangedPathBytes: 16 * 1024 ** 2,
  maxConflictsPerPublication: 10_000,
  maxConflictResultBytes: 4 * 1024 ** 2,
  terminalBranchRetentionMs: 30 * 24 * 60 * 60 * 1000,
  publicationResultRetentionMs: 30 * 24 * 60 * 60 * 1000,
});

export function resolveLimits<T extends object>(
  defaults: T,
  configured?: Partial<T>,
): Readonly<T> {
  return Object.freeze({ ...defaults, ...configured });
}

export function persistedWriterProfile(
  filesystem: Readonly<FilesystemLimits>,
  storage: Readonly<StorageLimits>,
  branch: Readonly<BranchConfiguration>,
): string {
  const domain = (value: Readonly<Record<string, number>>) =>
    Object.entries(value).sort(([left], [right]) => left.localeCompare(right));
  return JSON.stringify({
    branch: domain(branch as unknown as Readonly<Record<string, number>>),
    filesystem: domain(filesystem as unknown as Readonly<Record<string, number>>),
    storage: domain(storage as unknown as Readonly<Record<string, number>>),
  });
}

export function constrainStorageLimits(
  configured: Partial<StorageLimits> | undefined,
  adapter: StorageAdapterLimits,
): Readonly<StorageLimits> {
  const limits = resolveLimits(DEFAULT_STORAGE_LIMITS, configured);
  if (
    !Number.isSafeInteger(limits.maxManifestDepth) ||
    limits.maxManifestDepth < 1 ||
    limits.maxManifestDepth > 64
  )
    throw new RangeError("maxManifestDepth must be between 1 and 64");
  let entriesByDepth = 256;
  for (let depth = 1; depth < limits.maxManifestDepth; depth += 1)
    entriesByDepth = Math.min(0xffff_ffff, entriesByDepth * 128);
  const effectiveManifestEntries = Math.min(
    limits.maxManifestEntries,
    entriesByDepth,
    0xffff_ffff,
  );
  const fileBytesByEntries = Math.min(
    Number.MAX_SAFE_INTEGER,
    effectiveManifestEntries * (DEFAULT_FASTCDC_MINIMUM_BYTES + 1),
  );
  const result = {
    ...limits,
    maxManifestEntries: effectiveManifestEntries,
    maxFileBytes: Math.min(limits.maxFileBytes, fileBytesByEntries),
    maxWriteBytes: Math.min(limits.maxWriteBytes, adapter.maxBlobBytes),
    maxFinalTransactionBytes: Math.min(
      limits.maxFinalTransactionBytes,
      adapter.maxBlobBytes + CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES,
    ),
    maxPhysicalDatabaseBytes: Math.min(
      limits.maxPhysicalDatabaseBytes,
      adapter.maxPhysicalDatabaseBytes,
    ),
    maxJournalBytes: Math.min(limits.maxJournalBytes, adapter.maxJournalBytes),
    maxQueryBatchSize: Math.min(limits.maxQueryBatchSize, adapter.maxBindings),
  };
  for (const [name, value] of Object.entries(result))
    if (!Number.isSafeInteger(value) || value <= 0)
      throw new RangeError(`${name} must be a positive safe integer`);
  if (
    result.maxManifestNodeBytes < 9248 ||
    result.maxManifestNodeBytes > adapter.maxBlobBytes
  )
    throw new RangeError("adapter cannot admit canonical manifest nodes");
  if (adapter.maxBindings < 8)
    throw new RangeError("adapter must support at least eight bindings");
  const minimumTransactionRows = Math.max(64, result.maxManifestDepth * 4 + 16);
  if (result.maxFinalTransactionRows > Math.floor(Number.MAX_SAFE_INTEGER / 4))
    throw new RangeError(
      "maxFinalTransactionRows exceeds the safe derived statement envelope",
    );
  if (result.maxPatchesPerFile > Math.floor(Number.MAX_SAFE_INTEGER / 32))
    throw new RangeError(
      "maxPatchesPerFile exceeds the safe structural segment envelope",
    );
  if (result.maxFinalTransactionRows < minimumTransactionRows)
    throw new RangeError(
      `maxFinalTransactionRows must be at least ${minimumTransactionRows} for the configured manifest depth`,
    );
  if (
    result.maxFinalTransactionBytes <
    result.maxManifestNodeBytes + CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES
  )
    throw new RangeError(
      "maxFinalTransactionBytes cannot persist one canonical manifest node",
    );
  if (maxPersistedContentObjectBytes(result) < DEFAULT_FASTCDC_MAXIMUM_BYTES)
    throw new RangeError(
      "storage transaction/blob limits cannot persist the default FastCDC maximum",
    );
  if (
    result.maxMaintenanceBytes < MIN_MAINTENANCE_BYTES ||
    result.maintenanceReserveBytes < MIN_MAINTENANCE_BYTES
  )
    throw new RangeError(
      `maintenance limits must reserve at least ${MIN_MAINTENANCE_BYTES} bytes for bounded progress`,
    );
  if (result.maintenanceReserveBytes >= result.maxManagedPayloadBytes)
    throw new RangeError(
      "maintenance reserve must be smaller than managed payload limit",
    );
  return Object.freeze(result);
}

export function validateRuntimeLimits(
  filesystem: FilesystemLimits,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  cowPageBytes: number,
): void {
  filesystem = Object.freeze({ ...filesystem });
  storage = Object.freeze({ ...storage });
  runtime = Object.freeze({ ...runtime });
  cowPageBytes = validateCowPageBytes(cowPageBytes);
  for (const [name, value] of Object.entries({ ...filesystem, ...runtime }))
    if (!Number.isSafeInteger(value) || value <= 0)
      throw new RangeError(`${name} must be a positive safe integer`);
  if (filesystem.maxMaterializedBytes > runtime.maxPreparedResultBytes)
    throw new RangeError("maxMaterializedBytes exceeds maxPreparedResultBytes");
  if (runtime.maxWriteSessionBytes > runtime.maxPendingWriteBytes)
    throw new RangeError("maxWriteSessionBytes exceeds aggregate pending-write limit");
  if (runtime.maxPendingWriteBytes < maxPersistedContentObjectBytes(storage))
    throw new RangeError(
      "maxPendingWriteBytes cannot hold one supported durable content object",
    );
  if (
    runtime.maxQueryBatchBytes <
    storage.maxManifestNodeBytes + DURABLE_METADATA_ROW_BYTES
  )
    throw new RangeError(
      "maxQueryBatchBytes cannot hold one canonical manifest query row",
    );
  const progress = requiredRuntimeProgressBytes(filesystem, storage, cowPageBytes);
  if (runtime.maxManagedResidentBytes < progress)
    throw new RangeError(
      "managed-memory limit cannot hold the minimum progress working set",
    );
}

export function requiredRuntimeProgressBytes(
  filesystem: FilesystemLimits,
  storage: StorageLimits,
  cowPageBytes: number,
): number {
  const preferredStreamChunkBytes = checkedInteger(
    filesystem.preferredStreamChunkBytes,
    "preferredStreamChunkBytes",
  );
  const maxManifestNodeBytes = checkedInteger(
    storage.maxManifestNodeBytes,
    "maxManifestNodeBytes",
  );
  cowPageBytes = validateCowPageBytes(cowPageBytes);
  if (preferredStreamChunkBytes === 0)
    throw new RangeError("preferredStreamChunkBytes must be positive");
  if (maxManifestNodeBytes < MIN_CANONICAL_MANIFEST_NODE_BYTES)
    throw new RangeError(
      `maxManifestNodeBytes must be at least ${MIN_CANONICAL_MANIFEST_NODE_BYTES}`,
    );
  let progress = checkedMultiply(
    MAX_CONTENT_OBJECT_BYTES,
    MAX_CONTENT_WORKING_SET_COPIES,
    "content working-set bytes",
  );
  progress = checkedAdd(progress, cowPageBytes, "runtime progress bytes");
  progress = checkedAdd(
    progress,
    MAX_CONTENT_COLLECTOR_PUSH_BYTES,
    "runtime collector output bytes",
  );
  progress = checkedAdd(
    progress,
    checkedMultiply(
      MAX_CONTENT_COLLECTOR_REFERENCES,
      CONTENT_COLLECTOR_REFERENCE_BYTES,
      "runtime collector reference bytes",
    ),
    "runtime progress bytes",
  );
  progress = checkedAdd(
    progress,
    checkedMultiply(maxManifestNodeBytes, 2, "manifest progress bytes"),
    "runtime progress bytes",
  );
  return checkedAdd(progress, preferredStreamChunkBytes, "runtime progress bytes");
}

export class AdmissionController {
  readonly #limit: number;
  #used = 0;
  #peak = 0;
  constructor(limit: number) {
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("admission limit must be a positive safe integer");
    this.#limit = limit;
  }
  reserve(bytes: number): () => void {
    if (!Number.isSafeInteger(bytes) || bytes < 0 || this.#used + bytes > this.#limit)
      throw new RangeError("managed resident memory limit exceeded");
    this.#used += bytes;
    this.#peak = Math.max(this.#peak, this.#used);
    let active = true;
    return () => {
      if (active) {
        active = false;
        this.#used -= bytes;
      }
    };
  }
  get usedBytes(): number {
    return this.#used;
  }
  get peakBytes(): number {
    return this.#peak;
  }
  get limitBytes(): number {
    return this.#limit;
  }
}
import { checkedAdd, checkedInteger, checkedMultiply } from "./safe-integers.js";

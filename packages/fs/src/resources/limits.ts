import type { SQLiteDriverCapabilities } from "../sqlite-driver.js";

export interface FilesystemLimits {
  readonly maxPathBytes: number; readonly maxNameBytes: number; readonly maxSymlinkTargetBytes: number; readonly maxSymlinkTraversals: number;
  readonly maxMaterializedBytes: number; readonly preferredStreamChunkBytes: number; readonly maxAtomicTreeEntries: number; readonly maxReaddirEntries: number;
}
export interface StorageLimits {
  readonly maxManifestEntries: number; readonly maxManifestNodeBytes: number; readonly maxManifestDepth: number; readonly maxFileBytes: number; readonly maxWriteBytes: number;
  readonly maxManagedPayloadBytes: number; readonly maxChargedMetadataBytes: number; readonly maxPhysicalDatabaseBytes: number; readonly maxJournalBytes: number;
  readonly maxStagingPayloadBytes: number; readonly maxBranchOverlayBytes: number; readonly maxMaintenanceBytes: number; readonly maintenanceReserveBytes: number;
  readonly maxPermanentIdentifiers: number; readonly maxFinalTransactionRows: number; readonly maxFinalTransactionBytes: number; readonly maxRevisionReplaySteps: number;
  readonly maxPatchesPerFile: number; readonly maxPatchBytesPerFile: number; readonly maxQueryBatchSize: number; readonly maxGcBatchSize: number;
  readonly maxRetainedRevisions: number; readonly readLeaseMs: number; readonly stagingLeaseMs: number;
}
export interface RuntimeLimits {
  readonly maxManagedResidentBytes: number; readonly maxCacheBytes: number; readonly maxPendingWriteBytes: number; readonly maxWriteSessionBytes: number;
  readonly maxPrefetchBytes: number; readonly maxQueryBatchBytes: number; readonly maxPreparedResultBytes: number; readonly maxConcurrentStreams: number;
  readonly maxConcurrentOperations: number; readonly maxOpenBranchHandles: number; readonly maxOpenNodeVfsSessions: number;
}
export interface BranchConfiguration {
  readonly maxBranchIdBytes: number; readonly maxOperationIdBytes: number; readonly maxActiveBranches: number; readonly maxChangedPathsPerBranch: number;
  readonly maxChangedPathBytes: number; readonly maxConflictsPerPublication: number; readonly maxConflictResultBytes: number;
  readonly terminalBranchRetentionMs: number; readonly publicationResultRetentionMs: number;
}

export const DEFAULT_FILESYSTEM_LIMITS: FilesystemLimits = Object.freeze({ maxPathBytes: 4096, maxNameBytes: 255, maxSymlinkTargetBytes: 4096, maxSymlinkTraversals: 40, maxMaterializedBytes: 64 * 1024 * 1024, preferredStreamChunkBytes: 256 * 1024, maxAtomicTreeEntries: 10_000, maxReaddirEntries: 10_000 });
export const DEFAULT_STORAGE_LIMITS: StorageLimits = Object.freeze({ maxManifestEntries: 0xffff_ffff, maxManifestNodeBytes: 16 * 1024, maxManifestDepth: 8, maxFileBytes: 16 * 1024 ** 3, maxWriteBytes: 64 * 1024 ** 2, maxManagedPayloadBytes: 8 * 1024 ** 3, maxChargedMetadataBytes: 1024 ** 3, maxPhysicalDatabaseBytes: 10 * 1024 ** 3, maxJournalBytes: 1024 ** 3, maxStagingPayloadBytes: 512 * 1024 ** 2, maxBranchOverlayBytes: 1024 ** 3, maxMaintenanceBytes: 64 * 1024 ** 2, maintenanceReserveBytes: 64 * 1024 ** 2, maxPermanentIdentifiers: 10_000_000, maxFinalTransactionRows: 100_000, maxFinalTransactionBytes: 16 * 1024 ** 2, maxRevisionReplaySteps: 1_000, maxPatchesPerFile: 256, maxPatchBytesPerFile: 16 * 1024 ** 2, maxQueryBatchSize: 256, maxGcBatchSize: 1_000, maxRetainedRevisions: 1_000, readLeaseMs: 300_000, stagingLeaseMs: 900_000 });
export const DEFAULT_RUNTIME_LIMITS: RuntimeLimits = Object.freeze({ maxManagedResidentBytes: 128 * 1024 ** 2, maxCacheBytes: 16 * 1024 ** 2, maxPendingWriteBytes: 32 * 1024 ** 2, maxWriteSessionBytes: 8 * 1024 ** 2, maxPrefetchBytes: 4 * 1024 ** 2, maxQueryBatchBytes: 4 * 1024 ** 2, maxPreparedResultBytes: 4 * 1024 ** 2, maxConcurrentStreams: 64, maxConcurrentOperations: 128, maxOpenBranchHandles: 256, maxOpenNodeVfsSessions: 64 });
export const DEFAULT_BRANCH_CONFIGURATION: BranchConfiguration = Object.freeze({ maxBranchIdBytes: 128, maxOperationIdBytes: 128, maxActiveBranches: 1_000, maxChangedPathsPerBranch: 100_000, maxChangedPathBytes: 16 * 1024 ** 2, maxConflictsPerPublication: 10_000, maxConflictResultBytes: 4 * 1024 ** 2, terminalBranchRetentionMs: 30 * 24 * 60 * 60 * 1000, publicationResultRetentionMs: 30 * 24 * 60 * 60 * 1000 });

export function resolveLimits<T extends object>(defaults: T, configured?: Partial<T>): Readonly<T> { return Object.freeze({ ...defaults, ...configured }); }

export function constrainStorageLimits(configured: Partial<StorageLimits> | undefined, adapter: SQLiteDriverCapabilities): Readonly<StorageLimits> {
  const limits = resolveLimits(DEFAULT_STORAGE_LIMITS, configured);
  const result = { ...limits, maxPhysicalDatabaseBytes: Math.min(limits.maxPhysicalDatabaseBytes, adapter.maxPhysicalDatabaseBytes), maxJournalBytes: Math.min(limits.maxJournalBytes, adapter.maxJournalBytes) };
  if (result.maxManifestNodeBytes < 9248 || result.maxManifestNodeBytes > adapter.maxBlobBytes) throw new RangeError("adapter cannot admit canonical manifest nodes");
  if (adapter.maxBindings < 8) throw new RangeError("adapter must support at least eight bindings");
  if (result.maintenanceReserveBytes >= result.maxManagedPayloadBytes) throw new RangeError("maintenance reserve must be smaller than managed payload limit");
  return Object.freeze(result);
}

export class AdmissionController {
  readonly #limit: number; #used = 0; #peak = 0;
  constructor(limit: number) { if (!Number.isSafeInteger(limit) || limit <= 0) throw new RangeError("admission limit must be a positive safe integer"); this.#limit = limit; }
  reserve(bytes: number): () => void {
    if (!Number.isSafeInteger(bytes) || bytes < 0 || this.#used + bytes > this.#limit) throw new RangeError("managed resident memory limit exceeded");
    this.#used += bytes; this.#peak = Math.max(this.#peak, this.#used); let active = true;
    return () => { if (active) { active = false; this.#used -= bytes; } };
  }
  get usedBytes(): number { return this.#used; } get peakBytes(): number { return this.#peak; } get limitBytes(): number { return this.#limit; }
}


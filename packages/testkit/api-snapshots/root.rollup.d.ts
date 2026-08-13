/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-testkit; subpath: .; entry: packages/testkit/dist/index.d.ts */

/* ===== packages/fs/dist/branches/types.d.ts ===== */
import type { EphemeralFilesystem, EphemeralFilesystemAdministration } from "../filesystem/types.js";
export type RevisionId = string;
export type BranchState = "active" | "merged" | "discarded";
export interface BranchInfo {
    readonly id: string;
    readonly baseRevision: RevisionId;
    readonly state: BranchState;
    readonly generation: number;
    readonly createdAt: number;
    readonly terminalAt: number | null;
    readonly mergedRevision: RevisionId | null;
}
export interface CreateBranchOptions {
    readonly id?: string;
    readonly baseRevision?: RevisionId;
}
export interface PublishOptions {
    readonly operationId?: string;
}
export type ConflictReason = "entry-changed" | "node-changed" | "source-changed" | "destination-changed" | "subtree-changed" | "ancestor-changed";
export interface PublishConflict {
    readonly path: string;
    readonly reason: ConflictReason;
    readonly expectedRevision: RevisionId | null;
    readonly actualRevision: RevisionId | null;
}
export interface MergedPublishResult {
    readonly outcome: "merged";
    readonly branchId: string;
    readonly operationId: string | null;
    readonly baseRevision: RevisionId;
    readonly parentRevision: RevisionId;
    readonly revision: RevisionId;
    readonly changedPaths: string[];
    readonly conflicts: [];
}
export interface ConflictPublishResult {
    readonly outcome: "conflict";
    readonly branchId: string;
    readonly operationId: string | null;
    readonly baseRevision: RevisionId;
    readonly headRevision: RevisionId;
    readonly revision: null;
    readonly changedPaths: [];
    readonly conflicts: PublishConflict[];
}
export type PublishResult = MergedPublishResult | ConflictPublishResult;
export interface EphemeralBranch extends Omit<EphemeralFilesystem, "close"> {
    readonly id: string;
    info(): Promise<BranchInfo>;
    publish(options?: PublishOptions): Promise<PublishResult>;
    discard(): Promise<BranchInfo>;
    close(): Promise<void>;
}
export interface Branches {
    create(id: string): Promise<EphemeralBranch>;
    create(options?: CreateBranchOptions): Promise<EphemeralBranch>;
    open(id: string): Promise<EphemeralBranch>;
    get(id: string): Promise<BranchInfo>;
    replay(operationId: string, branchId?: string): Promise<PublishResult>;
}
export interface BranchCapableFilesystem extends EphemeralFilesystem, EphemeralFilesystemAdministration {
    readonly branches: Branches;
}
export type BranchErrorCode = "InvalidBranchId" | "InvalidOperationId" | "BranchNotFound" | "BranchNotActive" | "RevisionNotFound" | "BranchChanged" | "OperationBranchMismatch" | "OperationNotFound" | "OperationResultExpired" | "LimitExceeded";
export declare class BranchError extends Error {
    readonly name: "BranchError";
    readonly code: BranchErrorCode;
    readonly branchId?: string;
    readonly operationId?: string;
    readonly limit?: string;
    constructor(code: BranchErrorCode, message: string, details?: {
        branchId?: string;
        operationId?: string;
        limit?: string;
    });
}

/* ===== packages/fs/dist/cow/pages.d.ts ===== */
export type CowPageBytes = 4096 | 8192 | 16384;
/** 64 MiB at 4 KiB plus both partial endpoints. */
export declare const MAX_COW_PAGES_PER_WRITE = 16385;
export declare const MAX_DIRTY_RANGES = 16384;
export interface DirtyRange {
    readonly start: number;
    readonly end: number;
}
export interface CowPage {
    readonly index: number;
    readonly bytes: Uint8Array;
}
export type CowPageIndex = number & {
    readonly __cowPageIndex: unique symbol;
};
export interface CowPageKey {
    readonly branchId: string;
    readonly inodeId: string;
    readonly pageIndex: CowPageIndex;
}
export declare function validateCowPageBytes(value: number): asserts value is CowPageBytes;
export declare function cowPageIndex(value: number): CowPageIndex;
export declare function createCowPageKey(branchId: string, inodeId: string, index: number): CowPageKey;
export declare function pageIndex(offset: number, pageBytes: CowPageBytes): CowPageIndex;
export declare function pageRange(offset: number, length: number, pageBytes: CowPageBytes, maxPages?: number): readonly number[];
export declare function mergeDirtyRanges(ranges: readonly DirtyRange[], maxRanges?: number): DirtyRange[];
export declare function writeCowPages(base: Uint8Array, offset: number, content: Uint8Array, pageBytes: CowPageBytes): CowPage[];
export declare function overlayCowPages(base: Uint8Array, pages: readonly CowPage[], pageBytes: CowPageBytes, logicalSize?: number, maxPages?: number): Uint8Array;

/* ===== packages/fs/dist/filesystem/ephemeral-fs.d.ts ===== */
import type { OpenFilesystemOptions } from "./types.js";
/** Public composition root: injects the private SQLite storage-port adapter. */
export declare class EphemeralFS {
    private constructor();
    static open(options: OpenFilesystemOptions): Promise<EphemeralFS>;
}

/* ===== packages/fs/dist/filesystem/errors.d.ts ===== */
export type FilesystemErrorCode = "EINVAL" | "ENOENT" | "ENOTDIR" | "EISDIR" | "EEXIST" | "ENOTEMPTY" | "ELOOP" | "EPERM" | "EROFS" | "EBADF" | "EAGAIN" | "EBUSY" | "EFBIG" | "ENOSPC" | "ECORRUPT" | "ESCHEMA" | "EIO";
export declare class FilesystemError extends Error {
    readonly name: "FilesystemError";
    readonly code: FilesystemErrorCode;
    readonly syscall?: string;
    readonly path?: string;
    readonly destination?: string;
    constructor(code: FilesystemErrorCode, message: string, options?: {
        syscall?: string;
        path?: string;
        destination?: string;
        cause?: unknown;
    });
}
export declare function fsError(code: FilesystemErrorCode, syscall: string, path: string | undefined, detail: string, cause?: unknown): FilesystemError;
export declare function mapStorageError(error: unknown, syscall: string, path?: string): never;
export declare function abortError(): DOMException;

/* ===== packages/fs/dist/filesystem/types.d.ts ===== */
import type { FilesystemSQLiteDriver, SQLiteDriverCapabilities } from "../sqlite/driver.js";
import type { BranchConfiguration, FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { CowPageBytes } from "../cow/pages.js";
import type { FilesystemErrorCode } from "./errors.js";
export type FileType = "file" | "directory" | "symlink";
export type FileContent = string | Uint8Array | ReadableStream<Uint8Array>;
export interface FileStat {
    readonly id: string;
    readonly name: string;
    readonly type: FileType;
    readonly mode: number;
    readonly size: number;
    readonly nlink: number;
    readonly mtimeMs: number;
    readonly ctimeMs: number;
    readonly birthtimeMs: number;
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
}
export interface DirectoryEntry {
    readonly name: string;
    readonly parentPath: string;
    readonly type: FileType;
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
}
export interface ReadTextOptions {
    readonly encoding: "utf8";
}
export interface ReadRangeOptions {
    readonly offset: number;
    readonly length: number;
}
export interface ReadStreamOptions {
    readonly offset?: number;
    readonly length?: number;
    readonly signal?: AbortSignal;
}
export interface WriteFileOptions {
    readonly mode?: number;
    readonly exclusive?: boolean;
    readonly signal?: AbortSignal;
    /** Required upper bound for a streamed write; buffered values infer their length. */
    readonly maxBytes?: number;
}
export interface MkdirOptions {
    readonly recursive?: boolean;
    readonly mode?: number;
}
export interface ReaddirOptions {
    readonly limit?: number;
    readonly startAfter?: string;
}
export interface RmOptions {
    readonly recursive?: boolean;
    readonly force?: boolean;
}
export interface StorageFormatOptions {
    readonly cowPageBytes?: CowPageBytes;
}
export interface StorageFormat {
    readonly cowPageBytes: CowPageBytes;
    readonly hashAlgorithm: "sha256";
    readonly chunkerAlgorithm: "fastcdc-v1";
    readonly manifestFormat: "efs-merkle-manifest-v1";
}
export interface EffectiveLimit {
    readonly domain: "filesystem" | "storage" | "branch" | "runtime";
    readonly name: string;
    readonly value: number;
    readonly scope: "persisted" | "runtime";
    readonly constrainedBy: "configuration" | "format" | "adapter";
}
export interface FilesystemCapabilities {
    readonly adapter: SQLiteDriverCapabilities;
    readonly filesystem: Readonly<FilesystemLimits>;
    readonly storage: Readonly<StorageLimits>;
    readonly branch: Readonly<BranchConfiguration>;
    readonly runtime: Readonly<RuntimeLimits>;
    readonly format: Readonly<StorageFormat>;
    readonly effectiveLimits: readonly EffectiveLimit[];
    readonly readOnly: boolean;
}
export interface FilesystemObservation {
    readonly type: "operation" | "integrity" | "maintenance";
    readonly operation: string;
    readonly outcome: "success" | "error";
    readonly elapsedMs: number;
    readonly counters: Readonly<Record<string, number>>;
    readonly errorCode?: FilesystemErrorCode;
}
export type FilesystemObserver = (event: FilesystemObservation) => void;
export interface GarbageCollectionOptions {
    readonly runId?: string;
    readonly maxBatches?: number;
    readonly signal?: AbortSignal;
}
export interface GarbageCollectionResult {
    readonly runId: string;
    readonly state: "complete" | "paused" | "abandoned";
    readonly phase: "marking" | "sweeping-manifest-roots" | "sweeping-manifest-nodes" | "sweeping-objects" | "cleaning-marks" | "cleaning-root-journal" | "cleaning-terminal-runs" | "complete" | "abandoned";
    readonly progressCursor: string | null;
    /** Exact when zero; null means the remaining total is not boundedly knowable yet. */
    readonly remainingWork: number | null;
    readonly examinedManifestRootCount: number;
    readonly deletedManifestRootCount: number;
    readonly examinedManifestNodeCount: number;
    readonly deletedManifestNodeCount: number;
    readonly examinedManifestCount: number;
    readonly deletedManifestCount: number;
    readonly examinedObjectCount: number;
    readonly deletedObjectCount: number;
    readonly reclaimedObjectPayloadBytes: number;
    readonly reclaimedManifestPayloadBytes: number;
    readonly reclaimedBranchOverlayPayloadBytes: number;
    readonly committedBatches: number;
    readonly elapsedMs: number;
}
export interface StorageSnapshotOptions {
    readonly maxBatches?: number;
    readonly signal?: AbortSignal;
}
export interface PhysicalStorageSnapshot {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
    readonly freelistBytes?: number;
}
export interface StorageSnapshot {
    readonly state: "complete" | "paused";
    readonly phase: "roots" | "marking" | "stored-payload" | "logical-namespace" | "branch-overlays" | "mark-cleanup" | "mark-reset" | "complete";
    readonly progressCursor: string | null;
    /** Exact when zero; null means the remaining total is not boundedly knowable yet. */
    readonly remainingWork: number | null;
    readonly committedBatches: number;
    readonly batchSize: number;
    readonly elapsedMs: number;
    readonly peakManagedResidentBytes: number;
    readonly rootMutationGeneration: number;
    readonly mainLogicalBytes: number;
    readonly storedObjectPayloadBytes: number;
    readonly storedManifestPayloadBytes: number;
    readonly reachableObjectPayloadBytes: number;
    readonly reachableManifestPayloadBytes: number;
    readonly reclaimablePayloadBytes: number;
    readonly branchPageBytes: number;
    readonly branchPatchBytes: number;
    readonly branchExclusiveObjectBytes: number;
    readonly branchExclusiveManifestBytes: number;
    readonly branchExclusivePayloadBytes: number;
    readonly operationResultPayloadBytes: number;
    readonly objectCount: number;
    readonly manifestRootCount: number;
    readonly manifestNodeCount: number;
    readonly manifestCount: number;
    readonly chargedMetadataBytes: number;
    readonly revisionCount: number;
    readonly includesNamespaceMetadata: boolean;
    readonly includesOperationResults: boolean;
    readonly physical?: PhysicalStorageSnapshot;
}
export type VerificationScope = "metadata" | "namespace" | "manifests" | "objects" | "head";
export interface VerificationOptions {
    readonly scopes?: readonly VerificationScope[];
    readonly cursor?: string;
    readonly maxEntities?: number;
    readonly signal?: AbortSignal;
}
export interface VerificationResult {
    readonly rootMutationGeneration: number;
    readonly phase: "roots" | "nodes" | "objects" | "inodes" | "usage" | "complete";
    readonly progressCursor: string | null;
    readonly remainingWork: number | null;
    readonly committedBatches: 0;
    readonly elapsedMs: number;
    readonly peakManagedResidentBytes: number;
    readonly checkedEntities: number;
    readonly complete: boolean;
    readonly nextCursor: string | null;
}
export interface FilesystemMaintenance {
    collectGarbage(options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
    snapshotStorage(options?: StorageSnapshotOptions): Promise<StorageSnapshot>;
    verify(options?: VerificationOptions): Promise<VerificationResult>;
}
export interface OpenFilesystemOptions {
    readonly database: FilesystemSQLiteDriver;
    readonly clock?: () => number;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly branch?: Partial<BranchConfiguration>;
    readonly observer?: FilesystemObserver;
    readonly ownsDatabase?: boolean;
}
export interface EphemeralFilesystem {
    readFile(path: string): Promise<Uint8Array>;
    readFile(path: string, options: ReadTextOptions): Promise<string>;
    readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array>;
    readStream(path: string, options?: ReadStreamOptions): Promise<ReadableStream<Uint8Array>>;
    writeFile(path: string, content: FileContent, options?: WriteFileOptions): Promise<void>;
    writeRange(path: string, offset: number, content: Uint8Array): Promise<void>;
    replaceRange(path: string, offset: number, deleteLength: number, insertBytes: Uint8Array): Promise<void>;
    truncate(path: string, size?: number): Promise<void>;
    mkdir(path: string, options?: MkdirOptions): Promise<void>;
    readdir(path: string, options?: ReaddirOptions): Promise<DirectoryEntry[]>;
    stat(path: string): Promise<FileStat>;
    lstat(path: string): Promise<FileStat>;
    chmod(path: string, mode: number): Promise<void>;
    link(existingPath: string, newPath: string): Promise<void>;
    symlink(target: string, path: string): Promise<void>;
    readlink(path: string): Promise<string>;
    rename(oldPath: string, newPath: string): Promise<void>;
    unlink(path: string): Promise<void>;
    rm(path: string, options?: RmOptions): Promise<void>;
    close(): Promise<void>;
}
export interface EphemeralFilesystemAdministration {
    readonly capabilities: FilesystemCapabilities;
    readonly maintenance: FilesystemMaintenance;
}

/* ===== packages/fs/dist/index.d.ts ===== */
import type { BranchCapableFilesystem } from "./branches/types.js";
export declare const EPHEMERAL_AI_FS_VERSION = "0.1.0-rc.0";
export { EphemeralFS } from "./filesystem/ephemeral-fs.js";
declare module "./filesystem/ephemeral-fs.js" {
    interface EphemeralFS extends BranchCapableFilesystem {
    }
}
export { FilesystemError } from "./filesystem/errors.js";
export type { FilesystemErrorCode } from "./filesystem/errors.js";
export type * from "./filesystem/types.js";
export type { BranchConfiguration, FilesystemLimits, RuntimeLimits, StorageLimits, } from "./resources/limits.js";
export { BranchError } from "./branches/types.js";
export type * from "./branches/types.js";

/* ===== packages/fs/dist/resources/limits.d.ts ===== */
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
export declare const MAX_CONTENT_OBJECT_BYTES: number;
export declare const DEFAULT_FASTCDC_MINIMUM_BYTES = 32768;
export declare const DEFAULT_FASTCDC_MAXIMUM_BYTES = 524288;
/** Conservative per-object binding/row/index envelope in a durable transaction. */
export declare const CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES: number;
export declare function maxPersistedContentObjectBytes(storage: Pick<StorageLimits, "maxFinalTransactionBytes">): number;
/** Additional caller input one collecting FastCDC push may return with a prebuffer. */
export declare const MAX_CONTENT_COLLECTOR_PUSH_BYTES: number;
/** Maximum retained chunk references returned by one collecting push call. */
export declare const MAX_CONTENT_COLLECTOR_REFERENCES = 16384;
/** Conservative allocated-capacity charge for one JavaScript array element slot. */
export declare const CONTENT_COLLECTOR_REFERENCE_BYTES = 16;
/**
 * Source/carry, chunker, emitted chunk, sink handoff, retained object, and
 * replacement-window copies may coexist in the bounded rebuild pipeline.
 */
export declare const MAX_CONTENT_WORKING_SET_COPIES = 6;
export declare const MIN_CANONICAL_MANIFEST_NODE_BYTES = 9248;
export declare const DURABLE_METADATA_ROW_BYTES = 512;
export declare const MAX_MAINTENANCE_RUN_ROW_BYTES = 1024;
export declare const MAX_MAINTENANCE_MARK_ROW_BYTES = 704;
export declare const MAINTENANCE_CLEANUP_ROW_BYTES = 512;
export declare const MAINTENANCE_GC_EMERGENCY_BYTES: number;
export declare const MAINTENANCE_TOTAL_EMERGENCY_BYTES: number;
export declare const MIN_MAINTENANCE_BYTES: number;
export declare const DEFAULT_FILESYSTEM_LIMITS: FilesystemLimits;
export declare const DEFAULT_STORAGE_LIMITS: StorageLimits;
export declare const DEFAULT_RUNTIME_LIMITS: RuntimeLimits;
export declare const DEFAULT_BRANCH_CONFIGURATION: BranchConfiguration;
export declare function resolveLimits<T extends object>(defaults: T, configured?: Partial<T>): Readonly<T>;
export declare function persistedWriterProfile(filesystem: Readonly<FilesystemLimits>, storage: Readonly<StorageLimits>, branch: Readonly<BranchConfiguration>): string;
export declare function constrainStorageLimits(configured: Partial<StorageLimits> | undefined, adapter: StorageAdapterLimits): Readonly<StorageLimits>;
export declare function validateRuntimeLimits(filesystem: FilesystemLimits, storage: StorageLimits, runtime: RuntimeLimits, cowPageBytes: number): void;
export declare function requiredRuntimeProgressBytes(filesystem: FilesystemLimits, storage: StorageLimits, cowPageBytes: number): number;
export declare class AdmissionController {
    #private;
    constructor(limit: number);
    reserve(bytes: number): () => void;
    get usedBytes(): number;
    get peakBytes(): number;
    get limitBytes(): number;
}
/** Process-wide runtime admission shared by the main filesystem and branches. */
export declare class RuntimeConcurrency {
    #private;
    constructor(limits: Pick<RuntimeLimits, "maxConcurrentOperations" | "maxConcurrentStreams">);
    tryAcquireOperation(): (() => void) | undefined;
    tryAcquireStream(): (() => void) | undefined;
}

/* ===== packages/fs/dist/sqlite/driver.d.ts ===== */
export type SqliteValue = null | string | number | Uint8Array;
export type SqliteBindings = readonly SqliteValue[];
export type SqliteRow = Readonly<Record<string, SqliteValue>>;
export interface SqliteRunResult {
    readonly changes: number;
    /** Includes trigger/FK side effects when the adapter can report them. */
    readonly totalChanges?: number;
    readonly lastInsertRowid?: number;
}
export interface QueryBudget {
    readonly maxRows: number;
    readonly maxBytes: number;
}
export interface FilesystemSQLiteTransaction {
    readonly scope: symbol;
    run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
    all<Row extends SqliteRow = SqliteRow>(sql: string, bindings: SqliteBindings, budget: QueryBudget): readonly Row[];
}
export type TransactionMode = "read" | "write" | "exclusive";
export type SQLiteSchemaIdentityMode = "sqlite-header" | "durable-table";
export type SQLitePageMetricsMode = "sqlite-pragma" | "runtime-size-only";
export interface SQLiteDriverCapabilities {
    readonly maxBlobBytes: number;
    readonly maxBindings: number;
    readonly durability: "acknowledged" | "relaxed-test";
    readonly journalMode: "wal" | "rollback" | "runtime-managed";
    readonly memoryPolicy: "configured" | "runtime-managed";
    readonly cacheTargetBytes?: number;
    readonly mmapLimitBytes?: number;
    readonly maxPhysicalDatabaseBytes: number;
    readonly maxJournalBytes: number;
    readonly physicalQuotaPolicy: "driver-enforced" | "runtime-enforced";
    readonly journalQuotaPolicy?: "checkpoint-backpressure" | "runtime-enforced";
    readonly journalSizeLimitIsHard?: false;
    /**
     * Selects the durable schema identity representation. Omission preserves the
     * native SQLite-header contract for existing third-party adapters.
     */
    readonly schemaIdentityMode?: SQLiteSchemaIdentityMode;
    /** Selects native page/freelist PRAGMAs or a runtime-owned size-only counter. */
    readonly pageMetricsMode?: SQLitePageMetricsMode;
}
export interface SQLitePhysicalStorage {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
}
export interface SQLiteCheckpointResult {
    readonly mode: "passive" | "restart" | "truncate";
    readonly busy: number;
    readonly logFrames: number;
    readonly checkpointedFrames: number;
    readonly walBytes?: number;
}
export type SqliteHashFunction = (bytes: Uint8Array) => Uint8Array;
export type SqliteAsyncHashFunction = (bytes: Uint8Array) => Promise<Uint8Array>;
export interface FilesystemSQLiteDriver {
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    /**
     * Optional synchronous SHA-256 hasher. When the host adapter provides one
     * (node:crypto on Node), the operations storage uses it for content
     * hashing and verification; hosts without a synchronous native hasher
     * fall back to the byte-identical pure-JS implementation.
     */
    readonly hashBytes?: SqliteHashFunction;
    /**
     * Optional asynchronous SHA-256 hasher for write-path chunk hashing
     * (WebCrypto on workerd). When present, the streaming write pipeline hashes
     * its chunk batches concurrently with bounded parallelism; digests are
     * byte-identical to the synchronous implementations.
     */
    readonly hashBytesAsync?: SqliteAsyncHashFunction;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    physicalStorage?(): SQLitePhysicalStorage;
    checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
    close(): void | Promise<void>;
}

/* ===== packages/testkit/dist/branch.d.ts ===== */
import type { ConformanceAdapterFactory } from "./index.js";
export declare const PORTABLE_BRANCH_CASE_IDS: readonly ["branch-frozen-base", "branch-50-independent", "branch-50-conflicting", "branch-sibling-order", "branch-aba-alias-conflicts", "branch-deterministic-results", "branch-pagination", "branch-recursive-conflict", "branch-terminal-handles", "branch-stream-snapshot", "branch-replay-reopen", "branch-result-expiry-reservation"];
export type PortableBranchCaseId = (typeof PORTABLE_BRANCH_CASE_IDS)[number];
export interface PortableBranchCaseResult {
    readonly id: PortableBranchCaseId;
    readonly status: "passed";
}
/** Shared 50-writer, conflict, snapshot, replay, and restart branch suite. */
export declare function runBranchConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableBranchCaseResult[]>;

/* ===== packages/testkit/dist/cow.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export declare const PORTABLE_COW_PAGE_SIZES: readonly [4096, 8192, 16384];
export type PortableCowPageSize = (typeof PORTABLE_COW_PAGE_SIZES)[number];
export declare const PORTABLE_COW_CASE_IDS: readonly ["cow-repeated-page-head", "cow-boundary-crossing", "cow-final-partial-page", "cow-pinned-snapshot", "cow-physical-reopen", "cow-conflicting-format-refusal"];
export interface PortableCowPreparation {
    readonly schema: "efs-portable-cow-preparation-v1";
    readonly pageBytes: PortableCowPageSize;
    readonly branchId: string;
    readonly fixtureDigest: string;
    readonly repeatedWrites: 1000;
}
export interface PortableCowResult extends PortableCowPreparation {
    readonly cases: typeof PORTABLE_COW_CASE_IDS;
    readonly pageHeadCount: number;
    readonly pageVersionCount: number;
    readonly finalPartialBytes: number;
}
/** Prepare all public COW mutations, intentionally separate from physical reopen. */
export declare function preparePortableCowPageSize(adapter: FilesystemSQLiteDriver, pageBytes: PortableCowPageSize): Promise<PortableCowPreparation>;
/** Verify exact state and format refusal after the caller physically restarts storage. */
export declare function verifyPortableCowPageSize(adapter: FilesystemSQLiteDriver, preparation: PortableCowPreparation): Promise<PortableCowResult>;

/* ===== packages/testkit/dist/driver.d.ts ===== */
import type { ConformanceAdapterFactory } from "./index.js";
export type PortableDriverCaseId = "driver-capabilities" | "driver-transactions" | "driver-callback-error-identity" | "driver-integer-roundtrip" | "driver-blob-ownership" | "driver-bounds" | "driver-sql-shape" | "driver-reopen-lifecycle";
export interface PortableDriverCaseResult {
    readonly id: PortableDriverCaseId;
    readonly status: "passed";
}
export declare const PORTABLE_DRIVER_CASE_IDS: readonly ["driver-capabilities", "driver-transactions", "driver-callback-error-identity", "driver-integer-roundtrip", "driver-blob-ownership", "driver-bounds", "driver-sql-shape", "driver-reopen-lifecycle"];
/** Run the identical callback-scoped SQLite contract against a fresh adapter. */
export declare function runSQLiteDriverConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableDriverCaseResult[]>;

/* ===== packages/testkit/dist/fault.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory, ConformanceFaultController } from "./index.js";
declare const FAULT_POINT = "after-sql-statement";
export declare const PORTABLE_FAULT_SEED = 1024023;
export declare const PORTABLE_FAULT_OPERATION_POSITIONS: Readonly<{
    readonly "writeFile-create": 214;
    readonly "writeFile-stream": 214;
    readonly writeRange: 74;
    readonly replaceRange: 74;
    readonly truncate: 74;
    readonly mkdir: 175;
    readonly chmod: 29;
    readonly link: 70;
    readonly symlink: 59;
    readonly rename: 60;
    readonly unlink: 49;
    readonly "rm-recursive": 114;
}>;
export declare const PORTABLE_FAULT_POSITIONS = 1206;
export interface StatementFaultController extends ConformanceFaultController {
    wrap(driver: FilesystemSQLiteDriver): FilesystemSQLiteDriver;
    statementCount(): number;
}
/** Adapter-neutral statement fault injection used by both required SQLite drivers. */
export declare function createStatementFaultController(): StatementFaultController;
export interface PortableFaultMatrixResult {
    readonly schema: "efs-portable-fault-result-v1";
    readonly adapter: string;
    readonly seed: typeof PORTABLE_FAULT_SEED;
    readonly fixtureDigest: string;
    readonly faultPoint: typeof FAULT_POINT;
    readonly positions: number;
    readonly payloadBytes: number;
    readonly operationPositions: Readonly<Record<string, number>>;
}
/**
 * Fail after every SQL statement in every public filesystem mutation family.
 * Every injected position must reopen to the complete old state; the first
 * position beyond each operation must reopen to the complete new state.
 */
export declare function runFilesystemFaultMatrix(factory: ConformanceAdapterFactory): Promise<PortableFaultMatrixResult>;
export {};

/* ===== packages/testkit/dist/filesystem-fault-attempt.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import { PORTABLE_FAULT_OPERATION_POSITIONS, PORTABLE_FAULT_SEED, type StatementFaultController } from "./fault.js";
export type PortableFilesystemFaultOperation = keyof typeof PORTABLE_FAULT_OPERATION_POSITIONS;
export declare const PORTABLE_FILESYSTEM_FAULT_OPERATIONS: readonly ("writeRange" | "replaceRange" | "truncate" | "mkdir" | "chmod" | "link" | "symlink" | "rename" | "unlink" | "writeFile-create" | "writeFile-stream" | "rm-recursive")[];
/**
 * Exact topology for isolated per-operation fixtures. The three range mutations read
 * the small current manifest through four additional statements that the cumulative
 * mixed-state matrix satisfies from its already-authenticated cache.
 */
export declare const PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS: Readonly<{
    readonly writeRange: 78;
    readonly replaceRange: 78;
    readonly truncate: 78;
    readonly "writeFile-create": 214;
    readonly "writeFile-stream": 214;
    readonly mkdir: 175;
    readonly chmod: 29;
    readonly link: 70;
    readonly symlink: 59;
    readonly rename: 60;
    readonly unlink: 49;
    readonly "rm-recursive": 114;
}>;
export declare const PORTABLE_FILESYSTEM_RESTART_FAULT_POSITIONS = 1218;
export interface PortableFilesystemFaultAttemptResult {
    readonly operation: PortableFilesystemFaultOperation;
    readonly occurrence: number;
    readonly injected: boolean;
    readonly observedStatements: number;
    readonly seed: typeof PORTABLE_FAULT_SEED;
}
/**
 * Execute one selected mutation occurrence without orderly close. The caller owns the
 * physical driver/runtime restart before invoking `verifyFilesystemFaultAttempt`.
 */
export declare function prepareFilesystemFaultAttempt(adapter: FilesystemSQLiteDriver, operation: PortableFilesystemFaultOperation, occurrence: number, faults?: StatementFaultController): Promise<PortableFilesystemFaultAttemptResult>;
/** Verify complete old/new state after the caller has physically restarted storage. */
export declare function verifyFilesystemFaultAttempt(adapter: FilesystemSQLiteDriver, operation: PortableFilesystemFaultOperation, committed: boolean): Promise<void>;

/* ===== packages/testkit/dist/index.d.ts ===== */
import { EphemeralFS, type GarbageCollectionOptions, type GarbageCollectionResult } from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export * from "./smoke.js";
export * from "./fault.js";
export * from "./driver.js";
export * from "./branch.js";
export * from "./maintenance.js";
export * from "./scale.js";
export * from "./restart.js";
export * from "./schema.js";
export * from "./publication-fault.js";
export * from "./maintenance-fault.js";
export * from "./filesystem-fault-attempt.js";
export * from "./cow.js";
export * from "./storage.js";
export type ConformanceCapability = "read-only-reopen" | "second-connection" | "schema-fixtures" | "fault-injection" | "garbage-collection" | "physical-reopen" | "crash-recovery" | "ownership";
export interface ConformanceFaultController {
    arm(point: string, occurrence?: number): void;
    clear(): void;
}
export interface ConformanceFixtureOptions {
    readonly label?: string;
    readonly seed?: number;
}
export interface ConformanceDatabase {
    readonly adapter: FilesystemSQLiteDriver;
    readonly capabilities: readonly ConformanceCapability[];
    readonly faults?: ConformanceFaultController;
    reopen(options?: {
        readOnly?: boolean;
        physical?: boolean;
    }): Promise<FilesystemSQLiteDriver>;
    openSecondConnection?(): Promise<FilesystemSQLiteDriver>;
    reopenFromFixture?(fixtureName: string): Promise<FilesystemSQLiteDriver>;
    collectGarbage?(filesystem: EphemeralFS, options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
    crashAndReopen?(): Promise<FilesystemSQLiteDriver>;
    createOwnershipProbe?(): Promise<{
        readonly adapter: FilesystemSQLiteDriver;
        closeCallCount(): number;
    }>;
    dispose(): Promise<void>;
}
export interface ConformanceAdapterFactory {
    readonly name: string;
    create(options?: ConformanceFixtureOptions): Promise<ConformanceDatabase>;
}
export interface CorrectnessResult {
    readonly schema: "efs-correctness-result-v1";
    readonly commit: string;
    readonly adapter: string;
    readonly driver: string;
    readonly capabilities: Readonly<Record<string, string | number | boolean | null>>;
    readonly limits: Readonly<Record<string, number>>;
    readonly schemaVersion: number;
    readonly formatVersion: string;
    readonly seed: number;
    readonly fixtureDigest: string;
    readonly faultPoint: string | null;
    readonly commands: readonly string[];
    readonly environment: Readonly<Record<string, string>>;
    readonly passed: number;
    readonly failed: number;
    readonly elapsedMs: number;
}
export interface BenchmarkResult {
    readonly schema: "efs-benchmark-result-v1";
    readonly benchmark: string;
    readonly commit: string;
    readonly engine: string;
    readonly driver: string;
    readonly fixture: Readonly<{
        name: string;
        sha256: string;
    }>;
    readonly configuration: Readonly<Record<string, unknown>>;
    readonly trials: number;
    readonly latencyMs: Readonly<{
        p50: number;
        p95: number;
        p99: number;
    }>;
    readonly counters: Readonly<Record<string, number>>;
    readonly pass: boolean;
}
export type PortableConformanceCaseId = "storage-deduplication" | "filesystem-namespace" | "filesystem-path-errors" | "filesystem-range-edges" | "filesystem-link-semantics" | "filesystem-rename-removal" | "filesystem-metadata" | "filesystem-pagination-cap" | "filesystem-error-details" | "stream-snapshot" | "stream-abort-backpressure" | "lease-staging-lifecycle" | "read-side-effect-boundary" | "overlapping-operations" | "branch-publication" | "maintenance-cursors" | "resource-capabilities" | "durable-reopen" | "read-only-reopen" | "second-connection" | "close-lifecycle";
export declare const PORTABLE_CONFORMANCE_CASE_IDS: readonly ["storage-deduplication", "filesystem-namespace", "filesystem-path-errors", "filesystem-range-edges", "filesystem-link-semantics", "filesystem-rename-removal", "filesystem-metadata", "filesystem-pagination-cap", "filesystem-error-details", "stream-snapshot", "stream-abort-backpressure", "lease-staging-lifecycle", "read-side-effect-boundary", "overlapping-operations", "branch-publication", "maintenance-cursors", "resource-capabilities", "durable-reopen", "read-only-reopen", "second-connection", "close-lifecycle"];
export interface PortableConformanceCaseResult {
    readonly id: PortableConformanceCaseId;
    readonly status: "passed" | "skipped";
    readonly reason?: string;
}
/**
 * Runs the same host-neutral milestone conformance scenario against a real adapter
 * factory. Runtime harnesses may invoke this inside their storage-owning isolate.
 */
export declare function runFilesystemConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableConformanceCaseResult[]>;
/** Registers the normative shared filesystem suite with Vitest. */
export declare function filesystemConformance(factory: ConformanceAdapterFactory): void;
export type RecordingEvent = Readonly<{
    type: "create";
    factory: string;
    label: string | null;
    seed: number | null;
}> | Readonly<{
    type: "reopen";
    readOnly: boolean;
    physical: boolean;
}> | Readonly<{
    type: "second-connection";
}> | Readonly<{
    type: "dispose";
}>;
/** Wraps a real test factory without weakening its restart or connection behavior. */
export declare function createRecordingFactory(factory: ConformanceAdapterFactory, events: RecordingEvent[]): ConformanceAdapterFactory;

/* ===== packages/testkit/dist/maintenance-fault.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export type PortableMaintenanceFaultVariant = "snapshot" | "collection" | "abandoned";
export type PortableMaintenanceFaultKind = "statement" | "batch";
export declare const PORTABLE_MAINTENANCE_FAULT_TOPOLOGY: Readonly<{
    readonly snapshot: Readonly<{
        durableStatements: 110;
        committedBatches: 42;
        maxBatchStatements: 6;
    }>;
    readonly collection: Readonly<{
        durableStatements: 259;
        committedBatches: 128;
        maxBatchStatements: 3;
    }>;
    readonly abandoned: Readonly<{
        durableStatements: 61;
        committedBatches: 33;
        maxBatchStatements: 3;
    }>;
}>;
export interface PortableMaintenanceFaultMetrics {
    readonly durableStatements: number;
    readonly committedBatches: number;
    readonly maxBatchStatements: number;
}
export interface PortableMaintenanceFaultAttempt {
    readonly schema: "efs-portable-maintenance-fault-attempt-v1";
    readonly variant: PortableMaintenanceFaultVariant;
    readonly kind: PortableMaintenanceFaultKind;
    readonly ordinal: number;
    readonly injected: boolean;
    readonly metrics: PortableMaintenanceFaultMetrics;
    readonly resultCounters?: Readonly<Record<string, number>>;
}
/** Run one fresh post-commit maintenance fault attempt. */
export declare function runPortableMaintenanceFaultAttempt(adapter: FilesystemSQLiteDriver, variant: PortableMaintenanceFaultVariant, kind: PortableMaintenanceFaultKind, ordinal: number): Promise<PortableMaintenanceFaultAttempt>;
/** Resume and verify one selected maintenance operation after host restart/eviction. */
export declare function verifyPortableMaintenanceFaultRecovery(adapter: FilesystemSQLiteDriver, variant: PortableMaintenanceFaultVariant): Promise<Readonly<Record<string, number>>>;

/* ===== packages/testkit/dist/maintenance.d.ts ===== */
import type { ConformanceAdapterFactory } from "./index.js";
export declare const PORTABLE_MAINTENANCE_CASE_IDS: readonly ["maintenance-snapshot-restart", "maintenance-gc-root-reconciliation", "maintenance-corruption-no-sweep", "maintenance-quota-rollback", "maintenance-resource-envelopes"];
export type PortableMaintenanceCaseId = (typeof PORTABLE_MAINTENANCE_CASE_IDS)[number];
export interface PortableMaintenanceCaseResult {
    readonly id: PortableMaintenanceCaseId;
    readonly status: "passed";
}
/** Shared bounded maintenance, recovery, corruption, quota, and resource suite. */
export declare function runMaintenanceConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableMaintenanceCaseResult[]>;

/* ===== packages/testkit/dist/publication-fault.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export type PortablePublicationFaultVariant = "direct" | "prepared";
export declare const PORTABLE_PUBLICATION_FAULT_POSITIONS: Readonly<{
    readonly direct: 95;
    readonly prepared: 91;
}>;
export interface PortablePublicationFaultAttempt {
    readonly schema: "efs-portable-publication-fault-attempt-v1";
    readonly variant: PortablePublicationFaultVariant;
    readonly occurrence: number;
    readonly maxTransactionStatements: number;
    readonly injected: boolean;
}
/** Run one fresh publication attempt with a fault at one final-transaction position. */
export declare function runPortablePublicationFaultAttempt(adapter: FilesystemSQLiteDriver, variant: PortablePublicationFaultVariant, occurrence: number): Promise<PortablePublicationFaultAttempt>;
/** Verify old state after the caller has physically recreated the driver/runtime. */
export declare function verifyPortablePublicationFaultRecovery(adapter: FilesystemSQLiteDriver, variant: PortablePublicationFaultVariant): Promise<void>;

/* ===== packages/testkit/dist/restart.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export declare const PORTABLE_RESTART_SEED = 98925095;
export declare const PORTABLE_RESTART_CASE_IDS: readonly ["restart-committed-state", "restart-active-branch", "restart-lost-response-replay", "restart-abandoned-lease", "restart-interrupted-collection"];
export type PortableRestartCaseId = (typeof PORTABLE_RESTART_CASE_IDS)[number];
export interface PortableRestartPreparation {
    readonly schema: "efs-portable-restart-preparation-v1";
    readonly seed: typeof PORTABLE_RESTART_SEED;
    readonly fixtureDigest: string;
    readonly publicationResult: string;
    readonly activeLeaseRows: number;
    readonly collectionState: "paused";
}
export interface PortableRestartResult {
    readonly schema: "efs-portable-restart-result-v1";
    readonly seed: typeof PORTABLE_RESTART_SEED;
    readonly fixtureDigest: string;
    readonly cases: readonly PortableRestartCaseId[];
    readonly verifiedEntities: number;
    readonly activeLeaseRows: number;
    readonly stagingRows: number;
    readonly collectionState: "complete";
}
/**
 * Establish durable state immediately before an unorderly physical/runtime restart.
 * The caller MUST destroy the Node connection or evict the Durable Object after this
 * function returns, without orderly filesystem or branch close.
 */
export declare function preparePortableRestart(adapter: FilesystemSQLiteDriver): Promise<PortableRestartPreparation>;
/** Verify and finish the shared recovery scenario after a real physical/runtime restart. */
export declare function verifyPortableRestart(adapter: FilesystemSQLiteDriver, preparation: PortableRestartPreparation): Promise<PortableRestartResult>;

/* ===== packages/testkit/dist/scale.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";
export declare const PORTABLE_SCALE_SEED = 379422;
export interface PortableScaleResult {
    readonly schema: "efs-portable-scale-result-v1";
    readonly adapter: string;
    readonly seed: typeof PORTABLE_SCALE_SEED;
    readonly fixtureDigest: string;
    readonly rows: 100000;
    readonly baselineRows: 10240;
    readonly objectRows: number;
    readonly namespaceRows: number;
    readonly manifestRootRows: number;
    readonly manifestNodeRows: number;
    readonly baselineManagedPeakBytes: number;
    readonly fullManagedPeakBytes: number;
    readonly peakStorageMarks: number;
    readonly peakGcMarks: number;
    readonly verifiedRows: number;
    readonly maxMaintenanceCallMs: number;
    readonly mainFileBytes: number;
    readonly physicalRestarts?: number;
}
export type PortableScalePhaseOutcome = Readonly<{
    status: "restart";
    completedPhase: "baseline-built" | "baseline-measured" | "full-built" | "full-measured" | "collection-paused";
}> | Readonly<{
    status: "complete";
    result: PortableScaleResult;
}>;
/** Host-coordinated scale gate whose four restart boundaries require real eviction. */
export declare class PortableScaleSession {
    #private;
    constructor(adapterName: string);
    recordPhysicalRestart(): void;
    run(adapter: FilesystemSQLiteDriver): Promise<PortableScalePhaseOutcome>;
}
/** Shared 100,000-row cursor, restart, memory, and collection scale gate. */
export declare function runScaleConformance(factory: ConformanceAdapterFactory): Promise<PortableScaleResult>;

/* ===== packages/testkit/dist/schema.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export declare const PORTABLE_RELEASED_SCHEMA_VERSIONS: readonly [1, 2, 3];
export declare const PORTABLE_CURRENT_SCHEMA_VERSION = 13;
export declare const PORTABLE_APPLICATION_ID = 1161905747;
export declare const PORTABLE_MIGRATION_STATEMENT_COUNTS: Readonly<{
    readonly 1: 335;
    readonly 2: 310;
    readonly 3: 265;
}>;
export declare const PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS: Readonly<{
    readonly 1: 337;
    readonly 2: 312;
    readonly 3: 266;
}>;
export interface PortableMigrationAttemptResult {
    readonly schema: "efs-portable-migration-attempt-v1";
    readonly sourceVersion: 1 | 2 | 3;
    readonly occurrence: number;
    readonly observedStatements: number;
    readonly injected: boolean;
    readonly finalVersion: number;
}
/**
 * Run one fresh released-schema migration with a fault after the selected statement.
 * A caught fault must leave the exact source version usable; the first out-of-range
 * occurrence must migrate and open the current filesystem successfully.
 */
export declare function runPortableMigrationAttempt(adapter: FilesystemSQLiteDriver, sourceVersion: 1 | 2 | 3, occurrence: number): Promise<PortableMigrationAttemptResult>;
/** Validate a freshly initialized or migrated current schema through public behavior. */
export declare function verifyPortableCurrentSchema(adapter: FilesystemSQLiteDriver): Promise<void>;
/**
 * Validate that an injected migration left a transactionally self-consistent source,
 * intermediate, or current schema after the host has recreated the driver/isolate.
 */
export declare function verifyPortableRecoverableMigrationState(adapter: FilesystemSQLiteDriver, minimumVersion: 1 | 2 | 3, expectedVersion: number): void;

/* ===== packages/testkit/dist/smoke.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";
export declare const PORTABLE_SMOKE_SEED = 1592614637;
export declare const PORTABLE_SMOKE_PAYLOAD_BYTES: number;
export declare const PORTABLE_SMOKE_COW_EDITS = 5000;
export declare const PORTABLE_SMOKE_NAMESPACE_OPERATIONS = 2000;
export declare const PORTABLE_SMOKE_ACTORS_PER_KIND = 16;
export declare const PORTABLE_SMOKE_OPERATIONS_PER_ACTOR = 64;
export declare const PORTABLE_SMOKE_DEADLINE_MS = 60000;
export interface PortableSmokeOperationMetric {
    readonly name: string;
    readonly elapsedMs: number;
}
export interface PortableSmokeResult {
    readonly schema: "efs-portable-smoke-result-v1";
    readonly adapter: string;
    readonly seed: number;
    readonly fixtureDigest: string;
    readonly finalPayloadDigest: string;
    readonly namespaceDigest: string;
    readonly elapsedMs: number;
    readonly completedOperationCount: number;
    readonly namespaceOperationCount: number;
    readonly restarts: number;
    readonly peakManagedResidentBytes: number;
    readonly objectCount: number;
    readonly manifestCount: number;
    readonly slowestOperations: readonly PortableSmokeOperationMetric[];
}
export type PortableSmokePhaseOutcome = Readonly<{
    status: "restart";
    completedPhase: 0 | 1 | 2;
}> | Readonly<{
    status: "complete";
    result: PortableSmokeResult;
}>;
/**
 * Host-coordinated form of the exact smoke profile. The caller MUST perform a real
 * physical restart/eviction after every `restart` outcome, then call
 * `recordPhysicalRestart()` before entering the next adapter context.
 */
export declare class PortableSmokeSession {
    #private;
    constructor(adapterName: string);
    recordPhysicalRestart(elapsedMs: number): void;
    run(adapter: FilesystemSQLiteDriver): Promise<PortableSmokePhaseOutcome>;
}
/** Execute the exact finite 60-second profile against a real adapter factory. */
export declare function runFilesystemSmoke(factory: ConformanceAdapterFactory): Promise<PortableSmokeResult>;

/* ===== packages/testkit/dist/storage.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";
export declare const PORTABLE_STORAGE_CONFORMANCE_CASE_IDS: readonly ["storage-staging-closure-100001", "storage-certificate-field-corruption", "storage-sealed-membership-immutability", "storage-concurrent-payload-quota", "storage-usage-recount", "storage-manifest-range-corruption"];
export declare const PORTABLE_STORAGE_CASE_IDS: readonly ("storage-staging-closure-100001" | "storage-certificate-field-corruption" | "storage-sealed-membership-immutability" | "storage-concurrent-payload-quota" | "storage-usage-recount" | "storage-manifest-range-corruption" | "storage-staging-batch-crash-recovery")[];
export type PortableStorageCaseId = (typeof PORTABLE_STORAGE_CASE_IDS)[number];
export interface PortableStorageCaseResult {
    readonly id: PortableStorageCaseId;
    readonly status: "passed";
}
export interface PortableStagingClosureEvidence {
    readonly schema: "efs-portable-staging-closure-v1";
    readonly manifestEntries: 100_001;
    readonly uniqueClosureMembers: number;
    readonly reconciliationStatements: number;
    readonly finalValidationStatements: 1;
    readonly certificateFieldsRejected: 10;
    readonly sealedMembershipMutationsRejected: 2;
}
export interface PortableStorageInternals {
    runStagingClosure(adapter: FilesystemSQLiteDriver): Promise<PortableStagingClosureEvidence>;
    stageCrashBatch(adapter: FilesystemSQLiteDriver, batch: number): Promise<{
        readonly durableEntries: number;
    }>;
    recoverStagingCrash(adapter: FilesystemSQLiteDriver): Promise<{
        readonly activeLeases: number;
        readonly stagingCertificates: number;
        readonly stagingEntries: number;
        readonly stagingBytes: number;
        readonly ingestReservationBytes: number;
    }>;
}
export interface PortableStagingCrashEvidence {
    readonly schema: "efs-portable-staging-crash-v1";
    readonly batches: 3;
    readonly physicalRestarts: 3;
    readonly recovered: true;
}
export type PortableStagingCrashOutcome = {
    readonly status: "restart-required";
    readonly batch: number;
} | {
    readonly status: "complete";
    readonly result: PortableStagingCrashEvidence;
};
/** Host-coordinated staging crash scenario; adapters must physically restart between calls. */
export declare class PortableStagingCrashSession {
    #private;
    run(adapter: FilesystemSQLiteDriver, internals: PortableStorageInternals): Promise<PortableStagingCrashOutcome>;
}
export declare const PORTABLE_STORAGE_STORAGE_LIMITS: Readonly<{
    maxManagedPayloadBytes: number;
    maxStagingPayloadBytes: number;
    maxChargedMetadataBytes: number;
    maxMaintenanceBytes: number;
    maintenanceReserveBytes: number;
    maxBranchOverlayBytes: number;
    maxQueryBatchSize: 32;
    maxGcBatchSize: 32;
}>;
export declare const PORTABLE_STORAGE_RUNTIME_LIMITS: Readonly<{
    maxManagedResidentBytes: number;
    maxCacheBytes: number;
    maxPendingWriteBytes: number;
    maxWriteSessionBytes: number;
    maxPrefetchBytes: number;
    maxQueryBatchBytes: number;
}>;
/**
 * Runs the adapter-neutral M2 storage case whose implementation port remains private.
 * The injected port is shared by the Node and workerd harnesses; it may use private
 * repositories without widening the package export boundary.
 */
export declare function runStorageConformance(factory: ConformanceAdapterFactory, internals: PortableStorageInternals): Promise<readonly PortableStorageCaseResult[]>;

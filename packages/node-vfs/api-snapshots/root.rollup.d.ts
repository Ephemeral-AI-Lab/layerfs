/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-node-vfs; subpath: .; entry: packages/node-vfs/dist/index.d.ts */

/* ===== packages/fs/dist/branches/types.d.ts ===== */
import type { EphemeralFilesystem } from "../filesystem/types.js";
export type RevisionId = string;
export type BranchState = "active" | "merged" | "discarded";
export interface BranchInfo {
    readonly id: string;
    readonly baseRevision: RevisionId;
    readonly state: BranchState;
    readonly generation: number;
    readonly createdAt: number;
    readonly terminalAt: number | null;
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
import type { EphemeralFilesystem, OpenFilesystemOptions } from "./types.js";
/** Public composition root: injects the private SQLite storage-port adapter. */
export declare class EphemeralFS {
    static open(options: OpenFilesystemOptions): Promise<EphemeralFilesystem>;
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
export interface PhysicalStorageSnapshot {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
    readonly freelistBytes?: number;
}
export interface StorageSnapshot {
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
    readonly checkedEntities: number;
    readonly complete: boolean;
    readonly nextCursor: string | null;
}
export interface FilesystemMaintenance {
    collectGarbage(options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
    snapshotStorage(): Promise<StorageSnapshot>;
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

/* ===== packages/fs/dist/index.d.ts ===== */
export declare const EPHEMERAL_AI_FS_VERSION = "0.1.0-rc.0";
export { EphemeralFS } from "./filesystem/ephemeral-fs.js";
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
/** Conservative per-object binding/row/index envelope in a durable transaction. */
export declare const CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES = 256;
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
export declare const DEFAULT_FILESYSTEM_LIMITS: FilesystemLimits;
export declare const DEFAULT_STORAGE_LIMITS: StorageLimits;
export declare const DEFAULT_RUNTIME_LIMITS: RuntimeLimits;
export declare const DEFAULT_BRANCH_CONFIGURATION: BranchConfiguration;
export declare function resolveLimits<T extends object>(defaults: T, configured?: Partial<T>): Readonly<T>;
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

/* ===== packages/fs/dist/sqlite/driver.d.ts ===== */
export type SqliteValue = null | string | number | Uint8Array;
export type SqliteBindings = readonly SqliteValue[];
export type SqliteRow = Readonly<Record<string, SqliteValue>>;
export interface SqliteRunResult {
    readonly changes: number;
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
    readonly journalQuotaPolicy: "checkpoint-backpressure" | "runtime-enforced";
    readonly journalSizeLimitIsHard: false;
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
export interface FilesystemSQLiteDriver {
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    physicalStorage?(): SQLitePhysicalStorage;
    checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
    close(): void | Promise<void>;
}

/* ===== packages/node-vfs/dist/index.d.ts ===== */
import { EphemeralFS, type FileStat, type RuntimeLimits } from "@ephemeralai/fs";
import type { NodeSQLiteDriver } from "@ephemeralai/fs-sqlite-node";
export type CowPageBytes = 4096 | 8192 | 16384;
export interface OpenNodeVfsOptions {
    readonly database: NodeSQLiteDriver;
    readonly branchId?: string;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly observer?: NodeVfsObserver;
    readonly ownsDatabase?: boolean;
}
export interface NodeVfsCapabilities {
    readonly cowPageBytes: CowPageBytes;
    readonly runtime: Readonly<RuntimeLimits>;
    readonly preferredReadBytes: number;
    readonly supportsDirectRangeIo: true;
    readonly supportsWriteSessions: true;
    readonly supportsDataSync: boolean;
}
export interface OpenFileOptions {
    readonly writable?: boolean;
    readonly create?: boolean;
    readonly exclusive?: boolean;
    readonly truncate?: boolean;
    readonly mode?: number;
}
export interface FlushOptions {
    readonly dataOnly?: boolean;
}
export interface NodeFileSession {
    readonly id: string;
    readonly path: string;
    readonly writable: boolean;
    readIntoSync(destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    readRangeSync(position: number, length: number): Uint8Array;
    writeSync(content: Uint8Array, position: number): number;
    truncateSync(size: number): void;
    statSync(): FileStat;
    stagePrefixSync(): void;
    commitVisibleSync(options?: FlushOptions): void;
    flushSync(options?: FlushOptions): void;
    closeSync(): void;
    abortSync(): void;
}
export interface NodeVfsProvider {
    readonly capabilities: NodeVfsCapabilities;
    readonly metrics: NodeVfsMetrics;
    existsSync(path: string): boolean;
    statSync(path: string): FileStat;
    lstatSync(path: string): FileStat;
    readdirSync(path: string): string[];
    readlinkSync(path: string): string;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    openFileSync(path: string, options?: OpenFileOptions): NodeFileSession;
    mkdirSync(path: string, options?: {
        recursive?: boolean;
        mode?: number;
    }): void;
    chmodSync(path: string, mode: number): void;
    linkSync(existingPath: string, newPath: string): void;
    symlinkSync(target: string, path: string): void;
    renameSync(oldPath: string, newPath: string): void;
    unlinkSync(path: string): void;
    rmdirSync(path: string): void;
    syncSync(): void;
    closeSync(): void;
}
export interface NodeVfsHandle {
    readonly filesystem: EphemeralFS;
    readonly provider: NodeVfsProvider;
    close(): Promise<void>;
}
export interface NodeVfsMetricsSnapshot {
    readonly openSessions: number;
    readonly dirtySessions: number;
    readonly residentWriteBytes: number;
    readonly peakResidentWriteBytes: number;
    readonly residentControlBytes: number;
    readonly peakManagedResidentBytes: number;
    readonly stagedLogicalBytes: number;
    readonly admittedWriteBytes: number;
    readonly flushedWriteBytes: number;
    readonly flushCount: number;
    readonly forcedFlushCount: number;
    readonly failedFlushCount: number;
    readonly rejectedWriteCount: number;
    readonly directReadBytes: number;
    readonly coreBatchCount: number;
}
export interface NodeVfsMetrics {
    snapshot(): NodeVfsMetricsSnapshot;
}
export type NodeVfsObservation = {
    readonly kind: "session-open";
    readonly sessionId: string;
} | {
    readonly kind: "session-close";
    readonly sessionId: string;
} | {
    readonly kind: "forced-flush";
    readonly bytes: number;
} | {
    readonly kind: "flush-failed";
    readonly code: string;
} | {
    readonly kind: "memory-rejected";
    readonly bytes: number;
};
export type NodeVfsObserver = (event: NodeVfsObservation) => void;
export declare function openNodeVfs(options: OpenNodeVfsOptions): Promise<NodeVfsHandle>;

/* ===== packages/sqlite-node/dist/index.d.ts ===== */
import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SQLiteDriverCapabilities, SQLiteCheckpointResult, SQLitePhysicalStorage, TransactionMode } from "@ephemeralai/fs/sqlite-driver";
export interface OpenNodeSqliteOptions {
    readonly filename: string;
    readonly readOnly?: boolean;
    readonly create?: boolean;
    readonly busyTimeoutMs?: number;
    readonly durability?: "acknowledged" | "relaxed-test";
    readonly cacheTargetBytes?: number;
    readonly mmapLimitBytes?: number;
    readonly maxPhysicalDatabaseBytes?: number;
    readonly maxJournalBytes?: number;
}
export declare class NodeSQLiteDriver implements FilesystemSQLiteDriver {
    #private;
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    constructor(options: OpenNodeSqliteOptions);
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void;
    physicalStorage(): SQLitePhysicalStorage;
    checkpoint(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
}
export declare function openNodeSqlite(options: OpenNodeSqliteOptions): Promise<NodeSQLiteDriver>;

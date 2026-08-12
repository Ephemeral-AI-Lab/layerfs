/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: .; entry: packages/fs/dist/index.d.ts */

/* export: BranchConfiguration; kinds: type */
/* source: packages/fs/dist/resources/limits.d.ts */
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

/* export: BranchError; kinds: value,type */
/* source: packages/fs/dist/branches/types.d.ts */
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

/* export: BranchErrorCode; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export type BranchErrorCode = "InvalidBranchId" | "InvalidOperationId" | "BranchNotFound" | "BranchNotActive" | "RevisionNotFound" | "BranchChanged" | "OperationBranchMismatch" | "OperationNotFound" | "OperationResultExpired" | "LimitExceeded";

/* export: Branches; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface Branches {
    create(id: string): Promise<EphemeralBranch>;
    create(options?: CreateBranchOptions): Promise<EphemeralBranch>;
    open(id: string): Promise<EphemeralBranch>;
    get(id: string): Promise<BranchInfo>;
    replay(operationId: string, branchId?: string): Promise<PublishResult>;
}

/* export: BranchInfo; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface BranchInfo {
    readonly id: string;
    readonly baseRevision: RevisionId;
    readonly state: BranchState;
    readonly generation: number;
    readonly createdAt: number;
    readonly terminalAt: number | null;
    readonly mergedRevision: RevisionId | null;
}

/* export: BranchState; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export type BranchState = "active" | "merged" | "discarded";

/* export: ConflictPublishResult; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface ConflictPublishResult {
    readonly outcome: "conflict";
    readonly branchId: string;
    readonly operationId: string | null;
    readonly baseRevision: RevisionId;
    readonly headRevision: RevisionId;
    readonly revision: null;
    readonly changedPaths: [
    ];
    readonly conflicts: PublishConflict[];
}

/* export: ConflictReason; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export type ConflictReason = "entry-changed" | "node-changed" | "source-changed" | "destination-changed" | "subtree-changed" | "ancestor-changed";

/* export: CreateBranchOptions; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface CreateBranchOptions {
    readonly id?: string;
    readonly baseRevision?: RevisionId;
}

/* export: DirectoryEntry; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface DirectoryEntry {
    readonly name: string;
    readonly parentPath: string;
    readonly type: FileType;
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
}

/* export: EffectiveLimit; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface EffectiveLimit {
    readonly domain: "filesystem" | "storage" | "branch" | "runtime";
    readonly name: string;
    readonly value: number;
    readonly scope: "persisted" | "runtime";
    readonly constrainedBy: "configuration" | "format" | "adapter";
}

/* export: EPHEMERAL_AI_FS_VERSION; kinds: value */
/* source: packages/fs/dist/index.d.ts */
EPHEMERAL_AI_FS_VERSION = "0.1.0-rc.0"

/* export: EphemeralBranch; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface EphemeralBranch extends Omit<EphemeralFilesystem, "close"> {
    readonly id: string;
    info(): Promise<BranchInfo>;
    publish(options?: PublishOptions): Promise<PublishResult>;
    discard(): Promise<BranchInfo>;
    close(): Promise<void>;
}

/* export: EphemeralFilesystem; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: EphemeralFS; kinds: value,type */
/* source: packages/fs/dist/filesystem/ephemeral-fs.d.ts */
/** Public composition root: injects the private SQLite storage-port adapter. */
export declare class EphemeralFS {
    static open(options: OpenFilesystemOptions): Promise<EphemeralFilesystem>;
}

/* export: FileContent; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type FileContent = string | Uint8Array | ReadableStream<Uint8Array>;

/* export: FileStat; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: FilesystemCapabilities; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: FilesystemError; kinds: value,type */
/* source: packages/fs/dist/filesystem/errors.d.ts */
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

/* export: FilesystemErrorCode; kinds: type */
/* source: packages/fs/dist/filesystem/errors.d.ts */
export type FilesystemErrorCode = "EINVAL" | "ENOENT" | "ENOTDIR" | "EISDIR" | "EEXIST" | "ENOTEMPTY" | "ELOOP" | "EPERM" | "EROFS" | "EBADF" | "EAGAIN" | "EBUSY" | "EFBIG" | "ENOSPC" | "ECORRUPT" | "ESCHEMA" | "EIO";

/* export: FilesystemLimits; kinds: type */
/* source: packages/fs/dist/resources/limits.d.ts */
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

/* export: FilesystemMaintenance; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface FilesystemMaintenance {
    collectGarbage(options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
    snapshotStorage(options?: StorageSnapshotOptions): Promise<StorageSnapshot>;
    verify(options?: VerificationOptions): Promise<VerificationResult>;
}

/* export: FilesystemObservation; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface FilesystemObservation {
    readonly type: "operation" | "integrity" | "maintenance";
    readonly operation: string;
    readonly outcome: "success" | "error";
    readonly elapsedMs: number;
    readonly counters: Readonly<Record<string, number>>;
    readonly errorCode?: FilesystemErrorCode;
}

/* export: FilesystemObserver; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type FilesystemObserver = (event: FilesystemObservation) => void;

/* export: FileType; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type FileType = "file" | "directory" | "symlink";

/* export: GarbageCollectionOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface GarbageCollectionOptions {
    readonly runId?: string;
    readonly maxBatches?: number;
    readonly signal?: AbortSignal;
}

/* export: GarbageCollectionResult; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: MergedPublishResult; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface MergedPublishResult {
    readonly outcome: "merged";
    readonly branchId: string;
    readonly operationId: string | null;
    readonly baseRevision: RevisionId;
    readonly parentRevision: RevisionId;
    readonly revision: RevisionId;
    readonly changedPaths: string[];
    readonly conflicts: [
    ];
}

/* export: MkdirOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface MkdirOptions {
    readonly recursive?: boolean;
    readonly mode?: number;
}

/* export: OpenFilesystemOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: PhysicalStorageSnapshot; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface PhysicalStorageSnapshot {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
    readonly freelistBytes?: number;
}

/* export: PublishConflict; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface PublishConflict {
    readonly path: string;
    readonly reason: ConflictReason;
    readonly expectedRevision: RevisionId | null;
    readonly actualRevision: RevisionId | null;
}

/* export: PublishOptions; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface PublishOptions {
    readonly operationId?: string;
}

/* export: PublishResult; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export type PublishResult = MergedPublishResult | ConflictPublishResult;

/* export: ReaddirOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReaddirOptions {
    readonly limit?: number;
    readonly startAfter?: string;
}

/* export: ReadRangeOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReadRangeOptions {
    readonly offset: number;
    readonly length: number;
}

/* export: ReadStreamOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReadStreamOptions {
    readonly offset?: number;
    readonly length?: number;
    readonly signal?: AbortSignal;
}

/* export: ReadTextOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReadTextOptions {
    readonly encoding: "utf8";
}

/* export: RevisionId; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export type RevisionId = string;

/* export: RmOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface RmOptions {
    readonly recursive?: boolean;
    readonly force?: boolean;
}

/* export: RuntimeLimits; kinds: type */
/* source: packages/fs/dist/resources/limits.d.ts */
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

/* export: StorageFormat; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface StorageFormat {
    readonly cowPageBytes: CowPageBytes;
    readonly hashAlgorithm: "sha256";
    readonly chunkerAlgorithm: "fastcdc-v1";
    readonly manifestFormat: "efs-merkle-manifest-v1";
}

/* export: StorageFormatOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface StorageFormatOptions {
    readonly cowPageBytes?: CowPageBytes;
}

/* export: StorageLimits; kinds: type */
/* source: packages/fs/dist/resources/limits.d.ts */
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

/* export: StorageSnapshot; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: StorageSnapshotOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface StorageSnapshotOptions {
    readonly maxBatches?: number;
    readonly signal?: AbortSignal;
}

/* export: VerificationOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface VerificationOptions {
    readonly scopes?: readonly VerificationScope[];
    readonly cursor?: string;
    readonly maxEntities?: number;
    readonly signal?: AbortSignal;
}

/* export: VerificationResult; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: VerificationScope; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type VerificationScope = "metadata" | "namespace" | "manifests" | "objects" | "head";

/* export: WriteFileOptions; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface WriteFileOptions {
    readonly mode?: number;
    readonly exclusive?: boolean;
    readonly signal?: AbortSignal;
    /** Required upper bound for a streamed write; buffered values infer their length. */
    readonly maxBytes?: number;
}

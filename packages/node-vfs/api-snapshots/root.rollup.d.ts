/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-node-vfs; subpath: .; entry: packages/node-vfs/dist/index.d.ts */

/* ===== packages/fs/dist/branches/types.d.ts ===== */
import type { EphemeralFilesystem, EphemeralFilesystemAdministration } from "../filesystem/types.js";
export type RevisionId = string;
export type BranchState = "active" | "merged" | "discarded";
export interface BranchInfo {
    readonly id: string;
    readonly baseRevision: RevisionId;
    readonly state: BranchState;
    readonly generation: number;
    /** Canonical digest of the complete semantic branch generation. */
    readonly generationDigest: string;
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
    readonly expectedGeneration?: number;
    readonly expectedGenerationDigest?: string;
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
    readonly branchGeneration: number;
    readonly branchGenerationDigest: string;
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
    readonly branchGeneration: number;
    readonly branchGenerationDigest: string;
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
export type BranchErrorCode = "InvalidBranchId" | "InvalidOperationId" | "InvalidPublicationExpectation" | "BranchNotFound" | "BranchNotActive" | "RevisionNotFound" | "BranchChanged" | "OperationBranchMismatch" | "OperationRequestMismatch" | "OperationNotFound" | "OperationResultExpired" | "LimitExceeded";
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

/* ===== packages/fs/dist/cache/content-cache.d.ts ===== */
import { AdmissionController } from "../resources/limits.js";
export type ContentCacheKind = "object" | "manifest-root" | "manifest-node";
export interface ContentCacheMetrics {
    readonly bytes: number;
    readonly highWaterBytes: number;
    readonly hits: number;
    readonly misses: number;
    readonly admissions: number;
    readonly bypasses: number;
    readonly evictions: number;
}
export interface ContentCacheReservation {
    readonly weight: number;
    release(): void;
}
export interface ContentCacheUse<T> {
    readonly value: T;
}
export declare class ContentCache {
    #private;
    constructor(limitBytes: number, admission: AdmissionController);
    withCopy<T>(kind: ContentCacheKind, hash: Uint8Array, consume: (bytes: Uint8Array) => T): ContentCacheUse<T> | undefined;
    copyInto(kind: ContentCacheKind, hash: Uint8Array, expectedSize: number, sourceOffset: number, destination: Uint8Array, destinationOffset: number, length: number): boolean | undefined;
    containsExact(kind: ContentCacheKind, hash: Uint8Array, expectedSize: number): boolean | undefined;
    reserveOperation(weight: number): () => void;
    tryReserve(weight: number): ContentCacheReservation | undefined;
    reserve(weight: number): ContentCacheReservation | undefined;
    admit(kind: ContentCacheKind, hash: Uint8Array, bytes: Uint8Array, reservation: ContentCacheReservation): void;
    makeRoom(additionalBytes: number): void;
    clear(): void;
    metrics(): ContentCacheMetrics;
}

/* ===== packages/fs/dist/cas/sha256.d.ts ===== */
export declare class IncrementalSha256 {
    #private;
    update(input: Uint8Array): this;
    digest(): Uint8Array;
}
export type CasObjectId = string & {
    readonly __casObjectId: unique symbol;
};
export type ManifestId = string & {
    readonly __manifestId: unique symbol;
};
export type HashFunction = (bytes: Uint8Array) => Uint8Array;
export declare const sha256: HashFunction;
export declare function sha256Hex(bytes: Uint8Array): CasObjectId;
export declare function casObjectId(value: string): CasObjectId;
export declare function manifestId(value: string): ManifestId;
export declare function manifestIdFromHash(hash: Uint8Array): ManifestId;
export interface CasObject {
    readonly id: CasObjectId;
    readonly bytes: Uint8Array;
}
export declare function createCasObject(bytes: Uint8Array): CasObject;
export declare function verifyCasObject(expectedDigest: Uint8Array | string, bytes: Uint8Array): void;

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

/* ===== packages/fs/dist/filesystem/ephemeral-runtime.d.ts ===== */
import type { EphemeralFS as PublicEphemeralFS } from "./ephemeral-fs.js";
import type { OpenFilesystemOptions, ReplicationFilesystemBridge, ReplicationFilesystemIdentity, ReplicationRole } from "./types.js";
import type { NodeVfsFilesystemBridge } from "../operations/node-vfs-bridge.js";
export interface OpenEphemeralRuntimeOptions extends OpenFilesystemOptions {
    readonly provisioningState?: "bound" | "unbound-replica";
    readonly replicationIdentity?: {
        readonly authorityId: string;
        readonly role: ReplicationRole;
    };
}
/** One ownership root for the portable FS, replication, and branch Node VFS. */
export declare class EphemeralRuntime {
    #private;
    readonly provisioningState: "bound" | "unbound-replica";
    readonly identity: ReplicationFilesystemIdentity | null;
    readonly filesystem: PublicEphemeralFS | null;
    readonly replication: ReplicationFilesystemBridge;
    private constructor();
    static open(options: OpenEphemeralRuntimeOptions): Promise<EphemeralRuntime>;
    openNodeVfs(options?: {
        readonly branchId?: string;
    }): NodeVfsFilesystemBridge;
    close(): Promise<void>;
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
export type ReplicationTransferRecord = {
    readonly kind: "object-descriptor";
    readonly digest: Uint8Array;
    readonly byteLength: number;
} | {
    readonly kind: "object-payload";
    readonly digest: Uint8Array;
    readonly byteLength: number;
    readonly bytes: Uint8Array;
} | {
    readonly kind: "manifest-root-descriptor";
    readonly format: string;
    readonly digest: Uint8Array;
    readonly encodedLength: number;
    readonly logicalFileLength: number;
    readonly entryCount: number;
    readonly rootNodeDigest: Uint8Array;
} | {
    readonly kind: "manifest-node-descriptor";
    readonly digest: Uint8Array;
    readonly nodeKind: "leaf" | "internal";
    readonly encodedLength: number;
    readonly logicalSpan: number;
    readonly entryCount: number;
} | {
    readonly kind: "missing-content";
    readonly contentKind: "object" | "manifest-root" | "manifest-node";
    readonly digest: Uint8Array;
} | {
    readonly kind: "revision-fragment";
    readonly revisionId: string;
    readonly parentRevisionId: string | null;
    readonly fragmentIndex: number;
    readonly fragmentCount: number;
    readonly fragmentBytes: Uint8Array;
} | {
    readonly kind: "checkpoint-fragment";
    readonly checkpointId: string;
    readonly revisionId: string;
    readonly fragmentIndex: number;
    readonly fragmentCount: number;
    readonly fragmentBytes: Uint8Array;
} | {
    readonly kind: "branch-generation-fragment";
    readonly branchId: string;
    readonly baseRevision: string;
    readonly generation: number;
    readonly generationDigest: Uint8Array;
    readonly fragmentIndex: number;
    readonly fragmentCount: number;
    readonly fragmentBytes: Uint8Array;
} | {
    readonly kind: "terminal-result";
    readonly operationId: string;
    readonly branchId: string | null;
    readonly generation: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly resultDigest: Uint8Array;
    readonly resultBytes: Uint8Array;
};
export interface ReplicationExportMeta {
    readonly filesystemId: string;
    readonly rootInode: string;
    readonly mainRevision: number;
    readonly rootMutationGeneration: number;
    readonly nextAllocationSequence: number;
    readonly cowPageBytes: number;
    readonly createdAtMs: number;
    readonly maxManifestEntries: number;
    readonly maxManifestDepth: number;
    readonly maxFileBytes: number;
    readonly writerProfile: string;
    readonly manifestFormat: string;
    readonly chunkerFormat: string;
    readonly fastCdcMinimum: number;
    readonly fastCdcAverage: number;
    readonly fastCdcMaximum: number;
    readonly rootInodeType: number;
    readonly rootMode: number;
    readonly rootBirthtimeMs: number;
    readonly rootMtimeMs: number;
    readonly rootCtimeMs: number;
    readonly rootToken: number;
}
export type ReplicationAuthorityResult = {
    readonly kind: "publication";
    readonly operationId: string;
    readonly outcome: "merged" | "conflict";
    readonly resultDigest: Uint8Array;
} | {
    readonly kind: "discard";
    readonly operationId: string | null;
    readonly resultDigest: Uint8Array;
};
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
export type ReplicationFlow = "authority-main-to-replica" | "authority-branch-to-replica" | "replica-branch-to-authority" | "replica-branch-to-replica";
export type ReplicationRole = "main-authority" | "replica";
export interface ReplicationFastCdcConfiguration {
    readonly minimum: number;
    readonly average: number;
    readonly maximum: number;
}
export interface ReplicationBridgeFeatures {
    readonly authorityMainToReplica: boolean;
    readonly authorityBranchToReplica: boolean;
    readonly replicaBranchToAuthority: boolean;
    readonly replicaBranchToReplica: boolean;
    readonly checkpointBootstrap: boolean;
    readonly segmentedMerkleManifestTransfer: boolean;
    readonly durableStagingLeases: boolean;
    readonly physicalRestartRecovery: boolean;
    readonly terminalResultReplication: boolean;
    readonly freshReplicaProvisioning: boolean;
}
export interface ReplicationBridgeLimits {
    readonly maxBatchEntries: number;
    readonly maxBatchBytes: number;
    readonly maxRequestBytes: number;
    readonly maxResponseBytes: number;
    readonly maxBufferedBytes: number;
    readonly maxInFlightBatches: number;
    readonly maxConcurrentSessions: number;
    readonly maxStagingBytesPerSession: number;
    readonly maxReplicationSessionRows: number;
    readonly maxReplicationMetadataBytes: number;
    readonly maxReceiptsPerSession: number;
    readonly maxReceiptBytesPerSession: number;
    readonly maxCursorBytes: number;
    readonly maxTerminalResultBytes: number;
    readonly maxCursorAgeMs: number;
    readonly stagingLeaseMs: number;
    readonly resultRetentionMs: number;
    readonly maxRetryAttempts: number;
    readonly maxRetryElapsedMs: number;
    readonly minRetryDelayMs: number;
    readonly maxRetryDelayMs: number;
}
export interface ReplicationBridgeStorageCapabilities {
    readonly maxBlobBytes: number;
    readonly maxManifestNodeBytes: number;
    readonly maxManifestDepth: number;
    readonly maxManagedPayloadBytes: number;
    readonly maxStagingPayloadBytes: number;
    readonly maxMaintenanceBytes: number;
    readonly maintenanceReserveBytes: number;
    readonly maxPermanentIdentifiers: number;
    readonly maxFinalTransactionRows: number;
    readonly maxFinalTransactionBytes: number;
}
export interface ReplicationBridgeCapabilities {
    readonly provisioningState: "bound" | "unbound-replica";
    readonly filesystemId: string | null;
    readonly authorityId: string | null;
    readonly applicationId: number;
    readonly filesystemSchemaVersion: number | null;
    readonly storageUserVersion: number;
    readonly storageMigrationState: "none";
    readonly readableFilesystemSchemaVersions: readonly number[];
    readonly writableFilesystemSchemaVersion: number;
    readonly role: ReplicationRole;
    readonly activeManifestFormat: string | null;
    readonly supportedManifestFormats: readonly string[];
    readonly activeChunkerFormat: string | null;
    readonly supportedChunkerFormats: readonly string[];
    readonly fastCdc: ReplicationFastCdcConfiguration | null;
    readonly supportedFastCdcConfigurations: readonly ReplicationFastCdcConfiguration[];
    readonly copyOnWritePageBytes: 4096 | 8192 | 16384 | null;
    readonly supportedCopyOnWritePageBytes: readonly (4096 | 8192 | 16384)[];
    readonly features: ReplicationBridgeFeatures;
    readonly limits: ReplicationBridgeLimits;
    readonly storage: ReplicationBridgeStorageCapabilities;
}
export interface ReplicationFilesystemIdentity {
    readonly filesystemId: string;
    readonly authorityId: string;
    readonly role: ReplicationRole;
}
export type ReplicationPhase = "handshake" | "plan-selection" | "content-offer" | "missing-content" | "content-transfer" | "state-transfer" | "activation" | "result-acknowledgement" | "cleanup";
export interface ReplicationSessionBinding {
    readonly operationId: string;
    readonly sessionId: string;
    readonly resumeKey: Uint8Array;
    readonly ownerNonce: Uint8Array;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
    readonly sourceFilesystemId: string;
    readonly destinationFilesystemId: string;
    readonly sourceRole: ReplicationRole;
    readonly destinationRole: ReplicationRole;
    readonly sourceAuthorizationDigest: Uint8Array;
    readonly destinationAuthorizationDigest: Uint8Array;
    readonly sourceCapabilityDigest: Uint8Array;
    readonly destinationCapabilityDigest: Uint8Array;
    readonly effectiveLimitsDigest: Uint8Array;
    readonly maxBatchEntries: number;
    readonly maxBatchBytes: number;
    readonly maxRequestBytes: number;
    readonly maxResponseBytes: number;
    readonly maxBufferedBytes: number;
    readonly maxInFlightBatches: number;
    readonly maxConcurrentSessions: number;
    readonly maxCursorBytes: number;
    readonly maxReplicationSessionRows: number;
    readonly maxReplicationMetadataBytes: number;
    readonly maxReceiptsPerSession: number;
    readonly maxReceiptBytesPerSession: number;
    readonly maxStagingBytesPerSession: number;
    readonly maxAcknowledgementBytes: number;
    readonly maxTerminalResultBytes: number;
    readonly maxCursorAgeMs: number;
    readonly stagingLeaseMs: number;
    readonly maxRetryAttempts: number;
    readonly maxRetryElapsedMs: number;
    readonly minRetryDelayMs: number;
    readonly maxRetryDelayMs: number;
    readonly resultRetentionMs: number;
}
export interface CreateReplicationSessionRequest {
    readonly binding: ReplicationSessionBinding;
    readonly phase: ReplicationPhase;
    readonly cursor: Uint8Array;
    readonly cursorDigest: Uint8Array;
    readonly now: number;
    readonly expiresAtMs: number;
}
export interface ReplicationSessionSnapshot {
    readonly operationId: string;
    readonly sessionId: string;
    readonly phase: ReplicationPhase;
    readonly cursor: Uint8Array;
    readonly cursorDigest: Uint8Array;
    readonly nextSequence: number;
    readonly chainDigest: Uint8Array;
    readonly acceptedEntries: number;
    readonly acceptedBytes: number;
    readonly stagedBytes: number;
    readonly attempts: number;
    readonly elapsedRetryMs: number;
    readonly lastWallClockMs: number;
    readonly retryDeadlineMs: number;
    readonly terminal: boolean;
}
export interface ReplicationExportSelection {
    readonly selectedRevision: number;
    readonly selectedGeneration: number | null;
    readonly destinationHead: number;
    readonly rootMutationGeneration: number;
    readonly nextAllocationSequence: number;
    readonly rootInode: string;
}
export interface ReplicationExportBatch {
    readonly records: readonly ReplicationTransferRecord[];
    readonly complete: boolean;
    readonly offered: number;
    readonly reused: number;
}
export interface ReplicationExportSummary {
    readonly selectedRevision: number;
    readonly selectedGeneration: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly baseRevision: number;
    readonly rootCount: number;
    readonly nodeCount: number;
    readonly objectCount: number;
    readonly objectBytes: number;
    readonly stateRows: number;
    readonly complete: boolean;
}
export interface ReplicationGenesisCapture {
    readonly meta: ReplicationExportMeta;
    readonly rows: readonly {
        readonly inodeId: string;
        readonly tombstone: boolean;
        readonly encoded: Uint8Array | null;
    }[];
}
export interface ReplicationImportApply {
    readonly stagedBytesDelta: number;
    readonly insertedObjects: number;
    readonly reusedObjects: number;
    readonly insertedNodes: number;
    readonly reusedNodes: number;
    readonly insertedRoots: number;
    readonly reusedRoots: number;
    readonly missingCount: number;
    readonly transferredCount: number;
}
export interface ReplicationBatchAcceptanceRequest {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly sequence: number;
    readonly phase: ReplicationPhase;
    readonly priorCursorDigest: Uint8Array;
    /** SHA-256 of the complete canonical v1 batch envelope, computed by the package. */
    readonly batchEnvelopeDigest: Uint8Array;
    readonly payloadDigest: Uint8Array;
    readonly entryCount: number;
    readonly payloadByteCount: number;
    readonly nextPhase: ReplicationPhase;
    readonly nextCursor: Uint8Array;
    readonly nextCursorDigest: Uint8Array;
    /** Exact canonical v1 batch-acknowledgement envelope. */
    readonly acknowledgement: Uint8Array;
    readonly stagedBytesDelta: number;
    readonly now: number;
}
export interface ReplicationSessionStore {
    filesystemIdentity(): ReplicationFilesystemIdentity | undefined;
    bindFilesystemIdentity(identity: ReplicationFilesystemIdentity): ReplicationFilesystemIdentity;
    createOrResume(request: CreateReplicationSessionRequest): Readonly<{
        created: boolean;
        session: ReplicationSessionSnapshot;
    }>;
    resume(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly resumeKey: Uint8Array;
    }): ReplicationSessionSnapshot;
    findSession(request: {
        readonly operationId: string;
        readonly resumeKey: Uint8Array;
    }): Readonly<{
        readonly binding: ReplicationSessionBinding;
        readonly session: ReplicationSessionSnapshot;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
    }>;
    loadSession(request: {
        readonly operationId: string;
    }): Readonly<{
        readonly binding: ReplicationSessionBinding;
        readonly session: ReplicationSessionSnapshot;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
    }>;
    acceptBatch(request: ReplicationBatchAcceptanceRequest): Readonly<{
        replayed: boolean;
        acknowledgement: Uint8Array;
        session: ReplicationSessionSnapshot;
    }>;
    compactReceipts(request: {
        readonly operationId: string;
        readonly ownerNonce: Uint8Array;
        readonly throughSequence: number;
        readonly maxRows: number;
    }): Readonly<{
        readonly compactedThrough: number;
        readonly deletedRows: number;
        readonly deletedBytes: number;
    }>;
    maintenance(request: {
        readonly now: number;
        readonly maxRows: number;
    }): Readonly<{
        readonly expiredSessions: number;
    }>;
    abortSession(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
    }): void;
    consumeAttempt(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly wallNowMs: number;
        readonly monotonicElapsedMs: number;
        readonly delayMs: number;
    }): Readonly<{
        attempts: number;
        elapsedRetryMs: number;
        lastWallClockMs: number;
        exhausted: boolean;
    }>;
    recordOutboundBatch(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly sequence: number;
        readonly phase: ReplicationPhase;
        readonly nextPhase: ReplicationPhase;
    }): ReplicationSessionSnapshot;
    storeTerminalResult(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly result: Uint8Array;
        readonly now: number;
    }): Uint8Array;
    replayTerminalResult(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly resumeKey: Uint8Array;
        readonly now: number;
    }): Uint8Array;
}
/**
 * Schema-free durable session seam consumed by the protocol package. Content,
 * revision, checkpoint, and branch transfer commands run through the typed
 * core transfer store; no SQL, table, repository, standalone CAS insertion,
 * or standalone COW mutation is exposed here.
 */
export interface ReplicationFilesystemBridge {
    readonly capabilities: ReplicationBridgeCapabilities;
    createOrResumeSession(request: CreateReplicationSessionRequest): Promise<Readonly<{
        created: boolean;
        session: ReplicationSessionSnapshot;
    }>>;
    resumeSession(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly resumeKey: Uint8Array;
    }): Promise<ReplicationSessionSnapshot>;
    findSession(request: {
        readonly operationId: string;
        readonly resumeKey: Uint8Array;
    }): Promise<Readonly<{
        readonly binding: ReplicationSessionBinding;
        readonly session: ReplicationSessionSnapshot;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
    }>>;
    loadSession(request: {
        readonly operationId: string;
    }): Promise<Readonly<{
        readonly binding: ReplicationSessionBinding;
        readonly session: ReplicationSessionSnapshot;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
    }>>;
    recordOutboundBatch(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly sequence: number;
        readonly phase: ReplicationPhase;
        readonly nextPhase: ReplicationPhase;
        readonly nextCursor: Uint8Array;
        readonly nextCursorDigest: Uint8Array;
    }): Promise<ReplicationSessionSnapshot>;
    acceptBatch(request: ReplicationBatchAcceptanceRequest & {
        readonly records?: readonly ReplicationTransferRecord[];
    }): Promise<Readonly<{
        replayed: boolean;
        acknowledgement: Uint8Array;
        session: ReplicationSessionSnapshot;
        apply?: ReplicationImportApply;
    }>>;
    compactReceipts(request: {
        readonly operationId: string;
        readonly ownerNonce: Uint8Array;
        readonly throughSequence: number;
        readonly maxRows: number;
    }): Promise<Readonly<{
        readonly compactedThrough: number;
        readonly deletedRows: number;
        readonly deletedBytes: number;
    }>>;
    maintenance(request: {
        readonly now: number;
        readonly maxRows: number;
    }): Promise<Readonly<{
        readonly expiredSessions: number;
        readonly expiredLeases: number;
        readonly cleanupPasses: number;
    }>>;
    consumeAttempt(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly wallNowMs: number;
        readonly monotonicElapsedMs: number;
        readonly delayMs: number;
    }): Promise<Readonly<{
        attempts: number;
        elapsedRetryMs: number;
        lastWallClockMs: number;
        exhausted: boolean;
    }>>;
    recordOutboundBatch(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly sequence: number;
        readonly phase: ReplicationPhase;
        readonly nextPhase: ReplicationPhase;
        readonly nextCursor: Uint8Array;
        readonly nextCursorDigest: Uint8Array;
    }): Promise<ReplicationSessionSnapshot>;
    storeTerminalResult(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly result: Uint8Array;
        readonly now: number;
    }): Promise<Uint8Array>;
    replayTerminalResult(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly resumeKey: Uint8Array;
        readonly now: number;
    }): Promise<Uint8Array>;
    captureExport(request: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
        readonly destinationHead: number;
        readonly now: number;
    }): Promise<ReplicationExportSelection>;
    captureGenesis(request: {
        readonly sessionId: string;
        readonly now: number;
    }): Promise<ReplicationGenesisCapture>;
    readExportBatch(request: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
        readonly maxEntries: number;
        readonly maxBytes: number;
        readonly now: number;
    }): Promise<ReplicationExportBatch>;
    readExportPayloads(request: {
        readonly sessionId: string;
        readonly requested: readonly {
            readonly contentKind: "object" | "manifest-root" | "manifest-node";
            readonly digest: Uint8Array;
        }[];
        readonly maxEntries: number;
        readonly maxBytes: number;
        readonly now: number;
    }): Promise<{
        readonly records: readonly ReplicationTransferRecord[];
    }>;
    readExportStateBatch(request: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
        readonly maxEntries: number;
        readonly maxBytes: number;
        readonly now: number;
        readonly checkpoint: boolean;
        readonly allowTerminal: boolean;
    }): Promise<Readonly<{
        readonly records: readonly ReplicationTransferRecord[];
        readonly complete: boolean;
        readonly terminalResult: Readonly<{
            readonly operationId: string;
            readonly branchId: string | null;
            readonly generation: number;
            readonly generationDigest: Uint8Array;
            readonly resultBytes: Uint8Array;
        }> | null;
    }>>;
    exportSummary(request: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
    }): Promise<ReplicationExportSummary>;
    beginImport(request: {
        readonly sessionId: string;
        readonly kind: 0 | 1 | 2;
        readonly leaseId: string;
        readonly ownerNonce: Uint8Array;
        readonly branchId: string | null;
        readonly baseRevision: number | null;
        readonly generation: number | null;
        readonly expectedGenerationDigest: Uint8Array | null;
        readonly now: number;
        readonly expiresAt: number;
        readonly maxStagingBytesPerSession: number;
        readonly resultRetentionMs: number;
    }): Promise<void>;
    readMissingContent(request: {
        readonly sessionId: string;
        readonly maxEntries: number;
        readonly maxBytes: number;
    }): Promise<{
        readonly records: readonly ReplicationTransferRecord[];
    }>;
    finalizeImport(request: {
        readonly sessionId: string;
        readonly kind: 0 | 1 | 2;
        readonly expectedRevision: number;
        readonly expectedRootMutationGeneration: number;
        readonly expectedNextAllocationSequence: number;
        readonly expectedRootInode: string;
        readonly expectedRevisionCount: number;
        readonly expectedStateRows: number;
        readonly expectedClosureRoots: number;
        readonly expectedClosureNodes: number;
        readonly expectedClosureObjects: number;
        readonly expectedClosureObjectBytes: number;
        readonly branchId: string | null;
        readonly baseRevision: string | null;
        readonly generation: number | null;
        readonly generationDigest: Uint8Array | null;
        readonly checkpoint: boolean;
        readonly terminalState: 0 | 1 | 2;
        readonly terminalResultOperationId: string | null;
        readonly terminalResultBytes: Uint8Array | null;
        readonly genesisMeta: ReplicationExportMeta | null;
        readonly genesisRows: readonly {
            readonly inodeId: string;
            readonly tombstone: boolean;
            readonly encoded: Uint8Array | null;
        }[];
        readonly now: number;
    }): Promise<ReplicationFinalization>;
    renewImportLease(request: {
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
        readonly expiresAt: number;
    }): Promise<boolean>;
    abortImport(request: {
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
    }): Promise<void>;
    abortSession(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
    }): Promise<void>;
}
export interface ReplicationFinalization {
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: ReplicationAuthorityResult | null;
    readonly reusedBytes: number;
}

/* ===== packages/fs/dist/index.d.ts ===== */
import type { BranchCapableFilesystem } from "./branches/types.js";
export declare const EPHEMERAL_AI_FS_VERSION = "0.1.0-rc.0";
export { EphemeralFS } from "./filesystem/ephemeral-fs.js";
export { EphemeralRuntime } from "./filesystem/ephemeral-runtime.js";
export type { OpenEphemeralRuntimeOptions } from "./filesystem/ephemeral-runtime.js";
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

/* ===== packages/fs/dist/integrations/node-vfs.d.ts ===== */
import type { EphemeralFilesystem, OpenFilesystemOptions, StorageFormatOptions } from "../filesystem/types.js";
import type { EphemeralFS as PublicEphemeralFS } from "../filesystem/ephemeral-fs.js";
import { type NodeVfsFilesystemBridge, type NodeVfsManagedSlab, type NodeVfsPreparedContent, type NodeVfsPinnedReadBridge, type SynchronousContentSource } from "../operations/node-vfs-bridge.js";
import type { FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteDriver } from "../sqlite/driver.js";
/** Public composition-root options for the synchronous Node VFS bridge. */
export interface CreateNodeVfsBridgeOptions {
    readonly database: FilesystemSQLiteDriver;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly clock?: () => number;
}
export interface OpenNodeVfsBridgeResult {
    /** Async view matching the bridge: main, or the selected private branch. */
    readonly filesystem: EphemeralFilesystem;
    /** Owner of the shared cache, admission controller, and all branch handles. */
    readonly runtime: PublicEphemeralFS;
    readonly bridge: NodeVfsFilesystemBridge;
}
export interface OpenNodeVfsBridgeOptions extends OpenFilesystemOptions {
    readonly branchId?: string;
}
/**
 * Open the portable filesystem and its synchronous bridge as one core instance.
 * This is the production Node VFS composition root: both views share limits,
 * caches, concurrency, and the aggregate admission controller.
 */
export declare function openNodeVfsBridge(options: OpenNodeVfsBridgeOptions): Promise<OpenNodeVfsBridgeResult>;
/** Compose the public bridge with the private SQLite storage implementation. */
export declare function createNodeVfsBridge(options: CreateNodeVfsBridgeOptions): NodeVfsFilesystemBridge;
export type { NodeVfsFilesystemBridge, NodeVfsManagedSlab, NodeVfsPreparedContent, NodeVfsPinnedReadBridge, SynchronousContentSource, };

/* ===== packages/fs/dist/manifests/codec.d.ts ===== */
export declare const ROOT_ENVELOPE_BYTES = 68;
export declare const NODE_HEADER_BYTES = 32;
export declare const LEAF_RECORD_BYTES = 36;
export declare const INTERNAL_RECORD_BYTES = 48;
export declare const MAX_MANIFEST_ENTRY_COUNT = 4294967295;
export declare const MAX_MANIFEST_NODE_BYTES: number;
export interface ManifestParameters {
    readonly minimum: number;
    readonly average: number;
    readonly maximum: number;
}
export interface ManifestRoot {
    readonly parameters: ManifestParameters;
    readonly fileSize: number;
    readonly entryCount: number;
    readonly rootNodeHash: Uint8Array;
}
export interface ManifestEntry {
    readonly hash: Uint8Array;
    readonly length: number;
}
export interface ManifestChild {
    readonly hash: Uint8Array;
    readonly span: number;
    readonly entryCount: number;
}
export interface ManifestLeaf {
    readonly kind: "leaf";
    readonly span: number;
    readonly entryCount: number;
    readonly entries: readonly ManifestEntry[];
}
export interface ManifestInternal {
    readonly kind: "internal";
    readonly span: number;
    readonly entryCount: number;
    readonly children: readonly ManifestChild[];
}
export type ManifestNode = ManifestLeaf | ManifestInternal;
export declare function snapshotManifestParameters(parameters: ManifestParameters): Readonly<ManifestParameters>;
export declare function validateManifestParameters(parameters: ManifestParameters): void;
/**
 * Validates parameters that this runtime may use to construct or materialize
 * content. Binary inspection remains format-complete for valid uint32 values.
 */
export declare function validateSupportedManifestParameters(parameters: ManifestParameters): void;
export declare function encodeManifestRoot(root: ManifestRoot): Uint8Array;
export declare function decodeManifestRoot(bytes: Uint8Array, expectedHash?: Uint8Array): ManifestRoot;
export declare function encodeManifestNode(node: ManifestNode): Uint8Array;
export declare function decodeManifestNode(bytes: Uint8Array, expectedHash?: Uint8Array): ManifestNode;

/* ===== packages/fs/dist/namespace/paths.d.ts ===== */
import type { FilesystemLimits } from "../resources/limits.js";
export interface CanonicalPath {
    readonly value: string;
    readonly segments: readonly string[];
    readonly encodedSegments: readonly Uint8Array[];
}
export declare function canonicalizePath(input: string, limits: FilesystemLimits, syscall: string): CanonicalPath;
export declare function validateName(name: string, limits: FilesystemLimits, syscall: string): Uint8Array;
export declare function validateSymlinkTarget(target: string, limits: FilesystemLimits, syscall: string): void;
export declare function compareUtf8(left: string, right: string): number;
export declare function assertCanonicalNameBytes(name: string, bytes: Uint8Array): void;

/* ===== packages/fs/dist/operations/node-vfs-bridge.d.ts ===== */
import { AdmissionController, type FilesystemLimits, type RuntimeLimits, type StorageLimits } from "../resources/limits.js";
import { ContentCache } from "../cache/content-cache.js";
import type { DirectoryEntry, FileStat, StorageFormatOptions } from "../filesystem/types.js";
import { type SynchronousContentSource } from "./streaming-prepare.js";
import type { ClosureCertificate, OperationsStorage } from "./storage-ports.js";
/** Opaque durable content owned by the core bridge. */
export interface NodeVfsPreparedContent {
    readonly size: number;
    /** Bounded source bytes read while applying page-local edits. */
    readonly editSourceBytes?: number;
}
export interface NodeVfsOverwriteEdit {
    readonly offset: number;
    readonly source: SynchronousContentSource;
}
export interface NodeVfsCommitResult {
    readonly pinned: NodeVfsPinnedReadBridge;
}
export interface SyncPreparedContent {
    readonly manifestHash: Uint8Array;
    readonly size: number;
    readonly certificate: ClosureCertificate;
    /** Source token captured by a bounded edit preparation. */
    readonly expectedToken?: number;
    readonly preparationMode?: "local-rebuild" | "durable-path-copy";
    readonly sourceBytesRead?: number;
}
export interface NodeVfsPinnedReadBridge {
    readonly canonicalPath: string;
    readonly inodeId: string;
    readonly stat: FileStat;
    readonly size: number;
    /** Branch generation pinned by this read, absent for the main view. */
    readonly generation?: number;
    readIntoSync(destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    closeSync(): void;
}
export interface NodeVfsManagedSlab {
    readonly bytes: Uint8Array;
    release(): void;
}
export interface NodeVfsManagedMemorySnapshot {
    readonly usedBytes: number;
    readonly peakBytes: number;
    readonly limitBytes: number;
}
export interface NodeVfsResolvedPath {
    readonly canonicalPath: string;
    readonly stat: FileStat;
}
/** Core-private semantic branch view used by the synchronous bridge. */
export interface NodeVfsBranchOperations {
    version(): number;
    resolve(path: string, followFinal: boolean): NodeVfsResolvedPath;
    openPinnedRead(path: string): NodeVfsPinnedReadBridge;
    readdir(path: string): DirectoryEntry[];
    readlink(path: string): string;
    readInto(path: string, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    /** Branch-visible COW preparation; the bytes always compose base+overlay. */
    prepareOverwriteSync?: (path: string, offset: number, source: SynchronousContentSource) => SyncPreparedContent | undefined;
    prepareOverwritesSync?: (path: string, edits: readonly NodeVfsOverwriteEdit[]) => SyncPreparedContent | undefined;
    commitPrepared(path: string, prepared: SyncPreparedContent, options: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
        inodeId?: string;
        aliases?: readonly string[];
        expectedGeneration?: number;
    }): NodeVfsCommitResult;
    mkdir(path: string, options: {
        recursive?: boolean;
        mode?: number;
    }): void;
    chmod(path: string, mode: number): void;
    link(existingPath: string, newPath: string): void;
    symlink(target: string, path: string): void;
    rename(oldPath: string, newPath: string): void;
    unlink(path: string): void;
    rmdir(path: string): void;
}
export interface NodeVfsOperationsBridgeOptions {
    readonly port: OperationsStorage;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly clock?: () => number;
    readonly branch?: NodeVfsBranchOperations;
    /** Core-derived execution-replica policy for the main view only. */
    readonly mainReadOnly?: boolean;
    /** Core-owned bounded COW preparation; never exposed outside this bridge. */
    readonly prepareOverwriteSync?: (path: string, offset: number, source: SynchronousContentSource) => SyncPreparedContent | undefined;
    readonly prepareOverwritesSync?: (path: string, edits: readonly NodeVfsOverwriteEdit[]) => SyncPreparedContent | undefined;
    /** Existing filesystem resources supplied by the core composition root. */
    readonly shared?: {
        readonly filesystemLimits: Readonly<FilesystemLimits>;
        readonly storageLimits: Readonly<StorageLimits>;
        readonly runtimeLimits: Readonly<RuntimeLimits>;
        readonly cowPageBytes: 4096 | 8192 | 16384;
        readonly admission: AdmissionController;
        readonly cache: ContentCache;
    };
}
export interface NodeVfsFilesystemBridge {
    readonly filesystemLimits: Readonly<FilesystemLimits>;
    readonly storageLimits: Readonly<StorageLimits>;
    readonly runtimeLimits: Readonly<RuntimeLimits>;
    readonly cowPageBytes: 4096 | 8192 | 16384;
    /** True for the main view of an execution replica; false for branch views. */
    readonly mainReadOnly: boolean;
    activationVersionSync(): number;
    canonicalPathSync(path: string, syscall?: string): string;
    resolvePathSync(path: string, followFinal?: boolean): NodeVfsResolvedPath;
    openPinnedReadSync(path: string): NodeVfsPinnedReadBridge;
    acquireSlabSync(source: Uint8Array, sourceOffset: number, length: number): NodeVfsManagedSlab | undefined;
    reserveControlSync(bytes: number): (() => void) | undefined;
    managedMemorySync(): NodeVfsManagedMemorySnapshot;
    existsSync(path: string): boolean;
    statSync(path: string, followFinal?: boolean): FileStat;
    readdirSync(path: string): DirectoryEntry[];
    readlinkSync(path: string): string;
    readIntoSync(path: string, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    readFileSync(path: string): Uint8Array;
    prepareContentSync(bytes: Uint8Array): NodeVfsPreparedContent;
    prepareContentSourceSync(source: SynchronousContentSource): NodeVfsPreparedContent;
    prepareOverwriteSync(path: string, offset: number, source: SynchronousContentSource): NodeVfsPreparedContent | undefined;
    prepareOverwritesSync(path: string, edits: readonly NodeVfsOverwriteEdit[]): NodeVfsPreparedContent | undefined;
    abortPreparedSync(prepared: NodeVfsPreparedContent): void;
    readPreparedIntoSync(prepared: NodeVfsPreparedContent, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    commitPreparedSync(path: string, prepared: NodeVfsPreparedContent, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
        inodeId?: string;
        aliases?: readonly string[];
        expectedGeneration?: number;
    }): NodeVfsCommitResult;
    writeFileSync(path: string, bytes: Uint8Array, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
    }): void;
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
}
export declare function createNodeVfsOperationsBridge(options: NodeVfsOperationsBridgeOptions): NodeVfsFilesystemBridge;
export type { SynchronousContentSource } from "./streaming-prepare.js";

/* ===== packages/fs/dist/operations/storage-ports.d.ts ===== */
import type { BranchConfiguration, FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { CanonicalPath } from "../namespace/paths.js";
import type { CowPage, CowPageBytes } from "../cow/pages.js";
import type { ContentCache } from "../cache/content-cache.js";
import type { ManifestNode, ManifestParameters } from "../manifests/codec.js";
import type { HashFunction } from "../cas/sha256.js";
import type { ReplicationAuthorityResult, ReplicationExportMeta, ReplicationFlow, ReplicationSessionStore, ReplicationTransferRecord } from "../filesystem/types.js";
export type { ReplicationAuthorityResult, ReplicationExportMeta, ReplicationTransferRecord, } from "../filesystem/types.js";
export type StorageTransactionMode = "read" | "write" | "exclusive";
export interface StorageWorkBudget {
    readonly maxRows: number;
    readonly maxBytes: number;
    readonly maxStatements?: number;
    readonly maxElapsedMs?: number;
    readonly maxResultRows?: number;
    readonly maxResultBytes?: number;
}
export interface StorageAdapterCapabilities {
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
    readonly schemaIdentityMode?: "sqlite-header" | "durable-table";
    readonly pageMetricsMode?: "sqlite-pragma" | "runtime-size-only";
}
export interface StoragePhysicalFiles {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
}
export interface StorageCheckpointResult {
    readonly mode: "passive" | "restart" | "truncate";
    readonly busy: number;
    readonly logFrames: number;
    readonly checkpointedFrames: number;
    readonly walBytes?: number;
}
export interface StorageMetadata {
    readonly filesystemId: string;
    readonly mainRevision: number;
    readonly rootInode: string;
    readonly cowPageBytes: CowPageBytes;
}
export interface ContentObjectInput {
    readonly hash: Uint8Array;
    readonly bytes: Uint8Array;
}
export interface ContentBatchResult {
    readonly inserted: number;
    readonly deduplicated: number;
    readonly insertedBytes: number;
}
export interface AuthenticatedManifestCursorSource {
    readObjectInto(hash: Uint8Array, expectedSize: number, sourceOffset: number, destination: Uint8Array, destinationOffset: number, length: number): boolean;
    batchFetchObjects(requests: readonly {
        readonly hash: Uint8Array;
        readonly expectedSize: number;
    }[]): void;
    withManifestNode<T>(hash: Uint8Array, consume: (encoded: Uint8Array) => T): T | undefined;
}
export interface AuthenticatedManifestCursor {
    readonly fileSize: number;
    readonly position: number;
    peekEntry(): AuthenticatedManifestEntry | null;
    nextEntry(): AuthenticatedManifestEntry | null;
    readInto(destination: Uint8Array, destinationOffset: number, length: number): number;
    /**
     * Rebind the cursor's content source to the current storage transaction.
     * Carried cursors outlive any single transaction; every readInto call must
     * run against a live transaction, so the stream rebinds before each pull.
     */
    bindSource(source: AuthenticatedManifestCursorSource): void;
    close(): void;
}
export interface AuthenticatedManifestEntry {
    readonly hash: Uint8Array;
    readonly length: number;
    readonly offset: number;
}
export interface ContentStore {
    putObject(hash: Uint8Array, bytes: Uint8Array): boolean;
    putObjectsBatch(input: readonly ContentObjectInput[], trustedDigests?: boolean): ContentBatchResult;
    readObjectInto(hash: Uint8Array, expectedSize: number, sourceOffset: number, destination: Uint8Array, destinationOffset: number, length: number): boolean;
    batchFetchObjects(requests: readonly {
        readonly hash: Uint8Array;
        readonly expectedSize: number;
    }[]): void;
    verifyObject(hash: Uint8Array, expectedSize?: number, forceStorage?: boolean): boolean;
    putManifestNode(hash: Uint8Array, encoded: Uint8Array): boolean;
    putManifestNodesBatch(nodes: readonly {
        readonly hash: Uint8Array;
        readonly encoded: Uint8Array;
    }[]): ContentBatchResult;
    putManifestRoot(hash: Uint8Array, encoded: Uint8Array): boolean;
    withManifestRoot<T>(hash: Uint8Array, consume: (encoded: Uint8Array) => T): T | undefined;
    withManifestNode<T>(hash: Uint8Array, consume: (encoded: Uint8Array) => T): T | undefined;
    openManifestCursor(manifestHash: Uint8Array, offset: number): AuthenticatedManifestCursor;
}
export interface AuthenticatedManifestTreePathNode {
    readonly hash: Uint8Array;
    readonly path: readonly number[];
    readonly offset: number;
    readonly finalAtLevel: boolean;
    readonly node: ManifestNode;
    readonly selectedChildIndex?: number;
}
export interface AuthenticatedManifestTreePath {
    readonly manifestHash: Uint8Array;
    readonly parameters: ManifestParameters;
    readonly fileSize: number;
    readonly entryCount: number;
    readonly nodesRead: number;
    readonly nodes: readonly AuthenticatedManifestTreePathNode[];
    readonly leafOffset: number;
    readonly entryIndex: number;
    readonly entryOffset: number;
}
export interface ManifestTreeStore {
    pathAtOffset(manifestHash: Uint8Array, offset: number): AuthenticatedManifestTreePath;
    recordSubtreeSummaries(nodes: readonly {
        readonly hash: Uint8Array;
        readonly encoded: Uint8Array;
    }[]): void;
    protectSourceManifest(leaseId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array): void;
    registerReusedSubtrees(leaseId: string, ownerNonce: Uint8Array, sourceManifestHash: Uint8Array, claims: readonly {
        readonly sourcePath: readonly number[];
        readonly nodeHash: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
    }[], options?: {
        readonly knownObjectHashes?: readonly Uint8Array[];
        readonly knownNodeHashes?: readonly Uint8Array[];
        /** The same transaction already called protectSourceManifest. */
        readonly sourceManifestProtected?: boolean;
        /** Disable summary aggregation when overlap state cannot span batches. */
        readonly allowSummaries?: boolean;
        readonly certificateState?: {
            readonly chainDigest: Uint8Array;
            readonly chainFold: Uint8Array;
            readonly objectCount: number;
            readonly objectBytes: number;
            readonly nodeCount: number;
            readonly nodeBytes: number;
            readonly membershipCount: number;
        };
        readonly deferCertificateWrite?: boolean;
        readonly certificatePatch?: {
            value?: {
                readonly chainDigest: Uint8Array;
                readonly chainFold: Uint8Array;
                readonly objectCount: number;
                readonly objectBytes: number;
                readonly nodeCount: number;
                readonly nodeBytes: number;
                readonly membershipCount: number;
            };
        };
        /** Source-authenticated proof supplied by the bounded local path. */
        readonly authenticatedClaims?: readonly {
            readonly sourcePath: readonly number[];
            readonly nodeHash: Uint8Array;
            readonly span: number;
            readonly entryCount: number;
            readonly sourceFinalAtLevel: boolean;
            readonly sourceLeafDelta: number;
        }[];
    }): readonly {
        readonly nodeHash: Uint8Array;
        readonly sourceManifestHash: Uint8Array;
        readonly sourcePath: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
        readonly validatedNonfinalLeafDelta: number | null;
        readonly validatedFinalLeafDelta: number | null;
        readonly summaryUsable: boolean;
        readonly summary?: {
            readonly objectCount: number;
            readonly objectBytes: number;
            readonly nodeCount: number;
            readonly nodeBytes: number;
            readonly membershipCount: number;
            readonly closureFold: Uint8Array;
        };
    }[];
}
export interface InodeRow {
    readonly id: string;
    readonly type: number;
    readonly mode: number;
    readonly birthtime_ms: number;
    readonly mtime_ms: number;
    readonly ctime_ms: number;
    readonly nlink: number;
    readonly size: number | null;
    readonly manifest_hash: Uint8Array | null;
    readonly symlink_target: string | null;
    readonly token: number;
}
export interface EntryRow {
    readonly parent_inode: string;
    readonly name_sort: Uint8Array;
    readonly name: string | null;
    readonly inode_id: string | null;
    readonly token: number;
}
export interface ChildRow {
    readonly name: string;
    readonly name_sort: Uint8Array;
    readonly inode_id: string;
    readonly token: number;
    readonly type: number;
}
export interface ResolvedPath {
    readonly path: CanonicalPath;
    readonly inode: InodeRow;
    readonly parentInode: string | null;
    readonly name: string;
    readonly nameSort: Uint8Array | null;
    readonly entryToken: number | null;
    /** Read-snapshot namespace state, when supplied by the SQLite resolver. */
    readonly mainRevision?: number;
    readonly rootMutationGeneration?: number;
}
export interface NamespaceStore {
    meta(): {
        readonly root_inode: string;
        readonly main_revision: number;
        readonly root_mutation_generation: number;
    };
    inode(id: string): InodeRow | undefined;
    entry(parentInode: string, nameSort: Uint8Array): EntryRow | undefined;
    resolve(input: string | CanonicalPath, followFinal?: boolean): ResolvedPath;
    resolveOptional(input: string | CanonicalPath, followFinal?: boolean): ResolvedPath | undefined;
    resolveParent(path: CanonicalPath): {
        readonly parent: ResolvedPath;
        readonly name: string;
        readonly nameSort: Uint8Array;
    };
    nextRevision(now: number, changeCount: number, writer?: string): number;
    /** Optimistic local-edit handoff; falls back internally if the snapshot is stale. */
    nextRevisionFromSnapshot?(now: number, changeCount: number, mainRevision: number, rootMutationGeneration: number, writer?: string): number;
    recordInode(revision: number, inodeId: string, tombstone?: boolean): void;
    /** Records a just-allocated file revision from its already-updated inode state. */
    recordFileContentRevision?(revision: number, inode: InodeRow): void;
    recordEntry(revision: number, parentInode: string, nameSort: Uint8Array, tombstone?: boolean): void;
    putEntry(parentInode: string, nameSort: Uint8Array, name: string | null, inodeId: string | null, token: number): void;
    children(parentInode: string, limit: number, maxBytes: number, startAfter?: Uint8Array): readonly ChildRow[];
    childCount(parentInode: string): number;
    linkCount(inodeId: string): number;
    createInode(value: {
        readonly id: string;
        readonly type: number;
        readonly mode: number;
        readonly now: number;
        readonly revision: number;
        readonly size?: number | null;
        readonly manifestHash?: Uint8Array | null;
        readonly symlinkTarget?: string | null;
    }): void;
    upsertInode(value: {
        readonly id: string;
        readonly type: number;
        readonly mode: number;
        readonly birthtimeMs: number;
        readonly mtimeMs: number;
        readonly ctimeMs: number;
        readonly nlink: number;
        readonly size: number | null;
        readonly manifestHash: Uint8Array | null;
        readonly symlinkTarget: string | null;
        readonly token: number;
    }): void;
    setFileContent(id: string, size: number, manifestHash: Uint8Array, mtime: number, ctime: number, token: number, expectedToken?: number): number;
    setMode(id: string, mode: number, ctime: number, token: number): void;
    incrementLinks(id: string, ctime: number, token: number): void;
    decrementLinks(id: string, ctime: number, token: number): void;
    setLinks(id: string, count: number, ctime: number, token: number): void;
    touch(id: string, mtime: number, ctime: number, token: number): void;
    deleteEntriesUnder(parentInode: string, tombstonesOnly?: boolean): void;
    deleteInode(id: string): void;
    bumpRoot(kind: number, id: string, mayRemoveRoots?: boolean): void;
}
export interface BranchRow {
    readonly id: string;
    readonly base_revision: number;
    readonly state: number;
    readonly generation: number;
    readonly created_at_ms: number;
    readonly terminal_at_ms: number | null;
    readonly merged_revision: number | null;
}
export interface BranchHistoryRow {
    readonly tombstone: number;
    readonly encoded: Uint8Array | null;
}
export interface BranchHistoryEntryRow {
    readonly name_sort: Uint8Array;
    readonly tombstone: number;
    readonly encoded: Uint8Array | null;
}
export interface BranchChangeRow {
    readonly path: Uint8Array;
    readonly expected_token: number | null;
    readonly kind: number;
    readonly encoded: Uint8Array | null;
}
export interface BranchResultRow {
    readonly branch_id: string;
    readonly generation: number;
    readonly reservation_nonce: Uint8Array;
    readonly outcome: number;
    readonly encoded: Uint8Array | null;
    readonly expires_at_ms: number | null;
}
export interface BranchStore {
    filesystemId(): string;
    rootInodeId(): string;
    historyEntries(parentInode: string, revision: number): readonly BranchHistoryEntryRow[];
    historicEntry(parentInode: string, nameSort: Uint8Array, revision: number): BranchHistoryRow | undefined;
    historicInode(inodeId: string, revision: number): BranchHistoryRow | undefined;
    inodeOverlay(branchId: string, inodeId: string, maxBytes: number): Uint8Array | undefined;
    change(branchId: string, path: Uint8Array): BranchChangeRow | undefined;
    changes(branchId: string): readonly BranchChangeRow[];
    activeCount(): number;
    headRevision(): number;
    revisionExists(revision: number): boolean;
    create(id: string, baseRevision: number, now: number): BranchRow;
    row(id: string): BranchRow | undefined;
    terminalGenerationDigest(branchId: string, generation: number): string | undefined;
    putTerminalGenerationDigest(branchId: string, generation: number, digest: string): void;
    operationResult(operationId: string, maxBytes: number): BranchResultRow | undefined;
    reserveOperation(operationId: string, branchId: string, generation: number, now: number, reservationExpiresAt: number, reservationNonce: Uint8Array, requestBinding: Uint8Array): void;
    reclaimOperation(operationId: string, branchId: string, generation: number, now: number, reservationExpiresAt: number, reservationNonce: Uint8Array): boolean;
    expireOperation(operationId: string, reservationNonce: Uint8Array, now: number): void;
    releaseOperation(operationId: string, reservationNonce?: Uint8Array): void;
    putChange(branchId: string, path: Uint8Array, expectedToken: number | null, kind: number, encoded: Uint8Array | null): void;
    putInodeExpectation(branchId: string, inodeId: string, expectedToken: number | null): void;
    setManifestRoot(branchId: string, path: Uint8Array, manifestHash?: Uint8Array): void;
    changeCount(branchId: string): number;
    changeBytes(branchId: string): number;
    changePathBytes(branchId: string): number;
    subtreeChanged(inodeId: string, baseRevision: number): boolean;
    incrementGeneration(branchId: string): void;
    putInodeOverlay(branchId: string, inodeId: string, expectedToken: number | null, encoded: Uint8Array): void;
    finish(branchId: string, state: 1 | 2, now: number, mergedRevision?: number | null): void;
    terminalCleanupRows(branchId: string): number;
    clearChanges(branchId: string): void;
    storeResult(operationId: string, outcome: number, encoded: Uint8Array, expiresAt: number, revision: number | null): void;
    pruneExpiredResults(now: number, limit: number): number;
    pruneTerminalBranches(now: number, retentionMs: number, limit: number): number;
    maintainRevisionRetention(maxRetainedRevisions: number, now: number, limit: number): number;
}
export type StagingMemberKind = "object" | "manifest-root" | "manifest-node";
export interface StagingMember {
    readonly kind: StagingMemberKind;
    readonly hash: Uint8Array;
    readonly size: number;
    /**
     * Count-only members are already-durable objects referenced by the rebuilt
     * closure: they extend the chain and the certificate counts, but they get
     * no membership row, no metadata charge, and no staging-byte admission.
     */
    readonly counted?: boolean;
}
export interface StagingEntryRow {
    readonly entry_index: number;
    readonly object_hash: Uint8Array;
    readonly length: number;
}
export interface StagingLevelRow {
    readonly record_index: number;
    readonly node_hash: Uint8Array;
    readonly span: number;
    readonly entry_count: number;
}
export interface ClosureCertificate {
    readonly leaseId: string;
    readonly ownerNonce: Uint8Array;
    readonly manifestHash: Uint8Array;
    readonly chainDigest: Uint8Array;
    /** Commutative XOR fold of every chain member hash (the closure binding). */
    readonly chainFold: Uint8Array;
    readonly objectCount: number;
    readonly objectBytes: number;
    readonly nodeCount: number;
    readonly nodeBytes: number;
    readonly membershipCount: number;
}
export interface ValidatedSealedLease {
    readonly leaseId: string;
    readonly ownerNonce: Uint8Array;
    readonly stagedBytes: number;
    readonly ingestReservationBytes: number;
    readonly metadataReservationBytes: number;
}
export interface ReconciliationProgress {
    readonly processed: number;
    readonly complete: boolean;
}
export interface LeaseCleanupProgress {
    readonly worked: boolean;
    readonly deletedRows: number;
    readonly deletedLeases: number;
}
export interface StagingStore {
    invalidateCertificateCache(leaseId?: string): void;
    applyCertificatePatch(leaseId: string, patch: {
        readonly chainDigest: Uint8Array;
        readonly chainFold: Uint8Array;
        readonly objectCount: number;
        readonly objectBytes: number;
        readonly nodeCount: number;
        readonly nodeBytes: number;
        readonly membershipCount: number;
    }): void;
    begin(options: {
        readonly leaseId: string;
        readonly ownerId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
        readonly expiresAt: number;
        readonly kind?: number;
        readonly branchId?: string;
        readonly generation?: number;
        readonly ingestReservationBytes?: number;
        readonly metadataReservationBytes?: number;
    }): void;
    consumeIngestReservation(leaseId: string, ownerNonce: Uint8Array, bytes: number): void;
    consumeMetadataReservation(leaseId: string, ownerNonce: Uint8Array, bytes: number): void;
    putEntry(leaseId: string, entryIndex: number, objectHash: Uint8Array, length: number): void;
    putEntriesBatch(leaseId: string, entries: readonly {
        readonly entryIndex: number;
        readonly objectHash: Uint8Array;
        readonly length: number;
    }[]): void;
    entriesAfter(leaseId: string, cursor: number, limit: number, maxBytes: number): readonly StagingEntryRow[];
    putLevelRecord(leaseId: string, level: number, recordIndex: number, nodeHash: Uint8Array, span: number, entryCount: number): void;
    putLevelRecordsBatch(leaseId: string, level: number, records: readonly {
        readonly recordIndex: number;
        readonly nodeHash: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
    }[]): void;
    levelRecordsAfter(leaseId: string, level: number, cursor: number, limit: number, maxBytes: number): readonly StagingLevelRow[];
    bumpRoot(kind: number, id: string, mayRemoveRoots?: boolean): void;
    release(leaseId: string, ownerNonce: Uint8Array, requireSealed: boolean, validated?: ValidatedSealedLease): boolean;
    delete(leaseId: string, ownerNonce: Uint8Array): boolean;
    acquireReadLease(leaseId: string, ownerId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array, expiresAt: number, branchId?: string, generation?: number): void;
    renewReadLease(leaseId: string, ownerId: string, ownerNonce: Uint8Array, priorExpiresAt: number, now: number, expiresAt: number): boolean;
    releaseReadLease(leaseId: string, ownerId: string, ownerNonce: Uint8Array): boolean;
    expireBatch(now: number, limit: number): number;
    cleanupBatch(limit: number): LeaseCleanupProgress;
    appendBatch(leaseId: string, ownerNonce: Uint8Array, members: readonly StagingMember[]): ClosureCertificate;
    /** Append source-manifest boundary objects whose durability was authenticated by the caller. */
    appendCountedBatch(leaseId: string, ownerNonce: Uint8Array, members: readonly StagingMember[]): ClosureCertificate;
    /** Cache metadata for source-authenticated reused nodes registered in this transaction. */
    cacheReusedSubtreeMetadata(leaseId: string, nodeHashes: readonly Uint8Array[], metadata?: readonly {
        readonly nodeHash: Uint8Array;
        readonly sourceManifestHash: Uint8Array;
        readonly sourcePath: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
        readonly validatedNonfinalLeafDelta: number | null;
        readonly validatedFinalLeafDelta: number | null;
        readonly summaryUsable: boolean;
        readonly summary?: {
            readonly objectCount: number;
            readonly objectBytes: number;
            readonly nodeCount: number;
            readonly nodeBytes: number;
            readonly membershipCount: number;
            readonly closureFold: Uint8Array;
        };
    }[], verifiedNodeSizes?: ReadonlyMap<string, number>): void;
    /** Register local-path objects already authenticated before reconciliation. */
    registerTrustedObjects(objects: readonly {
        readonly hash: Uint8Array;
        readonly length: number;
    }[]): void;
    flushBatchedCertificate(): void;
    snapshot(leaseId: string, ownerNonce: Uint8Array): ClosureCertificate;
    beginReconciliation(leaseId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array): void;
    /** Local merged rebuild fast path; generic callers retain queued validation. */
    beginTrustedReconciliation?(leaseId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array): void;
    reconcileBatch(leaseId: string, ownerNonce: Uint8Array, workLimit: number, options?: {
        readonly skipObjectBackingCheck?: boolean;
    }): ReconciliationProgress;
    /** Complete a locally authenticated manifest without materializing queues. */
    completeTrustedLocalReconciliation?(leaseId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array, freshNodeHashes: readonly Uint8Array[], rootSize: number, leafDepth: number): ReconciliationProgress;
    seal(certificate: ClosureCertificate): void;
    validateSealed(certificate: ClosureCertificate, now?: number): ValidatedSealedLease;
}
export interface GcRunRow {
    readonly id: string;
    readonly state: number;
    readonly high_water: number;
    readonly root_generation: number;
    readonly cursor_kind: number;
    readonly cursor_value: Uint8Array | null;
    readonly examined_roots: number;
    readonly deleted_roots: number;
    readonly examined_nodes: number;
    readonly deleted_nodes: number;
    readonly examined_objects: number;
    readonly deleted_objects: number;
    readonly reclaimed_object_bytes: number;
    readonly reclaimed_manifest_bytes: number;
    readonly reclaimed_overlay_bytes: number;
}
export interface GcMarkRow {
    readonly kind: number;
    readonly hash: Uint8Array;
    readonly edge_cursor: number;
    readonly payload_size: number;
}
export interface PayloadRow {
    readonly hash: Uint8Array;
    readonly size: number;
    readonly allocation_sequence: number;
    readonly eligible?: number;
    readonly scanned_count?: number;
    readonly scanned_through?: number;
    readonly eligible_count?: number;
}
export interface StorageSnapshotRow {
    readonly object_count: number;
    readonly object_bytes: number;
    readonly manifest_root_count: number;
    readonly manifest_root_bytes: number;
    readonly manifest_node_count: number;
    readonly manifest_node_bytes: number;
    readonly page_bytes: number;
    readonly patch_bytes: number;
    readonly result_bytes: number;
    readonly charged_metadata_bytes: number;
    readonly generation: number;
    readonly logical_bytes: number;
    readonly revisions: number;
}
export interface StorageSnapshotRunRow {
    readonly state: number;
    readonly high_water: number;
    readonly root_generation: number;
    readonly last_root_removal_generation: number;
    readonly evaluation_time_ms: number;
    readonly next_root_expiry_ms: number | null;
    readonly root_kind: number;
    readonly root_cursor: Uint8Array | null;
    readonly mark_kind: number;
    readonly mark_cursor: Uint8Array | null;
    readonly stored_kind: number;
    readonly stored_cursor: number;
    readonly logical_cursor: string;
    readonly logical_complete: number;
    readonly logical_bytes: number;
    readonly overlay_kind: number;
    readonly overlay_branch_cursor: string;
    readonly overlay_inode_cursor: string;
    readonly overlay_sequence_cursor: number;
    readonly overlay_index_cursor: number;
    readonly stored_page_bytes: number;
    readonly stored_patch_bytes: number;
    readonly reclaimable_overlay_bytes: number;
    readonly result_bytes: number;
    readonly charged_metadata_bytes: number;
    readonly revision_count: number;
    readonly stored_object_count: number;
    readonly stored_object_bytes: number;
    readonly stored_manifest_root_count: number;
    readonly stored_manifest_root_bytes: number;
    readonly stored_manifest_node_count: number;
    readonly stored_manifest_node_bytes: number;
    readonly reachable_object_count: number;
    readonly reachable_object_bytes: number;
    readonly reachable_manifest_root_count: number;
    readonly reachable_manifest_root_bytes: number;
    readonly reachable_manifest_node_count: number;
    readonly reachable_manifest_node_bytes: number;
    readonly branch_exclusive_object_bytes: number;
    readonly branch_exclusive_manifest_root_bytes: number;
    readonly branch_exclusive_manifest_node_bytes: number;
    readonly committed_batches: number;
    readonly created_at_ms: number;
    readonly updated_at_ms: number;
    readonly current?: number;
}
export interface StorageSnapshotMarkRow {
    readonly kind: number;
    readonly hash: Uint8Array;
    readonly edge_cursor: number;
    readonly accounted: number;
    readonly scope_mask: number;
    readonly payload_size: number;
}
export interface StoragePayloadRow {
    readonly hash: Uint8Array;
    readonly size: number;
    readonly allocation_sequence: number;
    readonly scope_mask: number;
}
export interface StorageInodeRow {
    readonly id: string;
    readonly size: number | null;
}
export interface HashRow {
    readonly hash: Uint8Array;
    readonly encoded: Uint8Array;
}
export interface InodeVerifyRow {
    readonly id: string;
    readonly type: number;
    readonly size: number | null;
    readonly manifest_hash: Uint8Array | null;
    readonly nlink: number;
    readonly actual_links: number;
}
export interface UsageVerificationState {
    readonly mutationSequence: number;
    readonly counters: readonly number[];
}
export interface UsageVerificationBatch {
    readonly checkedRows: number;
    readonly deltas: readonly number[];
    readonly nextKey: string | null;
    readonly complete: boolean;
}
export interface MaintenanceStore {
    beginRun(runId: string, now: number): void;
    abandonRun(runId: string, completeState: number, abandonedState: number): void;
    resumeAbandonedRun(runId: string, abandonedState: number, cleanupMarksState: number): void;
    run(id: string): GcRunRow | undefined;
    activeRun(): GcRunRow | undefined;
    snapshot(): StorageSnapshotRow | undefined;
    physical(): {
        readonly pageCount: number;
        readonly pageSize: number;
        readonly freePages: number;
    };
    generation(): number;
    hashes(kind: "roots" | "nodes", after: Uint8Array, limit: number, maxBytes: number): readonly HashRow[];
    objects(after: Uint8Array, limit: number, maxBytes: number): readonly PayloadRow[];
    inodes(after: string, limit: number, maxBytes: number): readonly InodeVerifyRow[];
    pendingMarks(runId: string, limit: number, maxBytes: number): readonly GcMarkRow[];
    addMark(runId: string, kind: number, hash: Uint8Array): void;
    advanceMark(runId: string, kind: number, hash: Uint8Array, edgeCursor: number, processed: boolean): void;
    addExamined(runId: string, roots: number, nodes: number, objects: number): void;
    seedRootsBatch(runId: string, limit: number, maxBytes: number): boolean;
    sweepCandidates(runId: string, state: number, highWater: number, afterAllocationSequence: number, resultLimit: number, scanLimit: number, maxBytes: number): readonly PayloadRow[];
    reconcileSweepGeneration(runId: string, state: number): boolean;
    applySweep(runId: string, state: number, rows: readonly PayloadRow[], completeState: number, scannedThrough: number, scanComplete: boolean): void;
    cleanupMarks(runId: string, limit: number, nextState: number): boolean;
    cleanupRootJournal(runId: string, limit: number, nextState: number): boolean;
    cleanupTerminalRuns(runId: string, limit: number, completeState: number, abandonedState: number, nextState: number): boolean;
    usageVerificationState(): UsageVerificationState;
    usageVerificationPhaseCount(): number;
    usageVerificationBatch(phase: number, afterKey: string | null, limit: number, maxBytes: number): UsageVerificationBatch;
    storageSnapshot(): StorageSnapshotRunRow | undefined;
    storageSnapshotCurrent(now: number): boolean;
    storageSnapshotResult(now: number): StorageSnapshotRunRow | undefined;
    beginStorageSnapshot(now: number): void;
    recordStorageSnapshotBatch(): void;
    storageRootBatch(limit: number, maxBytes: number, now: number): boolean;
    storageMarks(limit: number, maxBytes: number): readonly StorageSnapshotMarkRow[];
    addStorageMark(kind: number, hash: Uint8Array, scopeMask: number): boolean;
    accountStorageMark(kind: number, hash: Uint8Array, payloadBytes: number): boolean;
    storagePayloadSize(kind: number, hash: Uint8Array): number | undefined;
    advanceStorageMark(kind: number, hash: Uint8Array, edgeCursor: number, processed: boolean): void;
    reconcileStorageSnapshotGeneration(now: number): boolean;
    finishStorageMarking(now: number): boolean;
    storageStoredBatch(limit: number, maxBytes: number, now: number): boolean;
    storageLogicalBatch(limit: number, maxBytes: number, now: number): boolean;
    cleanupStorageMarks(limit: number, maxBytes: number, now: number): boolean;
    resetStorageMarksBatch(limit: number, maxBytes: number): boolean;
    addReclaimedOverlayBytes(runId: string, bytes: number): void;
}
export interface PersistedPatch {
    readonly sequence: number;
    readonly generation: number;
    readonly offset: number;
    readonly deleteLength: number;
    readonly insertLength: number;
    readonly segments: readonly Uint8Array[];
}
export interface OverlayStore {
    writePages(branchId: string, inodeId: string, fileSize: number, pages: readonly CowPage[], now: number): number;
    headPages(branchId: string, inodeId: string, firstPage: number, lastPage: number): readonly CowPage[];
    leasedPages(leaseId: string, branchId: string, inodeId: string, firstPage: number, lastPage: number, baseGeneration?: number, ownerNonce?: Uint8Array): readonly CowPage[];
    leaseMembershipFits(branchId: string, inodeId: string, firstPage: number, lastPage: number, baseGeneration: number, includePages: boolean, includePatches: boolean): boolean;
    pinHeads(leaseId: string, branchId: string, inodeId: string, firstPage: number, lastPage: number, ownerNonce: Uint8Array): number;
    pinPatches(leaseId: string, branchId: string, inodeId: string, ownerNonce: Uint8Array, baseGeneration?: number): number;
    leasedPatches(leaseId: string, branchId: string, inodeId: string, ownerNonce?: Uint8Array, baseGeneration?: number): readonly PersistedPatch[];
    hasPages(branchId: string, inodeId: string): boolean;
    hasPatchesAfter(branchId: string, inodeId: string, baseGeneration: number): boolean;
    appendPatch(branchId: string, inodeId: string, currentSize: number, offset: number, deleteLength: number, segments: readonly Uint8Array[]): number;
    patches(branchId: string, inodeId: string, minimumGeneration?: number, minimumSequence?: number): readonly PersistedPatch[];
    clearPages(branchId: string, inodeId: string): void;
    clearPatches(branchId: string, inodeId: string): void;
    cleanupUnleased(limit: number): {
        readonly worked: boolean;
        readonly reclaimedPayloadBytes: number;
    };
}
export interface ReplicationExportState {
    readonly selectedRevision: number;
    readonly selectedGeneration: number | null;
    readonly destinationHead: number;
    readonly rootMutationGeneration: number;
    readonly nextAllocationSequence: number;
    readonly rootInode: string;
    readonly complete: boolean;
}
export interface ReplicationImportSummary {
    readonly leaseId: string;
    readonly kind: 0 | 1 | 2;
    readonly branchId: string | null;
    readonly baseRevision: number | null;
    readonly generation: number | null;
    readonly stagedRows: number;
    readonly stagedBytes: number;
    readonly missingCount: number;
    readonly sealed: boolean;
}
/**
 * Durable, schema-free transfer seam used by the replication bridge. Every
 * command runs inside one storage transaction; export cursors and import
 * staging survive restart and are owned exclusively by SQLite.
 */
export interface ReplicationTransferStore {
    captureExport(options: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
        readonly destinationHead: number;
        readonly now: number;
        readonly expiresAt: number;
    }): ReplicationExportState;
    captureGenesis(options: {
        readonly sessionId: string;
        readonly now: number;
        readonly expiresAt: number;
    }): Readonly<{
        readonly meta: ReplicationExportMeta;
        readonly rows: readonly {
            readonly inodeId: string;
            readonly tombstone: boolean;
            readonly encoded: Uint8Array | null;
        }[];
    }>;
    readExportBatch(options: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
        readonly maxEntries: number;
        readonly maxBytes: number;
        readonly now: number;
    }): Readonly<{
        readonly records: readonly ReplicationTransferRecord[];
        readonly complete: boolean;
        readonly offered: number;
        readonly reused: number;
    }>;
    readExportPayloads(options: {
        readonly sessionId: string;
        readonly requested: readonly {
            readonly contentKind: "object" | "manifest-root" | "manifest-node";
            readonly digest: Uint8Array;
        }[];
        readonly maxEntries: number;
        readonly maxBytes: number;
        readonly now: number;
    }): Readonly<{
        readonly records: readonly ReplicationTransferRecord[];
        readonly complete: boolean;
    }>;
    readExportStateBatch(options: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
        readonly branchId: string | null;
        readonly maxEntries: number;
        readonly maxBytes: number;
        readonly now: number;
        readonly checkpoint: boolean;
        readonly allowTerminal: boolean;
    }): Readonly<{
        readonly records: readonly ReplicationTransferRecord[];
        readonly complete: boolean;
        readonly terminalResult: Readonly<{
            readonly operationId: string;
            readonly branchId: string | null;
            readonly generation: number;
            readonly generationDigest: Uint8Array;
            readonly resultBytes: Uint8Array;
        }> | null;
    }>;
    exportSummary(options: {
        readonly sessionId: string;
        readonly flow: ReplicationFlow;
    }): Readonly<{
        readonly selectedRevision: number;
        readonly selectedGeneration: number | null;
        readonly generationDigest: Uint8Array | null;
        readonly baseRevision: number;
        readonly rootCount: number;
        readonly nodeCount: number;
        readonly objectCount: number;
        readonly objectBytes: number;
        readonly stateRows: number;
        readonly complete: boolean;
    }>;
    beginImport(options: {
        readonly sessionId: string;
        readonly kind: 0 | 1 | 2;
        readonly leaseId: string;
        readonly ownerNonce: Uint8Array;
        readonly branchId: string | null;
        readonly baseRevision: number | null;
        readonly generation: number | null;
        readonly expectedGenerationDigest: Uint8Array | null;
        readonly now: number;
        readonly expiresAt: number;
        readonly ingestReservationBytes: number;
        readonly metadataReservationBytes: number;
        readonly resultRetentionMs?: number;
    }): void;
    applyImportRecords(options: {
        readonly sessionId: string;
        readonly records: readonly ReplicationTransferRecord[];
        readonly now: number;
    }): Readonly<{
        readonly stagedBytesDelta: number;
        readonly insertedObjects: number;
        readonly reusedObjects: number;
        readonly insertedNodes: number;
        readonly reusedNodes: number;
        readonly insertedRoots: number;
        readonly reusedRoots: number;
        readonly missingCount: number;
        readonly transferredCount: number;
    }>;
    readMissingContent(options: {
        readonly sessionId: string;
        readonly maxEntries: number;
        readonly maxBytes: number;
    }): Readonly<{
        readonly records: readonly ReplicationTransferRecord[];
        readonly complete: boolean;
    }>;
    finalizeImport(options: {
        readonly sessionId: string;
        readonly kind: 0 | 1 | 2;
        readonly expectedRevision: number;
        readonly expectedRootMutationGeneration: number;
        readonly expectedNextAllocationSequence: number;
        readonly expectedRootInode: string;
        readonly expectedRevisionCount: number;
        readonly expectedStateRows: number;
        readonly expectedClosureRoots: number;
        readonly expectedClosureNodes: number;
        readonly expectedClosureObjects: number;
        readonly expectedClosureObjectBytes: number;
        readonly branchId: string | null;
        readonly baseRevision: string | null;
        readonly generation: number | null;
        readonly generationDigest: Uint8Array | null;
        readonly checkpoint: boolean;
        readonly terminalState: 0 | 1 | 2;
        readonly terminalResultOperationId: string | null;
        readonly terminalResultBytes: Uint8Array | null;
        readonly genesisMeta: ReplicationExportMeta | null;
        readonly genesisRows: readonly {
            readonly inodeId: string;
            readonly tombstone: boolean;
            readonly encoded: Uint8Array | null;
        }[];
        readonly now: number;
    }): Readonly<{
        readonly revision: string;
        readonly branchId: string | null;
        readonly baseRevision: string | null;
        readonly generation: number;
        readonly generationDigest: Uint8Array | null;
        readonly state: 0 | 1 | 2;
        readonly authorityResult: ReplicationAuthorityResult | null;
        readonly reusedBytes: number;
    }>;
    renewLease(options: {
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
        readonly expiresAt: number;
    }): boolean;
    abortImport(options: {
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
    }): void;
    abortImportIfPresent(options: {
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly now: number;
    }): boolean;
    maintenance(options: {
        readonly now: number;
        readonly limit: number;
    }): Readonly<{
        readonly expiredLeases: number;
        readonly cleanupPasses: number;
    }>;
}
export interface StorageTransactionPorts {
    content(limits: StorageLimits, cache?: ContentCache): ContentStore;
    manifestTree(limits: StorageLimits, cache?: ContentCache): ManifestTreeStore;
    namespace(filesystem: FilesystemLimits, storage: StorageLimits, syscall: string): NamespaceStore;
    branches(limits: StorageLimits): BranchStore;
    staging(limits: StorageLimits, cache?: ContentCache): StagingStore;
    maintenance(limits: StorageLimits): MaintenanceStore;
    overlay(limits: StorageLimits, pageBytes: CowPageBytes): OverlayStore;
    replication(limits?: StorageLimits): ReplicationSessionStore;
    replicationTransfer(limits?: StorageLimits, cache?: ContentCache, branchDigest?: (branchId: string, generation: number) => string): ReplicationTransferStore;
}
export interface OperationsStorage {
    readonly readOnly: boolean;
    readonly capabilities: StorageAdapterCapabilities;
    /**
     * Synchronous SHA-256 hashing capability injected by the host adapter.
     * Hosts that can provide a synchronous native hasher (node:crypto on Node)
     * do so; every other host falls back to the byte-identical pure-JS
     * implementation in `cas/sha256.ts`, so digests never depend on the host.
     */
    readonly hashBytes: HashFunction; /**
     * Optional asynchronous SHA-256 hasher (WebCrypto on workerd) used by the
     * streaming write pipeline to hash chunk batches concurrently with bounded
     * parallelism. Digest output is byte-identical to `hashBytes`.
     */
    readonly hashBytesAsync?: (bytes: Uint8Array) => Promise<Uint8Array>;
    initialize(options?: {
        readonly cowPageBytes?: CowPageBytes;
        readonly now?: number;
        readonly maxManifestEntries?: number;
        readonly maxManifestDepth?: number;
        readonly maxFileBytes?: number;
        readonly maxContentObjectBytes?: number;
        readonly writerProfile?: string;
    }): StorageMetadata;
    transaction<T>(mode: StorageTransactionMode, budget: StorageWorkBudget, callback: (ports: StorageTransactionPorts) => T): T;
    physicalStorage(): StoragePhysicalFiles;
    checkpoint(mode?: "passive" | "restart" | "truncate"): StorageCheckpointResult | undefined;
    close(): void | Promise<void>;
}
export interface OperationsContext {
    readonly storage: OperationsStorage;
    readonly filesystem: FilesystemLimits;
    readonly durable: StorageLimits;
    readonly runtime: RuntimeLimits;
    readonly branches: BranchConfiguration;
}

/* ===== packages/fs/dist/operations/streaming-prepare.d.ts ===== */
import { type ManifestParameters } from "../manifests/codec.js";
import { AdmissionController, type RuntimeLimits, type StorageLimits } from "../resources/limits.js";
import { ContentCache } from "../cache/content-cache.js";
import type { ClosureCertificate, OperationsStorage } from "./storage-ports.js";
export interface StreamPreparedManifest {
    readonly hash: Uint8Array;
    readonly size: number;
    readonly certificate: ClosureCertificate;
}
export interface StagedManifestEntryInput {
    readonly hash: Uint8Array;
    readonly length: number;
    /** Present only for newly chunked content. Existing CAS entries omit it. */
    readonly bytes?: Uint8Array;
}
/**
 * Synchronous, bounded content source used by the Node VFS bridge. The source
 * owns neither the destination nor any durable state and must fill exactly the
 * requested range before returning.
 */
export interface SynchronousContentSource {
    readonly size: number;
    readInto(destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
}
export declare function ingestReservationBytes(declaredBytes: number, storage: StorageLimits, minimumChunkBytes?: number): number;
export declare function metadataReservationBytes(declaredBytes: number, storage: StorageLimits, minimumChunkBytes?: number): number;
/**
 * Prepare a complete manifest from a synchronous range source without ever
 * materializing the complete value. This is the synchronous counterpart to
 * prepareContentStreaming and deliberately shares its staging, reconciliation,
 * admission, and manifest-building implementation.
 */
export declare function prepareContentSourceSync(port: OperationsStorage, source: SynchronousContentSource, storage: StorageLimits, runtime: RuntimeLimits, admission: AdmissionController, cache?: ContentCache, clock?: () => number): StreamPreparedManifest;
export declare function prepareContentStreaming(port: OperationsStorage, input: Uint8Array | ReadableStream<Uint8Array>, storage: StorageLimits, runtime: RuntimeLimits, admission: AdmissionController, signal?: AbortSignal, cache?: ContentCache, clock?: () => number, declaredMaxBytes?: number): Promise<StreamPreparedManifest>;
/**
 * Persists an authenticated entry stream without materializing the file. Entries
 * without `bytes` reuse an existing CAS object; entries with `bytes` are verified
 * and inserted before their durable staging reference is recorded.
 */
export declare function prepareContentEntriesStreaming(port: OperationsStorage, entries: Iterable<StagedManifestEntryInput>, parameters: ManifestParameters, expectedSize: number, storage: StorageLimits, runtime: RuntimeLimits, admission: AdmissionController, cache?: ContentCache, clock?: () => number): Promise<StreamPreparedManifest>;

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

/* ===== packages/node-vfs/dist/index.d.ts ===== */
import { type EphemeralFS, type EphemeralFilesystem, type FileStat, type RuntimeLimits } from "@ephemeralai/fs";
import { type NodeVfsFilesystemBridge } from "@ephemeralai/fs/integrations/node-vfs";
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
    readonly filesystem: EphemeralFilesystem;
    /** Owning core runtime; differs from `filesystem` for a branch-scoped handle. */
    readonly runtime: EphemeralFS;
    readonly provider: NodeVfsProvider;
    close(): Promise<void>;
}
export { createNodeVfsSynchronousFileSystem, type NodeVfsSynchronousFileSystem, } from "./synchronous-adapter.js";
export interface NodeVfsMetricsSnapshot {
    readonly openSessions: number;
    readonly peakOpenSessions: number;
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
    readonly cowEditCount: number;
    readonly cowEditSourceBytes: number;
    readonly callbackSizeDistribution: Readonly<{
        upTo4KiB: number;
        upTo64KiB: number;
        upTo1MiB: number;
        over1MiB: number;
    }>;
    readonly contiguousRunBytes: number;
    readonly peakContiguousRunBytes: number;
    readonly flushReasonCounts: Readonly<{
        explicitCommit: number;
        flush: number;
        close: number;
        providerSync: number;
    }>;
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
/** Create a provider from a bridge owned by an already-open shared core runtime. */
export declare function createNodeVfsProvider(bridge: NodeVfsFilesystemBridge, observer?: NodeVfsObserver): NodeVfsProvider;
export declare function openNodeVfs(options: OpenNodeVfsOptions): Promise<NodeVfsHandle>;

/* ===== packages/node-vfs/dist/synchronous-adapter.d.ts ===== */
import type { FileStat } from "@ephemeralai/fs";
import type { NodeVfsProvider } from "./index.js";
/**
 * Structural synchronous filesystem surface for host adapters such as FUSE.
 *
 * This deliberately has no dependency on a host filesystem library.  The
 * durable namespace, branch view, COW admission, and session semantics stay
 * in NodeVfsProvider; a host only supplies the object shape it already
 * consumes.
 */
export interface NodeVfsSynchronousFileSystem {
    existsSync(path: string): boolean;
    statSync(path: string): FileStat;
    lstatSync(path: string): FileStat;
    readdirSync(path: string): string[];
    readlinkSync(path: string): string;
    accessSync(path: string): void;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    readFileSync(path: string): Uint8Array;
    writeFileSync(path: string, bytes: Uint8Array, options?: {
        mode?: number;
    }): void;
    createFileSync(path: string, options?: {
        mode?: number;
    }): void;
    writeRangeSync(path: string, bytes: Uint8Array, position: number): number;
    truncateFileSync(path: string, size: number): void;
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
}
/** Adapt a branch-scoped Node VFS provider to a host's sync filesystem shape. */
export declare function createNodeVfsSynchronousFileSystem(provider: NodeVfsProvider): NodeVfsSynchronousFileSystem;

/* ===== packages/sqlite-node/dist/index.d.ts ===== */
import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SQLiteDriverCapabilities, SQLiteCheckpointResult, SQLitePhysicalStorage, SqliteHashFunction, TransactionMode } from "@ephemeralai/fs/sqlite-driver";
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
    readonly hashBytes: SqliteHashFunction;
    readonly capabilities: SQLiteDriverCapabilities & {
        readonly journalQuotaPolicy: "checkpoint-backpressure";
        readonly journalSizeLimitIsHard: false;
    };
    constructor(options: OpenNodeSqliteOptions);
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void;
    physicalStorage(): SQLitePhysicalStorage;
    checkpoint(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
}
export declare function openNodeSqlite(options: OpenNodeSqliteOptions): Promise<NodeSQLiteDriver>;

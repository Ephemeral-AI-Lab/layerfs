/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./integrations/replication; entry: packages/fs/dist/integrations/replication.d.ts */

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

/* ===== packages/fs/dist/integrations/replication.d.ts ===== */
export type { CreateReplicationSessionRequest, ReplicationBatchAcceptanceRequest, ReplicationFilesystemBridge, ReplicationFlow, ReplicationPhase, ReplicationRole, ReplicationSessionBinding, ReplicationSessionSnapshot, ReplicationExportSelection, ReplicationExportBatch, ReplicationExportSummary, ReplicationGenesisCapture, ReplicationImportApply, ReplicationFinalization, ReplicationBridgeCapabilities, ReplicationBridgeFeatures, ReplicationBridgeLimits, ReplicationBridgeStorageCapabilities, ReplicationFastCdcConfiguration, } from "../filesystem/types.js";
export type { ReplicationAuthorityResult, ReplicationExportMeta, ReplicationTransferRecord, } from "../filesystem/types.js";
export { encodeActivationRequest, decodeActivationRequest, encodeActivationResult, decodeActivationResult, encodeGenesisFragment, encodeRevisionFragment, encodeCheckpointFragment, encodeBranchGenerationFragment, } from "../sqlite/transfer-codec.js";
export type { TransferActivationRequest, TransferActivationResult, TransferAuthorityResult, TransferGenesisFragment, TransferRevisionFragment, TransferCheckpointFragment, TransferBranchGenerationFragment, } from "../sqlite/transfer-codec.js";

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

/* ===== packages/fs/dist/sqlite/transfer-codec.d.ts ===== */
/**
 * Frozen semantic fragment grammars for `efs-replication-v1` state-transfer
 * phases. These grammars are normative for this implementation and MUST NOT
 * change without new golden vectors and a protocol version bump.
 *
 * All integers are unsigned big-endian. `text` is uint32 byte length
 * followed by exactly that many well-formed UTF-8 bytes. `bytes` is uint32
 * byte length followed by exactly that many bytes. `optional` is 0x00, or
 * 0x01 followed by the encoded value. `digest32` is 32 raw bytes.
 */
export interface TransferInodeRow {
    readonly inodeId: string;
    readonly tombstone: boolean;
    readonly encoded: Uint8Array | null;
}
export interface TransferEntryRow {
    readonly parentInode: string;
    readonly nameSort: Uint8Array;
    readonly tombstone: boolean;
    readonly encoded: Uint8Array | null;
}
export interface TransferManifestRefRow {
    readonly inodeId: string;
    readonly manifestHash: Uint8Array;
}
export type TransferNamespaceRow = ({
    readonly kind: 1;
} & TransferInodeRow) | ({
    readonly kind: 2;
} & TransferEntryRow) | ({
    readonly kind: 3;
} & TransferManifestRefRow);
export interface TransferRevisionFragment {
    readonly revisionId: string;
    readonly parentRevisionId: string | null;
    readonly created_at_ms: number;
    readonly writerId: string;
    readonly changeCount: number;
    readonly rows: readonly TransferNamespaceRow[];
}
export interface TransferCheckpointFragment {
    readonly revisionId: string;
    readonly rows: readonly TransferNamespaceRow[];
}
export interface TransferBranchChangeRow {
    readonly path: Uint8Array;
    /** 0 for a present entry, 1 for a tombstone. */
    readonly disposition: number;
    readonly expectedToken: number | null;
    readonly encoded: Uint8Array | null;
}
export interface TransferBranchOverlayRow {
    readonly inodeId: string;
    readonly expectedToken: number | null;
    readonly encoded: Uint8Array;
}
export interface TransferBranchPageRow {
    readonly inodeId: string;
    readonly pageIndex: number;
    readonly generation: number;
    readonly bytes: Uint8Array;
    readonly created_at_ms: number;
    readonly head: boolean;
}
export interface TransferBranchPatchRow {
    readonly inodeId: string;
    readonly sequence: number;
    readonly generation: number;
    readonly offset: number;
    readonly deleteLength: number;
    readonly insertLength: number;
    readonly segments: readonly Uint8Array[];
}
export interface TransferBranchExpectationRow {
    readonly inodeId: string;
    readonly expectedToken: number | null;
}
export interface TransferBranchManifestRefRow {
    readonly path: Uint8Array;
    readonly manifestHash: Uint8Array;
}
export type TransferBranchRow = ({
    readonly kind: 1;
} & TransferBranchChangeRow) | ({
    readonly kind: 2;
} & TransferBranchOverlayRow) | ({
    readonly kind: 3;
} & TransferBranchPageRow) | ({
    readonly kind: 4;
} & TransferBranchPatchRow) | ({
    readonly kind: 5;
} & TransferBranchExpectationRow) | ({
    readonly kind: 6;
} & TransferBranchManifestRefRow);
export interface TransferBranchGenerationFragment {
    readonly branchId: string;
    readonly baseRevision: string;
    readonly generation: number;
    readonly generationDigest: Uint8Array;
    /**
     * The exact digest held by the destination before this generation.  A
     * destination may advance a lower generation only when both values match.
     */
    readonly previousGeneration: number | null;
    readonly previousGenerationDigest: Uint8Array | null;
    readonly state: number;
    readonly rows: readonly TransferBranchRow[];
}
export interface TransferGenesisRow {
    readonly inodeId: string;
    readonly tombstone: boolean;
    readonly encoded: Uint8Array | null;
}
export interface TransferGenesisFragment {
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
    readonly rows: readonly TransferGenesisRow[];
}
export interface TransferActivationResult {
    readonly kind: 0 | 1;
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: TransferAuthorityResult | null;
}
export type TransferAuthorityResult = {
    readonly kind: "publication";
    readonly operationId: string;
    readonly outcome: "merged" | "conflict";
    readonly resultDigest: Uint8Array;
} | {
    readonly kind: "discard";
    readonly operationId: string | null;
    readonly resultDigest: Uint8Array;
};
export declare function encodeRevisionFragment(fragment: TransferRevisionFragment): Uint8Array;
export declare function encodeCheckpointFragment(fragment: TransferCheckpointFragment): Uint8Array;
export declare function encodeBranchGenerationFragment(fragment: TransferBranchGenerationFragment): Uint8Array;
export declare function encodeGenesisFragment(fragment: TransferGenesisFragment): Uint8Array;
export declare function encodeActivationResult(result: TransferActivationResult): Uint8Array;
export declare function decodeActivationResult(value: Uint8Array): TransferActivationResult;
export interface TransferActivationRequest {
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
    readonly checkpoint: boolean;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly terminalState: 0 | 1 | 2;
    readonly terminalResultOperationId: string | null;
    readonly terminalResultBytes: Uint8Array | null;
    readonly genesis: TransferGenesisFragment | null;
}
export declare function encodeActivationRequest(request: TransferActivationRequest): Uint8Array;
export declare function decodeActivationRequest(value: Uint8Array): TransferActivationRequest;
export declare const TRANSFER_FRAGMENT_VERSIONS: Readonly<{
    readonly revision: 1;
    readonly checkpoint: 1;
    readonly branchGeneration: 1;
    readonly genesis: 1;
    readonly activationResult: 1;
    readonly activationRequest: 1;
}>;

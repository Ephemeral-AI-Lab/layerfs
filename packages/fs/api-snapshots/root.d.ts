/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: .; entry: packages/fs/dist/index.d.ts */

/* export: BranchCapableFilesystem; kinds: type */
/* source: packages/fs/dist/branches/types.d.ts */
export interface BranchCapableFilesystem extends EphemeralFilesystem, EphemeralFilesystemAdministration {
    readonly branches: Branches;
}

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
export type BranchErrorCode = "InvalidBranchId" | "InvalidOperationId" | "InvalidPublicationExpectation" | "BranchNotFound" | "BranchNotActive" | "RevisionNotFound" | "BranchChanged" | "OperationBranchMismatch" | "OperationRequestMismatch" | "OperationNotFound" | "OperationResultExpired" | "LimitExceeded";

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
    /** Canonical digest of the complete semantic branch generation. */
    readonly generationDigest: string;
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
    readonly branchGeneration: number;
    readonly branchGenerationDigest: string;
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

/* export: CreateReplicationSessionRequest; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface CreateReplicationSessionRequest {
    readonly binding: ReplicationSessionBinding;
    readonly phase: ReplicationPhase;
    readonly cursor: Uint8Array;
    readonly cursorDigest: Uint8Array;
    readonly now: number;
    readonly expiresAtMs: number;
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

/* export: EphemeralFilesystemAdministration; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface EphemeralFilesystemAdministration {
    readonly capabilities: FilesystemCapabilities;
    readonly maintenance: FilesystemMaintenance;
}

/* export: EphemeralFS; kinds: value,type */
/* source: packages/fs/dist/filesystem/ephemeral-fs.d.ts */
/** Public composition root: injects the private SQLite storage-port adapter. */
export declare class EphemeralFS {
    private constructor();
    static open(options: OpenFilesystemOptions): Promise<EphemeralFS>;
}
/* source: packages/fs/dist/index.d.ts */
interface EphemeralFS extends BranchCapableFilesystem {
}

/* export: EphemeralRuntime; kinds: value,type */
/* source: packages/fs/dist/filesystem/ephemeral-runtime.d.ts */
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
    readonly branchGeneration: number;
    readonly branchGenerationDigest: string;
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

/* export: OpenEphemeralRuntimeOptions; kinds: type */
/* source: packages/fs/dist/filesystem/ephemeral-runtime.d.ts */
export interface OpenEphemeralRuntimeOptions extends OpenFilesystemOptions {
    readonly provisioningState?: "bound" | "unbound-replica";
    readonly replicationIdentity?: {
        readonly authorityId: string;
        readonly role: ReplicationRole;
    };
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
    readonly expectedGeneration?: number;
    readonly expectedGenerationDigest?: string;
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

/* export: ReplicationAuthorityResult; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationBatchAcceptanceRequest; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationBridgeCapabilities; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationBridgeFeatures; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationBridgeLimits; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationBridgeStorageCapabilities; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationExportBatch; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReplicationExportBatch {
    readonly records: readonly ReplicationTransferRecord[];
    readonly complete: boolean;
    readonly offered: number;
    readonly reused: number;
}

/* export: ReplicationExportMeta; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationExportSelection; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReplicationExportSelection {
    readonly selectedRevision: number;
    readonly selectedGeneration: number | null;
    readonly destinationHead: number;
    readonly rootMutationGeneration: number;
    readonly nextAllocationSequence: number;
    readonly rootInode: string;
}

/* export: ReplicationExportSummary; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationFastCdcConfiguration; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReplicationFastCdcConfiguration {
    readonly minimum: number;
    readonly average: number;
    readonly maximum: number;
}

/* export: ReplicationFilesystemBridge; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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
        readonly requestDigest?: Uint8Array;
        readonly responseBytes?: Uint8Array;
    }): Promise<ReplicationSessionSnapshot>;
    replayOutboundBatch(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly sequence: number;
        readonly requestDigest: Uint8Array;
    }): Promise<Uint8Array>;
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
        readonly requestDigest?: Uint8Array;
        readonly responseBytes?: Uint8Array;
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
    releaseExport(request: {
        readonly sessionId: string;
        readonly now: number;
    }): Promise<void>;
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
        /** Authenticated source role; only the main authority may deliver terminal state. */
        readonly sourceRole?: "main-authority" | "replica";
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

/* export: ReplicationFilesystemIdentity; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReplicationFilesystemIdentity {
    readonly filesystemId: string;
    readonly authorityId: string;
    readonly role: ReplicationRole;
}

/* export: ReplicationFinalization; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReplicationFinalization {
    /** False means the core committed one bounded activation page and the
     * destination endpoint must call finalizeImport again with the same
     * authenticated request. */
    readonly complete?: boolean;
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: ReplicationAuthorityResult | null;
    readonly reusedBytes: number;
}

/* export: ReplicationFlow; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type ReplicationFlow = "authority-main-to-replica" | "authority-branch-to-replica" | "replica-branch-to-authority" | "replica-branch-to-replica";

/* export: ReplicationGenesisCapture; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export interface ReplicationGenesisCapture {
    readonly meta: ReplicationExportMeta;
    readonly rows: readonly {
        readonly inodeId: string;
        readonly tombstone: boolean;
        readonly encoded: Uint8Array | null;
    }[];
}

/* export: ReplicationImportApply; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationPhase; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type ReplicationPhase = "handshake" | "plan-selection" | "content-offer" | "missing-content" | "content-transfer" | "state-transfer" | "activation" | "result-acknowledgement" | "cleanup";

/* export: ReplicationRole; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
export type ReplicationRole = "main-authority" | "replica";

/* export: ReplicationSessionBinding; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationSessionSnapshot; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

/* export: ReplicationSessionStore; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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
        readonly requestDigest?: Uint8Array;
        readonly responseBytes?: Uint8Array;
    }): ReplicationSessionSnapshot;
    replayOutboundBatch(request: {
        readonly operationId: string;
        readonly sessionId: string;
        readonly ownerNonce: Uint8Array;
        readonly sequence: number;
        readonly requestDigest: Uint8Array;
    }): Uint8Array;
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

/* export: ReplicationTransferRecord; kinds: type */
/* source: packages/fs/dist/filesystem/types.d.ts */
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

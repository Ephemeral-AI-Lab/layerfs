/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./integrations/replication; entry: packages/fs/dist/integrations/replication.d.ts */

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

/* export: decodeActivationRequest; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function decodeActivationRequest(value: Uint8Array): TransferActivationRequest;

/* export: decodeActivationResult; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function decodeActivationResult(value: Uint8Array): TransferActivationResult;

/* export: encodeActivationRequest; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function encodeActivationRequest(request: TransferActivationRequest): Uint8Array;

/* export: encodeActivationResult; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function encodeActivationResult(result: TransferActivationResult): Uint8Array;

/* export: encodeBranchGenerationFragment; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function encodeBranchGenerationFragment(fragment: TransferBranchGenerationFragment): Uint8Array;

/* export: encodeCheckpointFragment; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function encodeCheckpointFragment(fragment: TransferCheckpointFragment): Uint8Array;

/* export: encodeGenesisFragment; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function encodeGenesisFragment(fragment: TransferGenesisFragment): Uint8Array;

/* export: encodeRevisionFragment; kinds: value */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export declare function encodeRevisionFragment(fragment: TransferRevisionFragment): Uint8Array;

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

/* export: TransferActivationRequest; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
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

/* export: TransferActivationResult; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
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

/* export: TransferAuthorityResult; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
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

/* export: TransferBranchGenerationFragment; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
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

/* export: TransferCheckpointFragment; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export interface TransferCheckpointFragment {
    readonly revisionId: string;
    readonly rows: readonly TransferNamespaceRow[];
}

/* export: TransferGenesisFragment; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
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

/* export: TransferRevisionFragment; kinds: type */
/* source: packages/fs/dist/sqlite/transfer-codec.d.ts */
export interface TransferRevisionFragment {
    readonly revisionId: string;
    readonly parentRevisionId: string | null;
    readonly created_at_ms: number;
    readonly writerId: string;
    readonly changeCount: number;
    readonly rows: readonly TransferNamespaceRow[];
}

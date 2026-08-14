export const REPLICATION_PROTOCOL_VERSION = "efs-replication-v1" as const;
export const REPLICATION_APPLICATION_ID = 0x4541_4653;
export const REPLICATION_FILESYSTEM_SCHEMA_VERSION = 13;
export const REPLICATION_STORAGE_USER_VERSION = 13;
export const REPLICATION_MANIFEST_FORMAT = "efs-merkle-manifest-v1" as const;
export const REPLICATION_CHUNKER_FORMAT = "fastcdc-v1" as const;
export const REPLICATION_HOST_PROFILE = "computer-efs-carrier-v1" as const;

export type ReplicationRole = "main-authority" | "replica";

export type ReplicationPlan =
  | { readonly flow: "authority-main-to-replica" }
  | {
      readonly flow: "authority-branch-to-replica";
      readonly branchId: string;
    }
  | {
      readonly flow: "replica-branch-to-authority";
      readonly branchId: string;
    }
  | {
      readonly flow: "replica-branch-to-replica";
      readonly branchId: string;
    };

export interface FastCdcConfiguration {
  readonly minimum: number;
  readonly average: number;
  readonly maximum: number;
}

export interface ReplicationFeatures {
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

export interface ReplicationLimits {
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

export type ReplicationCeilingLimits = Omit<ReplicationLimits, "minRetryDelayMs">;

export interface ReplicationLimitPolicy {
  readonly ceilings: ReplicationCeilingLimits;
  readonly minRetryDelayMsFloor: number;
}

export interface ReplicationStorageCapabilities {
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

export interface ReplicationCapabilities {
  readonly protocolVersions: readonly string[];
  readonly hostProfile: typeof REPLICATION_HOST_PROFILE;
  readonly provisioningState: "bound" | "unbound-replica";
  readonly filesystemId: string | null;
  readonly authorityId: string | null;
  readonly applicationId: number | null;
  readonly filesystemSchemaVersion: number | null;
  readonly storageUserVersion: number;
  readonly storageMigrationState: "none";
  readonly readableFilesystemSchemaVersions: readonly number[];
  readonly writableFilesystemSchemaVersion: number;
  readonly role: ReplicationRole;
  readonly hashAlgorithms: readonly ["sha256"];
  readonly activeManifestFormat: string | null;
  readonly supportedManifestFormats: readonly string[];
  readonly activeChunkerFormat: string | null;
  readonly supportedChunkerFormats: readonly string[];
  readonly fastCdc: FastCdcConfiguration | null;
  readonly supportedFastCdcConfigurations: readonly FastCdcConfiguration[];
  readonly copyOnWritePageBytes: 4096 | 8192 | 16384 | null;
  readonly supportedCopyOnWritePageBytes: readonly (4096 | 8192 | 16384)[];
  readonly features: ReplicationFeatures;
  readonly limits: ReplicationLimits;
  readonly storage: ReplicationStorageCapabilities;
}

export interface AuthorizedReplicationPeer {
  readonly principalId: string;
  readonly hostScopeId: string;
  readonly expectedFilesystemId: string;
  readonly expectedAuthorityId: string;
  readonly policyVersion: string;
  readonly hostProfile: typeof REPLICATION_HOST_PROFILE;
  readonly limitPolicy: ReplicationLimitPolicy;
  readonly allowedPlans: readonly ReplicationPlan[];
}

export interface CanonicalAuthorizationRecord {
  readonly authorization: AuthorizedReplicationPeer;
  readonly effectiveLimits: ReplicationLimits;
}

export type ReplicationPhase =
  | "handshake"
  | "plan-selection"
  | "content-offer"
  | "missing-content"
  | "content-transfer"
  | "state-transfer"
  | "activation"
  | "result-acknowledgement"
  | "cleanup";

export interface ReplicationCursorBinding {
  readonly sessionId: string;
  readonly ownerNonceDigest: Uint8Array;
  readonly sourceFilesystemId: string;
  readonly destinationFilesystemId: string;
  readonly plan: ReplicationPlan;
  readonly selectedIdentity: string;
  readonly selectedGeneration: number | null;
  readonly phase: ReplicationPhase;
  readonly nextSequence: number;
  readonly capabilityDigest: Uint8Array;
}

export interface ReplicationBatchAcknowledgement {
  readonly sessionId: string;
  readonly sequence: number;
  readonly phase: ReplicationPhase;
  readonly batchEnvelopeDigest: Uint8Array;
  readonly nextPhase: ReplicationPhase;
  readonly cursor: Uint8Array;
  readonly cursorDigest: Uint8Array;
  readonly chainDigest: Uint8Array;
  readonly acceptedEntries: number;
  readonly acceptedBytes: number;
  readonly stagedBytes: number;
}

export interface ReplicationRevisionFragment {
  readonly revisionId: string;
  readonly parentRevisionId: string | null;
  readonly fragmentIndex: number;
  readonly fragmentCount: number;
  readonly fragmentBytes: Uint8Array;
}

export interface ReplicationCheckpointFragment {
  readonly checkpointId: string;
  readonly revisionId: string;
  readonly fragmentIndex: number;
  readonly fragmentCount: number;
  readonly fragmentBytes: Uint8Array;
}

export interface ReplicationBranchGenerationFragment {
  readonly branchId: string;
  readonly baseRevision: string;
  readonly generation: number;
  readonly generationDigest: Uint8Array;
  readonly fragmentIndex: number;
  readonly fragmentCount: number;
  readonly fragmentBytes: Uint8Array;
}

export interface ReplicationTerminalResultRecord {
  readonly operationId: string;
  readonly branchId: string | null;
  readonly generation: number | null;
  readonly generationDigest: Uint8Array | null;
  readonly resultDigest: Uint8Array;
  readonly resultBytes: Uint8Array;
}

export type ReplicationBatchRecord =
  | {
      readonly kind: "object-descriptor";
      readonly digest: Uint8Array;
      readonly byteLength: number;
    }
  | {
      readonly kind: "object-payload";
      readonly digest: Uint8Array;
      readonly byteLength: number;
      readonly bytes: Uint8Array;
    }
  | {
      readonly kind: "manifest-root-descriptor";
      readonly format: string;
      readonly digest: Uint8Array;
      readonly encodedLength: number;
      readonly logicalFileLength: number;
      readonly entryCount: number;
      readonly rootNodeDigest: Uint8Array;
    }
  | {
      readonly kind: "manifest-node-descriptor";
      readonly digest: Uint8Array;
      readonly nodeKind: "leaf" | "internal";
      readonly encodedLength: number;
      readonly logicalSpan: number;
      readonly entryCount: number;
    }
  | {
      readonly kind: "missing-content";
      readonly contentKind: "object" | "manifest-root" | "manifest-node";
      readonly digest: Uint8Array;
    }
  | ({ readonly kind: "revision-fragment" } & ReplicationRevisionFragment)
  | ({ readonly kind: "checkpoint-fragment" } & ReplicationCheckpointFragment)
  | ({
      readonly kind: "branch-generation-fragment";
    } & ReplicationBranchGenerationFragment)
  | ({ readonly kind: "terminal-result" } & ReplicationTerminalResultRecord);

export interface ReplicationBatch {
  readonly sessionId: string;
  readonly plan: ReplicationPlan;
  readonly phase: ReplicationPhase;
  readonly sequence: number;
  readonly priorCursorDigest: Uint8Array;
  readonly entryCount: number;
  readonly payloadByteCount: number;
  readonly payloadDigest: Uint8Array;
  readonly records: readonly ReplicationBatchRecord[];
}

export interface ReplicationSemanticErrorRecord {
  readonly code: import("./errors.js").ReplicationErrorCode;
  readonly phase: ReplicationPhase | null;
  readonly sessionId: string | null;
  readonly message: string;
  readonly retryable: boolean;
}

export type CanonicalReplicationEnvelope =
  | { readonly kind: "capabilities"; readonly value: ReplicationCapabilities }
  | {
      readonly kind: "authorization";
      readonly value: CanonicalAuthorizationRecord;
    }
  | { readonly kind: "batch"; readonly value: ReplicationBatch }
  | { readonly kind: "cursor"; readonly value: ReplicationCursorBinding }
  | {
      readonly kind: "revision-fragment";
      readonly value: ReplicationRevisionFragment;
    }
  | {
      readonly kind: "checkpoint-fragment";
      readonly value: ReplicationCheckpointFragment;
    }
  | {
      readonly kind: "branch-generation-fragment";
      readonly value: ReplicationBranchGenerationFragment;
    }
  | {
      readonly kind: "terminal-result";
      readonly value: ReplicationTerminalResultRecord;
    }
  | {
      readonly kind: "batch-acknowledgement";
      readonly value: ReplicationBatchAcknowledgement;
    }
  | { readonly kind: "error"; readonly value: ReplicationSemanticErrorRecord };

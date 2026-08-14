import type {
  ReplicationBridgeCapabilities,
  ReplicationBridgeLimits,
  ReplicationBridgeStorageCapabilities,
  ReplicationFilesystemIdentity,
} from "../filesystem/types.js";
import type { StorageLimits } from "../resources/limits.js";

const MIB = 1024 * 1024;
const DAY_MS = 24 * 60 * 60 * 1000;

const DEFAULT_FASTCDC = Object.freeze({
  minimum: 32_768,
  average: 131_072,
  maximum: 524_288,
});

function bridgeLimits(storage: StorageLimits): ReplicationBridgeLimits {
  const stagingPerSession = Math.min(
    128 * MIB,
    storage.maxStagingPayloadBytes,
    storage.maxManagedPayloadBytes,
  );
  const metadataBytes = Math.min(64 * MIB, storage.maxChargedMetadataBytes);
  const receiptsBytes = Math.min(16 * MIB, metadataBytes);
  return Object.freeze({
    maxBatchEntries: 256,
    maxBatchBytes: 3 * MIB - 64 * 1024,
    maxRequestBytes: 3 * MIB,
    maxResponseBytes: 3 * MIB,
    maxBufferedBytes: 10 * MIB,
    maxInFlightBatches: 1,
    maxConcurrentSessions: 16,
    maxStagingBytesPerSession: stagingPerSession,
    maxReplicationSessionRows: 10_000,
    maxReplicationMetadataBytes: metadataBytes,
    maxReceiptsPerSession: 100_000,
    maxReceiptBytesPerSession: receiptsBytes,
    maxCursorBytes: 256,
    maxTerminalResultBytes: 1 * MIB,
    maxCursorAgeMs: DAY_MS,
    stagingLeaseMs: storage.stagingLeaseMs,
    resultRetentionMs: 30 * DAY_MS,
    maxRetryAttempts: 8,
    maxRetryElapsedMs: 5 * 60 * 1000,
    minRetryDelayMs: 100,
    maxRetryDelayMs: 10_000,
  });
}

function bridgeStorage(storage: StorageLimits): ReplicationBridgeStorageCapabilities {
  return Object.freeze({
    maxBlobBytes: storage.maxWriteBytes,
    maxManifestNodeBytes: storage.maxManifestNodeBytes,
    maxManifestDepth: storage.maxManifestDepth,
    maxManagedPayloadBytes: storage.maxManagedPayloadBytes,
    maxStagingPayloadBytes: storage.maxStagingPayloadBytes,
    maxMaintenanceBytes: storage.maxMaintenanceBytes,
    maintenanceReserveBytes: storage.maintenanceReserveBytes,
    maxPermanentIdentifiers: storage.maxPermanentIdentifiers,
    maxFinalTransactionRows: storage.maxFinalTransactionRows,
    maxFinalTransactionBytes: storage.maxFinalTransactionBytes,
  });
}

export function buildBoundReplicationCapabilities(options: {
  readonly identity: ReplicationFilesystemIdentity;
  readonly storage: StorageLimits;
  readonly cowPageBytes: 4096 | 8192 | 16384;
  readonly maxManifestEntries: number;
  readonly maxManifestDepth: number;
  readonly maxFileBytes: number;
  readonly writerProfile: string;
  readonly fastCdc?: {
    readonly minimum: number;
    readonly average: number;
    readonly maximum: number;
  };
}): ReplicationBridgeCapabilities {
  const fastCdc = options.fastCdc ?? DEFAULT_FASTCDC;
  const features = {
    authorityMainToReplica: true,
    authorityBranchToReplica: true,
    replicaBranchToAuthority: true,
    replicaBranchToReplica: true,
    checkpointBootstrap: true,
    segmentedMerkleManifestTransfer: true,
    durableStagingLeases: true,
    physicalRestartRecovery: true,
    terminalResultReplication: true,
    freshReplicaProvisioning: true,
  };
  return Object.freeze({
    provisioningState: "bound",
    filesystemId: options.identity.filesystemId,
    authorityId: options.identity.authorityId,
    applicationId: 0x4541_4653,
    filesystemSchemaVersion: 13,
    storageUserVersion: 13,
    storageMigrationState: "none",
    readableFilesystemSchemaVersions: Object.freeze([13]),
    writableFilesystemSchemaVersion: 13,
    role: options.identity.role,
    activeManifestFormat: "efs-merkle-manifest-v1",
    supportedManifestFormats: Object.freeze(["efs-merkle-manifest-v1"]),
    activeChunkerFormat: "fastcdc-v1",
    supportedChunkerFormats: Object.freeze(["fastcdc-v1"]),
    fastCdc: Object.freeze({ ...fastCdc }),
    supportedFastCdcConfigurations: Object.freeze([Object.freeze({ ...fastCdc })]),
    copyOnWritePageBytes: options.cowPageBytes,
    supportedCopyOnWritePageBytes: Object.freeze([4096, 8192, 16384] as const),
    features,
    limits: bridgeLimits(options.storage),
    storage: bridgeStorage(options.storage),
  });
}

export function buildUnboundReplicationCapabilities(
  storage: StorageLimits,
): ReplicationBridgeCapabilities {
  const features = {
    authorityMainToReplica: true,
    authorityBranchToReplica: true,
    replicaBranchToAuthority: true,
    replicaBranchToReplica: true,
    checkpointBootstrap: true,
    segmentedMerkleManifestTransfer: true,
    durableStagingLeases: true,
    physicalRestartRecovery: true,
    terminalResultReplication: true,
    freshReplicaProvisioning: true,
  };
  return Object.freeze({
    provisioningState: "unbound-replica",
    filesystemId: null,
    authorityId: null,
    applicationId: 0x4541_4653,
    filesystemSchemaVersion: null,
    storageUserVersion: 13,
    storageMigrationState: "none",
    readableFilesystemSchemaVersions: Object.freeze([13]),
    writableFilesystemSchemaVersion: 13,
    role: "replica",
    activeManifestFormat: null,
    supportedManifestFormats: Object.freeze(["efs-merkle-manifest-v1"]),
    activeChunkerFormat: null,
    supportedChunkerFormats: Object.freeze(["fastcdc-v1"]),
    fastCdc: null,
    supportedFastCdcConfigurations: Object.freeze([DEFAULT_FASTCDC]),
    copyOnWritePageBytes: null,
    supportedCopyOnWritePageBytes: Object.freeze([4096, 8192, 16384] as const),
    features,
    limits: bridgeLimits(storage),
    storage: bridgeStorage(storage),
  });
}

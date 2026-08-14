import {
  COMPUTER_EFS_CARRIER_V1_LIMITS,
  REPLICATION_APPLICATION_ID,
  REPLICATION_CHUNKER_FORMAT,
  REPLICATION_FILESYSTEM_SCHEMA_VERSION,
  REPLICATION_HOST_PROFILE,
  REPLICATION_MANIFEST_FORMAT,
  REPLICATION_PROTOCOL_VERSION,
  REPLICATION_STORAGE_USER_VERSION,
  batchEnvelopeDigest,
  capabilityDigest,
  createCanonicalBatch,
  createCanonicalBatchAcknowledgement,
  limitPolicyFromLimits,
  receiptChainDigest,
  replicationSha256,
  replicationOwnerNonceDigest,
} from "../../packages/replication/dist/index.js";

export const limits = COMPUTER_EFS_CARRIER_V1_LIMITS;

export const storage = Object.freeze({
  maxBlobBytes: 16 * 1024 * 1024,
  maxManifestNodeBytes: 16 * 1024,
  maxManifestDepth: 8,
  maxManagedPayloadBytes: 8 * 1024 * 1024 * 1024,
  maxStagingPayloadBytes: 512 * 1024 * 1024,
  maxMaintenanceBytes: 64 * 1024 * 1024,
  maintenanceReserveBytes: 64 * 1024 * 1024,
  maxPermanentIdentifiers: 10_000_000,
  maxFinalTransactionRows: 100_000,
  maxFinalTransactionBytes: 16_793_600,
});

export const features = Object.freeze({
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
});

export function capabilities(role = "main-authority") {
  return {
    protocolVersions: [REPLICATION_PROTOCOL_VERSION],
    hostProfile: REPLICATION_HOST_PROFILE,
    provisioningState: "bound",
    filesystemId: "fs-α",
    authorityId: "authority-01",
    applicationId: REPLICATION_APPLICATION_ID,
    filesystemSchemaVersion: REPLICATION_FILESYSTEM_SCHEMA_VERSION,
    storageUserVersion: REPLICATION_STORAGE_USER_VERSION,
    storageMigrationState: "none",
    readableFilesystemSchemaVersions: [REPLICATION_FILESYSTEM_SCHEMA_VERSION],
    writableFilesystemSchemaVersion: REPLICATION_FILESYSTEM_SCHEMA_VERSION,
    role,
    hashAlgorithms: ["sha256"],
    activeManifestFormat: REPLICATION_MANIFEST_FORMAT,
    supportedManifestFormats: [REPLICATION_MANIFEST_FORMAT],
    activeChunkerFormat: REPLICATION_CHUNKER_FORMAT,
    supportedChunkerFormats: [REPLICATION_CHUNKER_FORMAT],
    fastCdc: { minimum: 32_768, average: 131_072, maximum: 524_288 },
    supportedFastCdcConfigurations: [
      { minimum: 32_768, average: 131_072, maximum: 524_288 },
    ],
    copyOnWritePageBytes: 8192,
    supportedCopyOnWritePageBytes: [4096, 8192, 16_384],
    features,
    limits,
    storage,
  };
}

export function unboundReplicaCapabilities() {
  return {
    ...capabilities("replica"),
    provisioningState: "unbound-replica",
    filesystemId: null,
    authorityId: null,
    filesystemSchemaVersion: null,
    activeManifestFormat: null,
    activeChunkerFormat: null,
    fastCdc: null,
    copyOnWritePageBytes: null,
  };
}

export function authorization(allowedPlans) {
  return {
    principalId: "principal-01",
    hostScopeId: "workspace-01",
    expectedFilesystemId: "fs-α",
    expectedAuthorityId: "authority-01",
    policyVersion: "policy-7",
    hostProfile: REPLICATION_HOST_PROFILE,
    limitPolicy: limitPolicyFromLimits(limits),
    allowedPlans,
  };
}

export const mainPlan = Object.freeze({ flow: "authority-main-to-replica" });
export const branchPlan = Object.freeze({
  flow: "authority-branch-to-replica",
  branchId: "branch-é",
});

export const revisionFragment = Object.freeze({
  revisionId: "revision-2",
  parentRevisionId: "revision-1",
  fragmentIndex: 1,
  fragmentCount: 3,
  fragmentBytes: Uint8Array.of(0x10, 0x20, 0x30),
});

export const checkpointFragment = Object.freeze({
  checkpointId: "checkpoint-9",
  revisionId: "revision-9",
  fragmentIndex: 0,
  fragmentCount: 1,
  fragmentBytes: Uint8Array.of(0xaa, 0xbb),
});

export const branchGenerationFragment = Object.freeze({
  branchId: "branch-é",
  baseRevision: "revision-1",
  generation: 17,
  generationDigest: new Uint8Array(32).fill(0x33),
  fragmentIndex: 0,
  fragmentCount: 2,
  fragmentBytes: Uint8Array.of(0x44, 0x55, 0x66),
});

const resultBytes = new TextEncoder().encode("merged:revision-10");
export const terminalResult = Object.freeze({
  operationId: "operation-77",
  branchId: "branch-é",
  generation: 17,
  generationDigest: new Uint8Array(32).fill(0x33),
  resultDigest: replicationSha256(resultBytes),
  resultBytes,
});

const objectBytes = Uint8Array.of(0, 1, 2, 255);
const objectDigest = replicationSha256(objectBytes);
export const records = Object.freeze([
  { kind: "object-descriptor", digest: objectDigest, byteLength: objectBytes.length },
  {
    kind: "object-payload",
    digest: objectDigest,
    byteLength: objectBytes.length,
    bytes: objectBytes,
  },
  {
    kind: "manifest-root-descriptor",
    format: REPLICATION_MANIFEST_FORMAT,
    digest: new Uint8Array(32).fill(0x11),
    encodedLength: 68,
    logicalFileLength: 4,
    entryCount: 1,
    rootNodeDigest: new Uint8Array(32).fill(0x22),
  },
  {
    kind: "manifest-node-descriptor",
    digest: new Uint8Array(32).fill(0x22),
    nodeKind: "leaf",
    encodedLength: 68,
    logicalSpan: 4,
    entryCount: 1,
  },
  {
    kind: "missing-content",
    contentKind: "object",
    digest: objectDigest,
  },
  { kind: "revision-fragment", ...revisionFragment },
  { kind: "checkpoint-fragment", ...checkpointFragment },
  { kind: "branch-generation-fragment", ...branchGenerationFragment },
  { kind: "terminal-result", ...terminalResult },
]);

export const cursor = Object.freeze({
  sessionId: "00112233445566778899aabbccddeeff",
  ownerNonceDigest: replicationOwnerNonceDigest(new Uint8Array(16).fill(0x44)),
  sourceFilesystemId: "fs-α",
  destinationFilesystemId: "fs-α",
  plan: branchPlan,
  selectedIdentity: "branch-é",
  selectedGeneration: 17,
  phase: "state-transfer",
  nextSequence: 9,
  capabilityDigest: capabilityDigest(capabilities(), limits),
});

export const batch = createCanonicalBatch({
  sessionId: "00112233445566778899aabbccddeeff",
  plan: branchPlan,
  phase: "state-transfer",
  sequence: 8,
  priorCursorDigest: new Uint8Array(32).fill(0x55),
  records,
});

export const batchAcknowledgement = createCanonicalBatchAcknowledgement({
  batch,
  nextPhase: "activation",
  cursor: new Uint8Array(32).fill(0x66),
  chainDigest: receiptChainDigest(
    new Uint8Array(32),
    batch.sequence,
    batchEnvelopeDigest(batch),
  ),
  acceptedEntries: records.length,
  acceptedBytes: batch.payloadByteCount,
  stagedBytes: 4096,
});

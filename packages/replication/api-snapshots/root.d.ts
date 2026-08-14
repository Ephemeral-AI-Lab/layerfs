/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-replication; subpath: .; entry: packages/replication/dist/index.d.ts */

/* export: ACK_MAX_BYTES; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
ACK_MAX_BYTES: number

/* export: admitComputerEfsCarrierV1; kinds: value */
/* source: packages/replication/dist/computer-carrier.d.ts */
export declare function admitComputerEfsCarrierV1(options: {
    readonly limits: ComputerEfsCarrierV1Limits;
    readonly signal?: AbortSignal;
    readonly openEndpoint: () => ComputerEfsCarrierV1Endpoint | Promise<ComputerEfsCarrierV1Endpoint>;
}): Promise<AdmittedComputerEfsCarrierV1>;

/* export: AdmittedComputerEfsCarrierV1; kinds: type */
/* source: packages/replication/dist/computer-carrier.d.ts */
export interface AdmittedComputerEfsCarrierV1 extends AsyncDisposable {
    readonly target: Readonly<ComputerEfsCarrierV1RpcTarget>;
    readonly limits: Readonly<ValidatedComputerEfsCarrierV1>;
    close(): Promise<void>;
}

/* export: assertNotError; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
declare function assertNotError(envelope: CanonicalReplicationEnvelope): void;

/* export: authorizationDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function authorizationDigest(value: CanonicalAuthorizationRecord): Uint8Array;

/* export: authorizationDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function authorizationDigestHex(value: CanonicalAuthorizationRecord): string;

/* export: AuthorizedReplicationPeer; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: authorizeExchange; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
declare function authorizeExchangeImpl(authorization: AuthorizedReplicationPeer, peer: CanonicalAuthorizationRecord): void;

/* export: authorizeReplicationFlow; kinds: value */
/* source: packages/replication/dist/authorization.d.ts */
export declare function authorizeReplicationFlow(options: {
    readonly sourceRole: ReplicationRole;
    readonly destinationRole: ReplicationRole;
    readonly plan: ReplicationPlan;
    readonly sourceAuthorization: AuthorizedReplicationPeer;
    readonly destinationAuthorization: AuthorizedReplicationPeer;
}): void;

/* export: batchEnvelopeDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function batchEnvelopeDigest(value: ReplicationBatch): Uint8Array;

/* export: batchEnvelopeDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function batchEnvelopeDigestHex(value: ReplicationBatch): string;

/* export: batchPayloadByteCount; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function batchPayloadByteCount(records: readonly ReplicationBatchRecord[]): number;

/* export: batchPayloadDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function batchPayloadDigest(records: readonly ReplicationBatchRecord[]): Uint8Array;

/* export: batchPayloadDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function batchPayloadDigestHex(records: readonly ReplicationBatchRecord[]): string;

/* export: bytesToLowerHex; kinds: value */
/* source: packages/replication/dist/sha256.d.ts */
export declare function bytesToLowerHex(value: Uint8Array): string;

/* export: CanonicalAuthorizationRecord; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface CanonicalAuthorizationRecord {
    readonly authorization: AuthorizedReplicationPeer;
    readonly effectiveLimits: ReplicationLimits;
}

/* export: canonicalRecord; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
export declare function canonicalRecord(authorization: AuthorizedReplicationPeer, effectiveLimits: NegotiatedReplicationSession["limits"]): CanonicalAuthorizationRecord;

/* export: CanonicalReplicationEnvelope; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export type CanonicalReplicationEnvelope = {
    readonly kind: "capabilities";
    readonly value: ReplicationCapabilities;
} | {
    readonly kind: "authorization";
    readonly value: CanonicalAuthorizationRecord;
} | {
    readonly kind: "batch";
    readonly value: ReplicationBatch;
} | {
    readonly kind: "cursor";
    readonly value: ReplicationCursorBinding;
} | {
    readonly kind: "revision-fragment";
    readonly value: ReplicationRevisionFragment;
} | {
    readonly kind: "checkpoint-fragment";
    readonly value: ReplicationCheckpointFragment;
} | {
    readonly kind: "branch-generation-fragment";
    readonly value: ReplicationBranchGenerationFragment;
} | {
    readonly kind: "terminal-result";
    readonly value: ReplicationTerminalResultRecord;
} | {
    readonly kind: "batch-acknowledgement";
    readonly value: ReplicationBatchAcknowledgement;
} | {
    readonly kind: "error";
    readonly value: ReplicationSemanticErrorRecord;
};

/* export: capabilitiesFromBridge; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
/**
 * Map the core-owned bridge capabilities onto the canonical wire
 * capabilities. The host profile is the frozen Computer carrier profile.
 */
export declare function capabilitiesFromBridge(capabilities: import("@ephemeralai/fs/integrations/replication").ReplicationBridgeCapabilities): ReplicationCapabilities;

/* export: capabilityDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function capabilityDigest(value: ReplicationCapabilities, effectiveLimits: ReplicationLimits): Uint8Array;

/* export: capabilityDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function capabilityDigestHex(value: ReplicationCapabilities, effectiveLimits: ReplicationLimits): string;

/* export: COMPUTER_EFS_CARRIER_V1_LIMITS; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
COMPUTER_EFS_CARRIER_V1_LIMITS: Readonly<ReplicationLimits>

/* export: COMPUTER_EFS_CARRIER_V1_RESOURCES; kinds: value */
/* source: packages/replication/dist/computer-carrier.d.ts */
COMPUTER_EFS_CARRIER_V1_RESOURCES: Readonly<{
    hostProfile: "computer-efs-carrier-v1";
    maxDecodedEnvelopeBytes: number;
    maxBase64Bytes: number;
    rpcFramingBytes: number;
    maxRawFrameBytes: number;
    maxUtf16Bytes: number;
    maxMutatingAcknowledgementBytes: number;
    maxScratchBytes: number;
    processPoolBytes: number;
    maxReservationBytes: number;
    maxInFlightExchanges: 1;
    compression: false;
}>

/* export: ComputerEfsCarrierV1Endpoint; kinds: type */
/* source: packages/replication/dist/computer-carrier.d.ts */
export interface ComputerEfsCarrierV1Endpoint {
    exchange(request: Uint8Array): Promise<Uint8Array>;
    close?(): void | Promise<void>;
}

/* export: ComputerEfsCarrierV1Limits; kinds: type */
/* source: packages/replication/dist/computer-carrier.d.ts */
export interface ComputerEfsCarrierV1Limits {
    readonly hostProfile?: typeof REPLICATION_HOST_PROFILE;
    readonly maxRequestBytes: number;
    readonly maxResponseBytes: number;
    readonly maxInFlightBatches?: number;
    readonly maxMutatingAcknowledgementBytes?: number;
    readonly compression?: false;
}

/* export: ComputerEfsCarrierV1RpcTarget; kinds: type */
/* source: packages/replication/dist/computer-carrier.d.ts */
export interface ComputerEfsCarrierV1RpcTarget {
    exchange(request: Uint8Array): Promise<Uint8Array>;
}

/* export: computerEfsCarrierV1Stats; kinds: value */
/* source: packages/replication/dist/computer-carrier.d.ts */
export declare function computerEfsCarrierV1Stats(): Readonly<{
    reservedBytes: number;
    queued: number;
}>;

/* export: createCanonicalBatch; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function createCanonicalBatch(input: Omit<ReplicationBatch, "entryCount" | "payloadByteCount" | "payloadDigest">): ReplicationBatch;

/* export: createCanonicalBatchAcknowledgement; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function createCanonicalBatchAcknowledgement(options: {
    readonly batch: ReplicationBatch;
    readonly nextPhase: ReplicationPhase;
    readonly cursor: Uint8Array;
    readonly chainDigest: Uint8Array;
    readonly acceptedEntries: number;
    readonly acceptedBytes: number;
    readonly stagedBytes: number;
}): Readonly<ReplicationBatchAcknowledgement>;

/* export: createReplicationEndpoint; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
export declare function createReplicationEndpoint(options: {
    bridge: ReplicationFilesystemBridge;
    authorization: AuthorizedReplicationPeer;
}): ReplicationEndpoint;

/* export: cursorBindingDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function cursorBindingDigest(value: ReplicationCursorBinding): Uint8Array;

/* export: cursorBindingDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function cursorBindingDigestHex(value: ReplicationCursorBinding): string;

/* export: decodeCanonicalBatchAcknowledgement; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function decodeCanonicalBatchAcknowledgement(input: Uint8Array, options?: DecodeCanonicalEnvelopeOptions): ReplicationBatchAcknowledgement;

/* export: decodeCanonicalEnvelope; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function decodeCanonicalEnvelope(input: Uint8Array, options?: DecodeCanonicalEnvelopeOptions): CanonicalReplicationEnvelope;

/* export: DecodeCanonicalEnvelopeOptions; kinds: type */
/* source: packages/replication/dist/wire.d.ts */
export interface DecodeCanonicalEnvelopeOptions {
    readonly maxBytes?: number;
}

/* export: destinationOperationId; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
export declare function destinationOperationId(sessionId: string): string;

/* export: effectiveLimitsDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
/** Digest of the exact negotiated limits row, independent of either policy. */
export declare function effectiveLimitsDigest(value: ReplicationLimits): Uint8Array;

/* export: effectiveLimitsDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function effectiveLimitsDigestHex(value: ReplicationLimits): string;

/* export: EFS_REPLICATION_V1_WIRE; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
EFS_REPLICATION_V1_WIRE: Readonly<{
    magic: "EFSR";
    version: 1;
    byteOrder: "big-endian";
    headerBytes: 12;
    envelopeTags: Readonly<{
        capabilities: 1;
        authorization: 2;
        batch: 3;
        cursor: 4;
        "revision-fragment": 5;
        "checkpoint-fragment": 6;
        "branch-generation-fragment": 7;
        "terminal-result": 8;
        error: 9;
        "batch-acknowledgement": 10;
    }>;
    recordTags: Readonly<{
        "object-descriptor": 1;
        "object-payload": 2;
        "manifest-root-descriptor": 3;
        "manifest-node-descriptor": 4;
        "missing-content": 5;
        "revision-fragment": 6;
        "checkpoint-fragment": 7;
        "branch-generation-fragment": 8;
        "terminal-result": 9;
    }>;
    featureCount: 10;
    unknownFields: "reject";
}>

/* export: encodeAuthorizationPayload; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function encodeAuthorizationPayload(value: CanonicalAuthorizationRecord): Uint8Array;

/* export: encodeBatchRecordsPayload; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function encodeBatchRecordsPayload(records: readonly ReplicationBatchRecord[]): Uint8Array;

/* export: encodeCanonicalBatchAcknowledgement; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function encodeCanonicalBatchAcknowledgement(value: ReplicationBatchAcknowledgement): Uint8Array;

/* export: encodeCanonicalEnvelope; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function encodeCanonicalEnvelope(envelope: CanonicalReplicationEnvelope): Uint8Array;

/* export: encodeCapabilitiesPayload; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function encodeCapabilitiesPayload(value: ReplicationCapabilities): Uint8Array;

/* export: encodeCursorBindingPayload; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function encodeCursorBindingPayload(value: ReplicationCursorBinding): Uint8Array;

/* export: equalBytes; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function equalBytes(left: Uint8Array, right: Uint8Array): boolean;

/* export: FastCdcConfiguration; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface FastCdcConfiguration {
    readonly minimum: number;
    readonly average: number;
    readonly maximum: number;
}

/* export: generateReplicationSessionId; kinds: value */
/* source: packages/replication/dist/identifiers.d.ts */
export declare function generateReplicationSessionId(fill?: ReplicationRandomFill): string;

/* export: IncrementalReplicationSha256; kinds: value,type */
/* source: packages/replication/dist/sha256.d.ts */
export declare class IncrementalReplicationSha256 {
    #private;
    update(value: Uint8Array): this;
    digest(): Uint8Array;
}

/* export: initialSessionCursor; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
/** Deterministic shared initial cursor so both peers open the same chain. */
export declare function initialSessionCursor(sessionId: string): Uint8Array;

/* export: isReplicationErrorRetryable; kinds: value */
/* source: packages/replication/dist/errors.d.ts */
export declare function isReplicationErrorRetryable(code: ReplicationErrorCode): boolean;

/* export: limitPolicyFromLimits; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
export declare function limitPolicyFromLimits(input: ReplicationLimits): Readonly<ReplicationLimitPolicy>;

/* export: NegotiatedReplicationSession; kinds: type */
/* source: packages/replication/dist/authorization.d.ts */
export interface NegotiatedReplicationSession {
    readonly protocol: typeof REPLICATION_PROTOCOL_VERSION;
    readonly limits: Readonly<ReplicationLimits>;
    readonly sourceCapabilityDigest: Uint8Array;
    readonly destinationCapabilityDigest: Uint8Array;
    readonly sourceAuthorizationDigest: Uint8Array;
    readonly destinationAuthorizationDigest: Uint8Array;
    readonly provisioning: boolean;
}

/* export: negotiateReplicationLimits; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
export declare function negotiateReplicationLimits(options: NegotiateReplicationLimitsOptions): Readonly<ReplicationLimits>;

/* export: NegotiateReplicationLimitsOptions; kinds: type */
/* source: packages/replication/dist/limits.d.ts */
export interface NegotiateReplicationLimitsOptions {
    readonly source: ReplicationLimits;
    readonly destination: ReplicationLimits;
    readonly sourcePolicy: ReplicationLimitPolicy;
    readonly destinationPolicy: ReplicationLimitPolicy;
    readonly hostProfile?: ReplicationLimits;
}

/* export: negotiateReplicationSession; kinds: value */
/* source: packages/replication/dist/authorization.d.ts */
export declare function negotiateReplicationSession(options: {
    readonly source: ReplicationCapabilities;
    readonly destination: ReplicationCapabilities;
    readonly sourceAuthorization: AuthorizedReplicationPeer;
    readonly destinationAuthorization: AuthorizedReplicationPeer;
    readonly plan: ReplicationPlan;
}): NegotiatedReplicationSession;

/* export: nextPhaseFor; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
/**
 * Frozen phase-advance rule applied by the receiver of every batch. An empty
 * batch is the deterministic marker that completes a phase; every other batch
 * stays in its phase. This rule is identical on both peers, so their durable
 * phases advance in lockstep.
 */
export declare function nextPhaseFor(batch: ReplicationBatch): ReplicationBatch["phase"];

/* export: nextSessionCursor; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
/**
 * Deterministic shared session cursor. Both peers compute the same next
 * cursor from the prior cursor digest and the accepted batch envelope, so
 * their durable cursor chains converge without carrying cursor bytes.
 */
export declare function nextSessionCursor(priorCursorDigest: Uint8Array, acceptedBatchEnvelopeDigest: Uint8Array): Uint8Array;

/* export: planEquals; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
export declare function planEquals(left: ReplicationPlan, right: ReplicationPlan): boolean;

/* export: PRE_NEGOTIATION_BYTES; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
PRE_NEGOTIATION_BYTES: number

/* export: randomSessionId; kinds: value */
/* source: packages/replication/dist/endpoint.d.ts */
declare function randomSessionId(): string;

/* export: receiptChainDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function receiptChainDigest(priorChainDigest: Uint8Array, sequence: number, acceptedBatchEnvelopeDigest: Uint8Array): Uint8Array;

/* export: receiptChainDigestHex; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function receiptChainDigestHex(priorChainDigest: Uint8Array, sequence: number, acceptedBatchEnvelopeDigest: Uint8Array): string;

/* export: replicate; kinds: value */
/* source: packages/replication/dist/driver.d.ts */
export declare function replicate(options: ReplicateOptions): Promise<ReplicationRunResult>;

/* export: ReplicatedAuthorityResult; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export type ReplicatedAuthorityResult = {
    readonly kind: "publication";
    readonly operationId: string;
    readonly outcome: "merged" | "conflict";
    readonly resultDigest: string;
} | {
    readonly kind: "discard";
    readonly operationId: string | null;
    readonly resultDigest: string;
};

/* export: ReplicateOptions; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export interface ReplicateOptions {
    readonly bridge: ReplicationFilesystemBridge;
    readonly transport: ReplicationTransport;
    readonly authorization: AuthorizedReplicationPeer;
    /** Optional authenticated policy advertisement for the remote destination. */
    readonly destinationAuthorization?: AuthorizedReplicationPeer;
    readonly plan: ReplicationPlan;
    readonly operationId: string;
    readonly resumeKey?: Uint8Array;
    readonly signal?: AbortSignal;
}

/* export: REPLICATION_APPLICATION_ID; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_APPLICATION_ID = 1161905747

/* export: REPLICATION_CEILING_FIELDS; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
REPLICATION_CEILING_FIELDS: readonly ("maxBatchEntries" | "maxBatchBytes" | "maxRequestBytes" | "maxResponseBytes" | "maxBufferedBytes" | "maxInFlightBatches" | "maxConcurrentSessions" | "maxStagingBytesPerSession" | "maxReplicationSessionRows" | "maxReplicationMetadataBytes" | "maxReceiptsPerSession" | "maxReceiptBytesPerSession" | "maxCursorBytes" | "maxTerminalResultBytes" | "maxCursorAgeMs" | "stagingLeaseMs" | "resultRetentionMs" | "maxRetryAttempts" | "maxRetryElapsedMs" | "maxRetryDelayMs")[]

/* export: REPLICATION_CHUNKER_FORMAT; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_CHUNKER_FORMAT: "fastcdc-v1"

/* export: REPLICATION_FILESYSTEM_SCHEMA_VERSION; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_FILESYSTEM_SCHEMA_VERSION = 13

/* export: REPLICATION_HOST_PROFILE; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_HOST_PROFILE: "computer-efs-carrier-v1"

/* export: REPLICATION_LIMIT_FIELDS; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
REPLICATION_LIMIT_FIELDS: readonly [
    "maxBatchEntries",
    "maxBatchBytes",
    "maxRequestBytes",
    "maxResponseBytes",
    "maxBufferedBytes",
    "maxInFlightBatches",
    "maxConcurrentSessions",
    "maxStagingBytesPerSession",
    "maxReplicationSessionRows",
    "maxReplicationMetadataBytes",
    "maxReceiptsPerSession",
    "maxReceiptBytesPerSession",
    "maxCursorBytes",
    "maxTerminalResultBytes",
    "maxCursorAgeMs",
    "stagingLeaseMs",
    "resultRetentionMs",
    "maxRetryAttempts",
    "maxRetryElapsedMs",
    "minRetryDelayMs",
    "maxRetryDelayMs"
]

/* export: REPLICATION_MANIFEST_FORMAT; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_MANIFEST_FORMAT: "efs-merkle-manifest-v1"

/* export: REPLICATION_PROTOCOL_VERSION; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_PROTOCOL_VERSION: "efs-replication-v1"

/* export: REPLICATION_STORAGE_USER_VERSION; kinds: value */
/* source: packages/replication/dist/types.d.ts */
REPLICATION_STORAGE_USER_VERSION = 13

/* export: ReplicationActivation; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export type ReplicationActivation = {
    readonly kind: "main";
    readonly revision: string;
} | {
    readonly kind: "branch";
    readonly branchId: string;
    readonly baseRevision: string;
    readonly generation: number;
    readonly generationDigest: string;
    readonly state: "active" | "merged" | "discarded";
    readonly authorityResult: ReplicatedAuthorityResult | null;
};

/* export: ReplicationBatch; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: ReplicationBatchAcknowledgement; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: ReplicationBatchRecord; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export type ReplicationBatchRecord = {
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
} | ({
    readonly kind: "revision-fragment";
} & ReplicationRevisionFragment) | ({
    readonly kind: "checkpoint-fragment";
} & ReplicationCheckpointFragment) | ({
    readonly kind: "branch-generation-fragment";
} & ReplicationBranchGenerationFragment) | ({
    readonly kind: "terminal-result";
} & ReplicationTerminalResultRecord);

/* export: ReplicationBranchGenerationFragment; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface ReplicationBranchGenerationFragment {
    readonly branchId: string;
    readonly baseRevision: string;
    readonly generation: number;
    readonly generationDigest: Uint8Array;
    readonly fragmentIndex: number;
    readonly fragmentCount: number;
    readonly fragmentBytes: Uint8Array;
}

/* export: ReplicationCapabilities; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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
    readonly hashAlgorithms: readonly [
        "sha256"
    ];
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

/* export: ReplicationCeilingLimits; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export type ReplicationCeilingLimits = Omit<ReplicationLimits, "minRetryDelayMs">;

/* export: ReplicationCheckpointFragment; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface ReplicationCheckpointFragment {
    readonly checkpointId: string;
    readonly revisionId: string;
    readonly fragmentIndex: number;
    readonly fragmentCount: number;
    readonly fragmentBytes: Uint8Array;
}

/* export: ReplicationCursorBinding; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: ReplicationEndpoint; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export interface ReplicationEndpoint {
    exchange(request: Uint8Array): Promise<Uint8Array>;
    close(): Promise<void>;
    /** Internal: register the local session side so inbound batches authenticate. */
    bindLocalSession(session: {
        readonly sessionId: string;
        readonly operationId: string;
        readonly ownerNonce: Uint8Array;
        readonly binding: ReplicationSessionBinding;
        readonly session: ReplicationSessionSnapshot;
        readonly negotiated: NegotiatedReplicationSession;
    }): void;
    /** Internal: keep the local endpoint's session snapshot in sync. */
    updateLocalSession(sessionId: string, session: ReplicationSessionSnapshot): void;
}

/* export: ReplicationError; kinds: value,type */
/* source: packages/replication/dist/errors.d.ts */
export declare class ReplicationError extends Error {
    readonly name = "ReplicationError";
    readonly code: ReplicationErrorCode;
    readonly phase: ReplicationPhase | null;
    readonly sessionId: string | null;
    readonly retryable: boolean;
    constructor(code: ReplicationErrorCode, message: string, options?: {
        readonly phase?: ReplicationPhase | null;
        readonly sessionId?: string | null;
        readonly retryable?: boolean;
        readonly cause?: unknown;
    });
}

/* export: ReplicationErrorCode; kinds: type */
/* source: packages/replication/dist/errors.d.ts */
export type ReplicationErrorCode = "ProtocolMismatch" | "FilesystemMismatch" | "AuthorityMismatch" | "SchemaMismatch" | "CapabilityMismatch" | "IncompatibleLimit" | "UnauthorizedScope" | "ProvisioningRejected" | "OperationMismatch" | "MainDiverged" | "BaseRevisionMissing" | "BranchIdentityMismatch" | "BranchDiverged" | "CursorMismatch" | "CursorExpired" | "BatchReplayMismatch" | "StagingExpired" | "IntegrityFailure" | "ResourceLimit" | "Busy" | "TransportFailure" | "RetryExhausted" | "Aborted" | "Closed";

/* export: replicationErrorFromRecord; kinds: value */
/* source: packages/replication/dist/errors.d.ts */
export declare function replicationErrorFromRecord(record: ReplicationSemanticErrorRecord): ReplicationError;

/* export: replicationErrorRecord; kinds: value */
/* source: packages/replication/dist/errors.d.ts */
export declare function replicationErrorRecord(error: ReplicationError): ReplicationSemanticErrorRecord;

/* export: ReplicationFeatures; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: ReplicationLimitPolicy; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface ReplicationLimitPolicy {
    readonly ceilings: ReplicationCeilingLimits;
    readonly minRetryDelayMsFloor: number;
}

/* export: ReplicationLimits; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: replicationOwnerNonceDigest; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function replicationOwnerNonceDigest(ownerNonce: Uint8Array): Uint8Array;

/* export: ReplicationPhase; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export type ReplicationPhase = "handshake" | "plan-selection" | "content-offer" | "missing-content" | "content-transfer" | "state-transfer" | "activation" | "result-acknowledgement" | "cleanup";

/* export: ReplicationPlan; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export type ReplicationPlan = {
    readonly flow: "authority-main-to-replica";
} | {
    readonly flow: "authority-branch-to-replica";
    readonly branchId: string;
} | {
    readonly flow: "replica-branch-to-authority";
    readonly branchId: string;
} | {
    readonly flow: "replica-branch-to-replica";
    readonly branchId: string;
};

/* export: ReplicationRandomFill; kinds: type */
/* source: packages/replication/dist/identifiers.d.ts */
export type ReplicationRandomFill = (target: Uint8Array) => void;

/* export: ReplicationResult; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export interface ReplicationResult {
    readonly sessionId: string;
    readonly operationId: string;
    readonly plan: ReplicationPlan;
    readonly activation: ReplicationActivation;
    readonly finalCursor: string;
    readonly transferredBytes: number;
    readonly reusedBytes: number;
}

/* export: ReplicationRevisionFragment; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface ReplicationRevisionFragment {
    readonly revisionId: string;
    readonly parentRevisionId: string | null;
    readonly fragmentIndex: number;
    readonly fragmentCount: number;
    readonly fragmentBytes: Uint8Array;
}

/* export: ReplicationRole; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export type ReplicationRole = "main-authority" | "replica";

/* export: ReplicationRunResult; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export type ReplicationRunResult = {
    readonly status: "complete";
    readonly result: ReplicationResult;
} | {
    readonly status: "pending";
    readonly resumeKey: Uint8Array;
    readonly notBeforeMs: number;
    readonly reason: "busy" | "transport" | "backpressure";
};

/* export: ReplicationSemanticErrorRecord; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface ReplicationSemanticErrorRecord {
    readonly code: import("./errors.js").ReplicationErrorCode;
    readonly phase: ReplicationPhase | null;
    readonly sessionId: string | null;
    readonly message: string;
    readonly retryable: boolean;
}

/* export: replicationSha256; kinds: value */
/* source: packages/replication/dist/sha256.d.ts */
export declare function replicationSha256(value: Uint8Array): Uint8Array;

/* export: ReplicationStorageCapabilities; kinds: type */
/* source: packages/replication/dist/types.d.ts */
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

/* export: ReplicationTerminalResultRecord; kinds: type */
/* source: packages/replication/dist/types.d.ts */
export interface ReplicationTerminalResultRecord {
    readonly operationId: string;
    readonly branchId: string | null;
    readonly generation: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly resultDigest: Uint8Array;
    readonly resultBytes: Uint8Array;
}

/* export: ReplicationTransport; kinds: type */
/* source: packages/replication/dist/endpoint.d.ts */
export interface ReplicationTransport {
    exchange(request: Uint8Array, options?: {
        signal?: AbortSignal;
    }): Promise<Uint8Array>;
}

/* export: requiredRoles; kinds: value */
/* source: packages/replication/dist/authorization.d.ts */
export declare function requiredRoles(plan: ReplicationPlan): Readonly<{
    source: ReplicationRole;
    destination: ReplicationRole;
}>;

/* export: validateAuthorizedPeer; kinds: value */
/* source: packages/replication/dist/authorization.d.ts */
export declare function validateAuthorizedPeer(authorization: AuthorizedReplicationPeer, name?: string): void;

/* export: validateBatchAcknowledgement; kinds: value */
/* source: packages/replication/dist/wire.d.ts */
export declare function validateBatchAcknowledgement(batch: ReplicationBatch, acknowledgement: ReplicationBatchAcknowledgement): void;

/* export: validateComputerEfsCarrierV1; kinds: value */
/* source: packages/replication/dist/computer-carrier.d.ts */
export declare function validateComputerEfsCarrierV1(input: ComputerEfsCarrierV1Limits): Readonly<ValidatedComputerEfsCarrierV1>;

/* export: ValidatedComputerEfsCarrierV1; kinds: type */
/* source: packages/replication/dist/computer-carrier.d.ts */
export interface ValidatedComputerEfsCarrierV1 {
    readonly hostProfile: typeof REPLICATION_HOST_PROFILE;
    readonly maxRequestBytes: number;
    readonly maxResponseBytes: number;
    readonly maxInFlightBatches: 1;
    readonly maxMutatingAcknowledgementBytes: number;
    readonly compression: false;
    readonly reservationBytes: number;
}

/* export: validateLimitsAgainstStorage; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
export declare function validateLimitsAgainstStorage(inputLimits: ReplicationLimits, inputStorage: ReplicationStorageCapabilities, name?: string): void;

/* export: validateReplicationLimits; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
export declare function validateReplicationLimits(input: ReplicationLimits, name?: string): Readonly<ReplicationLimits>;

/* export: validateReplicationSessionId; kinds: value */
/* source: packages/replication/dist/identifiers.d.ts */
export declare function validateReplicationSessionId(value: string): string;

/* export: validateReplicationStorageCapabilities; kinds: value */
/* source: packages/replication/dist/limits.d.ts */
export declare function validateReplicationStorageCapabilities(input: ReplicationStorageCapabilities, name?: string): Readonly<ReplicationStorageCapabilities>;

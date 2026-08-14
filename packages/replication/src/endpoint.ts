import { ReplicationError } from "./errors.js";
import { negotiateReplicationSession, type NegotiatedReplicationSession } from "./authorization.js";
import {
  REPLICATION_PROTOCOL_VERSION,
  REPLICATION_HOST_PROFILE,
} from "./types.js";
import type {
  AuthorizedReplicationPeer,
  CanonicalAuthorizationRecord,
  CanonicalReplicationEnvelope,
  ReplicationBatch,
  ReplicationBatchAcknowledgement,
  ReplicationBatchRecord,
  ReplicationCapabilities,
  ReplicationCursorBinding,
  ReplicationPlan,
  ReplicationSemanticErrorRecord,
} from "./types.js";
import {
  createCanonicalBatchAcknowledgement,
  createCanonicalBatch,
  batchEnvelopeDigest,
  encodeCanonicalEnvelope,
  decodeCanonicalEnvelope,
  encodeCanonicalBatchAcknowledgement,
  receiptChainDigest,
  replicationOwnerNonceDigest,
  authorizationDigest,
  effectiveLimitsDigest,
  equalBytes,
  nextSessionCursor,
} from "./wire.js";
import {
  decodeActivationRequest,
  type TransferActivationRequest,
} from "@ephemeralai/fs/integrations/replication";
import type {
  ReplicationFilesystemBridge,
  ReplicationSessionBinding,
  ReplicationSessionSnapshot,
} from "@ephemeralai/fs/integrations/replication";
import { randomBytes, createHash } from "node:crypto";

function sha256Of(bytes: Uint8Array): Uint8Array {
  return createHash("sha256").update(bytes).digest();
}

/** Deterministic shared initial cursor so both peers open the same chain. */
export function initialSessionCursor(sessionId: string): Uint8Array {
  return sha256Of(
    createHash("sha256")
      .update("efs-replication-v1/initial-cursor\0")
      .update(sessionId)
      .digest(),
  );
}

export interface ReplicationTransport {
  exchange(
    request: Uint8Array,
    options?: { signal?: AbortSignal },
  ): Promise<Uint8Array>;
}

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
  updateLocalSession(
    sessionId: string,
    session: ReplicationSessionSnapshot,
  ): void;
}

export interface ReplicationResult {
  readonly sessionId: string;
  readonly operationId: string;
  readonly plan: ReplicationPlan;
  readonly activation: ReplicationActivation;
  readonly finalCursor: string;
  readonly transferredBytes: number;
  readonly reusedBytes: number;
}

export type ReplicationActivation =
  | { readonly kind: "main"; readonly revision: string }
  | {
      readonly kind: "branch";
      readonly branchId: string;
      readonly baseRevision: string;
      readonly generation: number;
      readonly generationDigest: string;
      readonly state: "active" | "merged" | "discarded";
      readonly authorityResult: ReplicatedAuthorityResult | null;
    };

export type ReplicatedAuthorityResult =
  | {
      readonly kind: "publication";
      readonly operationId: string;
      readonly outcome: "merged" | "conflict";
      readonly resultDigest: string;
    }
  | {
      readonly kind: "discard";
      readonly operationId: string | null;
      readonly resultDigest: string;
    };

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

export type ReplicationRunResult =
  | { readonly status: "complete"; readonly result: ReplicationResult }
  | {
      readonly status: "pending";
      readonly resumeKey: Uint8Array;
      readonly notBeforeMs: number;
      readonly reason: "busy" | "transport" | "backpressure";
    };

const MIB = 1024 * 1024;
const PRE_NEGOTIATION_BYTES = 64 * 1024;
const ACK_MAX_BYTES = 64 * 1024;
const PHASE_ORDER = [
  "handshake",
  "plan-selection",
  "content-offer",
  "missing-content",
  "content-transfer",
  "state-transfer",
  "activation",
  "result-acknowledgement",
  "cleanup",
] as const;

interface SessionState {
  readonly sessionId: string;
  readonly operationId: string;
  readonly ownerNonce: Uint8Array;
  readonly binding: ReplicationSessionBinding;
  session: ReplicationSessionSnapshot;
  readonly negotiated: NegotiatedReplicationSession;
}

function randomSessionId(): string {
  const bytes = randomBytes(16);
  let output = "";
  for (const byte of bytes) output += byte.toString(16).padStart(2, "0");
  return output;
}

export function canonicalRecord(
  authorization: AuthorizedReplicationPeer,
  effectiveLimits: NegotiatedReplicationSession["limits"],
): CanonicalAuthorizationRecord {
  return Object.freeze({
    authorization: Object.freeze({ ...authorization }),
    effectiveLimits: Object.freeze({ ...effectiveLimits }),
  });
}

export function planEquals(left: ReplicationPlan, right: ReplicationPlan): boolean {
  if (left.flow !== right.flow) return false;
  if (left.flow === "authority-main-to-replica") return true;
  return left.branchId === (right as { branchId: string }).branchId;
}

function encodeErrorEnvelope(error: unknown): Uint8Array {
  const code = error instanceof ReplicationError ? error.code : "TransportFailure";
  const message =
    error instanceof Error ? error.message.slice(0, 4096) : "unknown replication error";
  return encodeCanonicalEnvelope({
    kind: "error",
    value: {
      code,
      phase: null,
      sessionId: null,
      message,
      retryable: code === "Busy" || code === "TransportFailure",
    },
  });
}

function assertNotError(envelope: CanonicalReplicationEnvelope): void {
  if (envelope.kind === "error") {
    const value = envelope.value as ReplicationSemanticErrorRecord;
    throw new ReplicationError(value.code, value.message);
  }
}

/**
 * Map the core-owned bridge capabilities onto the canonical wire
 * capabilities. The host profile is the frozen Computer carrier profile.
 */
export function capabilitiesFromBridge(
  capabilities: import("@ephemeralai/fs/integrations/replication").ReplicationBridgeCapabilities,
): ReplicationCapabilities {
  return {
    protocolVersions: [REPLICATION_PROTOCOL_VERSION],
    hostProfile: REPLICATION_HOST_PROFILE,
    provisioningState: capabilities.provisioningState,
    filesystemId: capabilities.filesystemId,
    authorityId: capabilities.authorityId,
    applicationId: capabilities.applicationId,
    filesystemSchemaVersion: capabilities.filesystemSchemaVersion,
    storageUserVersion: capabilities.storageUserVersion,
    storageMigrationState: capabilities.storageMigrationState,
    readableFilesystemSchemaVersions: capabilities.readableFilesystemSchemaVersions,
    writableFilesystemSchemaVersion: capabilities.writableFilesystemSchemaVersion,
    role: capabilities.role,
    hashAlgorithms: ["sha256"],
    activeManifestFormat: capabilities.activeManifestFormat,
    supportedManifestFormats: capabilities.supportedManifestFormats,
    activeChunkerFormat: capabilities.activeChunkerFormat,
    supportedChunkerFormats: capabilities.supportedChunkerFormats,
    fastCdc: capabilities.fastCdc,
    supportedFastCdcConfigurations: capabilities.supportedFastCdcConfigurations,
    copyOnWritePageBytes: capabilities.copyOnWritePageBytes,
    supportedCopyOnWritePageBytes: capabilities.supportedCopyOnWritePageBytes,
    features: capabilities.features,
    limits: capabilities.limits,
    storage: capabilities.storage,
  };
}

/**
 * Frozen phase-advance rule applied by the receiver of every batch. An empty
 * batch is the deterministic marker that completes a phase; every other batch
 * stays in its phase. This rule is identical on both peers, so their durable
 * phases advance in lockstep.
 */
export function nextPhaseFor(batch: ReplicationBatch): ReplicationBatch["phase"] {
  switch (batch.phase) {
    case "content-offer":
      return batch.entryCount === 0 ? "missing-content" : "content-offer";
    case "missing-content":
      return batch.entryCount === 0 ? "content-transfer" : "missing-content";
    case "content-transfer":
      return batch.entryCount === 0 ? "state-transfer" : "content-transfer";
    case "state-transfer":
      return batch.entryCount === 0 ? "activation" : "state-transfer";
    case "activation":
      return "result-acknowledgement";
    case "result-acknowledgement":
      return "cleanup";
    default:
      return batch.phase;
  }
}

export function destinationOperationId(sessionId: string): string {
  return `efs-session-${sessionId}`;
}

export function createReplicationEndpoint(options: {
  bridge: ReplicationFilesystemBridge;
  authorization: AuthorizedReplicationPeer;
}): ReplicationEndpoint {
  const { bridge, authorization } = options;
  const sessions = new Map<string, SessionState>();
  let handshakePeerCapabilities: ReplicationCapabilities | null = null;
  let peerAuthorization: AuthorizedReplicationPeer | null = null;
  let decodedRequestMaxBytes = PRE_NEGOTIATION_BYTES;
  let closed = false;

  const endpoint: ReplicationEndpoint = {
    bindLocalSession(session: {
      readonly sessionId: string;
      readonly operationId: string;
      readonly ownerNonce: Uint8Array;
      readonly binding: ReplicationSessionBinding;
      readonly session: ReplicationSessionSnapshot;
      readonly negotiated: NegotiatedReplicationSession;
    }): void {
      sessions.set(session.sessionId, {
        sessionId: session.sessionId,
        operationId: session.operationId,
        ownerNonce: session.ownerNonce,
        binding: session.binding,
        session: session.session,
        negotiated: session.negotiated,
      });
    },
    updateLocalSession(
      sessionId: string,
      session: ReplicationSessionSnapshot,
    ): void {
      const existing = sessions.get(sessionId);
      if (existing) existing.session = session;
    },
    async exchange(request: Uint8Array): Promise<Uint8Array> {
      if (closed)
        return encodeErrorEnvelope(new ReplicationError("Closed", "endpoint is closed"));
      let envelope: CanonicalReplicationEnvelope;
      try {
        envelope = decodeCanonicalEnvelope(request, {
          maxBytes: decodedRequestMaxBytes,
        });
      } catch (error) {
        return encodeErrorEnvelope(error);
      }
      try {
        if (envelope.kind === "capabilities") {
          handshakePeerCapabilities = envelope.value;
          return encodeCanonicalEnvelope({
            kind: "capabilities",
            value: capabilitiesFromBridge(bridge.capabilities),
          });
        }
        if (envelope.kind === "authorization") {
          const received = envelope.value;
          peerAuthorization = received.authorization;
          return encodeCanonicalEnvelope({
            kind: "authorization",
            value: canonicalRecord(authorization, received.effectiveLimits),
          });
        }
        if (envelope.kind === "cursor") {
          if (!handshakePeerCapabilities)
            throw new ReplicationError(
              "ProtocolMismatch",
              "cursor binding arrived before the capability handshake",
            );
          if (!peerAuthorization)
            throw new ReplicationError(
              "ProtocolMismatch",
              "cursor binding arrived before authorization",
            );
          const mine = capabilitiesFromBridge(bridge.capabilities);
          const peerIsSource =
            handshakePeerCapabilities.filesystemId !== null &&
            envelope.value.sourceFilesystemId ===
              handshakePeerCapabilities.filesystemId;
          const negotiated = negotiateReplicationSession({
            source: peerIsSource ? handshakePeerCapabilities : mine,
            destination: peerIsSource ? mine : handshakePeerCapabilities,
            sourceAuthorization: peerIsSource ? peerAuthorization : authorization,
            destinationAuthorization: peerIsSource ? authorization : peerAuthorization,
            plan: envelope.value.plan,
          });
          decodedRequestMaxBytes = negotiated.limits.maxRequestBytes;
          const sessionId = envelope.value.sessionId;
          const existing = sessions.get(sessionId);
          if (existing) {
            return encodeCanonicalEnvelope({
              kind: "cursor",
              value: {
                ...envelope.value,
                phase: existing.session.phase,
                nextSequence: existing.session.nextSequence,
              },
            });
          }
          const proposedBinding = buildDestinationBinding(
            bridge,
            authorization,
            negotiated,
            envelope.value,
          );
          // The destination endpoint is process-local, but the session is
          // durable. Rehydrate the exact binding after a restart so owner
          // nonce, opaque resume key, limits, and authorization digests are
          // not replaced by fresh random values.
          const loaded = await bridge
            .loadSession({ operationId: proposedBinding.operationId })
            .catch((error: unknown) => {
              if (
                error instanceof Error &&
                error.message.startsWith("OperationMismatch: replication operation is unknown")
              )
                return null;
              throw error;
            });
          const binding = loaded
            ? Object.freeze({
                ...proposedBinding,
                sessionId: loaded.binding.sessionId,
                resumeKey: loaded.binding.resumeKey,
                ownerNonce: loaded.binding.ownerNonce,
              })
            : proposedBinding;
          const initialCursor = initialSessionCursor(sessionId);
          const sessionNow = Date.now();
          const outcome = await bridge.createOrResumeSession({
            binding,
            phase: "content-offer",
            cursor: initialCursor,
            cursorDigest: sha256Of(initialCursor),
            now: sessionNow,
            expiresAtMs: sessionNow + negotiated.limits.maxCursorAgeMs,
          });
          sessions.set(sessionId, {
            sessionId,
            operationId: binding.operationId,
            ownerNonce: binding.ownerNonce,
            binding,
            session: outcome.session,
            negotiated,
          });
          return encodeCanonicalEnvelope({
            kind: "cursor",
            value: {
              ...envelope.value,
              phase: outcome.session.phase,
              nextSequence: outcome.session.nextSequence,
            },
          });
        }
        if (envelope.kind === "batch") {
          return exchangeBatch(envelope.value);
        }
        if (envelope.kind === "batch-acknowledgement") {
          const state = sessions.get(envelope.value.sessionId);
          if (!state)
            throw new ReplicationError(
              "CursorMismatch",
              "acknowledgement names an unknown session",
            );
          validateAckShape(envelope.value);
          return encodeCanonicalEnvelope({
            kind: "batch-acknowledgement",
            value: envelope.value,
          });
        }
        return encodeErrorEnvelope(
          new ReplicationError("ProtocolMismatch", "unsupported envelope kind"),
        );
      } catch (error) {
        return encodeErrorEnvelope(error);
      }
    },
    async close(): Promise<void> {
      closed = true;
      sessions.clear();
      handshakePeerCapabilities = null;
      peerAuthorization = null;
    },
  };

  async function exchangeBatch(batch: ReplicationBatch): Promise<Uint8Array> {
    const state = sessions.get(batch.sessionId);
    if (!state)
      throw new ReplicationError("CursorMismatch", "batch names an unknown session");
    const boundPlan: ReplicationPlan =
      state.binding.flow === "authority-main-to-replica"
        ? { flow: "authority-main-to-replica" }
        : { flow: state.binding.flow, branchId: state.binding.branchId ?? "" };
    if (!planEquals(batch.plan, boundPlan))
      throw new ReplicationError(
        "UnauthorizedScope",
        "batch plan does not match the bound session plan",
      );
    if (batch.phase === "missing-content" && batch.entryCount === 0) {
      return respondMissingContent(batch, state);
    }
    if (batch.phase !== state.session.phase)
      throw new ReplicationError(
        "CursorMismatch",
        "batch phase differs from the durable session phase",
      );
    if (batch.phase !== "result-acknowledgement") await ensureImport(state);
    const nextPhase = nextPhaseFor(batch);
    const now = Date.now();
    const nextCursor = nextSessionCursor(
      state.session.cursorDigest,
      batchEnvelopeDigest(batch),
    );
    const stagedDelta = outcomeStagedDelta(batch);
    const priorChain = state.session.chainDigest;
    const chainDigest = receiptChainDigest(
      priorChain,
      batch.sequence,
      batchEnvelopeDigest(batch),
    );
    const acknowledgement = createCanonicalBatchAcknowledgement({
      batch,
      nextPhase,
      cursor: nextCursor,
      chainDigest,
      acceptedEntries: state.session.acceptedEntries + batch.entryCount,
      acceptedBytes: state.session.acceptedBytes + batch.payloadByteCount,
      stagedBytes: state.session.stagedBytes + stagedDelta,
    });
    const encodedAck = encodeCanonicalBatchAcknowledgement(acknowledgement);
    const outcome = await bridge.acceptBatch({
      operationId: state.operationId,
      sessionId: batch.sessionId,
      ownerNonce: state.ownerNonce,
      sequence: batch.sequence,
      phase: batch.phase,
      priorCursorDigest: batch.priorCursorDigest,
      batchEnvelopeDigest: batchEnvelopeDigest(batch),
      payloadDigest: batch.payloadDigest,
      entryCount: batch.entryCount,
      payloadByteCount: batch.payloadByteCount,
      nextPhase,
      nextCursor,
      nextCursorDigest: acknowledgement.cursorDigest,
      acknowledgement: encodedAck,
      stagedBytesDelta: stagedDelta,
      now,
      records: batch.records as ReplicationBatchRecord[],
    });
    state.session = outcome.session;
    if (batch.phase === "activation" && !outcome.replayed) {
      const requestRecord = batch.records.find(
        (record): record is Extract<ReplicationBatchRecord, { kind: "terminal-result" }> =>
          record.kind === "terminal-result",
      );
      if (requestRecord) {
        const request = decodeActivationRequest(requestRecord.resultBytes);
        await finalizeDestination(bridge, state, request);
      }
    }
    if (batch.phase === "result-acknowledgement" && !outcome.replayed) {
      const resultRecord = batch.records.find(
        (record): record is Extract<ReplicationBatchRecord, { kind: "terminal-result" }> =>
          record.kind === "terminal-result",
      );
      if (resultRecord) {
        await bridge.storeTerminalResult({
          operationId: state.operationId,
          sessionId: batch.sessionId,
          ownerNonce: state.ownerNonce,
          result: resultRecord.resultBytes,
          now: Date.now(),
        });
      }
    }
    return encodedAck;
  }

  async function respondMissingContent(
    batch: ReplicationBatch,
    state: SessionState,
  ): Promise<Uint8Array> {
    if (
      state.session.phase !== "missing-content" &&
      state.session.phase !== "content-transfer"
    )
      throw new ReplicationError(
        "CursorMismatch",
        "missing-content request arrived outside the missing-content phase",
      );
    if (batch.sequence !== state.session.nextSequence)
      throw new ReplicationError(
        "CursorMismatch",
        "missing-content request sequence is not the next sequence",
      );
    const missing = await bridge.readMissingContent({
      sessionId: batch.sessionId,
      maxEntries: state.negotiated.limits.maxBatchEntries,
      maxBytes: state.negotiated.limits.maxBatchBytes,
    });
    const response = createCanonicalBatch({
      sessionId: batch.sessionId,
      plan: batch.plan,
      phase: "missing-content",
      sequence: batch.sequence,
      priorCursorDigest: state.session.cursorDigest,
      records: missing.records as ReplicationBatchRecord[],
    });
    const nextPhase = "content-transfer";
    const responseDigest = batchEnvelopeDigest(response);
    const advanced = await bridge.recordOutboundBatch({
      operationId: state.operationId,
      sessionId: batch.sessionId,
      ownerNonce: state.ownerNonce,
      sequence: batch.sequence,
      phase: "missing-content",
      nextPhase,
      nextCursor: nextSessionCursor(
        state.session.cursorDigest,
        responseDigest,
      ),
      nextCursorDigest: sha256Of(
        nextSessionCursor(state.session.cursorDigest, responseDigest),
      ),
    });
    state.session = advanced;
    return encodeCanonicalEnvelope({ kind: "batch", value: response });
  }

  async function ensureImport(state: SessionState): Promise<void> {
    const kind: 0 | 1 | 2 =
      bridge.capabilities.provisioningState === "unbound-replica"
        ? 2
        : state.binding.flow === "authority-main-to-replica"
          ? 0
          : 1;
    const leaseId = `replication-import-${state.sessionId}`;
    const now = Date.now();
    const expiresAt = now + state.negotiated.limits.stagingLeaseMs;
    try {
      const renewed = await bridge.renewImportLease({
        sessionId: state.sessionId,
        ownerNonce: state.ownerNonce,
        now,
        expiresAt,
      });
      if (renewed) return;
    } catch {
      // The import does not exist yet; create it below.
    }
    await bridge.beginImport({
      sessionId: state.sessionId,
      kind,
      leaseId,
      ownerNonce: state.ownerNonce,
      branchId: state.binding.branchId,
      baseRevision: null,
      generation: null,
      expectedGenerationDigest: null,
      now,
      expiresAt,
      maxStagingBytesPerSession: state.negotiated.limits.maxStagingBytesPerSession,
      resultRetentionMs: state.negotiated.limits.resultRetentionMs,
    });
  }

  function outcomeStagedDelta(batch: ReplicationBatch): number {
    if (batch.phase === "content-transfer") {
      let total = 0;
      for (const record of batch.records)
        if (record.kind === "object-payload") total += record.byteLength;
      return total;
    }
    return 0;
  }

  return endpoint;
}

function validateAckShape(acknowledgement: ReplicationBatchAcknowledgement): void {
  if (
    acknowledgement.cursor.byteLength < 16 ||
    acknowledgement.cursor.byteLength > 256
  )
    throw new ReplicationError(
      "ProtocolMismatch",
      "acknowledgement cursor is outside the canonical envelope",
    );
}

function buildDestinationBinding(
  bridge: ReplicationFilesystemBridge,
  authorization: AuthorizedReplicationPeer,
  negotiated: NegotiatedReplicationSession,
  value: ReplicationCursorBinding,
): ReplicationSessionBinding {
  const sessionId = value.sessionId;
  const mine = bridge.capabilities;
  const flow = value.plan.flow;
  const sourceRole =
    flow === "authority-main-to-replica" || flow === "authority-branch-to-replica"
      ? "main-authority"
      : "replica";
  const destinationRole =
    flow === "replica-branch-to-replica" ||
    flow === "authority-main-to-replica" ||
    flow === "authority-branch-to-replica"
      ? "replica"
      : "main-authority";
  if (mine.role !== destinationRole)
    throw new ReplicationError(
      "UnauthorizedScope",
      "destination endpoint role does not authorize the selected flow",
    );
  return {
    operationId: destinationOperationId(sessionId),
    sessionId,
    resumeKey: randomBytes(32),
    ownerNonce: randomBytes(16),
    flow,
    branchId: flow === "authority-main-to-replica" ? null : value.plan.branchId,
    sourceFilesystemId: value.sourceFilesystemId,
    destinationFilesystemId: value.destinationFilesystemId,
    sourceRole,
    destinationRole,
    sourceAuthorizationDigest: negotiated.sourceAuthorizationDigest,
    destinationAuthorizationDigest: negotiated.destinationAuthorizationDigest,
    sourceCapabilityDigest: negotiated.sourceCapabilityDigest,
    destinationCapabilityDigest: negotiated.destinationCapabilityDigest,
    effectiveLimitsDigest: effectiveLimitsDigest(negotiated.limits),
    maxBatchEntries: negotiated.limits.maxBatchEntries,
    maxBatchBytes: negotiated.limits.maxBatchBytes,
    maxRequestBytes: negotiated.limits.maxRequestBytes,
    maxResponseBytes: negotiated.limits.maxResponseBytes,
    maxBufferedBytes: negotiated.limits.maxBufferedBytes,
    maxInFlightBatches: negotiated.limits.maxInFlightBatches,
    maxConcurrentSessions: negotiated.limits.maxConcurrentSessions,
    maxCursorBytes: negotiated.limits.maxCursorBytes,
    maxReplicationSessionRows: negotiated.limits.maxReplicationSessionRows,
    maxReplicationMetadataBytes: negotiated.limits.maxReplicationMetadataBytes,
    maxReceiptsPerSession: negotiated.limits.maxReceiptsPerSession,
    maxReceiptBytesPerSession: negotiated.limits.maxReceiptBytesPerSession,
    maxStagingBytesPerSession: negotiated.limits.maxStagingBytesPerSession,
    maxAcknowledgementBytes: ACK_MAX_BYTES,
    maxTerminalResultBytes: negotiated.limits.maxTerminalResultBytes,
    maxCursorAgeMs: negotiated.limits.maxCursorAgeMs,
    stagingLeaseMs: negotiated.limits.stagingLeaseMs,
    maxRetryAttempts: negotiated.limits.maxRetryAttempts,
    maxRetryElapsedMs: negotiated.limits.maxRetryElapsedMs,
    minRetryDelayMs: negotiated.limits.minRetryDelayMs,
    maxRetryDelayMs: negotiated.limits.maxRetryDelayMs,
    resultRetentionMs: negotiated.limits.resultRetentionMs,
  };
}

async function finalizeDestination(
  bridge: ReplicationFilesystemBridge,
  state: SessionState,
  request: TransferActivationRequest,
): Promise<void> {
  await bridge.finalizeImport({
    sessionId: state.binding.sessionId,
    kind: request.kind,
    expectedRevision: request.expectedRevision,
    expectedRootMutationGeneration: request.expectedRootMutationGeneration,
    expectedNextAllocationSequence: request.expectedNextAllocationSequence,
    expectedRootInode: request.expectedRootInode,
    expectedRevisionCount: request.expectedRevisionCount,
    expectedStateRows: request.expectedStateRows,
    expectedClosureRoots: request.expectedClosureRoots,
    expectedClosureNodes: request.expectedClosureNodes,
    expectedClosureObjects: request.expectedClosureObjects,
    expectedClosureObjectBytes: request.expectedClosureObjectBytes,
    branchId: request.branchId,
    baseRevision: request.baseRevision,
    generation: request.generation,
    generationDigest: request.generationDigest ?? null,
    checkpoint: request.checkpoint,
    terminalState: request.terminalState,
    terminalResultOperationId: request.terminalResultOperationId,
    terminalResultBytes: request.terminalResultBytes,
    genesisMeta: request.genesis
      ? {
          filesystemId: request.genesis.filesystemId,
          rootInode: request.genesis.rootInode,
          mainRevision: request.genesis.mainRevision,
          rootMutationGeneration: request.genesis.rootMutationGeneration,
          nextAllocationSequence: request.genesis.nextAllocationSequence,
          cowPageBytes: request.genesis.cowPageBytes,
          createdAtMs: request.genesis.createdAtMs,
          maxManifestEntries: request.genesis.maxManifestEntries,
          maxManifestDepth: request.genesis.maxManifestDepth,
          maxFileBytes: request.genesis.maxFileBytes,
          writerProfile: request.genesis.writerProfile,
          manifestFormat: request.genesis.manifestFormat,
          chunkerFormat: request.genesis.chunkerFormat,
          fastCdcMinimum: request.genesis.fastCdcMinimum,
          fastCdcAverage: request.genesis.fastCdcAverage,
          fastCdcMaximum: request.genesis.fastCdcMaximum,
          rootInodeType: request.genesis.rootInodeType,
          rootMode: request.genesis.rootMode,
          rootBirthtimeMs: request.genesis.rootBirthtimeMs,
          rootMtimeMs: request.genesis.rootMtimeMs,
          rootCtimeMs: request.genesis.rootCtimeMs,
          rootToken: request.genesis.rootToken,
        }
      : null,
    genesisRows: request.genesis ? request.genesis.rows : [],
    now: Date.now(),
  });
}

export {
  randomSessionId,
  replicationOwnerNonceDigest,
  assertNotError,
  encodeCanonicalEnvelope,
  decodeCanonicalEnvelope,
  encodeCanonicalBatchAcknowledgement,
  createCanonicalBatchAcknowledgement,
  createCanonicalBatch,
  batchEnvelopeDigest,
  receiptChainDigest,
  authorizeExchangeImpl as authorizeExchange,
  ACK_MAX_BYTES,
  PRE_NEGOTIATION_BYTES,
};

function authorizeExchangeImpl(
  authorization: AuthorizedReplicationPeer,
  peer: CanonicalAuthorizationRecord,
): void {
  const expected = authorizationDigest(
    canonicalRecord(authorization, peer.effectiveLimits),
  );
  if (!equalBytes(expected, authorizationDigest(peer)))
    throw new ReplicationError(
      "UnauthorizedScope",
      "peer authorization record does not match the authenticated scope",
    );
}

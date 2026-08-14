import { ReplicationError } from "./errors.js";
import { negotiateReplicationSession, requiredRoles } from "./authorization.js";
import type {
  AuthorizedReplicationPeer,
  CanonicalReplicationEnvelope,
  ReplicationBatch,
  ReplicationBatchRecord,
  ReplicationCapabilities,
  ReplicationCursorBinding,
  ReplicationPlan,
} from "./types.js";
import {
  createCanonicalBatch,
  encodeCanonicalEnvelope,
  decodeCanonicalEnvelope,
  replicationOwnerNonceDigest,
  batchEnvelopeDigest,
  authorizationDigest,
  effectiveLimitsDigest,
  createCanonicalBatchAcknowledgement,
  encodeCanonicalBatchAcknowledgement,
  receiptChainDigest,
  nextSessionCursor,
} from "./wire.js";
import {
  encodeActivationRequest,
  encodeActivationResult,
  decodeActivationResult,
  encodeGenesisFragment,
} from "@ephemeralai/fs/integrations/replication";
import type {
  ReplicationFilesystemBridge,
  ReplicationSessionBinding,
  ReplicationSessionSnapshot,
} from "@ephemeralai/fs/integrations/replication";
import {
  canonicalRecord,
  nextPhaseFor,
  randomSessionId,
  destinationOperationId,
  assertNotError,
  capabilitiesFromBridge,
  initialSessionCursor,
  createReplicationEndpoint,
  type ReplicationActivation,
  type ReplicatedAuthorityResult,
  type ReplicationEndpoint,
  type ReplicationResult,
  type ReplicationRunResult,
  type ReplicateOptions,
  type ReplicationTransport,
} from "./endpoint.js";
import { randomBytes, createHash } from "node:crypto";
import { equalBytes } from "./wire.js";

const PRE_NEGOTIATION_BYTES = 64 * 1024;
const ACK_MAX_BYTES = 64 * 1024;

interface DriverState {
  readonly bridge: ReplicationFilesystemBridge;
  readonly transport: ReplicationTransport;
  readonly endpoint: ReplicationEndpoint;
  readonly authorization: AuthorizedReplicationPeer;
  readonly plan: ReplicationPlan;
  readonly operationId: string;
  readonly sessionId: string;
  readonly ownerNonce: Uint8Array;
  readonly negotiated: import("./authorization.js").NegotiatedReplicationSession;
  readonly binding: ReplicationSessionBinding;
  session: ReplicationSessionSnapshot;
  selectedRootInode: string;
  selectedRootGeneration: number;
  selectedAllocationSequence: number;
  sharedCursorDigest: Uint8Array;
  transferredBytes: number;
  reusedBytes: number;
  terminalState: 0 | 1 | 2;
  terminalResult: {
    readonly operationId: string;
    readonly resultBytes: Uint8Array;
  } | null;
}

function hashBytes(bytes: Uint8Array): Uint8Array {
  return createHash("sha256").update(bytes).digest();
}

function bytesToHex(bytes: Uint8Array): string {
  let output = "";
  for (const byte of bytes) output += byte.toString(16).padStart(2, "0");
  return output;
}

/** Read the stable state byte from the core-owned branch fragment envelope. */
function branchGenerationState(bytes: Uint8Array): 0 | 1 | 2 {
  let offset = 0;
  if (bytes[offset++] !== 1)
    throw new ReplicationError(
      "IntegrityFailure",
      "branch fragment version is invalid",
    );
  const skipText = (name: string): void => {
    if (offset + 4 > bytes.byteLength)
      throw new ReplicationError("IntegrityFailure", `${name} is truncated`);
    const length = new DataView(bytes.buffer, bytes.byteOffset + offset, 4).getUint32(
      0,
      false,
    );
    offset += 4 + length;
    if (offset > bytes.byteLength)
      throw new ReplicationError("IntegrityFailure", `${name} is truncated`);
  };
  skipText("branch fragment id");
  skipText("branch fragment base");
  if (offset + 8 + 32 + 1 > bytes.byteLength)
    throw new ReplicationError(
      "IntegrityFailure",
      "branch fragment header is truncated",
    );
  offset += 8 + 32;
  const skipOptional = (name: string, width: number): void => {
    if (offset >= bytes.byteLength)
      throw new ReplicationError("IntegrityFailure", `${name} is truncated`);
    const tag = bytes[offset++];
    if (tag === 0) return;
    if (tag !== 1 || offset + width > bytes.byteLength)
      throw new ReplicationError("IntegrityFailure", `${name} is invalid`);
    offset += width;
  };
  skipOptional("branch predecessor generation", 8);
  skipOptional("branch predecessor digest", 32);
  if (offset >= bytes.byteLength)
    throw new ReplicationError(
      "IntegrityFailure",
      "branch fragment state is truncated",
    );
  const state = bytes[offset];
  if (state !== 0 && state !== 1 && state !== 2)
    throw new ReplicationError("IntegrityFailure", "branch fragment state is invalid");
  return state;
}

function randomCursor(): Uint8Array {
  return randomBytes(32);
}

async function exchange(
  state: DriverState,
  envelope: CanonicalReplicationEnvelope,
  maxBytes: number,
): Promise<CanonicalReplicationEnvelope> {
  const request = encodeCanonicalEnvelope(envelope);
  let responseBytes: Uint8Array;
  try {
    responseBytes = await state.transport.exchange(request);
  } catch (error) {
    if (error instanceof ReplicationError) throw error;
    throw new ReplicationError(
      "TransportFailure",
      "replication transport exchange failed",
      {
        cause: error,
      },
    );
  }
  const response = decodeCanonicalEnvelope(responseBytes, { maxBytes });
  assertNotError(response);
  return response;
}

async function sendBatchAndAck(
  state: DriverState,
  batch: ReplicationBatch,
): Promise<import("./types.js").ReplicationBatchAcknowledgement> {
  const response = await exchange(
    state,
    { kind: "batch", value: batch },
    state.negotiated.limits.maxResponseBytes,
  );
  if (response.kind !== "batch-acknowledgement")
    throw new ReplicationError(
      "ProtocolMismatch",
      "peer did not return a batch acknowledgement",
    );
  const ack = response.value;
  if (
    ack.sessionId !== batch.sessionId ||
    ack.sequence !== batch.sequence ||
    ack.phase !== batch.phase ||
    !equalBytes(ack.batchEnvelopeDigest, batchEnvelopeDigest(batch))
  )
    throw new ReplicationError(
      "BatchReplayMismatch",
      "peer acknowledgement does not bind the complete request envelope",
    );
  if (ack.cursor.byteLength < 16 || ack.cursor.byteLength > 256)
    throw new ReplicationError(
      "ProtocolMismatch",
      "peer acknowledgement cursor is outside the canonical envelope",
    );
  return ack;
}

/** Record an outbound batch on the local durable session, then send it. */
async function sendBatch(
  state: DriverState,
  batch: ReplicationBatch,
): Promise<import("./types.js").ReplicationBatchAcknowledgement> {
  state.session = await state.bridge.recordOutboundBatch({
    operationId: state.operationId,
    sessionId: state.sessionId,
    ownerNonce: state.ownerNonce,
    sequence: batch.sequence,
    phase: batch.phase,
    nextPhase: nextPhaseFor(batch),
    nextCursor: nextSessionCursor(
      state.session.cursorDigest,
      batchEnvelopeDigest(batch),
    ),
    nextCursorDigest: createHash("sha256")
      .update(nextSessionCursor(state.session.cursorDigest, batchEnvelopeDigest(batch)))
      .digest(),
  });
  state.endpoint.updateLocalSession(state.sessionId, state.session);
  const ack = await sendBatchAndAck(state, batch);
  state.sharedCursorDigest = ack.cursorDigest;
  return ack;
}

function buildBinding(options: {
  readonly operationId: string;
  readonly sessionId: string;
  readonly resumeKey: Uint8Array;
  readonly ownerNonce: Uint8Array;
  readonly bridge: ReplicationFilesystemBridge;
  readonly authorization: AuthorizedReplicationPeer;
  readonly plan: ReplicationPlan;
  readonly negotiated: import("./authorization.js").NegotiatedReplicationSession;
}): ReplicationSessionBinding {
  const mine = options.bridge.capabilities;
  const roles = requiredRoles(options.plan);
  if (mine.role !== roles.source)
    throw new ReplicationError(
      "UnauthorizedScope",
      "source runtime role does not authorize the selected flow",
    );
  const flow = options.plan.flow;
  return {
    operationId: options.operationId,
    sessionId: options.sessionId,
    resumeKey: options.resumeKey,
    ownerNonce: options.ownerNonce,
    flow,
    branchId: flow === "authority-main-to-replica" ? null : options.plan.branchId,
    sourceFilesystemId: mine.filesystemId ?? options.authorization.expectedFilesystemId,
    destinationFilesystemId:
      mine.filesystemId ?? options.authorization.expectedFilesystemId,
    sourceRole: roles.source,
    destinationRole: roles.destination,
    sourceAuthorizationDigest: options.negotiated.sourceAuthorizationDigest,
    destinationAuthorizationDigest: options.negotiated.destinationAuthorizationDigest,
    sourceCapabilityDigest: options.negotiated.sourceCapabilityDigest,
    destinationCapabilityDigest: options.negotiated.destinationCapabilityDigest,
    effectiveLimitsDigest: effectiveLimitsDigest(options.negotiated.limits),
    maxBatchEntries: options.negotiated.limits.maxBatchEntries,
    maxBatchBytes: options.negotiated.limits.maxBatchBytes,
    maxRequestBytes: options.negotiated.limits.maxRequestBytes,
    maxResponseBytes: options.negotiated.limits.maxResponseBytes,
    maxBufferedBytes: options.negotiated.limits.maxBufferedBytes,
    maxInFlightBatches: options.negotiated.limits.maxInFlightBatches,
    maxConcurrentSessions: options.negotiated.limits.maxConcurrentSessions,
    maxCursorBytes: options.negotiated.limits.maxCursorBytes,
    maxReplicationSessionRows: options.negotiated.limits.maxReplicationSessionRows,
    maxReplicationMetadataBytes: options.negotiated.limits.maxReplicationMetadataBytes,
    maxReceiptsPerSession: options.negotiated.limits.maxReceiptsPerSession,
    maxReceiptBytesPerSession: options.negotiated.limits.maxReceiptBytesPerSession,
    maxStagingBytesPerSession: options.negotiated.limits.maxStagingBytesPerSession,
    maxAcknowledgementBytes: ACK_MAX_BYTES,
    maxTerminalResultBytes: options.negotiated.limits.maxTerminalResultBytes,
    maxCursorAgeMs: options.negotiated.limits.maxCursorAgeMs,
    stagingLeaseMs: options.negotiated.limits.stagingLeaseMs,
    maxRetryAttempts: options.negotiated.limits.maxRetryAttempts,
    maxRetryElapsedMs: options.negotiated.limits.maxRetryElapsedMs,
    minRetryDelayMs: options.negotiated.limits.minRetryDelayMs,
    maxRetryDelayMs: options.negotiated.limits.maxRetryDelayMs,
    resultRetentionMs: options.negotiated.limits.resultRetentionMs,
  };
}

const BINDING_SCALARS = [
  "operationId",
  "sessionId",
  "flow",
  "branchId",
  "sourceFilesystemId",
  "destinationFilesystemId",
  "sourceRole",
  "destinationRole",
  "maxBatchEntries",
  "maxBatchBytes",
  "maxRequestBytes",
  "maxResponseBytes",
  "maxBufferedBytes",
  "maxInFlightBatches",
  "maxConcurrentSessions",
  "maxCursorBytes",
  "maxReplicationSessionRows",
  "maxReplicationMetadataBytes",
  "maxReceiptsPerSession",
  "maxReceiptBytesPerSession",
  "maxStagingBytesPerSession",
  "maxAcknowledgementBytes",
  "maxTerminalResultBytes",
  "maxCursorAgeMs",
  "stagingLeaseMs",
  "maxRetryAttempts",
  "maxRetryElapsedMs",
  "minRetryDelayMs",
  "maxRetryDelayMs",
  "resultRetentionMs",
] as const satisfies readonly (keyof ReplicationSessionBinding)[];

const BINDING_BYTES = [
  "resumeKey",
  "ownerNonce",
  "sourceAuthorizationDigest",
  "destinationAuthorizationDigest",
  "sourceCapabilityDigest",
  "destinationCapabilityDigest",
  "effectiveLimitsDigest",
] as const satisfies readonly (keyof ReplicationSessionBinding)[];

function bindingMatches(
  left: ReplicationSessionBinding,
  right: ReplicationSessionBinding,
): boolean {
  for (const name of BINDING_SCALARS) if (left[name] !== right[name]) return false;
  for (const name of BINDING_BYTES)
    if (!equalBytes(left[name], right[name])) return false;
  return true;
}

export async function replicate(
  options: ReplicateOptions,
): Promise<ReplicationRunResult> {
  const { bridge, transport, authorization, plan, operationId, signal } = options;
  const destinationAuthorization = options.destinationAuthorization ?? authorization;
  let existing: Awaited<ReturnType<ReplicationFilesystemBridge["findSession"]>> | null =
    null;
  let sessionId = randomSessionId();
  let resumeKey: Uint8Array = options.resumeKey ?? randomBytes(32);
  let ownerNonce: Uint8Array = randomBytes(16);
  let endpoint: ReplicationEndpoint | undefined;

  let peerCapabilities: ReplicationCapabilities;
  let retryNegotiated:
    import("./authorization.js").NegotiatedReplicationSession | undefined;
  const attemptStartedAt = performance?.now() ?? Date.now();
  const skeleton = (
    negotiated: import("./authorization.js").NegotiatedReplicationSession | null,
  ): DriverState => ({
    bridge,
    transport,
    endpoint: endpoint!,
    authorization,
    plan,
    operationId,
    sessionId,
    ownerNonce,
    negotiated: negotiated as never,
    binding: null as never,
    session: null as never,
    selectedRootInode: "",
    selectedRootGeneration: 0,
    selectedAllocationSequence: 1,
    sharedCursorDigest: new Uint8Array(32),
    transferredBytes: 0,
    reusedBytes: 0,
    terminalState: 0,
    terminalResult: null,
  });
  try {
    const capsResponse = await exchange(
      skeleton(null),
      { kind: "capabilities", value: capabilitiesFromBridge(bridge.capabilities) },
      PRE_NEGOTIATION_BYTES,
    );
    if (capsResponse.kind !== "capabilities")
      throw new ReplicationError(
        "ProtocolMismatch",
        "peer did not return capabilities",
      );
    peerCapabilities = capsResponse.value;

    const provisional = negotiateReplicationSession({
      source: capabilitiesFromBridge(bridge.capabilities),
      destination: peerCapabilities,
      sourceAuthorization: authorization,
      destinationAuthorization,
      plan,
    });
    const myRecord = canonicalRecord(authorization, provisional.limits);
    const authResponse = await exchange(
      skeleton(provisional),
      { kind: "authorization", value: myRecord },
      PRE_NEGOTIATION_BYTES,
    );
    if (authResponse.kind !== "authorization")
      throw new ReplicationError(
        "ProtocolMismatch",
        "peer did not return its authorization record",
      );
    if (
      options.destinationAuthorization !== undefined &&
      !equalBytes(
        authorizationDigest(
          canonicalRecord(
            options.destinationAuthorization,
            authResponse.value.effectiveLimits,
          ),
        ),
        authorizationDigest(authResponse.value),
      )
    )
      throw new ReplicationError(
        "UnauthorizedScope",
        "peer authorization record does not match the authenticated destination scope",
      );
    const negotiatedDestinationAuthorization = authResponse.value.authorization;

    const negotiated = negotiateReplicationSession({
      source: capabilitiesFromBridge(bridge.capabilities),
      destination: peerCapabilities,
      sourceAuthorization: authorization,
      destinationAuthorization: negotiatedDestinationAuthorization,
      plan,
    });
    retryNegotiated = negotiated;
    existing = options.resumeKey
      ? await bridge
          .findSession({ operationId, resumeKey: options.resumeKey })
          .catch((error: unknown) => {
            if (
              error instanceof Error &&
              error.message.startsWith(
                "OperationMismatch: replication operation is unknown",
              )
            )
              return null;
            throw error;
          })
      : null;
    if (existing) {
      sessionId = existing.binding.sessionId;
      resumeKey = existing.binding.resumeKey;
      ownerNonce = existing.binding.ownerNonce;
    }
    const proposedBinding = buildBinding({
      operationId,
      sessionId,
      resumeKey,
      ownerNonce,
      bridge,
      authorization,
      plan,
      negotiated,
    });
    if (existing && !bindingMatches(existing.binding, proposedBinding))
      throw new ReplicationError(
        "UnauthorizedScope",
        "replication resume binding changed after authenticated negotiation",
      );
    if (existing?.session.terminal) {
      const resultBytes = await bridge.replayTerminalResult({
        operationId,
        sessionId,
        resumeKey,
        now: Date.now(),
      });
      const decoded = decodeActivationResult(resultBytes);
      const replayPlan: ReplicationPlan =
        proposedBinding.flow === "authority-main-to-replica"
          ? { flow: "authority-main-to-replica" }
          : { flow: proposedBinding.flow, branchId: proposedBinding.branchId ?? "" };
      return {
        status: "complete",
        result: {
          sessionId,
          operationId,
          plan: replayPlan,
          activation: activationFromDecoded(decoded),
          finalCursor: bytesToHex(existing.session.cursor),
          transferredBytes: existing.session.acceptedBytes,
          reusedBytes: 0,
        },
      };
    }
    endpoint = createReplicationEndpoint({ bridge, authorization });
    // A restart resumes the exact durable binding. Passing the persisted
    // owner nonce, session id, and opaque resume key back through the core
    // lets it reject any changed authorization, plan, profile, or limits.
    const binding = existing
      ? Object.freeze({
          ...proposedBinding,
          sessionId: existing.binding.sessionId,
          resumeKey: existing.binding.resumeKey,
          ownerNonce: existing.binding.ownerNonce,
        })
      : proposedBinding;
    const state: DriverState = skeleton(negotiated);
    const initialCursor = initialSessionCursor(sessionId);
    const sessionNow = Date.now();
    const created = await bridge.createOrResumeSession({
      binding,
      phase: "content-offer",
      cursor: initialCursor,
      cursorDigest: createHash("sha256").update(initialCursor).digest(),
      now: sessionNow,
      expiresAtMs: sessionNow + negotiated.limits.maxCursorAgeMs,
    });
    state.session = created.session;
    state.sharedCursorDigest = created.session.cursorDigest;
    endpoint.bindLocalSession({
      sessionId,
      operationId,
      ownerNonce,
      binding,
      session: created.session,
      negotiated,
    });

    const provisioning = peerCapabilities.provisioningState === "unbound-replica";
    let exportSelection: Awaited<
      ReturnType<ReplicationFilesystemBridge["captureExport"]>
    > | null = null;
    let genesisCapture: Awaited<
      ReturnType<ReplicationFilesystemBridge["captureGenesis"]>
    > | null = null;
    if (provisioning) {
      genesisCapture = await bridge.captureGenesis({ sessionId, now: Date.now() });
      state.selectedRootInode = genesisCapture.meta.rootInode;
      state.selectedRootGeneration = genesisCapture.meta.rootMutationGeneration;
      state.selectedAllocationSequence = genesisCapture.meta.nextAllocationSequence;
    } else {
      exportSelection = await bridge.captureExport({
        sessionId,
        flow: plan.flow,
        branchId: plan.flow === "authority-main-to-replica" ? null : plan.branchId,
        // The destination's main head is not part of the capability row for
        // branch flows.  Capture the branch against its exact base; the
        // destination finalizer performs the authoritative base-presence and
        // divergence check after the main prefix has been verified.
        destinationHead:
          plan.flow === "authority-main-to-replica" ? 0 : Number.MAX_SAFE_INTEGER,
        now: Date.now(),
      });
      state.selectedRootInode = exportSelection.rootInode;
      state.selectedRootGeneration = exportSelection.rootMutationGeneration;
      state.selectedAllocationSequence = exportSelection.nextAllocationSequence;
    }

    const bindingValue: ReplicationCursorBinding = Object.freeze({
      sessionId,
      ownerNonceDigest: replicationOwnerNonceDigest(ownerNonce),
      sourceFilesystemId: binding.sourceFilesystemId,
      destinationFilesystemId: binding.destinationFilesystemId,
      plan,
      selectedIdentity: provisioning
        ? authorization.expectedFilesystemId
        : plan.flow === "authority-main-to-replica"
          ? String(exportSelection!.selectedRevision)
          : plan.branchId,
      selectedGeneration: exportSelection?.selectedGeneration ?? null,
      phase: "content-offer",
      nextSequence: state.session.nextSequence,
      capabilityDigest: negotiated.sourceCapabilityDigest,
    });
    const cursorResponse = await exchange(
      state,
      { kind: "cursor", value: bindingValue },
      PRE_NEGOTIATION_BYTES,
    );
    if (cursorResponse.kind !== "cursor")
      throw new ReplicationError(
        "ProtocolMismatch",
        "peer did not return its cursor binding",
      );
    validatePeerCursor(cursorResponse.value, bindingValue);

    if (provisioning) {
      await runProvisioning(state, peerCapabilities, genesisCapture!);
    } else if (plan.flow === "authority-main-to-replica") {
      await runMain(state, peerCapabilities, exportSelection!.selectedRevision);
    } else {
      await runBranch(state, peerCapabilities);
    }

    const activation = await buildActivationResult(state, plan);
    let resultBytes: Uint8Array | undefined;
    if (state.session.phase === "result-acknowledgement") {
      try {
        resultBytes = await bridge.replayTerminalResult({
          operationId,
          sessionId,
          resumeKey,
          now: Date.now(),
        });
      } catch (error) {
        if (
          !(error instanceof Error) ||
          !error.message.startsWith(
            "OperationMismatch: terminal result is not available",
          )
        )
          throw error;
      }
    }
    resultBytes ??= encodeActivationResult(toTransferActivation(activation));
    if (state.session.phase !== "result-acknowledgement" || resultBytes !== undefined)
      await bridge.storeTerminalResult({
        operationId,
        sessionId,
        ownerNonce,
        result: resultBytes,
        now: Date.now(),
      });
    const resultRecord: ReplicationBatchRecord = {
      kind: "terminal-result",
      operationId,
      branchId: plan.flow === "authority-main-to-replica" ? null : plan.branchId,
      generation: null,
      generationDigest: null,
      resultDigest: hashBytes(resultBytes),
      resultBytes,
    };
    const ackBatch = createCanonicalBatch({
      sessionId,
      plan,
      phase: "result-acknowledgement",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: [resultRecord],
    });
    await sendBatch(state, ackBatch);
    await bridge.releaseExport({ sessionId, now: Date.now() });
    await endpoint!.close();
    return {
      status: "complete",
      result: {
        sessionId,
        operationId,
        plan,
        activation,
        finalCursor: bytesToHex(state.session.cursor),
        transferredBytes: state.transferredBytes,
        reusedBytes: state.reusedBytes,
      },
    };
  } catch (error) {
    await endpoint?.close();
    if (error instanceof ReplicationError && isRetryable(error.code)) {
      if (retryNegotiated === undefined) throw error;
      let exhausted = false;
      try {
        const accounting = await bridge.consumeAttempt({
          operationId,
          sessionId,
          ownerNonce,
          wallNowMs: Date.now(),
          monotonicElapsedMs: Math.max(
            0,
            Math.ceil((performance?.now() ?? Date.now()) - attemptStartedAt),
          ),
          delayMs: retryNegotiated.limits.minRetryDelayMs,
        });
        exhausted = accounting.exhausted;
      } catch {
        // The session may not exist yet; the attempt budget is durable once it does.
      }
      if (exhausted) {
        await bridge.abortSession({
          operationId,
          sessionId,
          ownerNonce,
          now: Date.now(),
        });
        throw new ReplicationError(
          "RetryExhausted",
          "durable replication retry budget is exhausted",
        );
      }
      return {
        status: "pending",
        resumeKey,
        notBeforeMs: Date.now() + retryNegotiated.limits.minRetryDelayMs,
        reason: error.code === "Busy" ? "busy" : "transport",
      };
    }
    throw error;
  }
}

function activationFromDecoded(
  decoded: import("@ephemeralai/fs/integrations/replication").TransferActivationResult,
): ReplicationActivation {
  if (decoded.kind === 0) return { kind: "main", revision: decoded.revision };
  const authorityResult = decoded.authorityResult
    ? decoded.authorityResult.kind === "publication"
      ? {
          kind: "publication" as const,
          operationId: decoded.authorityResult.operationId,
          outcome: decoded.authorityResult.outcome,
          resultDigest: bytesToHex(decoded.authorityResult.resultDigest),
        }
      : {
          kind: "discard" as const,
          operationId: decoded.authorityResult.operationId,
          resultDigest: bytesToHex(decoded.authorityResult.resultDigest),
        }
    : null;
  return {
    kind: "branch",
    branchId: decoded.branchId ?? "",
    baseRevision: decoded.baseRevision ?? "0",
    generation: decoded.generation,
    generationDigest: decoded.generationDigest
      ? bytesToHex(decoded.generationDigest)
      : "",
    state:
      decoded.state === 0 ? "active" : decoded.state === 1 ? "merged" : "discarded",
    authorityResult,
  };
}

async function runProvisioning(
  state: DriverState,
  peerCapabilities: ReplicationCapabilities,
  genesis: Awaited<ReturnType<ReplicationFilesystemBridge["captureGenesis"]>>,
): Promise<void> {
  const { bridge, sessionId } = state;
  await runContentNegotiation(state);
  await runStateTransfer(state);
  if (state.session.phase !== "activation") return;
  const summary = await bridge.exportSummary({
    sessionId,
    flow: "authority-main-to-replica",
  });
  const genesisFragment = encodeGenesisFragment({
    filesystemId: genesis.meta.filesystemId,
    rootInode: genesis.meta.rootInode,
    mainRevision: genesis.meta.mainRevision,
    rootMutationGeneration: genesis.meta.rootMutationGeneration,
    nextAllocationSequence: genesis.meta.nextAllocationSequence,
    cowPageBytes: genesis.meta.cowPageBytes,
    createdAtMs: genesis.meta.createdAtMs,
    maxManifestEntries: genesis.meta.maxManifestEntries,
    maxManifestDepth: genesis.meta.maxManifestDepth,
    maxFileBytes: genesis.meta.maxFileBytes,
    writerProfile: genesis.meta.writerProfile,
    manifestFormat: genesis.meta.manifestFormat,
    chunkerFormat: genesis.meta.chunkerFormat,
    fastCdcMinimum: genesis.meta.fastCdcMinimum,
    fastCdcAverage: genesis.meta.fastCdcAverage,
    fastCdcMaximum: genesis.meta.fastCdcMaximum,
    rootInodeType: genesis.meta.rootInodeType,
    rootMode: genesis.meta.rootMode,
    rootBirthtimeMs: genesis.meta.rootBirthtimeMs,
    rootMtimeMs: genesis.meta.rootMtimeMs,
    rootCtimeMs: genesis.meta.rootCtimeMs,
    rootToken: genesis.meta.rootToken,
    rows: [],
  });
  const activationRequest = encodeActivationRequest({
    kind: 2,
    expectedRevision: 0,
    expectedRootMutationGeneration: genesis.meta.rootMutationGeneration,
    expectedNextAllocationSequence: genesis.meta.nextAllocationSequence,
    expectedRootInode: genesis.meta.rootInode,
    expectedRevisionCount: 1,
    expectedStateRows: summary.stateRows,
    expectedClosureRoots: 0,
    expectedClosureNodes: 0,
    expectedClosureObjects: 0,
    expectedClosureObjectBytes: 0,
    checkpoint: false,
    branchId: null,
    baseRevision: null,
    generation: null,
    generationDigest: null,
    terminalState: 0,
    terminalResultOperationId: null,
    terminalResultBytes: null,
    genesis: {
      filesystemId: genesis.meta.filesystemId,
      rootInode: genesis.meta.rootInode,
      mainRevision: genesis.meta.mainRevision,
      rootMutationGeneration: genesis.meta.rootMutationGeneration,
      nextAllocationSequence: genesis.meta.nextAllocationSequence,
      cowPageBytes: genesis.meta.cowPageBytes,
      createdAtMs: genesis.meta.createdAtMs,
      maxManifestEntries: genesis.meta.maxManifestEntries,
      maxManifestDepth: genesis.meta.maxManifestDepth,
      maxFileBytes: genesis.meta.maxFileBytes,
      writerProfile: genesis.meta.writerProfile,
      manifestFormat: genesis.meta.manifestFormat,
      chunkerFormat: genesis.meta.chunkerFormat,
      fastCdcMinimum: genesis.meta.fastCdcMinimum,
      fastCdcAverage: genesis.meta.fastCdcAverage,
      fastCdcMaximum: genesis.meta.fastCdcMaximum,
      rootInodeType: genesis.meta.rootInodeType,
      rootMode: genesis.meta.rootMode,
      rootBirthtimeMs: genesis.meta.rootBirthtimeMs,
      rootMtimeMs: genesis.meta.rootMtimeMs,
      rootCtimeMs: genesis.meta.rootCtimeMs,
      rootToken: genesis.meta.rootToken,
      rows: [],
    },
  });
  await sendActivation(state, activationRequest);
  void peerCapabilities;
}

async function runMain(
  state: DriverState,
  peerCapabilities: ReplicationCapabilities,
  selectedRevision: number,
): Promise<void> {
  const { bridge, sessionId, plan } = state;
  await runContentNegotiation(state);
  await runStateTransfer(state);
  if (state.session.phase !== "activation") return;
  const summary = await bridge.exportSummary({ sessionId, flow: plan.flow });
  const activationRequest = encodeActivationRequest({
    kind: 0,
    expectedRevision: summary.selectedRevision,
    expectedRootMutationGeneration: state.selectedRootGeneration,
    expectedNextAllocationSequence: state.selectedAllocationSequence,
    expectedRootInode: state.selectedRootInode,
    expectedRevisionCount: summary.selectedRevision - summary.baseRevision,
    expectedStateRows: summary.stateRows,
    expectedClosureRoots: summary.rootCount,
    expectedClosureNodes: summary.nodeCount,
    expectedClosureObjects: summary.objectCount,
    expectedClosureObjectBytes: summary.objectBytes,
    checkpoint: false,
    branchId: null,
    baseRevision: null,
    generation: null,
    generationDigest: null,
    terminalState: 0,
    terminalResultOperationId: null,
    terminalResultBytes: null,
    genesis: null,
  });
  await sendActivation(state, activationRequest);
  void peerCapabilities;
  void selectedRevision;
}

async function runBranch(
  state: DriverState,
  peerCapabilities: ReplicationCapabilities,
): Promise<void> {
  const { bridge, sessionId, plan, negotiated } = state;
  const branchId = plan.flow === "authority-main-to-replica" ? null : plan.branchId;
  if (!branchId)
    throw new ReplicationError("ProtocolMismatch", "branch flow requires a branchId");
  await runContentNegotiation(state);
  if (state.session.phase !== "state-transfer") {
    if (
      state.session.phase === "content-offer" ||
      state.session.phase === "missing-content" ||
      state.session.phase === "content-transfer"
    )
      throw new ReplicationError(
        "CursorMismatch",
        "branch transfer did not reach state-transfer",
      );
  }
  const isReturn =
    plan.flow === "replica-branch-to-authority" ||
    plan.flow === "replica-branch-to-replica";
  let terminalResult: Awaited<
    ReturnType<ReplicationFilesystemBridge["readExportStateBatch"]>
  >["terminalResult"] = null;
  let complete = state.session.phase !== "state-transfer";
  while (!complete) {
    const stateBatch = await bridge.readExportStateBatch({
      sessionId,
      flow: plan.flow,
      branchId,
      maxEntries: negotiated.limits.maxBatchEntries,
      maxBytes: negotiated.limits.maxBatchBytes,
      now: Date.now(),
      checkpoint: false,
      allowTerminal: !isReturn,
    });
    terminalResult = stateBatch.terminalResult ?? terminalResult;
    for (const record of stateBatch.records) {
      if (record.kind === "branch-generation-fragment")
        state.terminalState = branchGenerationState(record.fragmentBytes);
    }
    if (stateBatch.terminalResult !== null) {
      state.terminalResult = {
        operationId: stateBatch.terminalResult.operationId,
        resultBytes: stateBatch.terminalResult.resultBytes,
      };
    }
    const batch = createCanonicalBatch({
      sessionId,
      plan,
      phase: "state-transfer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: stateBatch.records as ReplicationBatchRecord[],
    });
    await sendBatch(state, batch);
    complete = stateBatch.complete;
  }
  if (state.session.phase === "state-transfer") {
    const marker = createCanonicalBatch({
      sessionId,
      plan,
      phase: "state-transfer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: [],
    });
    await sendBatch(state, marker);
  }
  if (state.session.phase !== "activation") return;
  const summary = await bridge.exportSummary({ sessionId, flow: plan.flow });
  const activationRequest = encodeActivationRequest({
    kind: 1,
    expectedRevision: summary.baseRevision,
    expectedRootMutationGeneration: state.selectedRootGeneration,
    expectedNextAllocationSequence: state.selectedAllocationSequence,
    expectedRootInode: state.selectedRootInode,
    expectedRevisionCount: 0,
    expectedStateRows: summary.stateRows,
    expectedClosureRoots: summary.rootCount,
    expectedClosureNodes: summary.nodeCount,
    expectedClosureObjects: summary.objectCount,
    expectedClosureObjectBytes: summary.objectBytes,
    checkpoint: false,
    branchId,
    baseRevision: String(summary.baseRevision),
    generation: summary.selectedGeneration,
    generationDigest: summary.generationDigest,
    terminalState: 0,
    terminalResultOperationId: terminalResult?.operationId ?? null,
    terminalResultBytes: terminalResult ? terminalResult.resultBytes : null,
    genesis: null,
  });
  await sendActivation(state, activationRequest);
  void peerCapabilities;
}

async function runContentNegotiation(state: DriverState): Promise<void> {
  const { bridge, sessionId, plan, negotiated } = state;
  const maxEntries = negotiated.limits.maxBatchEntries;
  const maxBytes = negotiated.limits.maxBatchBytes;
  // Durable outbound cursors make a nonterminal restart resumable. If the
  // offer marker was already committed, replay starts at missing-content;
  // if the missing-content response was committed, it starts at transfer.
  let offersComplete = state.session.phase !== "content-offer";
  let offered = 0;
  let offeredContentBytes = 0;
  let requestedContentBytes = 0;
  const offeredSizes = new Map<string, number>();
  while (!offersComplete) {
    const offer = await bridge.readExportBatch({
      sessionId,
      flow: plan.flow,
      branchId: plan.flow === "authority-main-to-replica" ? null : plan.branchId,
      maxEntries,
      maxBytes,
      now: Date.now(),
    });
    if (offer.records.length === 0) {
      offersComplete = true;
      break;
    }
    offered += offer.records.length;
    for (const record of offer.records) {
      if (record.kind === "object-descriptor") {
        offeredContentBytes += record.byteLength;
        offeredSizes.set(bytesToHex(record.digest), record.byteLength);
      } else if (record.kind === "manifest-root-descriptor") {
        offeredContentBytes += record.encodedLength;
        offeredSizes.set(bytesToHex(record.digest), record.encodedLength);
      } else if (record.kind === "manifest-node-descriptor") {
        offeredContentBytes += record.encodedLength;
        offeredSizes.set(bytesToHex(record.digest), record.encodedLength);
      }
    }
    const batch = createCanonicalBatch({
      sessionId,
      plan,
      phase: "content-offer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: offer.records as ReplicationBatchRecord[],
    });
    await sendBatch(state, batch);
    offersComplete = offer.complete;
  }
  if (state.session.phase === "content-offer") {
    const marker = createCanonicalBatch({
      sessionId,
      plan,
      phase: "content-offer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: [],
    });
    await sendBatch(state, marker);
  }
  if (
    state.session.phase !== "missing-content" &&
    state.session.phase !== "content-transfer"
  )
    return;
  while (true) {
    const requestBatch = createCanonicalBatch({
      sessionId,
      plan,
      phase: "missing-content",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: [],
    });
    const response = await exchange(
      state,
      { kind: "batch", value: requestBatch },
      negotiated.limits.maxResponseBytes,
    );
    if (response.kind !== "batch")
      throw new ReplicationError(
        "ProtocolMismatch",
        "peer did not return its missing-content batch",
      );
    const missingBatch = response.value;
    if (
      missingBatch.sessionId !== sessionId ||
      missingBatch.sequence !== requestBatch.sequence ||
      missingBatch.phase !== "missing-content"
    )
      throw new ReplicationError(
        "CursorMismatch",
        "peer missing-content batch does not match the request",
      );
    const localAck = await acceptLocalBatch(state, missingBatch);
    state.session = localAck.session;
    state.sharedCursorDigest = localAck.ack.cursorDigest;
    const requested = missingBatch.records
      .filter(
        (
          record,
        ): record is Extract<ReplicationBatchRecord, { kind: "missing-content" }> =>
          record.kind === "missing-content",
      )
      .map((record) => ({
        contentKind: record.contentKind,
        digest: record.digest,
      }));
    // Missing-content records carry only the digest.  Resolve their declared
    // sizes from the bounded descriptor offers so the result reports reused
    // immutable bytes without retaining payloads or duplicating envelopes.
    requestedContentBytes += requested.reduce(
      (sum, record) => sum + (offeredSizes.get(bytesToHex(record.digest)) ?? 0),
      0,
    );
    if (requested.length === 0) break;
    const payloads = await bridge.readExportPayloads({
      sessionId,
      requested,
      maxEntries,
      maxBytes,
      now: Date.now(),
    });
    let transferred = 0;
    let current: ReplicationBatchRecord[] = [];
    let currentBytes = 0;
    for (const record of payloads.records) {
      const bytes = record.kind === "object-payload" ? record.byteLength : 64;
      if (current.length >= maxEntries || currentBytes + bytes > maxBytes) {
        await sendTransferBatch(state, current);
        current = [];
        currentBytes = 0;
      }
      current.push(record as ReplicationBatchRecord);
      currentBytes += bytes;
      if (record.kind === "object-payload") transferred += record.byteLength;
    }
    if (current.length > 0) await sendTransferBatch(state, current);
    state.transferredBytes += transferred;
    state.reusedBytes += Math.max(0, requested.length - payloads.records.length);
    if (missingBatch.records.length < maxEntries) break;
  }
  state.reusedBytes += Math.max(0, offeredContentBytes - requestedContentBytes);
  if (state.session.phase === "content-transfer") {
    const marker = createCanonicalBatch({
      sessionId,
      plan,
      phase: "content-transfer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: [],
    });
    await sendBatch(state, marker);
  }
}

async function acceptLocalBatch(
  state: DriverState,
  batch: ReplicationBatch,
): Promise<{
  readonly ack: import("./types.js").ReplicationBatchAcknowledgement;
  readonly session: ReplicationSessionSnapshot;
}> {
  const nextPhase =
    batch.phase === "missing-content" ? "content-transfer" : nextPhaseFor(batch);
  const nextCursor = nextSessionCursor(
    state.session.cursorDigest,
    batchEnvelopeDigest(batch),
  );
  const chainDigest = receiptChainDigest(
    state.session.chainDigest,
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
    stagedBytes: state.session.stagedBytes,
  });
  const encodedAck = encodeCanonicalBatchAcknowledgement(acknowledgement);
  const outcome = await state.bridge.acceptBatch({
    operationId: state.operationId,
    sessionId: state.sessionId,
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
    stagedBytesDelta: 0,
    now: Date.now(),
  });
  return { ack: acknowledgement, session: outcome.session };
}

async function sendTransferBatch(
  state: DriverState,
  records: ReplicationBatchRecord[],
): Promise<void> {
  const batch = createCanonicalBatch({
    sessionId: state.sessionId,
    plan: state.plan,
    phase: "content-transfer",
    sequence: state.session.nextSequence,
    priorCursorDigest: state.sharedCursorDigest,
    records,
  });
  await sendBatch(state, batch);
}

async function runStateTransfer(state: DriverState): Promise<void> {
  const { bridge, sessionId, plan, negotiated } = state;
  if (state.session.phase !== "state-transfer") return;
  while (true) {
    const batchResult = await bridge.readExportStateBatch({
      sessionId,
      flow: plan.flow,
      branchId: plan.flow === "authority-main-to-replica" ? null : plan.branchId,
      maxEntries: negotiated.limits.maxBatchEntries,
      maxBytes: negotiated.limits.maxBatchBytes,
      now: Date.now(),
      checkpoint: false,
      allowTerminal: false,
    });
    if (batchResult.records.length === 0) break;
    const batch = createCanonicalBatch({
      sessionId,
      plan,
      phase: "state-transfer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: batchResult.records as ReplicationBatchRecord[],
    });
    await sendBatch(state, batch);
    if (batchResult.complete) break;
  }
  if (state.session.phase === "state-transfer") {
    const marker = createCanonicalBatch({
      sessionId,
      plan,
      phase: "state-transfer",
      sequence: state.session.nextSequence,
      priorCursorDigest: state.sharedCursorDigest,
      records: [],
    });
    await sendBatch(state, marker);
  }
}

async function sendActivation(
  state: DriverState,
  requestBytes: Uint8Array,
): Promise<void> {
  const requestRecord: ReplicationBatchRecord = {
    kind: "terminal-result",
    operationId: state.operationId,
    branchId:
      state.plan.flow === "authority-main-to-replica" ? null : state.plan.branchId,
    generation: null,
    generationDigest: null,
    resultDigest: hashBytes(requestBytes),
    resultBytes: requestBytes,
  };
  const batch = createCanonicalBatch({
    sessionId: state.sessionId,
    plan: state.plan,
    phase: "activation",
    sequence: state.session.nextSequence,
    priorCursorDigest: state.sharedCursorDigest,
    records: [requestRecord],
  });
  await sendBatch(state, batch);
}

function toTransferActivation(
  activation: ReplicationActivation,
): import("@ephemeralai/fs/integrations/replication").TransferActivationResult {
  if (activation.kind === "main") {
    return {
      kind: 0,
      revision: activation.revision,
      branchId: null,
      baseRevision: null,
      generation: 0,
      generationDigest: null,
      state: 0,
      authorityResult: null,
    };
  }
  const authorityResult = activation.authorityResult
    ? activation.authorityResult.kind === "publication"
      ? {
          kind: "publication" as const,
          operationId: activation.authorityResult.operationId,
          outcome: activation.authorityResult.outcome,
          resultDigest: hexBytes(activation.authorityResult.resultDigest),
        }
      : {
          kind: "discard" as const,
          operationId: activation.authorityResult.operationId,
          resultDigest: hexBytes(activation.authorityResult.resultDigest),
        }
    : null;
  return {
    kind: 1,
    revision: activation.baseRevision,
    branchId: activation.branchId,
    baseRevision: activation.baseRevision,
    generation: activation.generation,
    generationDigest:
      activation.generationDigest === "" ? null : hexBytes(activation.generationDigest),
    state: activation.state === "active" ? 0 : activation.state === "merged" ? 1 : 2,
    authorityResult,
  };
}

function hexBytes(value: string): Uint8Array {
  if (value.length % 2 !== 0 || !/^[0-9a-f]*$/u.test(value))
    throw new ReplicationError("ProtocolMismatch", "hex digest is invalid");
  const out = new Uint8Array(value.length / 2);
  for (let index = 0; index < out.length; index += 1)
    out[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return out;
}

async function buildActivationResult(
  state: DriverState,
  plan: ReplicationPlan,
): Promise<ReplicationActivation> {
  const { bridge, sessionId } = state;
  const summary = await bridge.exportSummary({ sessionId, flow: plan.flow });
  if (plan.flow === "authority-main-to-replica") {
    return { kind: "main", revision: String(summary.selectedRevision) };
  }
  return {
    kind: "branch",
    branchId: plan.branchId,
    baseRevision: String(summary.baseRevision),
    generation: summary.selectedGeneration ?? 0,
    generationDigest: summary.generationDigest
      ? bytesToHex(summary.generationDigest)
      : "",
    state:
      state.terminalState === 1
        ? "merged"
        : state.terminalState === 2
          ? "discarded"
          : "active",
    authorityResult: authorityResultFor(state),
  };
}

function authorityResultFor(state: DriverState): ReplicatedAuthorityResult | null {
  if (state.terminalState === 0 || state.terminalResult === null) return null;
  const resultDigest = bytesToHex(hashBytes(state.terminalResult.resultBytes));
  if (state.terminalState === 2)
    return { kind: "discard", operationId: null, resultDigest };
  let outcome: "merged" | "conflict" = "conflict";
  try {
    const value = JSON.parse(
      new TextDecoder().decode(state.terminalResult.resultBytes),
    ) as {
      readonly outcome?: unknown;
    };
    if (value.outcome === "merged" || value.outcome === 0) outcome = "merged";
  } catch {
    // Keep the result opaque; the destination finalizer authenticates it.
  }
  return {
    kind: "publication",
    operationId: state.terminalResult.operationId,
    outcome,
    resultDigest,
  };
}

function validatePeerCursor(
  received: ReplicationCursorBinding,
  sent: ReplicationCursorBinding,
): void {
  if (
    received.sessionId !== sent.sessionId ||
    received.sourceFilesystemId !== sent.sourceFilesystemId ||
    received.destinationFilesystemId !== sent.destinationFilesystemId ||
    received.plan.flow !== sent.plan.flow ||
    (sent.plan.flow !== "authority-main-to-replica" &&
      received.plan.flow !== "authority-main-to-replica" &&
      received.plan.branchId !== sent.plan.branchId)
  )
    throw new ReplicationError(
      "CursorMismatch",
      "peer cursor binding does not match the negotiated session",
    );
}

function isRetryable(code: string): boolean {
  return code === "Busy" || code === "TransportFailure";
}

export { destinationOperationId, ACK_MAX_BYTES, PRE_NEGOTIATION_BYTES };

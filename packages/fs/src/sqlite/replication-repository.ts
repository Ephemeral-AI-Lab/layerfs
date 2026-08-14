import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import {
  DURABLE_METADATA_ROW_BYTES,
  type StorageLimits,
} from "../resources/limits.js";
import { UsageRepository } from "./usage-repository.js";
import type {
  CreateReplicationSessionRequest,
  ReplicationBatchAcceptanceRequest,
  ReplicationFilesystemIdentity,
  ReplicationFlow,
  ReplicationPhase,
  ReplicationRole,
  ReplicationSessionBinding,
  ReplicationSessionSnapshot,
  ReplicationSessionStore,
} from "../filesystem/types.js";

interface DurableSessionState {
  readonly version: 1;
  readonly binding: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly resumeKey: string;
    readonly ownerNonce: string;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
    readonly sourceFilesystemId: string;
    readonly destinationFilesystemId: string;
    readonly sourceRole: ReplicationRole;
    readonly destinationRole: ReplicationRole;
    readonly sourceAuthorizationDigest: string;
    readonly destinationAuthorizationDigest: string;
    readonly sourceCapabilityDigest: string;
    readonly destinationCapabilityDigest: string;
    readonly effectiveLimitsDigest: string;
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
  };
  phase: ReplicationPhase;
  cursor: string;
  cursorDigest: string;
  nextSequence: number;
  chainDigest: string;
  acceptedEntries: number;
  acceptedBytes: number;
  receiptBytes: number;
  compactedThrough: number;
  attempts: number;
  elapsedRetryMs: number;
  lastWallClockMs: number;
  readonly retryDeadlineMs: number;
  readonly createdAtMs: number;
  readonly cursorExpiresAtMs: number;
  terminalResultDigest: string | null;
  terminalResultBytes: number;
  terminalExpiresAtMs: number | null;
}

interface SessionRow extends SqliteRow {
  readonly id: string;
  readonly state: number;
  readonly nonce: Uint8Array;
  readonly cursor: Uint8Array | null;
  readonly expires_at_ms: number;
  readonly staged_bytes: number;
}

interface ReceiptRow extends SqliteRow {
  readonly digest: Uint8Array;
  readonly encoded: Uint8Array;
}

interface ReplicationAggregateRow extends SqliteRow {
  readonly active_sessions: number;
  readonly session_rows: number;
  readonly metadata_bytes: number;
}

const PHASES = [
  "handshake",
  "plan-selection",
  "content-offer",
  "missing-content",
  "content-transfer",
  "state-transfer",
  "activation",
  "result-acknowledgement",
  "cleanup",
] as const satisfies readonly ReplicationPhase[];
const FLOWS = new Set<ReplicationFlow>([
  "authority-main-to-replica",
  "authority-branch-to-replica",
  "replica-branch-to-authority",
  "replica-branch-to-replica",
]);
const ROLES = new Set<ReplicationRole>(["main-authority", "replica"]);
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const ZERO_DIGEST = new Uint8Array(32);
const RECEIPT_CHAIN_DIGEST_DOMAIN = encoder.encode(
  "efs-replication-v1/receipt-chain\0",
);
const REPLICATION_IDENTITY_MARKER_ID = "efs-system-replication-identity-v1";
const REPLICATION_IDENTITY_MARKER_STATE = -3;
const REPLICATION_AGGREGATE_SQL = `SELECT
  (SELECT count(*) FROM efs_replication_sessions WHERE state=0) active_sessions,
  (SELECT count(*) FROM efs_replication_sessions WHERE state>=0) session_rows,
  (SELECT count(*)*${DURABLE_METADATA_ROW_BYTES}+coalesce(sum(length(cursor)),0) FROM efs_replication_sessions WHERE state>=0)
    +(SELECT count(*)*${DURABLE_METADATA_ROW_BYTES}+coalesce(sum(length(r.encoded)),0) FROM efs_replication_receipts r JOIN efs_replication_sessions s ON s.id=r.session_id WHERE s.state>=0) metadata_bytes`;

function replicationError(code: string, message: string): Error {
  return new Error(`${code}: ${message}`);
}

function isSafeNonnegative(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function safeNonnegative(value: number, name: string): number {
  if (!isSafeNonnegative(value)) throw new RangeError(`${name} is invalid`);
  return value;
}

function safePositive(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0)
    throw new RangeError(`${name} is invalid`);
  return value;
}

function hasUnpairedSurrogate(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const following = value.charCodeAt(index + 1);
      if (!(following >= 0xdc00 && following <= 0xdfff)) return true;
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return true;
  }
  return false;
}

function boundedText(value: string, name: string, maximum: number): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    hasUnpairedSurrogate(value) ||
    encoder.encode(value).byteLength > maximum
  )
    throw new RangeError(`${name} is outside its UTF-8 envelope`);
  return value;
}

function exactBytes(value: Uint8Array, length: number, name: string): Uint8Array {
  if (!(value instanceof Uint8Array) || value.byteLength !== length)
    throw new RangeError(`${name} must contain exactly ${length} bytes`);
  return value;
}

function boundedBytes(value: Uint8Array, maximum: number, name: string): Uint8Array {
  if (!(value instanceof Uint8Array) || value.byteLength > maximum)
    throw new RangeError(`${name} exceeds its byte envelope`);
  return value;
}

function publicCursor(value: Uint8Array, maximum: number, name: string): Uint8Array {
  boundedBytes(value, maximum, name);
  if (value.byteLength < 16)
    throw new RangeError(`${name} must contain at least 128 random bits`);
  return value;
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  let difference = 0;
  for (let index = 0; index < left.byteLength; index += 1)
    difference |= left[index]! ^ right[index]!;
  return difference === 0;
}

function toHex(value: Uint8Array): string {
  let output = "";
  for (const byte of value) output += byte.toString(16).padStart(2, "0");
  return output;
}

function fromHex(value: unknown, length: number, name: string): Uint8Array {
  if (
    typeof value !== "string" ||
    value.length !== length * 2 ||
    !/^[0-9a-f]*$/u.test(value)
  )
    throw replicationError("ECORRUPT", `${name} has invalid canonical hex`);
  const output = new Uint8Array(length);
  for (let index = 0; index < length; index += 1)
    output[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return output;
}

function fromBoundedHex(value: unknown, maximum: number, name: string): Uint8Array {
  if (
    typeof value !== "string" ||
    value.length % 2 !== 0 ||
    value.length > maximum * 2 ||
    !/^[0-9a-f]*$/u.test(value)
  )
    throw replicationError("ECORRUPT", `${name} has invalid canonical hex`);
  const output = new Uint8Array(value.length / 2);
  for (let index = 0; index < output.byteLength; index += 1)
    output[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return output;
}

function checkedAdd(left: number, right: number, name: string): number {
  const value = left + right;
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError(`${name} exceeds the safe-integer envelope`);
  return value;
}

function checkedAdjust(value: number, delta: number, name: string): number {
  const adjusted = value + delta;
  if (!Number.isSafeInteger(adjusted) || adjusted < 0)
    throw new RangeError(`${name} exceeds the safe-integer envelope`);
  return adjusted;
}

function encodeJson(value: unknown): Uint8Array {
  return encoder.encode(JSON.stringify(value));
}

function parseJson(bytes: Uint8Array, name: string): unknown {
  try {
    return JSON.parse(decoder.decode(bytes));
  } catch {
    throw replicationError("ECORRUPT", `${name} is not canonical JSON state`);
  }
}

function phase(value: unknown, name: string): ReplicationPhase {
  if (typeof value !== "string" || !(PHASES as readonly string[]).includes(value))
    throw replicationError("ECORRUPT", `${name} is invalid`);
  return value as ReplicationPhase;
}

function requiredRolePair(flow: ReplicationFlow): Readonly<{
  source: ReplicationRole;
  destination: ReplicationRole;
}> {
  switch (flow) {
    case "authority-main-to-replica":
    case "authority-branch-to-replica":
      return { source: "main-authority", destination: "replica" };
    case "replica-branch-to-authority":
      return { source: "replica", destination: "main-authority" };
    case "replica-branch-to-replica":
      return { source: "replica", destination: "replica" };
  }
}

function validatePhaseAdvance(current: ReplicationPhase, next: ReplicationPhase): void {
  const currentIndex = PHASES.indexOf(current);
  const nextIndex = PHASES.indexOf(next);
  if (nextIndex !== currentIndex && nextIndex !== currentIndex + 1)
    throw replicationError(
      "CursorMismatch",
      "batch phase advancement is not canonical",
    );
}

function validateBinding(binding: ReplicationSessionBinding): void {
  boundedText(binding.operationId, "operationId", 200);
  if (!/^[0-9a-f]{32}$/.test(binding.sessionId))
    throw new RangeError("sessionId must be canonical 128-bit lowercase hex");
  if (
    binding.operationId === "efs-unbound-replica-v1" ||
    binding.operationId.startsWith("efs-system-")
  )
    throw new RangeError("operationId is reserved");
  boundedBytes(binding.resumeKey, 256, "resumeKey");
  if (binding.resumeKey.byteLength < 16)
    throw new RangeError("resumeKey must contain at least 128 bits");
  exactBytes(binding.ownerNonce, 16, "ownerNonce");
  if (!FLOWS.has(binding.flow)) throw new RangeError("flow is invalid");
  if (binding.flow === "authority-main-to-replica") {
    if (binding.branchId !== null)
      throw new RangeError("main replication cannot bind a branch");
  } else if (binding.branchId === null) {
    throw new RangeError("branch replication requires a branchId");
  } else boundedText(binding.branchId, "branchId", 200);
  boundedText(binding.sourceFilesystemId, "sourceFilesystemId", 256);
  boundedText(binding.destinationFilesystemId, "destinationFilesystemId", 256);
  if (!ROLES.has(binding.sourceRole) || !ROLES.has(binding.destinationRole))
    throw new RangeError("replication role is invalid");
  const requiredRoles = requiredRolePair(binding.flow);
  if (
    binding.sourceRole !== requiredRoles.source ||
    binding.destinationRole !== requiredRoles.destination
  )
    throw replicationError(
      "UnauthorizedScope",
      "replication roles do not authorize the selected flow",
    );
  for (const [name, value] of [
    ["sourceAuthorizationDigest", binding.sourceAuthorizationDigest],
    ["destinationAuthorizationDigest", binding.destinationAuthorizationDigest],
    ["sourceCapabilityDigest", binding.sourceCapabilityDigest],
    ["destinationCapabilityDigest", binding.destinationCapabilityDigest],
    ["effectiveLimitsDigest", binding.effectiveLimitsDigest],
  ] as const)
    exactBytes(value, 32, name);
  for (const [name, value] of [
    ["maxBatchEntries", binding.maxBatchEntries],
    ["maxBatchBytes", binding.maxBatchBytes],
    ["maxRequestBytes", binding.maxRequestBytes],
    ["maxResponseBytes", binding.maxResponseBytes],
    ["maxBufferedBytes", binding.maxBufferedBytes],
    ["maxInFlightBatches", binding.maxInFlightBatches],
    ["maxConcurrentSessions", binding.maxConcurrentSessions],
    ["maxCursorBytes", binding.maxCursorBytes],
    ["maxReplicationSessionRows", binding.maxReplicationSessionRows],
    ["maxReplicationMetadataBytes", binding.maxReplicationMetadataBytes],
    ["maxReceiptsPerSession", binding.maxReceiptsPerSession],
    ["maxReceiptBytesPerSession", binding.maxReceiptBytesPerSession],
    ["maxStagingBytesPerSession", binding.maxStagingBytesPerSession],
    ["maxAcknowledgementBytes", binding.maxAcknowledgementBytes],
    ["maxTerminalResultBytes", binding.maxTerminalResultBytes],
    ["maxCursorAgeMs", binding.maxCursorAgeMs],
    ["stagingLeaseMs", binding.stagingLeaseMs],
    ["maxRetryAttempts", binding.maxRetryAttempts],
    ["maxRetryElapsedMs", binding.maxRetryElapsedMs],
    ["minRetryDelayMs", binding.minRetryDelayMs],
    ["maxRetryDelayMs", binding.maxRetryDelayMs],
    ["resultRetentionMs", binding.resultRetentionMs],
  ] as const)
    safePositive(value, name);
  if (
    binding.maxInFlightBatches !== 1 ||
    binding.maxBatchBytes > binding.maxRequestBytes ||
    binding.maxAcknowledgementBytes > binding.maxResponseBytes ||
    binding.minRetryDelayMs > binding.maxRetryDelayMs
  )
    throw new RangeError("replication limits violate a cross-field constraint");
}

function durableBinding(
  binding: ReplicationSessionBinding,
): DurableSessionState["binding"] {
  return {
    operationId: binding.operationId,
    sessionId: binding.sessionId,
    resumeKey: toHex(binding.resumeKey),
    ownerNonce: toHex(binding.ownerNonce),
    flow: binding.flow,
    branchId: binding.branchId,
    sourceFilesystemId: binding.sourceFilesystemId,
    destinationFilesystemId: binding.destinationFilesystemId,
    sourceRole: binding.sourceRole,
    destinationRole: binding.destinationRole,
    sourceAuthorizationDigest: toHex(binding.sourceAuthorizationDigest),
    destinationAuthorizationDigest: toHex(binding.destinationAuthorizationDigest),
    sourceCapabilityDigest: toHex(binding.sourceCapabilityDigest),
    destinationCapabilityDigest: toHex(binding.destinationCapabilityDigest),
    effectiveLimitsDigest: toHex(binding.effectiveLimitsDigest),
    maxBatchEntries: binding.maxBatchEntries,
    maxBatchBytes: binding.maxBatchBytes,
    maxRequestBytes: binding.maxRequestBytes,
    maxResponseBytes: binding.maxResponseBytes,
    maxBufferedBytes: binding.maxBufferedBytes,
    maxInFlightBatches: binding.maxInFlightBatches,
    maxConcurrentSessions: binding.maxConcurrentSessions,
    maxCursorBytes: binding.maxCursorBytes,
    maxReplicationSessionRows: binding.maxReplicationSessionRows,
    maxReplicationMetadataBytes: binding.maxReplicationMetadataBytes,
    maxReceiptsPerSession: binding.maxReceiptsPerSession,
    maxReceiptBytesPerSession: binding.maxReceiptBytesPerSession,
    maxStagingBytesPerSession: binding.maxStagingBytesPerSession,
    maxAcknowledgementBytes: binding.maxAcknowledgementBytes,
    maxTerminalResultBytes: binding.maxTerminalResultBytes,
    maxCursorAgeMs: binding.maxCursorAgeMs,
    stagingLeaseMs: binding.stagingLeaseMs,
    maxRetryAttempts: binding.maxRetryAttempts,
    maxRetryElapsedMs: binding.maxRetryElapsedMs,
    minRetryDelayMs: binding.minRetryDelayMs,
    maxRetryDelayMs: binding.maxRetryDelayMs,
    resultRetentionMs: binding.resultRetentionMs,
  };
}

function sameDurableBinding(
  stored: DurableSessionState["binding"],
  requested: DurableSessionState["binding"],
): boolean {
  return JSON.stringify(stored) === JSON.stringify(requested);
}

function decodedBinding(
  stored: DurableSessionState["binding"],
): ReplicationSessionBinding {
  if (!stored || typeof stored !== "object")
    throw replicationError("ECORRUPT", "durable replication binding is absent");
  const binding: ReplicationSessionBinding = {
    operationId: stored.operationId,
    sessionId: stored.sessionId,
    resumeKey: fromBoundedHex(stored.resumeKey, 256, "resumeKey"),
    ownerNonce: fromHex(stored.ownerNonce, 16, "ownerNonce"),
    flow: stored.flow,
    branchId: stored.branchId,
    sourceFilesystemId: stored.sourceFilesystemId,
    destinationFilesystemId: stored.destinationFilesystemId,
    sourceRole: stored.sourceRole,
    destinationRole: stored.destinationRole,
    sourceAuthorizationDigest: fromHex(
      stored.sourceAuthorizationDigest,
      32,
      "sourceAuthorizationDigest",
    ),
    destinationAuthorizationDigest: fromHex(
      stored.destinationAuthorizationDigest,
      32,
      "destinationAuthorizationDigest",
    ),
    sourceCapabilityDigest: fromHex(
      stored.sourceCapabilityDigest,
      32,
      "sourceCapabilityDigest",
    ),
    destinationCapabilityDigest: fromHex(
      stored.destinationCapabilityDigest,
      32,
      "destinationCapabilityDigest",
    ),
    effectiveLimitsDigest: fromHex(
      stored.effectiveLimitsDigest,
      32,
      "effectiveLimitsDigest",
    ),
    maxBatchEntries: stored.maxBatchEntries,
    maxBatchBytes: stored.maxBatchBytes,
    maxRequestBytes: stored.maxRequestBytes,
    maxResponseBytes: stored.maxResponseBytes,
    maxBufferedBytes: stored.maxBufferedBytes,
    maxInFlightBatches: stored.maxInFlightBatches,
    maxConcurrentSessions: stored.maxConcurrentSessions,
    maxCursorBytes: stored.maxCursorBytes,
    maxReplicationSessionRows: stored.maxReplicationSessionRows,
    maxReplicationMetadataBytes: stored.maxReplicationMetadataBytes,
    maxReceiptsPerSession: stored.maxReceiptsPerSession,
    maxReceiptBytesPerSession: stored.maxReceiptBytesPerSession,
    maxStagingBytesPerSession: stored.maxStagingBytesPerSession,
    maxAcknowledgementBytes: stored.maxAcknowledgementBytes,
    maxTerminalResultBytes: stored.maxTerminalResultBytes,
    maxCursorAgeMs: stored.maxCursorAgeMs,
    stagingLeaseMs: stored.stagingLeaseMs,
    maxRetryAttempts: stored.maxRetryAttempts,
    maxRetryElapsedMs: stored.maxRetryElapsedMs,
    minRetryDelayMs: stored.minRetryDelayMs,
    maxRetryDelayMs: stored.maxRetryDelayMs,
    resultRetentionMs: stored.resultRetentionMs,
  };
  validateBinding(binding);
  if (
    binding.maxBatchEntries > 256 ||
    binding.maxBatchBytes > 4 * 1024 * 1024 ||
    binding.maxRequestBytes > 4 * 1024 * 1024 + 64 * 1024 ||
    binding.maxResponseBytes > 4 * 1024 * 1024 + 64 * 1024 ||
    binding.maxInFlightBatches !== 1 ||
    binding.maxConcurrentSessions > 16 ||
    binding.maxCursorBytes > 256 ||
    binding.maxReplicationSessionRows > 10_000 ||
    binding.maxReplicationMetadataBytes > 64 * 1024 * 1024 ||
    binding.maxReceiptsPerSession > 100_000 ||
    binding.maxReceiptBytesPerSession > 16 * 1024 * 1024 ||
    binding.maxStagingBytesPerSession > 512 * 1024 * 1024 ||
    binding.maxAcknowledgementBytes > 64 * 1024 ||
    binding.maxTerminalResultBytes > 1024 * 1024 ||
    binding.maxCursorAgeMs > 24 * 60 * 60 * 1000 ||
    binding.maxRetryAttempts > 8 ||
    binding.maxRetryElapsedMs > 5 * 60 * 1000 ||
    binding.minRetryDelayMs > binding.maxRetryDelayMs ||
    binding.maxRetryDelayMs > 10_000 ||
    binding.resultRetentionMs > 30 * 24 * 60 * 60 * 1000
  )
    throw replicationError(
      "ECORRUPT",
      "durable replication binding exceeds version 1 ceilings",
    );
  return binding;
}

function decodeState(value: Uint8Array | null): DurableSessionState {
  if (!(value instanceof Uint8Array))
    throw replicationError("ECORRUPT", "replication session state is absent");
  const parsed = parseJson(value, "replication session");
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    throw replicationError("ECORRUPT", "replication session is not an object");
  const state = parsed as Partial<DurableSessionState>;
  if (state.version !== 1 || !state.binding || typeof state.binding !== "object")
    throw replicationError("ECORRUPT", "unsupported replication session state");
  decodedBinding(state.binding as DurableSessionState["binding"]);
  phase(state.phase, "replication session phase");
  for (const [name, number] of [
    ["nextSequence", state.nextSequence],
    ["acceptedEntries", state.acceptedEntries],
    ["acceptedBytes", state.acceptedBytes],
    ["receiptBytes", state.receiptBytes],
    ["attempts", state.attempts],
    ["elapsedRetryMs", state.elapsedRetryMs],
    ["lastWallClockMs", state.lastWallClockMs],
    ["retryDeadlineMs", state.retryDeadlineMs],
    ["createdAtMs", state.createdAtMs],
    ["cursorExpiresAtMs", state.cursorExpiresAtMs],
  ] as const)
    if (!isSafeNonnegative(number))
      throw replicationError("ECORRUPT", `${name} is invalid`);
  if (!Number.isSafeInteger(state.compactedThrough) || state.compactedThrough! < -1)
    throw replicationError("ECORRUPT", "compactedThrough is invalid");
  publicCursor(
    fromBoundedHex(state.cursor, state.binding.maxCursorBytes, "cursor"),
    state.binding.maxCursorBytes,
    "cursor",
  );
  fromHex(state.cursorDigest, 32, "cursorDigest");
  fromHex(state.chainDigest, 32, "chainDigest");
  if (
    state.terminalResultDigest !== null &&
    typeof state.terminalResultDigest !== "string"
  )
    throw replicationError("ECORRUPT", "terminal result digest is invalid");
  if (state.terminalResultDigest !== null)
    fromHex(state.terminalResultDigest, 32, "terminalResultDigest");
  if (
    !isSafeNonnegative(state.terminalResultBytes) ||
    state.terminalResultBytes > state.binding.maxTerminalResultBytes
  )
    throw replicationError("ECORRUPT", "terminal result byte count is invalid");
  if (
    state.terminalExpiresAtMs !== null &&
    !isSafeNonnegative(state.terminalExpiresAtMs)
  )
    throw replicationError("ECORRUPT", "terminal result expiry is invalid");
  return state as DurableSessionState;
}

/** Bounded reopen recognition for durable provisioning sessions. */
export function validateDurableReplicationSessions(
  tx: FilesystemSQLiteTransaction,
  hash: (value: Uint8Array) => Uint8Array,
): void {
  let cursor = "";
  let sessionCount = 0;
  for (;;) {
    const rows = tx.all<SessionRow>(
      "SELECT id,state,nonce,cursor,expires_at_ms,staged_bytes FROM efs_replication_sessions WHERE id>? AND state>=0 ORDER BY id LIMIT 65",
      [cursor],
      { maxRows: 65, maxBytes: 1024 * 1024 },
    );
    if (rows.length > 64)
      throw replicationError(
        "ResourceLimit",
        "unbound session recognition page overflow",
      );
    if (rows.length === 0) break;
    for (const row of rows) {
      sessionCount += 1;
      if (sessionCount > 10_000)
        throw replicationError(
          "ResourceLimit",
          "too many durable replication sessions",
        );
      if (
        typeof row.id !== "string" ||
        !(row.nonce instanceof Uint8Array) ||
        row.nonce.byteLength !== 16 ||
        !(row.cursor instanceof Uint8Array) ||
        !isSafeNonnegative(row.expires_at_ms) ||
        !isSafeNonnegative(row.staged_bytes)
      )
        throw replicationError("ECORRUPT", "durable replication row is invalid");
      const state = decodeState(row.cursor);
      const binding = decodedBinding(state.binding);
      const currentCursor = fromBoundedHex(
        state.cursor,
        binding.maxCursorBytes,
        "cursor",
      );
      if (
        state.binding.operationId !== row.id ||
        !equalBytes(row.nonce, binding.ownerNonce) ||
        !equalBytes(
          hash(currentCursor),
          fromHex(state.cursorDigest, 32, "cursorDigest"),
        ) ||
        row.staged_bytes > binding.maxStagingBytesPerSession ||
        (row.state === 0 && state.terminalResultDigest !== null) ||
        (row.state === 1 && state.terminalResultDigest === null) ||
        (row.state !== 0 && row.state !== 1) ||
        row.expires_at_ms !== (state.terminalExpiresAtMs ?? state.cursorExpiresAtMs)
      )
        throw replicationError(
          "ECORRUPT",
          "durable replication row binding is invalid",
        );
      const summary = tx.all<
        {
          count: number;
          bytes: number;
          minimum: number;
          maximum: number;
          invalid: number;
        } & SqliteRow
      >(
        "SELECT count(*) count,coalesce(sum(length(digest)+length(encoded)),0) bytes,coalesce(min(batch_index),-1) minimum,coalesce(max(batch_index),-1) maximum,coalesce(sum(CASE WHEN length(digest)=32 THEN 0 ELSE 1 END),0) invalid FROM efs_replication_receipts WHERE session_id=? AND batch_index>=0",
        [row.id],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      if (
        !summary ||
        !isSafeNonnegative(summary.count) ||
        !isSafeNonnegative(summary.bytes) ||
        summary.invalid !== 0 ||
        summary.count > binding.maxReceiptsPerSession ||
        summary.bytes !== state.receiptBytes ||
        (summary.count > 0 &&
          (summary.minimum <= state.compactedThrough ||
            summary.maximum >= state.nextSequence))
      )
        throw replicationError(
          "ECORRUPT",
          "durable replication receipt summary is invalid",
        );
      const terminal = tx.all<
        { digest: Uint8Array; encoded_bytes: number } & SqliteRow
      >(
        "SELECT digest,length(encoded) encoded_bytes FROM efs_replication_receipts WHERE session_id=? AND batch_index=-1",
        [row.id],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      if (
        (state.terminalResultDigest === null) !== (terminal === undefined) ||
        (terminal !== undefined &&
          (!(terminal.digest instanceof Uint8Array) ||
            state.terminalResultDigest === null ||
            !equalBytes(
              terminal.digest,
              fromHex(state.terminalResultDigest, 32, "terminalResultDigest"),
            ) ||
            terminal.encoded_bytes !== state.terminalResultBytes))
      )
        throw replicationError("ECORRUPT", "durable terminal result row is invalid");
      cursor = row.id;
    }
    if (rows.length < 64) break;
  }
}

function snapshot(
  state: DurableSessionState,
  stagedBytes: number,
): ReplicationSessionSnapshot {
  return Object.freeze({
    operationId: state.binding.operationId,
    sessionId: state.binding.sessionId,
    phase: state.phase,
    cursor: fromBoundedHex(state.cursor, state.binding.maxCursorBytes, "cursor"),
    cursorDigest: fromHex(state.cursorDigest, 32, "cursorDigest"),
    nextSequence: state.nextSequence,
    chainDigest: fromHex(state.chainDigest, 32, "chainDigest"),
    acceptedEntries: state.acceptedEntries,
    acceptedBytes: state.acceptedBytes,
    stagedBytes,
    attempts: state.attempts,
    elapsedRetryMs: state.elapsedRetryMs,
    lastWallClockMs: state.lastWallClockMs,
    retryDeadlineMs: state.retryDeadlineMs,
    terminal: state.terminalResultDigest !== null,
  });
}

function sequenceBytes(sequence: number): Uint8Array {
  safeNonnegative(sequence, "sequence");
  const output = new Uint8Array(8);
  new DataView(output.buffer).setBigUint64(0, BigInt(sequence), false);
  return output;
}

interface CanonicalBatchAcknowledgementRecord {
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

function decodeCanonicalBatchAcknowledgement(
  input: Uint8Array,
  maximumBytes: number,
  hash: (value: Uint8Array) => Uint8Array,
): CanonicalBatchAcknowledgementRecord {
  boundedBytes(input, maximumBytes, "acknowledgement");
  let offset = 0;
  const take = (length: number, name: string): Uint8Array => {
    if (
      !Number.isSafeInteger(length) ||
      length < 0 ||
      offset + length > input.byteLength
    )
      throw replicationError("ProtocolMismatch", `${name} is truncated`);
    const value = input.subarray(offset, offset + length);
    offset += length;
    return value;
  };
  const u8 = (name: string): number => take(1, name)[0]!;
  const u16 = (name: string): number => {
    const value = take(2, name);
    return new DataView(value.buffer, value.byteOffset, 2).getUint16(0, false);
  };
  const u32 = (name: string): number => {
    const value = take(4, name);
    return new DataView(value.buffer, value.byteOffset, 4).getUint32(0, false);
  };
  const u64 = (name: string): number => {
    const value = new DataView(
      take(8, name).buffer,
      input.byteOffset + offset - 8,
      8,
    ).getBigUint64(0, false);
    if (value > BigInt(Number.MAX_SAFE_INTEGER))
      throw replicationError("ProtocolMismatch", `${name} exceeds safe integers`);
    return Number(value);
  };
  const magic = take(4, "acknowledgement.magic");
  if (!equalBytes(magic, Uint8Array.of(0x45, 0x46, 0x53, 0x52)))
    throw replicationError("ProtocolMismatch", "acknowledgement magic is invalid");
  if (
    u16("acknowledgement.version") !== 1 ||
    u8("acknowledgement.kind") !== 0x0a ||
    u8("acknowledgement.flags") !== 0
  )
    throw replicationError("ProtocolMismatch", "acknowledgement header is invalid");
  if (u32("acknowledgement.payloadLength") !== input.byteLength - 12)
    throw replicationError("ProtocolMismatch", "acknowledgement length is invalid");
  const sessionLength = u32("acknowledgement.sessionId.length");
  if (sessionLength !== 32)
    throw replicationError("ProtocolMismatch", "acknowledgement session is invalid");
  let sessionId: string;
  try {
    sessionId = decoder.decode(take(sessionLength, "acknowledgement.sessionId"));
  } catch {
    throw replicationError("ProtocolMismatch", "acknowledgement session is not UTF-8");
  }
  if (!/^[0-9a-f]{32}$/.test(sessionId))
    throw replicationError("ProtocolMismatch", "acknowledgement session is invalid");
  const decodePhase = (name: string): ReplicationPhase => {
    const value = PHASES[u8(name) - 1];
    if (!value) throw replicationError("ProtocolMismatch", `${name} is invalid`);
    return value;
  };
  const sequence = u64("acknowledgement.sequence");
  const acceptedPhase = decodePhase("acknowledgement.phase");
  const batchEnvelopeDigest = take(32, "acknowledgement.batchEnvelopeDigest");
  const nextPhase = decodePhase("acknowledgement.nextPhase");
  validatePhaseAdvance(acceptedPhase, nextPhase);
  const cursorLength = u32("acknowledgement.cursor.length");
  if (cursorLength < 16 || cursorLength > 256)
    throw replicationError("ProtocolMismatch", "acknowledgement cursor is invalid");
  const cursor = take(cursorLength, "acknowledgement.cursor");
  const cursorDigest = take(32, "acknowledgement.cursorDigest");
  if (!equalBytes(hash(cursor), cursorDigest))
    throw replicationError(
      "IntegrityFailure",
      "acknowledgement cursor digest does not match",
    );
  const chainDigest = take(32, "acknowledgement.chainDigest");
  const acceptedEntries = u64("acknowledgement.acceptedEntries");
  const acceptedBytes = u64("acknowledgement.acceptedBytes");
  const stagedBytes = u64("acknowledgement.stagedBytes");
  if (offset !== input.byteLength)
    throw replicationError("ProtocolMismatch", "acknowledgement has trailing bytes");
  return {
    sessionId,
    sequence,
    phase: acceptedPhase,
    batchEnvelopeDigest,
    nextPhase,
    cursor,
    cursorDigest,
    chainDigest,
    acceptedEntries,
    acceptedBytes,
    stagedBytes,
  };
}

export class ReplicationSessionRepository implements ReplicationSessionStore {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #hash: (value: Uint8Array) => Uint8Array;
  readonly #limits: StorageLimits | undefined;

  constructor(
    tx: FilesystemSQLiteTransaction,
    hash: (value: Uint8Array) => Uint8Array,
    limits?: StorageLimits,
  ) {
    this.#tx = tx;
    this.#hash = hash;
    this.#limits = limits;
  }

  filesystemIdentity(): ReplicationFilesystemIdentity | undefined {
    const row = this.#tx.all<SessionRow>(
      "SELECT id,state,nonce,cursor,expires_at_ms,staged_bytes FROM efs_replication_sessions WHERE id=?",
      [REPLICATION_IDENTITY_MARKER_ID],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (!row) return undefined;
    if (
      row.id !== REPLICATION_IDENTITY_MARKER_ID ||
      row.state !== REPLICATION_IDENTITY_MARKER_STATE ||
      !(row.nonce instanceof Uint8Array) ||
      row.nonce.byteLength !== 16 ||
      !(row.cursor instanceof Uint8Array) ||
      row.expires_at_ms !== Number.MAX_SAFE_INTEGER ||
      row.staged_bytes !== 0 ||
      !equalBytes(row.nonce, this.#hash(row.cursor).subarray(0, 16))
    )
      throw replicationError("ECORRUPT", "replication identity marker is invalid");
    const parsed = parseJson(row.cursor, "replication identity") as Partial<
      ReplicationFilesystemIdentity & { readonly version: number }
    >;
    if (
      parsed.version !== 1 ||
      typeof parsed.filesystemId !== "string" ||
      typeof parsed.authorityId !== "string" ||
      !ROLES.has(parsed.role as ReplicationRole)
    )
      throw replicationError("ECORRUPT", "replication identity payload is invalid");
    const identity = Object.freeze({
      filesystemId: boundedText(parsed.filesystemId, "filesystemId", 256),
      authorityId: boundedText(parsed.authorityId, "authorityId", 256),
      role: parsed.role as ReplicationRole,
    });
    if (!equalBytes(row.cursor, encodeJson({ version: 1, ...identity })))
      throw replicationError("ECORRUPT", "replication identity is not canonical");
    return identity;
  }

  bindFilesystemIdentity(
    identity: ReplicationFilesystemIdentity,
  ): ReplicationFilesystemIdentity {
    const requested = Object.freeze({
      filesystemId: boundedText(identity.filesystemId, "filesystemId", 256),
      authorityId: boundedText(identity.authorityId, "authorityId", 256),
      role: identity.role,
    });
    if (!ROLES.has(requested.role))
      throw replicationError("UnauthorizedScope", "replication role is invalid");
    const existing = this.filesystemIdentity();
    if (existing) {
      if (
        existing.filesystemId !== requested.filesystemId ||
        existing.authorityId !== requested.authorityId ||
        existing.role !== requested.role
      )
        throw replicationError(
          "AuthorityMismatch",
          "filesystem replication identity is already bound differently",
        );
      return existing;
    }
    if (!this.#limits)
      throw replicationError(
        "ProvisioningRejected",
        "storage limits are required to bind a filesystem identity",
      );
    const cursor = encodeJson({ version: 1, ...requested });
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        permanent_identifiers: 1,
        charged_metadata_bytes: DURABLE_METADATA_ROW_BYTES + cursor.byteLength,
      },
      "replication identity binding",
    );
    this.#tx.run(
      "INSERT INTO efs_replication_sessions(id,state,nonce,cursor,expires_at_ms,staged_bytes) VALUES(?,?,?,?,?,0)",
      [
        REPLICATION_IDENTITY_MARKER_ID,
        REPLICATION_IDENTITY_MARKER_STATE,
        this.#hash(cursor).subarray(0, 16),
        cursor,
        Number.MAX_SAFE_INTEGER,
      ],
    );
    return requested;
  }

  #row(operationId: string): SessionRow | undefined {
    return this.#tx.all<SessionRow>(
      "SELECT id,state,nonce,cursor,expires_at_ms,staged_bytes FROM efs_replication_sessions WHERE id=?",
      [operationId],
      { maxRows: 1, maxBytes: 256 * 1024 },
    )[0];
  }

  #load(operationId: string): { row: SessionRow; state: DurableSessionState } {
    boundedText(operationId, "operationId", 200);
    const row = this.#row(operationId);
    if (!row || row.state === -1)
      throw replicationError("OperationMismatch", "replication operation is unknown");
    if (
      !(row.nonce instanceof Uint8Array) ||
      row.nonce.byteLength !== 16 ||
      !isSafeNonnegative(row.expires_at_ms) ||
      !isSafeNonnegative(row.staged_bytes)
    )
      throw replicationError("ECORRUPT", "replication session row is invalid");
    const state = decodeState(row.cursor);
    if (
      state.binding.operationId !== operationId ||
      !equalBytes(row.nonce, fromHex(state.binding.ownerNonce, 16, "ownerNonce")) ||
      (row.state === 0 && state.terminalResultDigest !== null) ||
      (row.state === 1 && state.terminalResultDigest === null) ||
      (row.state !== 0 && row.state !== 1)
    )
      throw replicationError(
        "ECORRUPT",
        "replication session row disagrees with state",
      );
    const expectedExpiry = state.terminalExpiresAtMs ?? state.cursorExpiresAtMs;
    if (row.expires_at_ms !== expectedExpiry)
      throw replicationError(
        "ECORRUPT",
        "replication session expiry disagrees with state",
      );
    return { row, state };
  }

  #aggregates(): ReplicationAggregateRow {
    const row = this.#tx.all<ReplicationAggregateRow>(REPLICATION_AGGREGATE_SQL, [], {
      maxRows: 1,
      maxBytes: 256,
    })[0];
    if (
      !row ||
      !isSafeNonnegative(row.active_sessions) ||
      !isSafeNonnegative(row.session_rows) ||
      !isSafeNonnegative(row.metadata_bytes)
    )
      throw replicationError("ECORRUPT", "replication aggregates are invalid");
    return row;
  }

  #assertAggregateAdmission(
    binding: DurableSessionState["binding"],
    change: Readonly<{
      activeSessions?: number;
      sessionRows?: number;
      metadataBytes?: number;
    }>,
  ): void {
    const aggregate = this.#aggregates();
    const activeSessionChange = change.activeSessions ?? 0;
    const activeSessions = checkedAdjust(
      aggregate.active_sessions,
      activeSessionChange,
      "active replication sessions",
    );
    if (activeSessionChange > 0 && activeSessions > binding.maxConcurrentSessions)
      throw replicationError(
        "ResourceLimit",
        "aggregate active replication session limit exceeded",
      );
    const sessionRowChange = change.sessionRows ?? 0;
    const sessionRows = checkedAdjust(
      aggregate.session_rows,
      sessionRowChange,
      "retained replication session rows",
    );
    if (sessionRowChange > 0 && sessionRows > binding.maxReplicationSessionRows)
      throw replicationError(
        "ResourceLimit",
        "aggregate retained replication session row limit exceeded",
      );
    const metadataByteChange = change.metadataBytes ?? 0;
    const metadataBytes = checkedAdjust(
      aggregate.metadata_bytes,
      metadataByteChange,
      "replication metadata bytes",
    );
    if (metadataByteChange > 0 && metadataBytes > binding.maxReplicationMetadataBytes)
      throw replicationError(
        "ResourceLimit",
        "aggregate replication metadata limit exceeded",
      );
  }

  createOrResume(request: CreateReplicationSessionRequest): Readonly<{
    created: boolean;
    session: ReplicationSessionSnapshot;
  }> {
    validateBinding(request.binding);
    const selectedPhase = phase(request.phase, "phase");
    safeNonnegative(request.now, "now");
    safePositive(request.expiresAtMs, "expiresAtMs");
    if (request.expiresAtMs <= request.now)
      throw new RangeError("expiresAtMs must be in the future");
    if (
      request.expiresAtMs >
      checkedAdd(request.now, request.binding.maxCursorAgeMs, "cursor expiry")
    )
      throw new RangeError("expiresAtMs exceeds the negotiated cursor lifetime");
    publicCursor(request.cursor, request.binding.maxCursorBytes, "cursor");
    exactBytes(request.cursorDigest, 32, "cursorDigest");
    if (!equalBytes(this.#hash(request.cursor), request.cursorDigest))
      throw replicationError("IntegrityFailure", "cursor digest does not match bytes");
    const requestedBinding = durableBinding(request.binding);
    const existing = this.#row(request.binding.operationId);
    if (existing) {
      const loaded = this.#load(request.binding.operationId);
      if (!sameDurableBinding(loaded.state.binding, requestedBinding))
        throw replicationError(
          "OperationMismatch",
          "operation identifier is already bound to another replication request",
        );
      return Object.freeze({
        created: false,
        session: snapshot(loaded.state, loaded.row.staged_bytes),
      });
    }
    const retryDeadlineMs = checkedAdd(
      request.now,
      request.binding.maxRetryElapsedMs,
      "retry deadline",
    );
    const state: DurableSessionState = {
      version: 1,
      binding: requestedBinding,
      phase: selectedPhase,
      cursor: toHex(request.cursor),
      cursorDigest: toHex(request.cursorDigest),
      nextSequence: 0,
      chainDigest: toHex(ZERO_DIGEST),
      acceptedEntries: 0,
      acceptedBytes: 0,
      receiptBytes: 0,
      compactedThrough: -1,
      attempts: 0,
      elapsedRetryMs: 0,
      lastWallClockMs: request.now,
      retryDeadlineMs,
      createdAtMs: request.now,
      cursorExpiresAtMs: request.expiresAtMs,
      terminalResultDigest: null,
      terminalResultBytes: 0,
      terminalExpiresAtMs: null,
    };
    const encodedState = encodeJson(state);
    this.#assertAggregateAdmission(requestedBinding, {
      activeSessions: 1,
      sessionRows: 1,
      metadataBytes: checkedAdd(
        DURABLE_METADATA_ROW_BYTES,
        encodedState.byteLength,
        "replication session metadata",
      ),
    });
    this.#tx.run(
      "INSERT INTO efs_replication_sessions(id,state,nonce,cursor,expires_at_ms,staged_bytes) VALUES(?,0,?,?,?,0)",
      [
        request.binding.operationId,
        request.binding.ownerNonce,
        encodedState,
        request.expiresAtMs,
      ],
    );
    return Object.freeze({ created: true, session: snapshot(state, 0) });
  }

  resume(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly resumeKey: Uint8Array;
  }): ReplicationSessionSnapshot {
    const loaded = this.#load(request.operationId);
    if (
      loaded.state.binding.sessionId !== request.sessionId ||
      !equalBytes(
        fromBoundedHex(loaded.state.binding.resumeKey, 256, "resumeKey"),
        request.resumeKey,
      )
    )
      throw replicationError("OperationMismatch", "session resume binding changed");
    return snapshot(loaded.state, loaded.row.staged_bytes);
  }

  findSession(request: {
    readonly operationId: string;
    readonly resumeKey: Uint8Array;
  }): Readonly<{
    readonly binding: ReplicationSessionBinding;
    readonly session: ReplicationSessionSnapshot;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
  }> {
    const loaded = this.#load(request.operationId);
    if (
      !equalBytes(
        fromBoundedHex(loaded.state.binding.resumeKey, 256, "resumeKey"),
        request.resumeKey,
      )
    )
      throw replicationError("OperationMismatch", "session resume binding changed");
    const binding = decodedBinding(loaded.state.binding);
    return Object.freeze({
      binding,
      session: snapshot(loaded.state, loaded.row.staged_bytes),
      flow: binding.flow,
      branchId: binding.branchId,
    });
  }

  loadSession(request: {
    readonly operationId: string;
  }): Readonly<{
    readonly binding: ReplicationSessionBinding;
    readonly session: ReplicationSessionSnapshot;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
  }> {
    const loaded = this.#load(request.operationId);
    const binding = decodedBinding(loaded.state.binding);
    return Object.freeze({
      binding,
      session: snapshot(loaded.state, loaded.row.staged_bytes),
      flow: binding.flow,
      branchId: binding.branchId,
    });
  }

  acceptBatch(request: ReplicationBatchAcceptanceRequest): Readonly<{
    replayed: boolean;
    acknowledgement: Uint8Array;
    session: ReplicationSessionSnapshot;
  }> {
    const loaded = this.#load(request.operationId);
    const { row, state } = loaded;
    this.#assertOwner(state, request.sessionId, request.ownerNonce);
    if (row.state !== 0)
      throw replicationError("OperationMismatch", "replication session is terminal");
    safeNonnegative(request.now, "now");
    if (request.now > state.cursorExpiresAtMs)
      throw replicationError("CursorExpired", "replication cursor has expired");
    safeNonnegative(request.sequence, "sequence");
    exactBytes(request.batchEnvelopeDigest, 32, "batchEnvelopeDigest");
    exactBytes(request.payloadDigest, 32, "payloadDigest");
    exactBytes(request.priorCursorDigest, 32, "priorCursorDigest");
    safeNonnegative(request.entryCount, "entryCount");
    safeNonnegative(request.payloadByteCount, "payloadByteCount");
    if (
      request.entryCount > state.binding.maxBatchEntries ||
      request.payloadByteCount > state.binding.maxBatchBytes
    )
      throw replicationError("ResourceLimit", "batch exceeds its effective limits");
    if (request.sequence < state.nextSequence)
      return this.#replayReceipt(loaded, request);
    if (request.sequence !== state.nextSequence)
      throw replicationError(
        "CursorMismatch",
        "batch sequence is not the next sequence",
      );
    const missingContentResponseDuringTransfer =
      state.phase === "content-transfer" &&
      request.phase === "missing-content" &&
      request.nextPhase === "content-transfer";
    if (request.phase !== state.phase && !missingContentResponseDuringTransfer)
      throw replicationError(
        "CursorMismatch",
        "batch phase differs from durable state",
      );
    if (
      !equalBytes(
        request.priorCursorDigest,
        fromHex(state.cursorDigest, 32, "cursorDigest"),
      )
    ) {
      throw replicationError(
        "CursorMismatch",
        "batch cursor differs from durable state",
      );
    }
    validatePhaseAdvance(state.phase, request.nextPhase);
    publicCursor(request.nextCursor, state.binding.maxCursorBytes, "nextCursor");
    exactBytes(request.nextCursorDigest, 32, "nextCursorDigest");
    if (!equalBytes(this.#hash(request.nextCursor), request.nextCursorDigest))
      throw replicationError(
        "IntegrityFailure",
        "next cursor digest does not match bytes",
      );
    const acknowledgement = decodeCanonicalBatchAcknowledgement(
      request.acknowledgement,
      state.binding.maxAcknowledgementBytes,
      this.#hash,
    );
    safeNonnegative(request.stagedBytesDelta, "stagedBytesDelta");
    const stagedBytes = checkedAdd(
      row.staged_bytes,
      request.stagedBytesDelta,
      "staged bytes",
    );
    if (stagedBytes > state.binding.maxStagingBytesPerSession)
      throw replicationError("ResourceLimit", "session staging limit exceeded");
    if (
      state.nextSequence - state.compactedThrough - 1 >=
      state.binding.maxReceiptsPerSession
    )
      throw replicationError("ResourceLimit", "receipt row limit requires compaction");
    const chargedReceiptBytes = checkedAdd(
      request.acknowledgement.byteLength,
      request.batchEnvelopeDigest.byteLength,
      "receipt bytes",
    );
    const receiptBytes = checkedAdd(
      state.receiptBytes,
      chargedReceiptBytes,
      "receipt bytes",
    );
    if (receiptBytes > state.binding.maxReceiptBytesPerSession)
      throw replicationError("ResourceLimit", "receipt byte limit requires compaction");
    const chainInput = new Uint8Array(
      RECEIPT_CHAIN_DIGEST_DOMAIN.byteLength + 32 + 8 + 32,
    );
    chainInput.set(RECEIPT_CHAIN_DIGEST_DOMAIN);
    chainInput.set(
      fromHex(state.chainDigest, 32, "chainDigest"),
      RECEIPT_CHAIN_DIGEST_DOMAIN.byteLength,
    );
    chainInput.set(
      sequenceBytes(request.sequence),
      RECEIPT_CHAIN_DIGEST_DOMAIN.byteLength + 32,
    );
    chainInput.set(
      request.batchEnvelopeDigest,
      RECEIPT_CHAIN_DIGEST_DOMAIN.byteLength + 40,
    );
    state.chainDigest = toHex(this.#hash(chainInput));
    state.phase = request.nextPhase;
    state.cursor = toHex(request.nextCursor);
    state.cursorDigest = toHex(request.nextCursorDigest);
    state.nextSequence += 1;
    state.acceptedEntries = checkedAdd(
      state.acceptedEntries,
      request.entryCount,
      "accepted entries",
    );
    state.acceptedBytes = checkedAdd(
      state.acceptedBytes,
      request.payloadByteCount,
      "accepted bytes",
    );
    state.receiptBytes = receiptBytes;
    if (
      acknowledgement.sessionId !== request.sessionId ||
      acknowledgement.sequence !== request.sequence ||
      acknowledgement.phase !== request.phase ||
      !equalBytes(acknowledgement.batchEnvelopeDigest, request.batchEnvelopeDigest) ||
      acknowledgement.nextPhase !== request.nextPhase ||
      !equalBytes(acknowledgement.cursor, request.nextCursor) ||
      !equalBytes(acknowledgement.cursorDigest, request.nextCursorDigest) ||
      !equalBytes(
        acknowledgement.chainDigest,
        fromHex(state.chainDigest, 32, "chainDigest"),
      ) ||
      acknowledgement.acceptedEntries !== state.acceptedEntries ||
      acknowledgement.acceptedBytes !== state.acceptedBytes ||
      acknowledgement.stagedBytes !== stagedBytes
    )
      throw replicationError(
        "BatchReplayMismatch",
        "acknowledgement does not bind the committed batch state",
      );
    const encodedState = encodeJson(state);
    this.#assertAggregateAdmission(state.binding, {
      metadataBytes:
        encodedState.byteLength -
        row.cursor!.byteLength +
        DURABLE_METADATA_ROW_BYTES +
        request.acknowledgement.byteLength,
    });
    this.#tx.run(
      "INSERT INTO efs_replication_receipts(session_id,batch_index,digest,encoded) VALUES(?,?,?,?)",
      [
        request.operationId,
        request.sequence,
        request.batchEnvelopeDigest,
        request.acknowledgement,
      ],
    );
    const updated = this.#tx.run(
      "UPDATE efs_replication_sessions SET cursor=?,staged_bytes=? WHERE id=? AND state=0 AND nonce=?",
      [encodedState, stagedBytes, request.operationId, request.ownerNonce],
    );
    if (updated.changes !== 1)
      throw replicationError(
        "Busy",
        "replication session changed during batch acceptance",
      );
    return Object.freeze({
      replayed: false,
      acknowledgement: new Uint8Array(request.acknowledgement),
      session: snapshot(state, stagedBytes),
    });
  }

  compactReceipts(request: {
    readonly operationId: string;
    readonly ownerNonce: Uint8Array;
    readonly throughSequence: number;
    readonly maxRows: number;
  }): Readonly<{ readonly compactedThrough: number; readonly deletedRows: number; readonly deletedBytes: number }> {
    const loaded = this.#load(request.operationId);
    const { row, state } = loaded;
    this.#assertOwner(state, state.binding.sessionId, request.ownerNonce);
    safeNonnegative(request.throughSequence, "throughSequence");
    safePositive(request.maxRows, "maxRows");
    if (request.maxRows > state.binding.maxReceiptsPerSession)
      throw replicationError("ResourceLimit", "receipt compaction batch is too large");
    const target = Math.min(request.throughSequence, state.nextSequence - 1);
    if (target <= state.compactedThrough)
      return Object.freeze({ compactedThrough: state.compactedThrough, deletedRows: 0, deletedBytes: 0 });
    const rows = this.#tx.all<ReceiptRow & { batch_index: number } & SqliteRow>(
      "SELECT batch_index,digest,encoded FROM efs_replication_receipts WHERE session_id=? AND batch_index>? AND batch_index<=? ORDER BY batch_index LIMIT ?",
      [request.operationId, state.compactedThrough, target, request.maxRows],
      { maxRows: request.maxRows, maxBytes: state.binding.maxReceiptBytesPerSession + 4096 },
    );
    if (rows.length === 0)
      throw replicationError("ECORRUPT", "receipt compaction found a missing receipt");
    let deletedBytes = 0;
    for (const receipt of rows) {
      deletedBytes = checkedAdd(
        deletedBytes,
        receipt.digest.byteLength + receipt.encoded.byteLength,
        "receipt bytes",
      );
      this.#tx.run(
        "DELETE FROM efs_replication_receipts WHERE session_id=? AND batch_index=? AND digest=?",
        [request.operationId, receipt.batch_index, receipt.digest],
      );
    }
    const compactedThrough = rows.length < request.maxRows ? target : rows[rows.length - 1]!.batch_index;
    state.compactedThrough = compactedThrough;
    state.receiptBytes -= deletedBytes;
    if (state.receiptBytes < 0)
      throw replicationError("ECORRUPT", "receipt byte accounting underflow");
    const encodedState = encodeJson(state);
    this.#assertAggregateAdmission(state.binding, {
      metadataBytes: encodedState.byteLength - row.cursor!.byteLength - deletedBytes,
    });
    const updated = this.#tx.run(
      "UPDATE efs_replication_sessions SET cursor=? WHERE id=? AND state IN (0,1) AND nonce=?",
      [encodedState, request.operationId, request.ownerNonce],
    );
    if (updated.changes !== 1)
      throw replicationError("Busy", "receipt compaction raced with another operation");
    return Object.freeze({ compactedThrough, deletedRows: rows.length, deletedBytes });
  }

  abortSession(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
  }): void {
    const loaded = this.#load(request.operationId);
    const { state, row } = loaded;
    this.#assertOwner(state, request.sessionId, request.ownerNonce);
    safeNonnegative(request.now, "now");
    if (request.operationId === REPLICATION_IDENTITY_MARKER_ID || row.state < 0)
      throw replicationError("OperationMismatch", "the durable replication identity cannot be aborted");
    if (row.state === 1)
      return;
    const deleted = this.#tx.run(
      "DELETE FROM efs_replication_sessions WHERE id=? AND state=0 AND nonce=?",
      [request.operationId, request.ownerNonce],
    );
    if (deleted.changes !== 1)
      throw replicationError("Busy", "replication session changed during abort");
  }

  maintenance(request: { readonly now: number; readonly maxRows: number }): Readonly<{ readonly expiredSessions: number }> {
    safeNonnegative(request.now, "now");
    safePositive(request.maxRows, "maxRows");
    const rows = this.#tx.all<{ id: string } & SqliteRow>(
      "SELECT id FROM efs_replication_sessions WHERE state>=0 AND expires_at_ms<=? ORDER BY id LIMIT ?",
      [request.now, request.maxRows],
      { maxRows: request.maxRows, maxBytes: Math.max(1024, request.maxRows * 128) },
    );
    for (const session of rows) {
      this.#tx.run("DELETE FROM efs_replication_sessions WHERE id=? AND state>=0", [session.id]);
    }
    return Object.freeze({ expiredSessions: rows.length });
  }

  #replayReceipt(
    loaded: { row: SessionRow; state: DurableSessionState },
    request: ReplicationBatchAcceptanceRequest,
  ): Readonly<{
    replayed: boolean;
    acknowledgement: Uint8Array;
    session: ReplicationSessionSnapshot;
  }> {
    const row = this.#tx.all<ReceiptRow>(
      "SELECT digest,encoded FROM efs_replication_receipts WHERE session_id=? AND batch_index=?",
      [request.operationId, request.sequence],
      { maxRows: 1, maxBytes: loaded.state.binding.maxAcknowledgementBytes + 4096 },
    )[0];
    if (
      !row ||
      !(row.digest instanceof Uint8Array) ||
      !(row.encoded instanceof Uint8Array)
    )
      throw replicationError("BatchReplayMismatch", "batch receipt was compacted");
    if (!equalBytes(row.digest, request.batchEnvelopeDigest))
      throw replicationError(
        "BatchReplayMismatch",
        "replayed batch differs from its durable receipt",
      );
    boundedBytes(
      row.encoded,
      loaded.state.binding.maxAcknowledgementBytes,
      "acknowledgement",
    );
    return Object.freeze({
      replayed: true,
      acknowledgement: new Uint8Array(row.encoded),
      session: snapshot(loaded.state, loaded.row.staged_bytes),
    });
  }

  recordOutboundBatch(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly sequence: number;
    readonly phase: ReplicationPhase;
    readonly nextPhase: ReplicationPhase;
    readonly nextCursor: Uint8Array;
    readonly nextCursorDigest: Uint8Array;
  }): ReplicationSessionSnapshot {
    const loaded = this.#load(request.operationId);
    const { row, state } = loaded;
    this.#assertOwner(state, request.sessionId, request.ownerNonce);
    const terminalResultAcknowledgement =
      row.state === 1 &&
      state.terminalResultDigest !== null &&
      state.phase === "result-acknowledgement" &&
      request.phase === "result-acknowledgement" &&
      request.nextPhase === "cleanup";
    if (row.state !== 0 && !terminalResultAcknowledgement)
      throw replicationError("OperationMismatch", "replication session is terminal");
    if (request.sequence !== state.nextSequence)
      throw replicationError(
        "CursorMismatch",
        "outbound batch sequence is not the next sequence",
      );
    const missingContentRequestDuringTransfer =
      state.phase === "content-transfer" &&
      request.phase === "missing-content" &&
      request.nextPhase === "content-transfer";
    if (request.phase !== state.phase && !missingContentRequestDuringTransfer)
      throw replicationError(
        "CursorMismatch",
        "outbound batch phase differs from durable state",
      );
    validatePhaseAdvance(state.phase, request.nextPhase);
    publicCursor(request.nextCursor, state.binding.maxCursorBytes, "nextCursor");
    exactBytes(request.nextCursorDigest, 32, "nextCursorDigest");
    if (!equalBytes(this.#hash(request.nextCursor), request.nextCursorDigest))
      throw replicationError(
        "IntegrityFailure",
        "next cursor digest does not match bytes",
      );
    const priorBytes = row.cursor!.byteLength;
    state.phase = request.nextPhase;
    state.nextSequence += 1;
    state.cursor = toHex(request.nextCursor);
    state.cursorDigest = toHex(request.nextCursorDigest);
    const encodedState = encodeJson(state);
    this.#assertAggregateAdmission(state.binding, {
      activeSessions: terminalResultAcknowledgement ? 0 : 1,
      sessionRows: 1,
      metadataBytes:
        DURABLE_METADATA_ROW_BYTES + encodedState.byteLength - priorBytes,
    });
    this.#tx.run(
      "UPDATE efs_replication_sessions SET cursor=? WHERE id=? AND nonce=? AND (state=0 OR state=1)",
      [encodedState, request.operationId, request.ownerNonce],
    );
    return snapshot(state, row.staged_bytes);
  }

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
  }> {
    const { row, state } = this.#load(request.operationId);
    this.#assertOwner(state, request.sessionId, request.ownerNonce);
    if (row.state !== 0)
      throw replicationError("OperationMismatch", "replication session is terminal");
    safeNonnegative(request.wallNowMs, "wallNowMs");
    safeNonnegative(request.monotonicElapsedMs, "monotonicElapsedMs");
    safeNonnegative(request.delayMs, "delayMs");
    if (
      request.delayMs < state.binding.minRetryDelayMs ||
      request.delayMs > state.binding.maxRetryDelayMs
    )
      throw replicationError(
        "ResourceLimit",
        "retry delay is outside the negotiated bounds",
      );
    state.attempts = checkedAdd(state.attempts, 1, "retry attempts");
    state.elapsedRetryMs = checkedAdd(
      state.elapsedRetryMs,
      request.monotonicElapsedMs,
      "retry elapsed time",
    );
    state.lastWallClockMs = Math.max(state.lastWallClockMs, request.wallNowMs);
    const exhausted =
      state.attempts > state.binding.maxRetryAttempts ||
      state.elapsedRetryMs > state.binding.maxRetryElapsedMs ||
      state.lastWallClockMs > state.retryDeadlineMs;
    const encodedState = encodeJson(state);
    this.#assertAggregateAdmission(state.binding, {
      metadataBytes: encodedState.byteLength - row.cursor!.byteLength,
    });
    const updated = this.#tx.run(
      "UPDATE efs_replication_sessions SET cursor=? WHERE id=? AND state=0 AND nonce=?",
      [encodedState, request.operationId, request.ownerNonce],
    );
    if (updated.changes !== 1)
      throw replicationError("Busy", "replication attempt accounting raced");
    return Object.freeze({
      attempts: state.attempts,
      elapsedRetryMs: state.elapsedRetryMs,
      lastWallClockMs: state.lastWallClockMs,
      exhausted,
    });
  }

  storeTerminalResult(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly result: Uint8Array;
    readonly now: number;
  }): Uint8Array {
    const { row, state } = this.#load(request.operationId);
    this.#assertOwner(state, request.sessionId, request.ownerNonce);
    boundedBytes(
      request.result,
      state.binding.maxTerminalResultBytes,
      "terminal result",
    );
    safeNonnegative(request.now, "now");
    if (row.state === 1) {
      const retained = this.#terminalResult(request.operationId, state);
      if (!equalBytes(retained, request.result))
        throw replicationError("OperationMismatch", "terminal result already differs");
      return retained;
    }
    const resultDigest = this.#hash(request.result);
    state.terminalResultDigest = toHex(resultDigest);
    state.terminalResultBytes = request.result.byteLength;
    state.terminalExpiresAtMs = checkedAdd(
      request.now,
      state.binding.resultRetentionMs,
      "terminal result expiry",
    );
    const encodedState = encodeJson(state);
    this.#assertAggregateAdmission(state.binding, {
      activeSessions: -1,
      metadataBytes:
        encodedState.byteLength -
        row.cursor!.byteLength +
        DURABLE_METADATA_ROW_BYTES +
        request.result.byteLength,
    });
    this.#tx.run(
      "INSERT INTO efs_replication_receipts(session_id,batch_index,digest,encoded) VALUES(?,-1,?,?)",
      [request.operationId, resultDigest, request.result],
    );
    const updated = this.#tx.run(
      "UPDATE efs_replication_sessions SET state=1,cursor=?,expires_at_ms=? WHERE id=? AND state=0 AND nonce=?",
      [
        encodedState,
        state.terminalExpiresAtMs,
        request.operationId,
        request.ownerNonce,
      ],
    );
    if (updated.changes !== 1)
      throw replicationError("Busy", "terminal result raced with another operation");
    return new Uint8Array(request.result);
  }

  replayTerminalResult(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly resumeKey: Uint8Array;
    readonly now: number;
  }): Uint8Array {
    const { row, state } = this.#load(request.operationId);
    if (
      state.binding.sessionId !== request.sessionId ||
      !equalBytes(
        fromBoundedHex(state.binding.resumeKey, 256, "resumeKey"),
        request.resumeKey,
      )
    )
      throw replicationError("OperationMismatch", "terminal replay binding changed");
    safeNonnegative(request.now, "now");
    if (
      row.state !== 1 ||
      state.terminalResultDigest === null ||
      state.terminalExpiresAtMs === null
    )
      throw replicationError("OperationMismatch", "terminal result is not available");
    if (request.now > state.terminalExpiresAtMs)
      throw replicationError("CursorExpired", "terminal result retention expired");
    return this.#terminalResult(request.operationId, state);
  }

  #terminalResult(operationId: string, state: DurableSessionState): Uint8Array {
    const row = this.#tx.all<ReceiptRow>(
      "SELECT digest,encoded FROM efs_replication_receipts WHERE session_id=? AND batch_index=-1",
      [operationId],
      { maxRows: 1, maxBytes: state.binding.maxTerminalResultBytes + 256 },
    )[0];
    if (
      !row ||
      !(row.digest instanceof Uint8Array) ||
      !(row.encoded instanceof Uint8Array) ||
      state.terminalResultDigest === null ||
      !equalBytes(
        row.digest,
        fromHex(state.terminalResultDigest, 32, "terminalResultDigest"),
      ) ||
      !equalBytes(this.#hash(row.encoded), row.digest) ||
      row.encoded.byteLength !== state.terminalResultBytes
    )
      throw replicationError("ECORRUPT", "terminal result row is invalid");
    return new Uint8Array(row.encoded);
  }

  #assertOwner(
    state: DurableSessionState,
    sessionId: string,
    ownerNonce: Uint8Array,
  ): void {
    if (
      state.binding.sessionId !== sessionId ||
      !equalBytes(fromHex(state.binding.ownerNonce, 16, "ownerNonce"), ownerNonce)
    )
      throw replicationError("OperationMismatch", "replication session owner changed");
  }
}

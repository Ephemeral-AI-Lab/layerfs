import type { ReplicationPhase, ReplicationSemanticErrorRecord } from "./types.js";

export type ReplicationErrorCode =
  | "ProtocolMismatch"
  | "FilesystemMismatch"
  | "AuthorityMismatch"
  | "SchemaMismatch"
  | "CapabilityMismatch"
  | "IncompatibleLimit"
  | "UnauthorizedScope"
  | "ProvisioningRejected"
  | "OperationMismatch"
  | "MainDiverged"
  | "BaseRevisionMissing"
  | "BranchIdentityMismatch"
  | "BranchDiverged"
  | "CursorMismatch"
  | "CursorExpired"
  | "BatchReplayMismatch"
  | "StagingExpired"
  | "IntegrityFailure"
  | "ResourceLimit"
  | "Busy"
  | "TransportFailure"
  | "RetryExhausted"
  | "Aborted"
  | "Closed";

const RETRYABLE_CODES = new Set<ReplicationErrorCode>(["Busy", "TransportFailure"]);

export function isReplicationErrorRetryable(code: ReplicationErrorCode): boolean {
  return RETRYABLE_CODES.has(code);
}

export class ReplicationError extends Error {
  readonly name = "ReplicationError";
  readonly code: ReplicationErrorCode;
  readonly phase: ReplicationPhase | null;
  readonly sessionId: string | null;
  readonly retryable: boolean;

  constructor(
    code: ReplicationErrorCode,
    message: string,
    options: {
      readonly phase?: ReplicationPhase | null;
      readonly sessionId?: string | null;
      readonly retryable?: boolean;
      readonly cause?: unknown;
    } = {},
  ) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause });
    const retryable = isReplicationErrorRetryable(code);
    if (options.retryable !== undefined && options.retryable !== retryable)
      throw new TypeError(
        "ReplicationError retryability must match its canonical code policy",
      );
    this.code = code;
    this.phase = options.phase ?? null;
    this.sessionId = options.sessionId ?? null;
    this.retryable = retryable;
  }
}

export function replicationErrorRecord(
  error: ReplicationError,
): ReplicationSemanticErrorRecord {
  return Object.freeze({
    code: error.code,
    phase: error.phase,
    sessionId: error.sessionId,
    message: error.message,
    retryable: error.retryable,
  });
}

export function replicationErrorFromRecord(
  record: ReplicationSemanticErrorRecord,
): ReplicationError {
  if (record.retryable !== isReplicationErrorRetryable(record.code))
    throw new ReplicationError(
      "ProtocolMismatch",
      "semantic error retryability does not match its canonical code policy",
    );
  return new ReplicationError(record.code, record.message, {
    phase: record.phase,
    sessionId: record.sessionId,
    retryable: record.retryable,
  });
}

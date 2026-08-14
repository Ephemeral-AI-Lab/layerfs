import { ReplicationError } from "./errors.js";
import type {
  ReplicationCeilingLimits,
  ReplicationLimitPolicy,
  ReplicationLimits,
  ReplicationStorageCapabilities,
} from "./types.js";
import { positiveSafeInteger, PRE_NEGOTIATION_ENVELOPE_BYTES } from "./validation.js";

const MIB = 1024 * 1024;
const DAY_MS = 24 * 60 * 60 * 1000;

export const REPLICATION_LIMIT_FIELDS = Object.freeze([
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
  "maxRetryDelayMs",
] as const satisfies readonly (keyof ReplicationLimits)[]);

export const REPLICATION_CEILING_FIELDS = Object.freeze(
  REPLICATION_LIMIT_FIELDS.filter(
    (field): field is keyof ReplicationCeilingLimits => field !== "minRetryDelayMs",
  ),
);

export const COMPUTER_EFS_CARRIER_V1_LIMITS: Readonly<ReplicationLimits> =
  Object.freeze({
    maxBatchEntries: 256,
    maxBatchBytes: 3 * MIB - 64 * 1024,
    maxRequestBytes: 3 * MIB,
    maxResponseBytes: 3 * MIB,
    maxBufferedBytes: 10 * MIB,
    maxInFlightBatches: 1,
    maxConcurrentSessions: 16,
    maxStagingBytesPerSession: 128 * MIB,
    maxReplicationSessionRows: 10_000,
    maxReplicationMetadataBytes: 64 * MIB,
    maxReceiptsPerSession: 100_000,
    maxReceiptBytesPerSession: 16 * MIB,
    maxCursorBytes: 256,
    maxTerminalResultBytes: 1 * MIB,
    maxCursorAgeMs: DAY_MS,
    stagingLeaseMs: 15 * 60 * 1000,
    resultRetentionMs: 30 * DAY_MS,
    maxRetryAttempts: 8,
    maxRetryElapsedMs: 5 * 60 * 1000,
    minRetryDelayMs: 100,
    maxRetryDelayMs: 10_000,
  });

const BATCH_FRAMING_ALLOWANCE_BYTES = 64 * 1024;
const CODEC_HEADROOM_BYTES = 2 * MIB;

function snapshotLimits(limits: ReplicationLimits, name: string): ReplicationLimits {
  const output = {} as Record<keyof ReplicationLimits, number>;
  for (const field of REPLICATION_LIMIT_FIELDS)
    output[field] = positiveSafeInteger(limits[field], `${name}.${field}`);
  return output;
}

function snapshotPolicy(
  policy: ReplicationLimitPolicy,
  name: string,
): { readonly ceilings: ReplicationCeilingLimits; readonly floor: number } {
  const ceilings = {} as Record<keyof ReplicationCeilingLimits, number>;
  for (const field of REPLICATION_CEILING_FIELDS)
    ceilings[field] = positiveSafeInteger(
      policy.ceilings[field],
      `${name}.ceilings.${field}`,
    );
  const floor = positiveSafeInteger(
    policy.minRetryDelayMsFloor,
    `${name}.minRetryDelayMsFloor`,
  );
  return { ceilings, floor };
}

export function validateReplicationLimits(
  input: ReplicationLimits,
  name = "limits",
): Readonly<ReplicationLimits> {
  const limits = snapshotLimits(input, name);
  if (limits.maxInFlightBatches !== 1)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxInFlightBatches must equal 1 for efs-replication-v1`,
    );
  if (limits.minRetryDelayMs > limits.maxRetryDelayMs)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.minRetryDelayMs exceeds maxRetryDelayMs`,
    );
  if (
    limits.maxBatchBytes + BATCH_FRAMING_ALLOWANCE_BYTES > limits.maxRequestBytes ||
    limits.maxBatchBytes + BATCH_FRAMING_ALLOWANCE_BYTES > limits.maxResponseBytes
  )
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxBatchBytes plus canonical framing exceeds a request or response`,
    );
  if (
    limits.maxRequestBytes + limits.maxResponseBytes + CODEC_HEADROOM_BYTES >
    limits.maxBufferedBytes
  )
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxBufferedBytes cannot contain one request, one response, and codec headroom`,
    );
  if (limits.maxCursorBytes > PRE_NEGOTIATION_ENVELOPE_BYTES)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxCursorBytes exceeds the pre-negotiation envelope ceiling`,
    );
  if (limits.maxTerminalResultBytes > limits.maxResponseBytes)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxTerminalResultBytes exceeds maxResponseBytes`,
    );
  if (limits.maxBatchBytes > limits.maxStagingBytesPerSession)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxStagingBytesPerSession cannot contain one maximum batch`,
    );
  if (
    limits.maxReceiptBytesPerSession > limits.maxReplicationMetadataBytes ||
    limits.maxTerminalResultBytes > limits.maxReplicationMetadataBytes
  )
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name} durable receipt or terminal result exceeds replication metadata capacity`,
    );
  if (limits.stagingLeaseMs > limits.maxCursorAgeMs)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.stagingLeaseMs exceeds maxCursorAgeMs`,
    );
  return Object.freeze(limits);
}

export interface NegotiateReplicationLimitsOptions {
  readonly source: ReplicationLimits;
  readonly destination: ReplicationLimits;
  readonly sourcePolicy: ReplicationLimitPolicy;
  readonly destinationPolicy: ReplicationLimitPolicy;
  readonly hostProfile?: ReplicationLimits;
}

export function negotiateReplicationLimits(
  options: NegotiateReplicationLimitsOptions,
): Readonly<ReplicationLimits> {
  const source = validateReplicationLimits(options.source, "source");
  const destination = validateReplicationLimits(options.destination, "destination");
  const sourcePolicy = snapshotPolicy(options.sourcePolicy, "sourcePolicy");
  const destinationPolicy = snapshotPolicy(
    options.destinationPolicy,
    "destinationPolicy",
  );
  const host = validateReplicationLimits(
    options.hostProfile ?? COMPUTER_EFS_CARRIER_V1_LIMITS,
    "hostProfile",
  );
  const output = {} as Record<keyof ReplicationLimits, number>;
  for (const field of REPLICATION_CEILING_FIELDS)
    output[field] = Math.min(
      source[field],
      destination[field],
      sourcePolicy.ceilings[field],
      destinationPolicy.ceilings[field],
      host[field],
    );
  output.minRetryDelayMs = Math.max(
    source.minRetryDelayMs,
    destination.minRetryDelayMs,
    sourcePolicy.floor,
    destinationPolicy.floor,
    host.minRetryDelayMs,
  );
  return validateReplicationLimits(output, "effectiveLimits");
}

export function limitPolicyFromLimits(
  input: ReplicationLimits,
): Readonly<ReplicationLimitPolicy> {
  const limits = validateReplicationLimits(input);
  const ceilings = {} as Record<keyof ReplicationCeilingLimits, number>;
  for (const field of REPLICATION_CEILING_FIELDS) ceilings[field] = limits[field];
  return Object.freeze({
    ceilings: Object.freeze(ceilings),
    minRetryDelayMsFloor: limits.minRetryDelayMs,
  });
}

const STORAGE_CAPABILITY_FIELDS = Object.freeze([
  "maxBlobBytes",
  "maxManifestNodeBytes",
  "maxManifestDepth",
  "maxManagedPayloadBytes",
  "maxStagingPayloadBytes",
  "maxMaintenanceBytes",
  "maintenanceReserveBytes",
  "maxPermanentIdentifiers",
  "maxFinalTransactionRows",
  "maxFinalTransactionBytes",
] as const satisfies readonly (keyof ReplicationStorageCapabilities)[]);

export function validateReplicationStorageCapabilities(
  input: ReplicationStorageCapabilities,
  name = "storage",
): Readonly<ReplicationStorageCapabilities> {
  const storage = {} as Record<keyof ReplicationStorageCapabilities, number>;
  for (const field of STORAGE_CAPABILITY_FIELDS)
    storage[field] = positiveSafeInteger(input[field], `${name}.${field}`);
  if (storage.maxStagingPayloadBytes > storage.maxManagedPayloadBytes)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxStagingPayloadBytes exceeds maxManagedPayloadBytes`,
    );
  if (storage.maintenanceReserveBytes > storage.maxManagedPayloadBytes)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maintenanceReserveBytes exceeds maxManagedPayloadBytes`,
    );
  if (storage.maxFinalTransactionRows < 64)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxFinalTransactionRows is below the version 1 minimum of 64`,
    );
  return Object.freeze(storage);
}

export function validateLimitsAgainstStorage(
  inputLimits: ReplicationLimits,
  inputStorage: ReplicationStorageCapabilities,
  name = "capabilities",
): void {
  const limits = validateReplicationLimits(inputLimits, `${name}.limits`);
  const storage = validateReplicationStorageCapabilities(
    inputStorage,
    `${name}.storage`,
  );
  if (limits.maxStagingBytesPerSession > storage.maxStagingPayloadBytes)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxStagingBytesPerSession exceeds maxStagingPayloadBytes`,
    );
  if (
    limits.maxStagingBytesPerSession + storage.maintenanceReserveBytes >
    storage.maxManagedPayloadBytes
  )
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name} staging plus maintenance reserve exceeds managed payload capacity`,
    );
  if (limits.maxReplicationMetadataBytes > storage.maxMaintenanceBytes)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name}.maxReplicationMetadataBytes exceeds maxMaintenanceBytes`,
    );
}

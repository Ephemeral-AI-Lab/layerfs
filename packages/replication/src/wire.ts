import {
  isReplicationErrorRetryable,
  ReplicationError,
  type ReplicationErrorCode,
} from "./errors.js";
import { REPLICATION_LIMIT_FIELDS } from "./limits.js";
import { validateReplicationSessionId } from "./identifiers.js";
import {
  bytesToLowerHex,
  IncrementalReplicationSha256,
  replicationSha256,
} from "./sha256.js";
import {
  REPLICATION_HOST_PROFILE,
  type AuthorizedReplicationPeer,
  type CanonicalAuthorizationRecord,
  type CanonicalReplicationEnvelope,
  type FastCdcConfiguration,
  type ReplicationBatch,
  type ReplicationBatchAcknowledgement,
  type ReplicationBatchRecord,
  type ReplicationBranchGenerationFragment,
  type ReplicationCapabilities,
  type ReplicationCheckpointFragment,
  type ReplicationCursorBinding,
  type ReplicationFeatures,
  type ReplicationLimits,
  type ReplicationPhase,
  type ReplicationPlan,
  type ReplicationRevisionFragment,
  type ReplicationSemanticErrorRecord,
  type ReplicationStorageCapabilities,
  type ReplicationTerminalResultRecord,
} from "./types.js";
import {
  boundedArray,
  canonicalUtf8,
  MAX_CANONICAL_ARRAY_ENTRIES,
  MAX_CANONICAL_ERROR_TEXT_BYTES,
  MAX_CANONICAL_TEXT_BYTES,
  nonnegativeSafeInteger,
  PRE_NEGOTIATION_ENVELOPE_BYTES,
} from "./validation.js";

const MAGIC = Uint8Array.of(0x45, 0x46, 0x53, 0x52); // EFSR
const WIRE_VERSION = 1;
const ENVELOPE_HEADER_BYTES = 12;
const BATCH_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/batch-payload\0",
);
const CURSOR_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/cursor-binding\0",
);
const AUTHORIZATION_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/authorization\0",
);
const EFFECTIVE_LIMITS_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/effective-limits\0",
);
const CAPABILITY_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/capabilities\0",
);
const BATCH_ENVELOPE_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/batch-envelope\0",
);
const OWNER_NONCE_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/owner-nonce\0",
);
const RECEIPT_CHAIN_DIGEST_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/receipt-chain\0",
);

const ENVELOPE_TAGS = {
  capabilities: 0x01,
  authorization: 0x02,
  batch: 0x03,
  cursor: 0x04,
  "revision-fragment": 0x05,
  "checkpoint-fragment": 0x06,
  "branch-generation-fragment": 0x07,
  "terminal-result": 0x08,
  error: 0x09,
  "batch-acknowledgement": 0x0a,
} as const;

const TAG_TO_ENVELOPE = new Map<number, keyof typeof ENVELOPE_TAGS>(
  Object.entries(ENVELOPE_TAGS).map(([name, tag]) => [
    tag,
    name as keyof typeof ENVELOPE_TAGS,
  ]),
);

const PLAN_TAGS = {
  "authority-main-to-replica": 0x01,
  "authority-branch-to-replica": 0x02,
  "replica-branch-to-authority": 0x03,
  "replica-branch-to-replica": 0x04,
} as const;

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

const ERROR_CODES = [
  "ProtocolMismatch",
  "FilesystemMismatch",
  "AuthorityMismatch",
  "SchemaMismatch",
  "CapabilityMismatch",
  "IncompatibleLimit",
  "UnauthorizedScope",
  "ProvisioningRejected",
  "OperationMismatch",
  "MainDiverged",
  "BaseRevisionMissing",
  "BranchIdentityMismatch",
  "BranchDiverged",
  "CursorMismatch",
  "CursorExpired",
  "BatchReplayMismatch",
  "StagingExpired",
  "IntegrityFailure",
  "ResourceLimit",
  "Busy",
  "TransportFailure",
  "RetryExhausted",
  "Aborted",
  "Closed",
] as const satisfies readonly ReplicationErrorCode[];

const RECORD_TAGS = {
  "object-descriptor": 0x01,
  "object-payload": 0x02,
  "manifest-root-descriptor": 0x03,
  "manifest-node-descriptor": 0x04,
  "missing-content": 0x05,
  "revision-fragment": 0x06,
  "checkpoint-fragment": 0x07,
  "branch-generation-fragment": 0x08,
  "terminal-result": 0x09,
} as const;

type EncodeCallback = (writer: CanonicalWriter) => void;

class CanonicalWriter {
  readonly #bytes: Uint8Array | null;
  readonly #hasher: IncrementalReplicationSha256 | null;
  readonly #integerScratch = new Uint8Array(8);
  #offset = 0;

  constructor(
    bytes: Uint8Array | null,
    hasher: IncrementalReplicationSha256 | null = null,
  ) {
    this.#bytes = bytes;
    this.#hasher = hasher;
  }

  get length(): number {
    return this.#offset;
  }

  u8(value: number, name: string): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xff)
      throw new ReplicationError("ProtocolMismatch", `${name} is not uint8`);
    if (this.#bytes) this.#bytes[this.#offset] = value;
    if (this.#hasher) {
      this.#integerScratch[0] = value;
      this.#hasher.update(this.#integerScratch.subarray(0, 1));
    }
    this.#offset += 1;
  }

  u16(value: number, name: string): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff)
      throw new ReplicationError("ProtocolMismatch", `${name} is not uint16`);
    if (this.#bytes)
      new DataView(this.#bytes.buffer).setUint16(this.#offset, value, false);
    if (this.#hasher) {
      new DataView(this.#integerScratch.buffer).setUint16(0, value, false);
      this.#hasher.update(this.#integerScratch.subarray(0, 2));
    }
    this.#offset += 2;
  }

  u32(value: number, name: string): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff)
      throw new ReplicationError("ProtocolMismatch", `${name} is not uint32`);
    if (this.#bytes)
      new DataView(this.#bytes.buffer).setUint32(this.#offset, value, false);
    if (this.#hasher) {
      new DataView(this.#integerScratch.buffer).setUint32(0, value, false);
      this.#hasher.update(this.#integerScratch.subarray(0, 4));
    }
    this.#offset += 4;
  }

  u64(value: number, name: string): void {
    nonnegativeSafeInteger(value, name);
    if (this.#bytes)
      new DataView(this.#bytes.buffer).setBigUint64(this.#offset, BigInt(value), false);
    if (this.#hasher) {
      new DataView(this.#integerScratch.buffer).setBigUint64(0, BigInt(value), false);
      this.#hasher.update(this.#integerScratch);
    }
    this.#offset += 8;
  }

  boolean(value: boolean, name: string): void {
    if (typeof value !== "boolean")
      throw new ReplicationError("ProtocolMismatch", `${name} must be boolean`);
    this.u8(value ? 1 : 0, name);
  }

  fixedBytes(value: Uint8Array, length: number, name: string): void {
    if (!(value instanceof Uint8Array) || value.byteLength !== length)
      throw new ReplicationError(
        "ProtocolMismatch",
        `${name} must contain exactly ${length} bytes`,
      );
    if (this.#bytes) this.#bytes.set(value, this.#offset);
    if (this.#hasher) this.#hasher.update(value);
    this.#offset += length;
  }

  bytes(value: Uint8Array, maximum: number, name: string): void {
    if (!(value instanceof Uint8Array) || value.byteLength > maximum)
      throw new ReplicationError(
        "ProtocolMismatch",
        `${name} exceeds its ${maximum}-byte limit`,
      );
    this.u32(value.byteLength, `${name}.length`);
    if (this.#bytes) this.#bytes.set(value, this.#offset);
    if (this.#hasher) this.#hasher.update(value);
    this.#offset += value.byteLength;
  }

  text(
    value: string,
    name: string,
    maximum = MAX_CANONICAL_TEXT_BYTES,
    allowEmpty = false,
  ): void {
    const bytes = canonicalUtf8(value, name, maximum, allowEmpty);
    this.u32(bytes.byteLength, `${name}.length`);
    if (this.#bytes) this.#bytes.set(bytes, this.#offset);
    if (this.#hasher) this.#hasher.update(bytes);
    this.#offset += bytes.byteLength;
  }

  optional<T>(value: T | null, name: string, encode: (value: T) => void): void {
    if (value === null) {
      this.u8(0, `${name}.tag`);
      return;
    }
    this.u8(1, `${name}.tag`);
    encode(value);
  }
}

class CanonicalReader {
  readonly #bytes: Uint8Array;
  #offset = 0;

  constructor(bytes: Uint8Array) {
    this.#bytes = bytes;
  }

  get remaining(): number {
    return this.#bytes.byteLength - this.#offset;
  }

  #take(length: number, name: string): Uint8Array {
    if (!Number.isSafeInteger(length) || length < 0 || length > this.remaining)
      throw new ReplicationError("ProtocolMismatch", `${name} is truncated`);
    const output = this.#bytes.subarray(this.#offset, this.#offset + length);
    this.#offset += length;
    return output;
  }

  u8(name: string): number {
    return this.#take(1, name)[0]!;
  }

  u16(name: string): number {
    const bytes = this.#take(2, name);
    return new DataView(bytes.buffer, bytes.byteOffset, 2).getUint16(0, false);
  }

  u32(name: string): number {
    const bytes = this.#take(4, name);
    return new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, false);
  }

  u64(name: string): number {
    const bytes = this.#take(8, name);
    const value = new DataView(bytes.buffer, bytes.byteOffset, 8).getBigUint64(
      0,
      false,
    );
    if (value > BigInt(Number.MAX_SAFE_INTEGER))
      throw new ReplicationError(
        "ProtocolMismatch",
        `${name} exceeds safe integer range`,
      );
    return Number(value);
  }

  boolean(name: string): boolean {
    const value = this.u8(name);
    if (value !== 0 && value !== 1)
      throw new ReplicationError(
        "ProtocolMismatch",
        `${name} has a noncanonical boolean`,
      );
    return value === 1;
  }

  fixedBytes(length: number, name: string): Uint8Array {
    return this.#take(length, name);
  }

  bytes(maximum: number, name: string): Uint8Array {
    const length = this.u32(`${name}.length`);
    if (length > maximum)
      throw new ReplicationError(
        "ProtocolMismatch",
        `${name} exceeds its ${maximum}-byte limit`,
      );
    return this.fixedBytes(length, name);
  }

  text(name: string, maximum = MAX_CANONICAL_TEXT_BYTES, allowEmpty = false): string {
    const bytes = this.bytes(maximum, name);
    let value: string;
    try {
      value = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw new ReplicationError("ProtocolMismatch", `${name} is not valid UTF-8`);
    }
    canonicalUtf8(value, name, maximum, allowEmpty);
    return value;
  }

  optional<T>(name: string, decode: () => T): T | null {
    const tag = this.u8(`${name}.tag`);
    if (tag === 0) return null;
    if (tag !== 1)
      throw new ReplicationError(
        "ProtocolMismatch",
        `${name} has an unknown optional tag`,
      );
    return decode();
  }

  nested(length: number, name: string): CanonicalReader {
    return new CanonicalReader(this.#take(length, name));
  }

  finish(name: string): void {
    if (this.remaining !== 0)
      throw new ReplicationError("ProtocolMismatch", `${name} contains trailing bytes`);
  }
}

function encodeExact(callback: EncodeCallback): Uint8Array {
  const sizer = new CanonicalWriter(null);
  callback(sizer);
  const output = new Uint8Array(sizer.length);
  const writer = new CanonicalWriter(output);
  callback(writer);
  if (writer.length !== output.byteLength)
    throw new ReplicationError(
      "ProtocolMismatch",
      "canonical value changed while encoding",
    );
  return output;
}

export function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  let different = 0;
  for (let index = 0; index < left.byteLength; index += 1)
    different |= left[index]! ^ right[index]!;
  return different === 0;
}

function digestDomain(domain: Uint8Array, payload: Uint8Array): Uint8Array {
  return new IncrementalReplicationSha256().update(domain).update(payload).digest();
}

function encodePlan(
  writer: CanonicalWriter,
  plan: ReplicationPlan,
  name: string,
): void {
  const tag = PLAN_TAGS[plan.flow];
  if (tag === undefined)
    throw new ReplicationError("UnauthorizedScope", `${name}.flow is unsupported`);
  writer.u8(tag, `${name}.flow`);
  if (plan.flow !== "authority-main-to-replica")
    writer.text(plan.branchId, `${name}.branchId`, 200);
}

function decodePlan(reader: CanonicalReader, name: string): ReplicationPlan {
  const tag = reader.u8(`${name}.flow`);
  switch (tag) {
    case 0x01:
      return { flow: "authority-main-to-replica" };
    case 0x02:
      return {
        flow: "authority-branch-to-replica",
        branchId: reader.text(`${name}.branchId`, 200),
      };
    case 0x03:
      return {
        flow: "replica-branch-to-authority",
        branchId: reader.text(`${name}.branchId`, 200),
      };
    case 0x04:
      return {
        flow: "replica-branch-to-replica",
        branchId: reader.text(`${name}.branchId`, 200),
      };
    default:
      throw new ReplicationError(
        "UnauthorizedScope",
        `${name}.flow has an unknown tag`,
      );
  }
}

function encodeFastCdc(
  writer: CanonicalWriter,
  value: FastCdcConfiguration,
  name: string,
): void {
  if (
    !Number.isSafeInteger(value.minimum) ||
    !Number.isSafeInteger(value.average) ||
    !Number.isSafeInteger(value.maximum) ||
    value.minimum <= 0 ||
    value.minimum > value.average ||
    value.average > value.maximum ||
    !Number.isInteger(Math.log2(value.average))
  )
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} is not a valid FastCDC row`,
    );
  writer.u32(value.minimum, `${name}.minimum`);
  writer.u32(value.average, `${name}.average`);
  writer.u32(value.maximum, `${name}.maximum`);
}

function decodeFastCdc(reader: CanonicalReader, name: string): FastCdcConfiguration {
  const value = {
    minimum: reader.u32(`${name}.minimum`),
    average: reader.u32(`${name}.average`),
    maximum: reader.u32(`${name}.maximum`),
  };
  if (
    value.minimum === 0 ||
    value.minimum > value.average ||
    value.average > value.maximum ||
    !Number.isInteger(Math.log2(value.average))
  )
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} is not a valid FastCDC row`,
    );
  return value;
}

function encodeFeatures(
  writer: CanonicalWriter,
  value: ReplicationFeatures,
  name: string,
): void {
  writer.boolean(value.authorityMainToReplica, `${name}.authorityMainToReplica`);
  writer.boolean(value.authorityBranchToReplica, `${name}.authorityBranchToReplica`);
  writer.boolean(value.replicaBranchToAuthority, `${name}.replicaBranchToAuthority`);
  writer.boolean(value.replicaBranchToReplica, `${name}.replicaBranchToReplica`);
  writer.boolean(value.checkpointBootstrap, `${name}.checkpointBootstrap`);
  writer.boolean(
    value.segmentedMerkleManifestTransfer,
    `${name}.segmentedMerkleManifestTransfer`,
  );
  writer.boolean(value.durableStagingLeases, `${name}.durableStagingLeases`);
  writer.boolean(value.physicalRestartRecovery, `${name}.physicalRestartRecovery`);
  writer.boolean(value.terminalResultReplication, `${name}.terminalResultReplication`);
  writer.boolean(value.freshReplicaProvisioning, `${name}.freshReplicaProvisioning`);
}

function decodeFeatures(reader: CanonicalReader, name: string): ReplicationFeatures {
  return {
    authorityMainToReplica: reader.boolean(`${name}.authorityMainToReplica`),
    authorityBranchToReplica: reader.boolean(`${name}.authorityBranchToReplica`),
    replicaBranchToAuthority: reader.boolean(`${name}.replicaBranchToAuthority`),
    replicaBranchToReplica: reader.boolean(`${name}.replicaBranchToReplica`),
    checkpointBootstrap: reader.boolean(`${name}.checkpointBootstrap`),
    segmentedMerkleManifestTransfer: reader.boolean(
      `${name}.segmentedMerkleManifestTransfer`,
    ),
    durableStagingLeases: reader.boolean(`${name}.durableStagingLeases`),
    physicalRestartRecovery: reader.boolean(`${name}.physicalRestartRecovery`),
    terminalResultReplication: reader.boolean(`${name}.terminalResultReplication`),
    freshReplicaProvisioning: reader.boolean(`${name}.freshReplicaProvisioning`),
  };
}

function encodeLimits(
  writer: CanonicalWriter,
  limits: ReplicationLimits,
  name: string,
): void {
  for (const field of REPLICATION_LIMIT_FIELDS)
    writer.u64(limits[field], `${name}.${field}`);
}

function decodeLimits(reader: CanonicalReader, name: string): ReplicationLimits {
  const output = {} as Record<keyof ReplicationLimits, number>;
  for (const field of REPLICATION_LIMIT_FIELDS)
    output[field] = reader.u64(`${name}.${field}`);
  return output;
}

const STORAGE_FIELDS = [
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
] as const satisfies readonly (keyof ReplicationStorageCapabilities)[];

function encodeStorage(
  writer: CanonicalWriter,
  storage: ReplicationStorageCapabilities,
  name: string,
): void {
  for (const field of STORAGE_FIELDS) writer.u64(storage[field], `${name}.${field}`);
}

function decodeStorage(
  reader: CanonicalReader,
  name: string,
): ReplicationStorageCapabilities {
  const output = {} as Record<keyof ReplicationStorageCapabilities, number>;
  for (const field of STORAGE_FIELDS) output[field] = reader.u64(`${name}.${field}`);
  return output;
}

function encodeTextArray(
  writer: CanonicalWriter,
  values: readonly string[],
  name: string,
): void {
  boundedArray(values, name);
  writer.u32(values.length, `${name}.count`);
  for (let index = 0; index < values.length; index += 1)
    writer.text(values[index]!, `${name}[${index}]`);
}

function decodeTextArray(reader: CanonicalReader, name: string): readonly string[] {
  const count = reader.u32(`${name}.count`);
  if (count > MAX_CANONICAL_ARRAY_ENTRIES)
    throw new ReplicationError("ProtocolMismatch", `${name} has too many entries`);
  const output: string[] = [];
  for (let index = 0; index < count; index += 1)
    output.push(reader.text(`${name}[${index}]`));
  return Object.freeze(output);
}

function validPageBytes(value: number, name: string): 4096 | 8192 | 16384 {
  if (value !== 4096 && value !== 8192 && value !== 16384)
    throw new ReplicationError("CapabilityMismatch", `${name} is not 4, 8, or 16 KiB`);
  return value;
}

function encodeCapabilitiesValue(
  writer: CanonicalWriter,
  value: ReplicationCapabilities,
): void {
  encodeTextArray(writer, value.protocolVersions, "capabilities.protocolVersions");
  if (value.hostProfile !== REPLICATION_HOST_PROFILE)
    throw new ReplicationError("CapabilityMismatch", "unsupported host profile");
  writer.u8(1, "capabilities.hostProfile");
  writer.u8(
    value.provisioningState === "bound"
      ? 0
      : value.provisioningState === "unbound-replica"
        ? 1
        : 0xff,
    "capabilities.provisioningState",
  );
  writer.optional(value.filesystemId, "capabilities.filesystemId", (item) =>
    writer.text(item, "capabilities.filesystemId.value"),
  );
  writer.optional(value.authorityId, "capabilities.authorityId", (item) =>
    writer.text(item, "capabilities.authorityId.value"),
  );
  writer.optional(value.applicationId, "capabilities.applicationId", (item) =>
    writer.u32(item, "capabilities.applicationId.value"),
  );
  writer.optional(
    value.filesystemSchemaVersion,
    "capabilities.filesystemSchemaVersion",
    (item) => writer.u32(item, "capabilities.filesystemSchemaVersion.value"),
  );
  writer.u32(value.storageUserVersion, "capabilities.storageUserVersion");
  if (value.storageMigrationState !== "none")
    throw new ReplicationError("SchemaMismatch", "storage migration is in progress");
  writer.u8(0, "capabilities.storageMigrationState");
  boundedArray(
    value.readableFilesystemSchemaVersions,
    "capabilities.readableFilesystemSchemaVersions",
  );
  writer.u32(
    value.readableFilesystemSchemaVersions.length,
    "capabilities.readableFilesystemSchemaVersions.count",
  );
  for (let index = 0; index < value.readableFilesystemSchemaVersions.length; index += 1)
    writer.u32(
      value.readableFilesystemSchemaVersions[index]!,
      `capabilities.readableFilesystemSchemaVersions[${index}]`,
    );
  writer.u32(
    value.writableFilesystemSchemaVersion,
    "capabilities.writableFilesystemSchemaVersion",
  );
  writer.u8(
    value.role === "main-authority" ? 1 : value.role === "replica" ? 2 : 0xff,
    "capabilities.role",
  );
  if (value.hashAlgorithms.length !== 1 || value.hashAlgorithms[0] !== "sha256")
    throw new ReplicationError("CapabilityMismatch", "hashAlgorithms must be [sha256]");
  writer.u32(1, "capabilities.hashAlgorithms.count");
  writer.u8(1, "capabilities.hashAlgorithms[0]");
  writer.optional(
    value.activeManifestFormat,
    "capabilities.activeManifestFormat",
    (item) => writer.text(item, "capabilities.activeManifestFormat.value"),
  );
  encodeTextArray(
    writer,
    value.supportedManifestFormats,
    "capabilities.supportedManifestFormats",
  );
  writer.optional(
    value.activeChunkerFormat,
    "capabilities.activeChunkerFormat",
    (item) => writer.text(item, "capabilities.activeChunkerFormat.value"),
  );
  encodeTextArray(
    writer,
    value.supportedChunkerFormats,
    "capabilities.supportedChunkerFormats",
  );
  writer.optional(value.fastCdc, "capabilities.fastCdc", (item) =>
    encodeFastCdc(writer, item, "capabilities.fastCdc.value"),
  );
  boundedArray(
    value.supportedFastCdcConfigurations,
    "capabilities.supportedFastCdcConfigurations",
  );
  writer.u32(
    value.supportedFastCdcConfigurations.length,
    "capabilities.supportedFastCdcConfigurations.count",
  );
  for (let index = 0; index < value.supportedFastCdcConfigurations.length; index += 1)
    encodeFastCdc(
      writer,
      value.supportedFastCdcConfigurations[index]!,
      `capabilities.supportedFastCdcConfigurations[${index}]`,
    );
  writer.optional(
    value.copyOnWritePageBytes,
    "capabilities.copyOnWritePageBytes",
    (item) =>
      writer.u32(
        validPageBytes(item, "capabilities.copyOnWritePageBytes.value"),
        "capabilities.copyOnWritePageBytes.value",
      ),
  );
  boundedArray(
    value.supportedCopyOnWritePageBytes,
    "capabilities.supportedCopyOnWritePageBytes",
  );
  writer.u32(
    value.supportedCopyOnWritePageBytes.length,
    "capabilities.supportedCopyOnWritePageBytes.count",
  );
  for (let index = 0; index < value.supportedCopyOnWritePageBytes.length; index += 1)
    writer.u32(
      validPageBytes(
        value.supportedCopyOnWritePageBytes[index]!,
        `capabilities.supportedCopyOnWritePageBytes[${index}]`,
      ),
      `capabilities.supportedCopyOnWritePageBytes[${index}]`,
    );
  encodeFeatures(writer, value.features, "capabilities.features");
  encodeLimits(writer, value.limits, "capabilities.limits");
  encodeStorage(writer, value.storage, "capabilities.storage");
}

function decodeCapabilitiesValue(reader: CanonicalReader): ReplicationCapabilities {
  const protocolVersions = decodeTextArray(reader, "capabilities.protocolVersions");
  if (reader.u8("capabilities.hostProfile") !== 1)
    throw new ReplicationError("CapabilityMismatch", "unknown host profile tag");
  const provisioningTag = reader.u8("capabilities.provisioningState");
  if (provisioningTag !== 0 && provisioningTag !== 1)
    throw new ReplicationError("CapabilityMismatch", "unknown provisioning state tag");
  const filesystemId = reader.optional("capabilities.filesystemId", () =>
    reader.text("capabilities.filesystemId.value"),
  );
  const authorityId = reader.optional("capabilities.authorityId", () =>
    reader.text("capabilities.authorityId.value"),
  );
  const applicationId = reader.optional("capabilities.applicationId", () =>
    reader.u32("capabilities.applicationId.value"),
  );
  const filesystemSchemaVersion = reader.optional(
    "capabilities.filesystemSchemaVersion",
    () => reader.u32("capabilities.filesystemSchemaVersion.value"),
  );
  const storageUserVersion = reader.u32("capabilities.storageUserVersion");
  if (reader.u8("capabilities.storageMigrationState") !== 0)
    throw new ReplicationError("SchemaMismatch", "unknown storage migration state");
  const schemaCount = reader.u32("capabilities.readableFilesystemSchemaVersions.count");
  if (schemaCount > MAX_CANONICAL_ARRAY_ENTRIES)
    throw new ReplicationError(
      "ProtocolMismatch",
      "too many readable filesystem schema versions",
    );
  const readableFilesystemSchemaVersions: number[] = [];
  for (let index = 0; index < schemaCount; index += 1)
    readableFilesystemSchemaVersions.push(
      reader.u32(`capabilities.readableFilesystemSchemaVersions[${index}]`),
    );
  const writableFilesystemSchemaVersion = reader.u32(
    "capabilities.writableFilesystemSchemaVersion",
  );
  const roleTag = reader.u8("capabilities.role");
  if (roleTag !== 1 && roleTag !== 2)
    throw new ReplicationError("UnauthorizedScope", "unknown replication role");
  if (
    reader.u32("capabilities.hashAlgorithms.count") !== 1 ||
    reader.u8("capabilities.hashAlgorithms[0]") !== 1
  )
    throw new ReplicationError("CapabilityMismatch", "unsupported hash algorithm row");
  const activeManifestFormat = reader.optional(
    "capabilities.activeManifestFormat",
    () => reader.text("capabilities.activeManifestFormat.value"),
  );
  const supportedManifestFormats = decodeTextArray(
    reader,
    "capabilities.supportedManifestFormats",
  );
  const activeChunkerFormat = reader.optional("capabilities.activeChunkerFormat", () =>
    reader.text("capabilities.activeChunkerFormat.value"),
  );
  const supportedChunkerFormats = decodeTextArray(
    reader,
    "capabilities.supportedChunkerFormats",
  );
  const fastCdc = reader.optional("capabilities.fastCdc", () =>
    decodeFastCdc(reader, "capabilities.fastCdc.value"),
  );
  const fastCdcCount = reader.u32("capabilities.supportedFastCdcConfigurations.count");
  if (fastCdcCount > MAX_CANONICAL_ARRAY_ENTRIES)
    throw new ReplicationError("ProtocolMismatch", "too many FastCDC configurations");
  const supportedFastCdcConfigurations: FastCdcConfiguration[] = [];
  for (let index = 0; index < fastCdcCount; index += 1)
    supportedFastCdcConfigurations.push(
      decodeFastCdc(reader, `capabilities.supportedFastCdcConfigurations[${index}]`),
    );
  const copyOnWritePageBytes = reader.optional(
    "capabilities.copyOnWritePageBytes",
    () =>
      validPageBytes(
        reader.u32("capabilities.copyOnWritePageBytes.value"),
        "capabilities.copyOnWritePageBytes.value",
      ),
  );
  const pageCount = reader.u32("capabilities.supportedCopyOnWritePageBytes.count");
  if (pageCount > MAX_CANONICAL_ARRAY_ENTRIES)
    throw new ReplicationError("ProtocolMismatch", "too many COW page configurations");
  const supportedCopyOnWritePageBytes: (4096 | 8192 | 16384)[] = [];
  for (let index = 0; index < pageCount; index += 1)
    supportedCopyOnWritePageBytes.push(
      validPageBytes(
        reader.u32(`capabilities.supportedCopyOnWritePageBytes[${index}]`),
        `capabilities.supportedCopyOnWritePageBytes[${index}]`,
      ),
    );
  return {
    protocolVersions,
    hostProfile: REPLICATION_HOST_PROFILE,
    provisioningState: provisioningTag === 0 ? "bound" : "unbound-replica",
    filesystemId,
    authorityId,
    applicationId,
    filesystemSchemaVersion,
    storageUserVersion,
    storageMigrationState: "none",
    readableFilesystemSchemaVersions: Object.freeze(readableFilesystemSchemaVersions),
    writableFilesystemSchemaVersion,
    role: roleTag === 1 ? "main-authority" : "replica",
    hashAlgorithms: ["sha256"],
    activeManifestFormat,
    supportedManifestFormats,
    activeChunkerFormat,
    supportedChunkerFormats,
    fastCdc,
    supportedFastCdcConfigurations: Object.freeze(supportedFastCdcConfigurations),
    copyOnWritePageBytes,
    supportedCopyOnWritePageBytes: Object.freeze(supportedCopyOnWritePageBytes),
    features: decodeFeatures(reader, "capabilities.features"),
    limits: decodeLimits(reader, "capabilities.limits"),
    storage: decodeStorage(reader, "capabilities.storage"),
  };
}

export function encodeCapabilitiesPayload(value: ReplicationCapabilities): Uint8Array {
  return encodeExact((writer) => encodeCapabilitiesValue(writer, value));
}

function byteCompare(left: Uint8Array, right: Uint8Array): number {
  const length = Math.min(left.byteLength, right.byteLength);
  for (let index = 0; index < length; index += 1) {
    const difference = left[index]! - right[index]!;
    if (difference !== 0) return difference;
  }
  return left.byteLength - right.byteLength;
}

function canonicalPlans(plans: readonly ReplicationPlan[]): readonly ReplicationPlan[] {
  boundedArray(plans, "authorization.allowedPlans");
  const entries = plans.map((plan) => ({
    plan,
    bytes: encodeExact((writer) => encodePlan(writer, plan, "plan")),
  }));
  entries.sort((left, right) => byteCompare(left.bytes, right.bytes));
  for (let index = 1; index < entries.length; index += 1)
    if (byteCompare(entries[index - 1]!.bytes, entries[index]!.bytes) === 0)
      throw new ReplicationError(
        "UnauthorizedScope",
        "allowedPlans contains a duplicate",
      );
  return entries.map((entry) => entry.plan);
}

function encodeAuthorizationValue(
  writer: CanonicalWriter,
  record: CanonicalAuthorizationRecord,
): void {
  const value = record.authorization;
  writer.text(value.principalId, "authorization.principalId");
  writer.text(value.hostScopeId, "authorization.hostScopeId");
  writer.text(value.expectedFilesystemId, "authorization.expectedFilesystemId");
  writer.text(value.expectedAuthorityId, "authorization.expectedAuthorityId");
  writer.text(value.policyVersion, "authorization.policyVersion");
  if (value.hostProfile !== REPLICATION_HOST_PROFILE)
    throw new ReplicationError(
      "CapabilityMismatch",
      "unsupported authorization profile",
    );
  writer.u8(1, "authorization.hostProfile");
  for (const field of REPLICATION_LIMIT_FIELDS)
    if (field !== "minRetryDelayMs")
      writer.u64(
        value.limitPolicy.ceilings[field],
        `authorization.limitPolicy.ceilings.${field}`,
      );
  writer.u64(
    value.limitPolicy.minRetryDelayMsFloor,
    "authorization.limitPolicy.minRetryDelayMsFloor",
  );
  const plans = canonicalPlans(value.allowedPlans);
  writer.u32(plans.length, "authorization.allowedPlans.count");
  for (let index = 0; index < plans.length; index += 1)
    encodePlan(writer, plans[index]!, `authorization.allowedPlans[${index}]`);
  encodeLimits(writer, record.effectiveLimits, "authorization.effectiveLimits");
}

function decodeAuthorizationValue(
  reader: CanonicalReader,
): CanonicalAuthorizationRecord {
  const principalId = reader.text("authorization.principalId");
  const hostScopeId = reader.text("authorization.hostScopeId");
  const expectedFilesystemId = reader.text("authorization.expectedFilesystemId");
  const expectedAuthorityId = reader.text("authorization.expectedAuthorityId");
  const policyVersion = reader.text("authorization.policyVersion");
  if (reader.u8("authorization.hostProfile") !== 1)
    throw new ReplicationError("CapabilityMismatch", "unknown authorization profile");
  const ceilings = {} as Record<
    Exclude<keyof ReplicationLimits, "minRetryDelayMs">,
    number
  >;
  for (const field of REPLICATION_LIMIT_FIELDS)
    if (field !== "minRetryDelayMs")
      ceilings[field] = reader.u64(`authorization.limitPolicy.ceilings.${field}`);
  const minRetryDelayMsFloor = reader.u64(
    "authorization.limitPolicy.minRetryDelayMsFloor",
  );
  const planCount = reader.u32("authorization.allowedPlans.count");
  if (planCount > MAX_CANONICAL_ARRAY_ENTRIES)
    throw new ReplicationError("ProtocolMismatch", "too many authorization plans");
  const allowedPlans: ReplicationPlan[] = [];
  let previousBytes: Uint8Array | null = null;
  for (let index = 0; index < planCount; index += 1) {
    const plan = decodePlan(reader, `authorization.allowedPlans[${index}]`);
    const bytes = encodeExact((writer) => encodePlan(writer, plan, "plan"));
    if (previousBytes && byteCompare(previousBytes, bytes) >= 0)
      throw new ReplicationError(
        "ProtocolMismatch",
        "authorization plans are duplicated or not in canonical order",
      );
    previousBytes = bytes;
    allowedPlans.push(plan);
  }
  const authorization: AuthorizedReplicationPeer = {
    principalId,
    hostScopeId,
    expectedFilesystemId,
    expectedAuthorityId,
    policyVersion,
    hostProfile: REPLICATION_HOST_PROFILE,
    limitPolicy: { ceilings, minRetryDelayMsFloor },
    allowedPlans: Object.freeze(allowedPlans),
  };
  return {
    authorization,
    effectiveLimits: decodeLimits(reader, "authorization.effectiveLimits"),
  };
}

export function encodeAuthorizationPayload(
  value: CanonicalAuthorizationRecord,
): Uint8Array {
  return encodeExact((writer) => encodeAuthorizationValue(writer, value));
}

export function capabilityDigest(
  value: ReplicationCapabilities,
  effectiveLimits: ReplicationLimits,
): Uint8Array {
  const hasher = new IncrementalReplicationSha256()
    .update(CAPABILITY_DIGEST_DOMAIN)
    .update(encodeCapabilitiesPayload(value));
  encodeLimits(
    new CanonicalWriter(null, hasher),
    effectiveLimits,
    "capabilityDigest.effectiveLimits",
  );
  return hasher.digest();
}

export function capabilityDigestHex(
  value: ReplicationCapabilities,
  effectiveLimits: ReplicationLimits,
): string {
  return bytesToLowerHex(capabilityDigest(value, effectiveLimits));
}

export function authorizationDigest(value: CanonicalAuthorizationRecord): Uint8Array {
  return digestDomain(AUTHORIZATION_DIGEST_DOMAIN, encodeAuthorizationPayload(value));
}

export function authorizationDigestHex(value: CanonicalAuthorizationRecord): string {
  return bytesToLowerHex(authorizationDigest(value));
}

/** Digest of the exact negotiated limits row, independent of either policy. */
export function effectiveLimitsDigest(value: ReplicationLimits): Uint8Array {
  const hasher = new IncrementalReplicationSha256().update(
    EFFECTIVE_LIMITS_DIGEST_DOMAIN,
  );
  encodeLimits(new CanonicalWriter(null, hasher), value, "effectiveLimitsDigest");
  return hasher.digest();
}

export function effectiveLimitsDigestHex(value: ReplicationLimits): string {
  return bytesToLowerHex(effectiveLimitsDigest(value));
}

function phaseTag(phase: ReplicationPhase, name: string): number {
  const index = PHASES.indexOf(phase);
  if (index < 0)
    throw new ReplicationError("ProtocolMismatch", `${name} is not a version 1 phase`);
  return index + 1;
}

function decodePhase(reader: CanonicalReader, name: string): ReplicationPhase {
  const tag = reader.u8(name);
  const phase = PHASES[tag - 1];
  if (!phase)
    throw new ReplicationError("ProtocolMismatch", `${name} has an unknown phase tag`);
  return phase;
}

function encodeCursorValue(
  writer: CanonicalWriter,
  value: ReplicationCursorBinding,
): void {
  writer.text(validateReplicationSessionId(value.sessionId), "cursor.sessionId");
  writer.fixedBytes(value.ownerNonceDigest, 32, "cursor.ownerNonceDigest");
  writer.text(value.sourceFilesystemId, "cursor.sourceFilesystemId");
  writer.text(value.destinationFilesystemId, "cursor.destinationFilesystemId");
  encodePlan(writer, value.plan, "cursor.plan");
  writer.text(value.selectedIdentity, "cursor.selectedIdentity");
  writer.optional(value.selectedGeneration, "cursor.selectedGeneration", (item) =>
    writer.u64(item, "cursor.selectedGeneration.value"),
  );
  writer.u8(phaseTag(value.phase, "cursor.phase"), "cursor.phase");
  writer.u64(value.nextSequence, "cursor.nextSequence");
  writer.fixedBytes(value.capabilityDigest, 32, "cursor.capabilityDigest");
}

function decodeCursorValue(reader: CanonicalReader): ReplicationCursorBinding {
  return {
    sessionId: validateReplicationSessionId(reader.text("cursor.sessionId")),
    ownerNonceDigest: reader.fixedBytes(32, "cursor.ownerNonceDigest"),
    sourceFilesystemId: reader.text("cursor.sourceFilesystemId"),
    destinationFilesystemId: reader.text("cursor.destinationFilesystemId"),
    plan: decodePlan(reader, "cursor.plan"),
    selectedIdentity: reader.text("cursor.selectedIdentity"),
    selectedGeneration: reader.optional("cursor.selectedGeneration", () =>
      reader.u64("cursor.selectedGeneration.value"),
    ),
    phase: decodePhase(reader, "cursor.phase"),
    nextSequence: reader.u64("cursor.nextSequence"),
    capabilityDigest: reader.fixedBytes(32, "cursor.capabilityDigest"),
  };
}

export function encodeCursorBindingPayload(
  value: ReplicationCursorBinding,
): Uint8Array {
  return encodeExact((writer) => encodeCursorValue(writer, value));
}

export function cursorBindingDigest(value: ReplicationCursorBinding): Uint8Array {
  return digestDomain(CURSOR_DIGEST_DOMAIN, encodeCursorBindingPayload(value));
}

export function cursorBindingDigestHex(value: ReplicationCursorBinding): string {
  return bytesToLowerHex(cursorBindingDigest(value));
}

export function replicationOwnerNonceDigest(ownerNonce: Uint8Array): Uint8Array {
  if (!(ownerNonce instanceof Uint8Array) || ownerNonce.byteLength !== 16)
    throw new ReplicationError(
      "ProtocolMismatch",
      "replication owner nonce must contain exactly 16 bytes",
    );
  return digestDomain(OWNER_NONCE_DIGEST_DOMAIN, ownerNonce);
}

function encodeBatchAcknowledgementValue(
  writer: CanonicalWriter,
  value: ReplicationBatchAcknowledgement,
): void {
  validateAcknowledgementPhaseAdvance(value.phase, value.nextPhase);
  writer.text(
    validateReplicationSessionId(value.sessionId),
    "batchAcknowledgement.sessionId",
  );
  writer.u64(value.sequence, "batchAcknowledgement.sequence");
  writer.u8(
    phaseTag(value.phase, "batchAcknowledgement.phase"),
    "batchAcknowledgement.phase",
  );
  writer.fixedBytes(
    value.batchEnvelopeDigest,
    32,
    "batchAcknowledgement.batchEnvelopeDigest",
  );
  writer.u8(
    phaseTag(value.nextPhase, "batchAcknowledgement.nextPhase"),
    "batchAcknowledgement.nextPhase",
  );
  writer.bytes(value.cursor, 256, "batchAcknowledgement.cursor");
  if (value.cursor.byteLength < 16)
    throw new ReplicationError(
      "ProtocolMismatch",
      "batch acknowledgement cursor must contain at least 128 random bits",
    );
  if (!equalBytes(replicationSha256(value.cursor), value.cursorDigest))
    throw new ReplicationError(
      "IntegrityFailure",
      "batch acknowledgement cursor digest does not match",
    );
  writer.fixedBytes(value.cursorDigest, 32, "batchAcknowledgement.cursorDigest");
  writer.fixedBytes(value.chainDigest, 32, "batchAcknowledgement.chainDigest");
  writer.u64(value.acceptedEntries, "batchAcknowledgement.acceptedEntries");
  writer.u64(value.acceptedBytes, "batchAcknowledgement.acceptedBytes");
  writer.u64(value.stagedBytes, "batchAcknowledgement.stagedBytes");
}

function decodeBatchAcknowledgementValue(
  reader: CanonicalReader,
): ReplicationBatchAcknowledgement {
  const value: ReplicationBatchAcknowledgement = {
    sessionId: validateReplicationSessionId(
      reader.text("batchAcknowledgement.sessionId"),
    ),
    sequence: reader.u64("batchAcknowledgement.sequence"),
    phase: decodePhase(reader, "batchAcknowledgement.phase"),
    batchEnvelopeDigest: reader.fixedBytes(
      32,
      "batchAcknowledgement.batchEnvelopeDigest",
    ),
    nextPhase: decodePhase(reader, "batchAcknowledgement.nextPhase"),
    cursor: reader.bytes(256, "batchAcknowledgement.cursor"),
    cursorDigest: reader.fixedBytes(32, "batchAcknowledgement.cursorDigest"),
    chainDigest: reader.fixedBytes(32, "batchAcknowledgement.chainDigest"),
    acceptedEntries: reader.u64("batchAcknowledgement.acceptedEntries"),
    acceptedBytes: reader.u64("batchAcknowledgement.acceptedBytes"),
    stagedBytes: reader.u64("batchAcknowledgement.stagedBytes"),
  };
  if (value.cursor.byteLength < 16)
    throw new ReplicationError(
      "ProtocolMismatch",
      "batch acknowledgement cursor must contain at least 128 random bits",
    );
  if (!equalBytes(replicationSha256(value.cursor), value.cursorDigest))
    throw new ReplicationError(
      "IntegrityFailure",
      "batch acknowledgement cursor digest does not match",
    );
  validateAcknowledgementPhaseAdvance(value.phase, value.nextPhase);
  return value;
}

function validateAcknowledgementPhaseAdvance(
  phase: ReplicationPhase,
  nextPhase: ReplicationPhase,
): void {
  const current = PHASES.indexOf(phase);
  const next = PHASES.indexOf(nextPhase);
  if (next !== current && next !== current + 1)
    throw new ReplicationError(
      "ProtocolMismatch",
      "batch acknowledgement phase advancement is not canonical",
    );
}

export function createCanonicalBatchAcknowledgement(options: {
  readonly batch: ReplicationBatch;
  readonly nextPhase: ReplicationPhase;
  readonly cursor: Uint8Array;
  readonly chainDigest: Uint8Array;
  readonly acceptedEntries: number;
  readonly acceptedBytes: number;
  readonly stagedBytes: number;
}): Readonly<ReplicationBatchAcknowledgement> {
  validateAcknowledgementPhaseAdvance(options.batch.phase, options.nextPhase);
  if (
    !(options.cursor instanceof Uint8Array) ||
    options.cursor.byteLength < 16 ||
    options.cursor.byteLength > 256
  )
    throw new ReplicationError(
      "ProtocolMismatch",
      "batch acknowledgement cursor must contain 16 through 256 bytes",
    );
  if (
    !(options.chainDigest instanceof Uint8Array) ||
    options.chainDigest.byteLength !== 32
  )
    throw new ReplicationError(
      "ProtocolMismatch",
      "batch acknowledgement chain digest must contain exactly 32 bytes",
    );
  nonnegativeSafeInteger(options.acceptedEntries, "acceptedEntries");
  nonnegativeSafeInteger(options.acceptedBytes, "acceptedBytes");
  nonnegativeSafeInteger(options.stagedBytes, "stagedBytes");
  const cursor = new Uint8Array(options.cursor);
  return Object.freeze({
    sessionId: options.batch.sessionId,
    sequence: options.batch.sequence,
    phase: options.batch.phase,
    batchEnvelopeDigest: batchEnvelopeDigest(options.batch),
    nextPhase: options.nextPhase,
    cursor,
    cursorDigest: replicationSha256(cursor),
    chainDigest: new Uint8Array(options.chainDigest),
    acceptedEntries: options.acceptedEntries,
    acceptedBytes: options.acceptedBytes,
    stagedBytes: options.stagedBytes,
  });
}

export function validateBatchAcknowledgement(
  batch: ReplicationBatch,
  acknowledgement: ReplicationBatchAcknowledgement,
): void {
  validateAcknowledgementPhaseAdvance(acknowledgement.phase, acknowledgement.nextPhase);
  if (
    acknowledgement.sessionId !== batch.sessionId ||
    acknowledgement.sequence !== batch.sequence ||
    acknowledgement.phase !== batch.phase ||
    !equalBytes(acknowledgement.batchEnvelopeDigest, batchEnvelopeDigest(batch))
  )
    throw new ReplicationError(
      "BatchReplayMismatch",
      "batch acknowledgement does not bind the complete request envelope",
    );
}

function encodeRevisionFragmentValue(
  writer: CanonicalWriter,
  value: ReplicationRevisionFragment,
  prefix = "revisionFragment",
): void {
  writer.text(value.revisionId, `${prefix}.revisionId`);
  writer.optional(value.parentRevisionId, `${prefix}.parentRevisionId`, (item) =>
    writer.text(item, `${prefix}.parentRevisionId.value`),
  );
  writer.u32(value.fragmentIndex, `${prefix}.fragmentIndex`);
  writer.u32(value.fragmentCount, `${prefix}.fragmentCount`);
  if (value.fragmentCount === 0 || value.fragmentIndex >= value.fragmentCount)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} has an invalid fragment range`,
    );
  writer.bytes(value.fragmentBytes, 0xffff_ffff, `${prefix}.fragmentBytes`);
}

function decodeRevisionFragmentValue(
  reader: CanonicalReader,
  prefix = "revisionFragment",
): ReplicationRevisionFragment {
  const value: ReplicationRevisionFragment = {
    revisionId: reader.text(`${prefix}.revisionId`),
    parentRevisionId: reader.optional(`${prefix}.parentRevisionId`, () =>
      reader.text(`${prefix}.parentRevisionId.value`),
    ),
    fragmentIndex: reader.u32(`${prefix}.fragmentIndex`),
    fragmentCount: reader.u32(`${prefix}.fragmentCount`),
    fragmentBytes: reader.bytes(0xffff_ffff, `${prefix}.fragmentBytes`),
  };
  if (value.fragmentCount === 0 || value.fragmentIndex >= value.fragmentCount)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} has an invalid fragment range`,
    );
  return value;
}

function encodeCheckpointFragmentValue(
  writer: CanonicalWriter,
  value: ReplicationCheckpointFragment,
  prefix = "checkpointFragment",
): void {
  writer.text(value.checkpointId, `${prefix}.checkpointId`);
  writer.text(value.revisionId, `${prefix}.revisionId`);
  writer.u32(value.fragmentIndex, `${prefix}.fragmentIndex`);
  writer.u32(value.fragmentCount, `${prefix}.fragmentCount`);
  if (value.fragmentCount === 0 || value.fragmentIndex >= value.fragmentCount)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} has an invalid fragment range`,
    );
  writer.bytes(value.fragmentBytes, 0xffff_ffff, `${prefix}.fragmentBytes`);
}

function decodeCheckpointFragmentValue(
  reader: CanonicalReader,
  prefix = "checkpointFragment",
): ReplicationCheckpointFragment {
  const value: ReplicationCheckpointFragment = {
    checkpointId: reader.text(`${prefix}.checkpointId`),
    revisionId: reader.text(`${prefix}.revisionId`),
    fragmentIndex: reader.u32(`${prefix}.fragmentIndex`),
    fragmentCount: reader.u32(`${prefix}.fragmentCount`),
    fragmentBytes: reader.bytes(0xffff_ffff, `${prefix}.fragmentBytes`),
  };
  if (value.fragmentCount === 0 || value.fragmentIndex >= value.fragmentCount)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} has an invalid fragment range`,
    );
  return value;
}

function encodeBranchGenerationFragmentValue(
  writer: CanonicalWriter,
  value: ReplicationBranchGenerationFragment,
  prefix = "branchGenerationFragment",
): void {
  writer.text(value.branchId, `${prefix}.branchId`, 200);
  writer.text(value.baseRevision, `${prefix}.baseRevision`);
  writer.u64(value.generation, `${prefix}.generation`);
  writer.fixedBytes(value.generationDigest, 32, `${prefix}.generationDigest`);
  writer.u32(value.fragmentIndex, `${prefix}.fragmentIndex`);
  writer.u32(value.fragmentCount, `${prefix}.fragmentCount`);
  if (value.fragmentCount === 0 || value.fragmentIndex >= value.fragmentCount)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} has an invalid fragment range`,
    );
  writer.bytes(value.fragmentBytes, 0xffff_ffff, `${prefix}.fragmentBytes`);
}

function decodeBranchGenerationFragmentValue(
  reader: CanonicalReader,
  prefix = "branchGenerationFragment",
): ReplicationBranchGenerationFragment {
  const value: ReplicationBranchGenerationFragment = {
    branchId: reader.text(`${prefix}.branchId`, 200),
    baseRevision: reader.text(`${prefix}.baseRevision`),
    generation: reader.u64(`${prefix}.generation`),
    generationDigest: reader.fixedBytes(32, `${prefix}.generationDigest`),
    fragmentIndex: reader.u32(`${prefix}.fragmentIndex`),
    fragmentCount: reader.u32(`${prefix}.fragmentCount`),
    fragmentBytes: reader.bytes(0xffff_ffff, `${prefix}.fragmentBytes`),
  };
  if (value.fragmentCount === 0 || value.fragmentIndex >= value.fragmentCount)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} has an invalid fragment range`,
    );
  return value;
}

function encodeTerminalResultValue(
  writer: CanonicalWriter,
  value: ReplicationTerminalResultRecord,
  prefix = "terminalResult",
): void {
  writer.text(value.operationId, `${prefix}.operationId`, 200);
  writer.optional(value.branchId, `${prefix}.branchId`, (item) =>
    writer.text(item, `${prefix}.branchId.value`, 200),
  );
  writer.optional(value.generation, `${prefix}.generation`, (item) =>
    writer.u64(item, `${prefix}.generation.value`),
  );
  writer.optional(value.generationDigest, `${prefix}.generationDigest`, (item) =>
    writer.fixedBytes(item, 32, `${prefix}.generationDigest.value`),
  );
  if ((value.generation === null) !== (value.generationDigest === null))
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} generation and digest must be present together`,
    );
  writer.fixedBytes(value.resultDigest, 32, `${prefix}.resultDigest`);
  writer.bytes(value.resultBytes, 1024 * 1024, `${prefix}.resultBytes`);
  const actual = replicationSha256(value.resultBytes);
  if (!equalBytes(actual, value.resultDigest))
    throw new ReplicationError(
      "IntegrityFailure",
      `${prefix}.resultDigest does not match`,
    );
}

function decodeTerminalResultValue(
  reader: CanonicalReader,
  prefix = "terminalResult",
): ReplicationTerminalResultRecord {
  const value: ReplicationTerminalResultRecord = {
    operationId: reader.text(`${prefix}.operationId`, 200),
    branchId: reader.optional(`${prefix}.branchId`, () =>
      reader.text(`${prefix}.branchId.value`, 200),
    ),
    generation: reader.optional(`${prefix}.generation`, () =>
      reader.u64(`${prefix}.generation.value`),
    ),
    generationDigest: reader.optional(`${prefix}.generationDigest`, () =>
      reader.fixedBytes(32, `${prefix}.generationDigest.value`),
    ),
    resultDigest: reader.fixedBytes(32, `${prefix}.resultDigest`),
    resultBytes: reader.bytes(1024 * 1024, `${prefix}.resultBytes`),
  };
  if ((value.generation === null) !== (value.generationDigest === null))
    throw new ReplicationError(
      "ProtocolMismatch",
      `${prefix} generation and digest must be present together`,
    );
  if (!equalBytes(replicationSha256(value.resultBytes), value.resultDigest))
    throw new ReplicationError(
      "IntegrityFailure",
      `${prefix}.resultDigest does not match`,
    );
  return value;
}

function encodeRecordValue(
  writer: CanonicalWriter,
  record: ReplicationBatchRecord,
): void {
  switch (record.kind) {
    case "object-descriptor":
      writer.fixedBytes(record.digest, 32, "record.objectDescriptor.digest");
      writer.u64(record.byteLength, "record.objectDescriptor.byteLength");
      return;
    case "object-payload":
      writer.fixedBytes(record.digest, 32, "record.objectPayload.digest");
      writer.u64(record.byteLength, "record.objectPayload.byteLength");
      if (record.byteLength !== record.bytes.byteLength)
        throw new ReplicationError(
          "ProtocolMismatch",
          "object payload declared length differs from its bytes",
        );
      if (!equalBytes(replicationSha256(record.bytes), record.digest))
        throw new ReplicationError(
          "IntegrityFailure",
          "object payload digest mismatch",
        );
      writer.bytes(record.bytes, 0xffff_ffff, "record.objectPayload.bytes");
      return;
    case "manifest-root-descriptor":
      writer.text(record.format, "record.manifestRoot.format");
      writer.fixedBytes(record.digest, 32, "record.manifestRoot.digest");
      writer.u64(record.encodedLength, "record.manifestRoot.encodedLength");
      writer.u64(record.logicalFileLength, "record.manifestRoot.logicalFileLength");
      writer.u64(record.entryCount, "record.manifestRoot.entryCount");
      writer.fixedBytes(
        record.rootNodeDigest,
        32,
        "record.manifestRoot.rootNodeDigest",
      );
      return;
    case "manifest-node-descriptor":
      writer.fixedBytes(record.digest, 32, "record.manifestNode.digest");
      writer.u8(
        record.nodeKind === "leaf" ? 1 : record.nodeKind === "internal" ? 2 : 0xff,
        "record.manifestNode.nodeKind",
      );
      writer.u64(record.encodedLength, "record.manifestNode.encodedLength");
      writer.u64(record.logicalSpan, "record.manifestNode.logicalSpan");
      writer.u64(record.entryCount, "record.manifestNode.entryCount");
      return;
    case "missing-content":
      writer.u8(
        record.contentKind === "object"
          ? 1
          : record.contentKind === "manifest-root"
            ? 2
            : record.contentKind === "manifest-node"
              ? 3
              : 0xff,
        "record.missingContent.contentKind",
      );
      writer.fixedBytes(record.digest, 32, "record.missingContent.digest");
      return;
    case "revision-fragment":
      encodeRevisionFragmentValue(writer, record, "record.revisionFragment");
      return;
    case "checkpoint-fragment":
      encodeCheckpointFragmentValue(writer, record, "record.checkpointFragment");
      return;
    case "branch-generation-fragment":
      encodeBranchGenerationFragmentValue(
        writer,
        record,
        "record.branchGenerationFragment",
      );
      return;
    case "terminal-result":
      encodeTerminalResultValue(writer, record, "record.terminalResult");
      return;
  }
}

function decodeRecordValue(
  reader: CanonicalReader,
  tag: number,
): ReplicationBatchRecord {
  switch (tag) {
    case 0x01:
      return {
        kind: "object-descriptor",
        digest: reader.fixedBytes(32, "record.objectDescriptor.digest"),
        byteLength: reader.u64("record.objectDescriptor.byteLength"),
      };
    case 0x02: {
      const digest = reader.fixedBytes(32, "record.objectPayload.digest");
      const byteLength = reader.u64("record.objectPayload.byteLength");
      const bytes = reader.bytes(0xffff_ffff, "record.objectPayload.bytes");
      if (byteLength !== bytes.byteLength)
        throw new ReplicationError(
          "ProtocolMismatch",
          "object payload declared length differs from its bytes",
        );
      if (!equalBytes(replicationSha256(bytes), digest))
        throw new ReplicationError(
          "IntegrityFailure",
          "object payload digest mismatch",
        );
      return { kind: "object-payload", digest, byteLength, bytes };
    }
    case 0x03:
      return {
        kind: "manifest-root-descriptor",
        format: reader.text("record.manifestRoot.format"),
        digest: reader.fixedBytes(32, "record.manifestRoot.digest"),
        encodedLength: reader.u64("record.manifestRoot.encodedLength"),
        logicalFileLength: reader.u64("record.manifestRoot.logicalFileLength"),
        entryCount: reader.u64("record.manifestRoot.entryCount"),
        rootNodeDigest: reader.fixedBytes(32, "record.manifestRoot.rootNodeDigest"),
      };
    case 0x04: {
      const digest = reader.fixedBytes(32, "record.manifestNode.digest");
      const nodeKind = reader.u8("record.manifestNode.nodeKind");
      if (nodeKind !== 1 && nodeKind !== 2)
        throw new ReplicationError("ProtocolMismatch", "unknown manifest node kind");
      return {
        kind: "manifest-node-descriptor",
        digest,
        nodeKind: nodeKind === 1 ? "leaf" : "internal",
        encodedLength: reader.u64("record.manifestNode.encodedLength"),
        logicalSpan: reader.u64("record.manifestNode.logicalSpan"),
        entryCount: reader.u64("record.manifestNode.entryCount"),
      };
    }
    case 0x05: {
      const contentTag = reader.u8("record.missingContent.contentKind");
      const contentKind =
        contentTag === 1
          ? "object"
          : contentTag === 2
            ? "manifest-root"
            : contentTag === 3
              ? "manifest-node"
              : null;
      if (!contentKind)
        throw new ReplicationError("ProtocolMismatch", "unknown missing content kind");
      return {
        kind: "missing-content",
        contentKind,
        digest: reader.fixedBytes(32, "record.missingContent.digest"),
      };
    }
    case 0x06:
      return {
        kind: "revision-fragment",
        ...decodeRevisionFragmentValue(reader, "record.revisionFragment"),
      };
    case 0x07:
      return {
        kind: "checkpoint-fragment",
        ...decodeCheckpointFragmentValue(reader, "record.checkpointFragment"),
      };
    case 0x08:
      return {
        kind: "branch-generation-fragment",
        ...decodeBranchGenerationFragmentValue(
          reader,
          "record.branchGenerationFragment",
        ),
      };
    case 0x09:
      return {
        kind: "terminal-result",
        ...decodeTerminalResultValue(reader, "record.terminalResult"),
      };
    default:
      throw new ReplicationError("ProtocolMismatch", "unknown batch record tag");
  }
}

function measureRecordPayload(record: ReplicationBatchRecord): number {
  const writer = new CanonicalWriter(null);
  encodeRecordValue(writer, record);
  return writer.length;
}

function encodeRecordFrames(
  writer: CanonicalWriter,
  records: readonly ReplicationBatchRecord[],
): void {
  if (records.length > 256)
    throw new ReplicationError(
      "ResourceLimit",
      "batch records exceed the version 1 maximum of 256",
    );
  writer.u32(records.length, "batch.records.count");
  for (let index = 0; index < records.length; index += 1) {
    const record = records[index]!;
    const tag = RECORD_TAGS[record.kind];
    if (tag === undefined)
      throw new ReplicationError("ProtocolMismatch", "unknown batch record kind");
    writer.u8(tag, `batch.records[${index}].tag`);
    const length = measureRecordPayload(record);
    writer.u32(length, `batch.records[${index}].length`);
    encodeRecordValue(writer, record);
  }
}

export function encodeBatchRecordsPayload(
  records: readonly ReplicationBatchRecord[],
): Uint8Array {
  return encodeExact((writer) => encodeRecordFrames(writer, records));
}

export function batchPayloadDigest(
  records: readonly ReplicationBatchRecord[],
): Uint8Array {
  const hasher = new IncrementalReplicationSha256().update(BATCH_DIGEST_DOMAIN);
  encodeRecordFrames(new CanonicalWriter(null, hasher), records);
  return hasher.digest();
}

export function batchPayloadDigestHex(
  records: readonly ReplicationBatchRecord[],
): string {
  return bytesToLowerHex(batchPayloadDigest(records));
}

export function batchPayloadByteCount(
  records: readonly ReplicationBatchRecord[],
): number {
  let total = 0;
  for (const record of records) {
    const length = measureRecordPayload(record);
    if (total + length > Number.MAX_SAFE_INTEGER)
      throw new ReplicationError("ResourceLimit", "batch payload byte count overflow");
    total += length;
  }
  return total;
}

export function createCanonicalBatch(
  input: Omit<ReplicationBatch, "entryCount" | "payloadByteCount" | "payloadDigest">,
): ReplicationBatch {
  boundedArray(input.records, "batch.records", 256);
  const records = Object.freeze([...input.records]);
  return Object.freeze({
    ...input,
    records,
    entryCount: records.length,
    payloadByteCount: batchPayloadByteCount(records),
    payloadDigest: batchPayloadDigest(records),
  });
}

function encodeBatchValue(writer: CanonicalWriter, value: ReplicationBatch): void {
  writer.text(validateReplicationSessionId(value.sessionId), "batch.sessionId");
  encodePlan(writer, value.plan, "batch.plan");
  writer.u8(phaseTag(value.phase, "batch.phase"), "batch.phase");
  writer.u64(value.sequence, "batch.sequence");
  writer.fixedBytes(value.priorCursorDigest, 32, "batch.priorCursorDigest");
  if (value.entryCount !== value.records.length)
    throw new ReplicationError("ProtocolMismatch", "batch entry count mismatch");
  writer.u32(value.entryCount, "batch.entryCount");
  const payloadBytes = batchPayloadByteCount(value.records);
  if (value.payloadByteCount !== payloadBytes)
    throw new ReplicationError("ProtocolMismatch", "batch payload byte count mismatch");
  writer.u64(value.payloadByteCount, "batch.payloadByteCount");
  const digest = batchPayloadDigest(value.records);
  if (!equalBytes(value.payloadDigest, digest))
    throw new ReplicationError("IntegrityFailure", "batch payload digest mismatch");
  writer.fixedBytes(value.payloadDigest, 32, "batch.payloadDigest");
  encodeRecordFrames(writer, value.records);
}

function decodeBatchValue(reader: CanonicalReader): ReplicationBatch {
  const sessionId = validateReplicationSessionId(reader.text("batch.sessionId"));
  const plan = decodePlan(reader, "batch.plan");
  const phase = decodePhase(reader, "batch.phase");
  const sequence = reader.u64("batch.sequence");
  const priorCursorDigest = reader.fixedBytes(32, "batch.priorCursorDigest");
  const entryCount = reader.u32("batch.entryCount");
  if (entryCount > 256)
    throw new ReplicationError("ResourceLimit", "batch entry count exceeds 256");
  const payloadByteCount = reader.u64("batch.payloadByteCount");
  const payloadDigest = reader.fixedBytes(32, "batch.payloadDigest");
  const recordCount = reader.u32("batch.records.count");
  if (recordCount !== entryCount)
    throw new ReplicationError("ProtocolMismatch", "batch record count mismatch");
  const records: ReplicationBatchRecord[] = [];
  for (let index = 0; index < recordCount; index += 1) {
    const tag = reader.u8(`batch.records[${index}].tag`);
    const length = reader.u32(`batch.records[${index}].length`);
    const nested = reader.nested(length, `batch.records[${index}]`);
    const record = decodeRecordValue(nested, tag);
    nested.finish(`batch.records[${index}]`);
    records.push(record);
  }
  const actualByteCount = batchPayloadByteCount(records);
  if (payloadByteCount !== actualByteCount)
    throw new ReplicationError("ProtocolMismatch", "batch payload byte count mismatch");
  const actualDigest = batchPayloadDigest(records);
  if (!equalBytes(payloadDigest, actualDigest))
    throw new ReplicationError("IntegrityFailure", "batch payload digest mismatch");
  return {
    sessionId,
    plan,
    phase,
    sequence,
    priorCursorDigest,
    entryCount,
    payloadByteCount,
    payloadDigest,
    records: Object.freeze(records),
  };
}

function encodeErrorValue(
  writer: CanonicalWriter,
  value: ReplicationSemanticErrorRecord,
): void {
  const codeIndex = ERROR_CODES.indexOf(value.code);
  if (codeIndex < 0)
    throw new ReplicationError("ProtocolMismatch", "unknown semantic error code");
  if (value.retryable !== isReplicationErrorRetryable(value.code))
    throw new ReplicationError(
      "ProtocolMismatch",
      "semantic error retryability does not match its canonical code policy",
    );
  writer.u8(codeIndex + 1, "error.code");
  writer.optional(value.phase, "error.phase", (item) =>
    writer.u8(phaseTag(item, "error.phase.value"), "error.phase.value"),
  );
  writer.optional(value.sessionId, "error.sessionId", (item) =>
    writer.text(validateReplicationSessionId(item), "error.sessionId.value"),
  );
  writer.text(value.message, "error.message", MAX_CANONICAL_ERROR_TEXT_BYTES);
  writer.boolean(value.retryable, "error.retryable");
}

function decodeErrorValue(reader: CanonicalReader): ReplicationSemanticErrorRecord {
  const code = ERROR_CODES[reader.u8("error.code") - 1];
  if (!code)
    throw new ReplicationError("ProtocolMismatch", "unknown semantic error code tag");
  const value: ReplicationSemanticErrorRecord = {
    code,
    phase: reader.optional("error.phase", () =>
      decodePhase(reader, "error.phase.value"),
    ),
    sessionId: reader.optional("error.sessionId", () =>
      validateReplicationSessionId(reader.text("error.sessionId.value")),
    ),
    message: reader.text("error.message", MAX_CANONICAL_ERROR_TEXT_BYTES),
    retryable: reader.boolean("error.retryable"),
  };
  if (value.retryable !== isReplicationErrorRetryable(value.code))
    throw new ReplicationError(
      "ProtocolMismatch",
      "semantic error retryability does not match its canonical code policy",
    );
  return value;
}

function payloadEncoder(envelope: CanonicalReplicationEnvelope): EncodeCallback {
  switch (envelope.kind) {
    case "capabilities":
      return (writer) => encodeCapabilitiesValue(writer, envelope.value);
    case "authorization":
      return (writer) => encodeAuthorizationValue(writer, envelope.value);
    case "batch":
      return (writer) => encodeBatchValue(writer, envelope.value);
    case "cursor":
      return (writer) => encodeCursorValue(writer, envelope.value);
    case "revision-fragment":
      return (writer) => encodeRevisionFragmentValue(writer, envelope.value);
    case "checkpoint-fragment":
      return (writer) => encodeCheckpointFragmentValue(writer, envelope.value);
    case "branch-generation-fragment":
      return (writer) => encodeBranchGenerationFragmentValue(writer, envelope.value);
    case "terminal-result":
      return (writer) => encodeTerminalResultValue(writer, envelope.value);
    case "batch-acknowledgement":
      return (writer) => encodeBatchAcknowledgementValue(writer, envelope.value);
    case "error":
      return (writer) => encodeErrorValue(writer, envelope.value);
  }
}

export function encodeCanonicalEnvelope(
  envelope: CanonicalReplicationEnvelope,
): Uint8Array {
  const encodePayload = payloadEncoder(envelope);
  const payloadSizer = new CanonicalWriter(null);
  encodePayload(payloadSizer);
  if (payloadSizer.length > 0xffff_ffff)
    throw new ReplicationError(
      "ResourceLimit",
      "canonical envelope payload is too large",
    );
  return encodeExact((writer) => {
    writer.fixedBytes(MAGIC, 4, "envelope.magic");
    writer.u16(WIRE_VERSION, "envelope.version");
    writer.u8(ENVELOPE_TAGS[envelope.kind], "envelope.kind");
    writer.u8(0, "envelope.flags");
    writer.u32(payloadSizer.length, "envelope.payloadLength");
    encodePayload(writer);
  });
}

export function batchEnvelopeDigest(value: ReplicationBatch): Uint8Array {
  const envelope = { kind: "batch", value } as const;
  const encodePayload = payloadEncoder(envelope);
  const payloadSizer = new CanonicalWriter(null);
  encodePayload(payloadSizer);
  const hasher = new IncrementalReplicationSha256().update(
    BATCH_ENVELOPE_DIGEST_DOMAIN,
  );
  const writer = new CanonicalWriter(null, hasher);
  writer.fixedBytes(MAGIC, 4, "envelope.magic");
  writer.u16(WIRE_VERSION, "envelope.version");
  writer.u8(ENVELOPE_TAGS.batch, "envelope.kind");
  writer.u8(0, "envelope.flags");
  writer.u32(payloadSizer.length, "envelope.payloadLength");
  encodePayload(writer);
  return hasher.digest();
}

export function batchEnvelopeDigestHex(value: ReplicationBatch): string {
  return bytesToLowerHex(batchEnvelopeDigest(value));
}

export function receiptChainDigest(
  priorChainDigest: Uint8Array,
  sequence: number,
  acceptedBatchEnvelopeDigest: Uint8Array,
): Uint8Array {
  const hasher = new IncrementalReplicationSha256().update(RECEIPT_CHAIN_DIGEST_DOMAIN);
  const writer = new CanonicalWriter(null, hasher);
  writer.fixedBytes(priorChainDigest, 32, "receiptChain.priorDigest");
  writer.u64(sequence, "receiptChain.sequence");
  writer.fixedBytes(
    acceptedBatchEnvelopeDigest,
    32,
    "receiptChain.batchEnvelopeDigest",
  );
  return hasher.digest();
}

const SESSION_CURSOR_DOMAIN = new TextEncoder().encode(
  "efs-replication-v1/session-cursor\0",
);

/**
 * Deterministic shared session cursor. Both peers compute the same next
 * cursor from the prior cursor digest and the accepted batch envelope, so
 * their durable cursor chains converge without carrying cursor bytes.
 */
export function nextSessionCursor(
  priorCursorDigest: Uint8Array,
  acceptedBatchEnvelopeDigest: Uint8Array,
): Uint8Array {
  const hasher = new IncrementalReplicationSha256().update(SESSION_CURSOR_DOMAIN);
  const writer = new CanonicalWriter(null, hasher);
  writer.fixedBytes(priorCursorDigest, 32, "sessionCursor.priorDigest");
  writer.fixedBytes(
    acceptedBatchEnvelopeDigest,
    32,
    "sessionCursor.batchEnvelopeDigest",
  );
  return hasher.digest();
}

export function receiptChainDigestHex(
  priorChainDigest: Uint8Array,
  sequence: number,
  acceptedBatchEnvelopeDigest: Uint8Array,
): string {
  return bytesToLowerHex(
    receiptChainDigest(priorChainDigest, sequence, acceptedBatchEnvelopeDigest),
  );
}

export function encodeCanonicalBatchAcknowledgement(
  value: ReplicationBatchAcknowledgement,
): Uint8Array {
  return encodeCanonicalEnvelope({ kind: "batch-acknowledgement", value });
}

export function decodeCanonicalBatchAcknowledgement(
  input: Uint8Array,
  options: DecodeCanonicalEnvelopeOptions = {},
): ReplicationBatchAcknowledgement {
  const envelope = decodeCanonicalEnvelope(input, options);
  if (envelope.kind !== "batch-acknowledgement")
    throw new ReplicationError(
      "ProtocolMismatch",
      "canonical envelope is not a batch acknowledgement",
    );
  return envelope.value;
}

export interface DecodeCanonicalEnvelopeOptions {
  readonly maxBytes?: number;
}

export function decodeCanonicalEnvelope(
  input: Uint8Array,
  options: DecodeCanonicalEnvelopeOptions = {},
): CanonicalReplicationEnvelope {
  if (!(input instanceof Uint8Array))
    throw new TypeError("envelope must be Uint8Array");
  const maxBytes = options.maxBytes ?? PRE_NEGOTIATION_ENVELOPE_BYTES;
  if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0)
    throw new TypeError("maxBytes must be a positive safe integer");
  if (input.byteLength > maxBytes)
    throw new ReplicationError("ResourceLimit", "canonical envelope exceeds maxBytes");
  if (input.byteLength < ENVELOPE_HEADER_BYTES)
    throw new ReplicationError(
      "ProtocolMismatch",
      "canonical envelope header is truncated",
    );
  const reader = new CanonicalReader(input);
  if (!equalBytes(reader.fixedBytes(4, "envelope.magic"), MAGIC))
    throw new ReplicationError("ProtocolMismatch", "canonical envelope magic mismatch");
  if (reader.u16("envelope.version") !== WIRE_VERSION)
    throw new ReplicationError(
      "ProtocolMismatch",
      "unsupported canonical wire version",
    );
  const kind = TAG_TO_ENVELOPE.get(reader.u8("envelope.kind"));
  if (!kind)
    throw new ReplicationError("ProtocolMismatch", "unknown canonical envelope kind");
  if (reader.u8("envelope.flags") !== 0)
    throw new ReplicationError(
      "ProtocolMismatch",
      "canonical envelope flags are nonzero",
    );
  const payloadLength = reader.u32("envelope.payloadLength");
  if (payloadLength !== reader.remaining)
    throw new ReplicationError(
      "ProtocolMismatch",
      "canonical envelope length mismatch",
    );
  const payload = reader.nested(payloadLength, "envelope.payload");
  let envelope: CanonicalReplicationEnvelope;
  switch (kind) {
    case "capabilities":
      envelope = { kind, value: decodeCapabilitiesValue(payload) };
      break;
    case "authorization":
      envelope = { kind, value: decodeAuthorizationValue(payload) };
      break;
    case "batch":
      envelope = { kind, value: decodeBatchValue(payload) };
      break;
    case "cursor":
      envelope = { kind, value: decodeCursorValue(payload) };
      break;
    case "revision-fragment":
      envelope = { kind, value: decodeRevisionFragmentValue(payload) };
      break;
    case "checkpoint-fragment":
      envelope = { kind, value: decodeCheckpointFragmentValue(payload) };
      break;
    case "branch-generation-fragment":
      envelope = { kind, value: decodeBranchGenerationFragmentValue(payload) };
      break;
    case "terminal-result":
      envelope = { kind, value: decodeTerminalResultValue(payload) };
      break;
    case "batch-acknowledgement":
      envelope = { kind, value: decodeBatchAcknowledgementValue(payload) };
      break;
    case "error":
      envelope = { kind, value: decodeErrorValue(payload) };
      break;
  }
  payload.finish("envelope.payload");
  reader.finish("envelope");
  return envelope;
}

export const EFS_REPLICATION_V1_WIRE = Object.freeze({
  magic: "EFSR",
  version: WIRE_VERSION,
  byteOrder: "big-endian" as const,
  headerBytes: ENVELOPE_HEADER_BYTES,
  envelopeTags: Object.freeze({ ...ENVELOPE_TAGS }),
  recordTags: Object.freeze({ ...RECORD_TAGS }),
  featureCount: 10,
  unknownFields: "reject" as const,
});

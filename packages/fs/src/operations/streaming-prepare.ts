import { DEFAULT_FASTCDC, StreamingFastCdc } from "../cdc/fastcdc.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
  validateSupportedManifestParameters,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
  type ManifestParameters,
} from "../manifests/codec.js";
import {
  advanceManifestGroupingState,
  isManifestGroupBoundary,
} from "../manifests/grouping.js";
import {
  AdmissionController,
  DURABLE_METADATA_ROW_BYTES,
  maxPersistedContentObjectBytes,
  type RuntimeLimits,
  type StorageLimits,
} from "../resources/limits.js";
import { ContentCache } from "../cache/content-cache.js";
import type {
  ClosureCertificate,
  ContentObjectInput,
  OperationsStorage,
  StagingEntryRow as EntryRow,
  StagingLevelRow as LevelRow,
} from "./storage-ports.js";
import {
  bytesToHex,
  copyBytes,
  intrinsicByteLength,
  intrinsicByteRange,
} from "../cas/bytes.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";
interface PreparedNode {
  readonly hash: Uint8Array;
  readonly encoded: Uint8Array;
  readonly span: number;
  readonly entryCount: number;
}

function durableWriteBatchLimit(storage: StorageLimits): number {
  // A retained item may change physical content, a staging record, membership,
  // certificate/reservation state, and the usage authority. Keep fixed
  // transaction bookkeeping outside the per-item envelope.
  return Math.max(
    1,
    Math.min(
      storage.maxQueryBatchSize,
      Math.floor((storage.maxFinalTransactionRows - 24) / 6),
    ),
  );
}
export interface StreamPreparedManifest {
  readonly hash: Uint8Array;
  readonly size: number;
  readonly certificate: ClosureCertificate;
}

export interface StagedManifestEntryInput {
  readonly hash: Uint8Array;
  readonly length: number;
  /** Present only for newly chunked content. Existing CAS entries omit it. */
  readonly bytes?: Uint8Array;
}

function randomNonce(): Uint8Array {
  return globalThis.crypto.getRandomValues(new Uint8Array(16));
}

export function ingestReservationBytes(
  declaredBytes: number,
  storage: StorageLimits,
  minimumChunkBytes = DEFAULT_FASTCDC.minimum,
): number {
  const maximumEntries = checkedAdd(
    Math.ceil(declaredBytes / minimumChunkBytes),
    1,
    "declared stream entry envelope",
  );
  let manifestBytes = checkedMultiply(
    maximumEntries,
    256,
    "declared stream manifest envelope",
  );
  manifestBytes = checkedAdd(
    manifestBytes,
    checkedMultiply(
      storage.maxManifestDepth,
      storage.maxManifestNodeBytes,
      "declared stream manifest depth envelope",
    ),
    "declared stream manifest envelope",
  );
  manifestBytes = checkedAdd(manifestBytes, 68, "declared stream root envelope");
  return checkedMultiply(
    checkedAdd(declaredBytes, manifestBytes, "declared stream payload envelope"),
    2,
    "declared stream physical and logical envelope",
  );
}

export function metadataReservationBytes(
  declaredBytes: number,
  storage: StorageLimits,
  minimumChunkBytes = DEFAULT_FASTCDC.minimum,
): number {
  const entries = checkedAdd(
    Math.ceil(declaredBytes / minimumChunkBytes),
    1,
    "declared metadata entry envelope",
  );
  const manifestRecords = checkedAdd(
    checkedMultiply(entries, 2, "declared manifest record envelope"),
    storage.maxManifestDepth,
    "declared manifest record envelope",
  );
  const rows = checkedAdd(
    checkedMultiply(entries, 4, "declared content metadata envelope"),
    checkedAdd(
      checkedMultiply(manifestRecords, 5, "declared manifest metadata envelope"),
      8,
      "declared fixed metadata envelope",
    ),
    "declared durable metadata envelope",
  );
  return checkedMultiply(
    rows,
    DURABLE_METADATA_ROW_BYTES,
    "declared metadata reservation",
  );
}

export async function prepareContentStreaming(
  port: OperationsStorage,
  input: Uint8Array | ReadableStream<Uint8Array>,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  signal?: AbortSignal,
  cache?: ContentCache,
  clock: () => number = Date.now,
  declaredMaxBytes?: number,
): Promise<StreamPreparedManifest> {
  const borrowedBufferedInput = input instanceof Uint8Array ? input : undefined;
  const bufferedLength = borrowedBufferedInput
    ? intrinsicByteLength(borrowedBufferedInput)
    : 0;
  if (bufferedLength > storage.maxWriteBytes)
    throw new RangeError("buffered write exceeds maxWriteBytes");
  const streamInput = borrowedBufferedInput
    ? undefined
    : (input as ReadableStream<Uint8Array>);
  const declaredBytes = borrowedBufferedInput ? bufferedLength : declaredMaxBytes;
  if (
    declaredBytes === undefined ||
    !Number.isSafeInteger(declaredBytes) ||
    declaredBytes < 0 ||
    declaredBytes > storage.maxFileBytes
  )
    throw new RangeError(
      "streamed writes require a declared maximum byte length within maxFileBytes",
    );
  const durableIngestReservation = ingestReservationBytes(declaredBytes, storage);
  const durableMetadataReservation = metadataReservationBytes(declaredBytes, storage);
  cache ??= new ContentCache(1, admission);
  const leaseId = globalThis.crypto.randomUUID();
  const ownerId = globalThis.crypto.randomUUID();
  const ownerNonce = randomNonce();
  const now = clock();
  if (!Number.isSafeInteger(now) || now < 0)
    throw new Error("clock must return a nonnegative safe integer");
  const workBudget = {
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
    maxStatements: storage.maxFinalTransactionRows * 4,
    maxElapsedMs: 5_000,
  };
  const pendingLimit = Math.max(
    DEFAULT_FASTCDC.maximum,
    Math.min(
      runtime.maxPendingWriteBytes,
      Math.floor(storage.maxFinalTransactionBytes / 2),
    ),
  );
  const inputBudget = bufferedLength;
  const builderBudget = Math.min(
    runtime.maxQueryBatchBytes + storage.maxManifestNodeBytes * 2,
    runtime.maxManagedResidentBytes -
      DEFAULT_FASTCDC.maximum -
      pendingLimit -
      inputBudget,
  );
  if (builderBudget <= 0)
    throw new RangeError(
      "managed resident memory limit cannot admit streaming manifest construction",
    );
  const reservationBytes =
    DEFAULT_FASTCDC.maximum + pendingLimit + builderBudget + inputBudget;
  const releases: Array<() => void> = [];
  let bufferedInput: Uint8Array | undefined;
  let leaseBegun = false;
  let chunker!: StreamingFastCdc;
  let total = 0;
  let sourceBytes = 0;
  let entryIndex = 0;
  let pendingBytes = 0;
  const pending: ContentObjectInput[] = [];
  const durableBatchLimit = durableWriteBatchLimit(storage);
  const flushObjects = (): void => {
    if (!pending.length) return;
    const batch = pending.splice(0);
    pendingBytes = 0;
    port.transaction("write", workBudget, (tx) => {
      const staging = tx.staging(storage, cache);
      const unique = [
        ...new Map(batch.map((item) => [bytesToHex(item.hash), item])).values(),
      ];
      staging.consumeIngestReservation(
        leaseId,
        ownerNonce,
        unique.reduce(
          (sum, item) => checkedAdd(sum, intrinsicByteLength(item.bytes)),
          0,
        ),
      );
      staging.consumeMetadataReservation(
        leaseId,
        ownerNonce,
        unique.length * DURABLE_METADATA_ROW_BYTES,
      );
      tx.content(storage, cache).putObjectsBatch(batch);
      staging.putEntriesBatch(
        leaseId,
        batch.map((item) =>
          Object.freeze({
            entryIndex: entryIndex++,
            objectHash: item.hash,
            length: intrinsicByteLength(item.bytes),
          }),
        ),
      );
      staging.appendBatch(
        leaseId,
        ownerNonce,
        unique.map((item) =>
          Object.freeze({
            kind: "object" as const,
            hash: item.hash,
            size: intrinsicByteLength(item.bytes),
          }),
        ),
      );
    });
  };
  const acceptChunk = (chunk: Uint8Array): void => {
    // StreamingFastCdc#emitChunk already returns a fresh, detached slice of
    // its internal buffer, so no defensive copy is needed here.
    const chunkLength = intrinsicByteLength(chunk);
    total = checkedAdd(total, chunkLength);
    if (total > storage.maxFileBytes) throw new RangeError("file exceeds maxFileBytes");
    if (
      pending.length >= durableBatchLimit ||
      checkedAdd(pendingBytes, chunkLength) > pendingLimit
    )
      flushObjects();
    pending.push(Object.freeze({ hash: port.hashBytes(chunk), bytes: chunk }));
    pendingBytes = checkedAdd(pendingBytes, chunkLength);
  };
  const feed = (bytes: Uint8Array): void => {
    bytes = intrinsicByteRange(bytes);
    for (
      let offset = 0;
      offset < intrinsicByteLength(bytes);
      offset += runtime.maxWriteSessionBytes
    )
      chunker.drain(
        intrinsicByteRange(
          bytes,
          offset,
          Math.min(intrinsicByteLength(bytes), offset + runtime.maxWriteSessionBytes),
        ),
        acceptChunk,
      );
  };
  try {
    cache?.makeRoom(reservationBytes);
    releases.push(admission.reserve(DEFAULT_FASTCDC.maximum));
    releases.push(admission.reserve(pendingLimit));
    releases.push(admission.reserve(builderBudget));
    if (inputBudget) releases.push(admission.reserve(inputBudget));
    if (borrowedBufferedInput) bufferedInput = copyBytes(borrowedBufferedInput);
    port.transaction("write", workBudget, (tx) => {
      const staging = tx.staging(storage, cache);
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: now + storage.stagingLeaseMs,
        ingestReservationBytes: durableIngestReservation,
        metadataReservationBytes: durableMetadataReservation,
      });
      staging.bumpRoot(5, leaseId);
    });
    leaseBegun = true;
    chunker = new StreamingFastCdc(DEFAULT_FASTCDC);
    if (bufferedInput) feed(bufferedInput);
    else {
      const reader = streamInput!.getReader();
      let completed = false;
      let streamError: unknown;
      try {
        while (true) {
          if (signal?.aborted)
            throw new DOMException("The operation was aborted", "AbortError");
          const { done, value } = await reader.read();
          if (done) {
            completed = true;
            break;
          }
          if (!(value instanceof Uint8Array))
            throw new TypeError("write stream chunks must be Uint8Array values");
          const valueLength = intrinsicByteLength(value);
          if (valueLength > runtime.maxWriteSessionBytes)
            throw new RangeError("write stream chunk exceeds maxWriteSessionBytes");
          sourceBytes = checkedAdd(sourceBytes, valueLength, "streamed input bytes");
          if (sourceBytes > declaredBytes)
            throw new RangeError(
              "write stream exceeds its declared maximum byte length",
            );
          cache?.makeRoom(valueLength);
          const releaseInput = admission.reserve(valueLength);
          try {
            const ownedValue = copyBytes(value);
            feed(ownedValue);
          } finally {
            releaseInput();
          }
        }
      } catch (error) {
        streamError = error;
        throw error;
      } finally {
        if (!completed)
          try {
            await reader.cancel(streamError);
          } catch {}
        reader.releaseLock();
      }
    }
    chunker.drain(new Uint8Array(), acceptChunk, true);
    flushObjects();
    return finalizeStagedManifest(
      port,
      storage,
      runtime,
      leaseId,
      ownerNonce,
      workBudget,
      DEFAULT_FASTCDC,
      total,
      entryIndex,
      true,
      cache,
    );
  } catch (error) {
    if (leaseBegun)
      try {
        port.transaction("write", workBudget, (tx) => {
          tx.staging(storage, cache).delete(leaseId, ownerNonce);
        });
      } catch {}
    throw error;
  } finally {
    for (let index = releases.length - 1; index >= 0; index -= 1) releases[index]!();
  }
}

/**
 * Persists an authenticated entry stream without materializing the file. Entries
 * without `bytes` reuse an existing CAS object; entries with `bytes` are verified
 * and inserted before their durable staging reference is recorded.
 */
export async function prepareContentEntriesStreaming(
  port: OperationsStorage,
  entries: Iterable<StagedManifestEntryInput>,
  parameters: ManifestParameters,
  expectedSize: number,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  cache?: ContentCache,
  clock: () => number = Date.now,
): Promise<StreamPreparedManifest> {
  validateSupportedManifestParameters(parameters);
  if (parameters.maximum > maxPersistedContentObjectBytes(storage))
    throw new RangeError(
      "manifest FastCDC maximum exceeds the durable object transaction envelope",
    );
  if (
    !Number.isSafeInteger(expectedSize) ||
    expectedSize < 0 ||
    expectedSize > storage.maxFileBytes
  )
    throw new RangeError("staged manifest size exceeds configured limit");
  const durableIngestReservation = ingestReservationBytes(
    expectedSize,
    storage,
    parameters.minimum,
  );
  const durableMetadataReservation = metadataReservationBytes(
    expectedSize,
    storage,
    parameters.minimum,
  );
  cache ??= new ContentCache(1, admission);
  const leaseId = globalThis.crypto.randomUUID();
  const ownerId = globalThis.crypto.randomUUID();
  const ownerNonce = randomNonce();
  const now = clock();
  if (!Number.isSafeInteger(now) || now < 0)
    throw new Error("clock must return a nonnegative safe integer");
  const workBudget = Object.freeze({
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
    maxStatements: storage.maxFinalTransactionRows * 4,
    maxElapsedMs: 5_000,
  });
  const pendingLimit = Math.max(
    parameters.maximum,
    Math.min(
      runtime.maxPendingWriteBytes,
      Math.floor(storage.maxFinalTransactionBytes / 2),
    ),
  );
  const entryMetadataBudget = checkedMultiply(
    storage.maxQueryBatchSize,
    32,
    "staged entry hash snapshots",
  );
  const entrySnapshotBudget = checkedAdd(
    pendingLimit,
    entryMetadataBudget,
    "staged entry snapshots",
  );
  const builderBudget = Math.min(
    runtime.maxQueryBatchBytes + storage.maxManifestNodeBytes * 2,
    runtime.maxManagedResidentBytes - entrySnapshotBudget,
  );
  if (builderBudget <= 0)
    throw new RangeError(
      "managed resident memory limit cannot admit staged manifest construction",
    );
  const releases: Array<() => void> = [];
  let leaseBegun = false;
  let total = 0;
  let entryIndex = 0;
  let previousLength: number | undefined;
  let pendingBytes = 0;
  const pending: Array<{
    readonly hash: Uint8Array;
    readonly length: number;
    readonly bytes?: Uint8Array;
    readonly release: () => void;
  }> = [];
  const durableBatchLimit = durableWriteBatchLimit(storage);
  const flush = (): void => {
    if (!pending.length) return;
    const batch = pending.splice(0);
    pendingBytes = 0;
    try {
      port.transaction("write", workBudget, (tx) => {
        const objects = batch
          .filter(
            (item): item is typeof item & { readonly bytes: Uint8Array } =>
              item.bytes !== undefined,
          )
          .map((item) => Object.freeze({ hash: item.hash, bytes: item.bytes }));
        const staging = tx.staging(storage, cache);
        const unique = [
          ...new Map(batch.map((item) => [bytesToHex(item.hash), item])).values(),
        ];
        staging.consumeIngestReservation(
          leaseId,
          ownerNonce,
          unique.reduce((sum, item) => checkedAdd(sum, item.length), 0),
        );
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          new Set(objects.map((item) => bytesToHex(item.hash))).size *
            DURABLE_METADATA_ROW_BYTES,
        );
        if (objects.length) tx.content(storage, cache).putObjectsBatch(objects);
        staging.putEntriesBatch(
          leaseId,
          batch.map((item) =>
            Object.freeze({
              entryIndex: entryIndex++,
              objectHash: item.hash,
              length: item.length,
            }),
          ),
        );
        staging.appendBatch(
          leaseId,
          ownerNonce,
          unique.map((item) =>
            Object.freeze({
              kind: "object" as const,
              hash: item.hash,
              size: item.length,
            }),
          ),
        );
      });
    } finally {
      for (const item of batch) item.release();
    }
  };
  try {
    cache?.makeRoom(entrySnapshotBudget + builderBudget);
    port.transaction("write", workBudget, (tx) => {
      const staging = tx.staging(storage, cache);
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: now + storage.stagingLeaseMs,
        ingestReservationBytes: durableIngestReservation,
        metadataReservationBytes: durableMetadataReservation,
      });
      staging.bumpRoot(5, leaseId);
    });
    leaseBegun = true;
    releases.push(admission.reserve(builderBudget));
    for (const borrowed of entries) {
      const length = borrowed.length;
      const borrowedHash = borrowed.hash;
      const borrowedBytes = borrowed.bytes;
      const hashLength = intrinsicByteLength(borrowedHash);
      const bytesLength =
        borrowedBytes === undefined ? 0 : intrinsicByteLength(borrowedBytes);
      if (
        hashLength !== 32 ||
        !Number.isSafeInteger(length) ||
        length <= 0 ||
        length > parameters.maximum ||
        (borrowedBytes !== undefined && bytesLength !== length)
      )
        throw new RangeError("invalid staged manifest entry");
      if (previousLength !== undefined && previousLength < parameters.minimum)
        throw new Error("ECORRUPT: non-final manifest entry is below FastCDC minimum");
      previousLength = length;
      total = checkedAdd(total, length);
      if (total > expectedSize)
        throw new Error("staged entry stream exceeds declared file size");
      if (
        pending.length >= durableBatchLimit ||
        pendingBytes + bytesLength > pendingLimit
      )
        flush();
      const snapshotBytes = checkedAdd(32, bytesLength);
      cache.makeRoom(snapshotBytes);
      const release = admission.reserve(snapshotBytes);
      try {
        const hash = copyBytes(borrowedHash);
        const bytes =
          borrowedBytes === undefined ? undefined : copyBytes(borrowedBytes);
        pending.push(
          Object.freeze({
            hash,
            length,
            ...(bytes === undefined ? {} : { bytes }),
            release,
          }),
        );
        pendingBytes = checkedAdd(pendingBytes, bytesLength);
      } catch (error) {
        release();
        throw error;
      }
      if (entryIndex + pending.length > storage.maxManifestEntries)
        throw new RangeError("manifest entry count exceeds configured limit");
    }
    flush();
    if (total !== expectedSize)
      throw new Error("staged entry stream ended before declared file size");
    if ((expectedSize === 0) !== (entryIndex === 0))
      throw new Error("staged empty-file totals mismatch");
    return finalizeStagedManifest(
      port,
      storage,
      runtime,
      leaseId,
      ownerNonce,
      workBudget,
      parameters,
      total,
      entryIndex,
      true,
      cache,
    );
  } catch (error) {
    if (leaseBegun)
      try {
        port.transaction("write", workBudget, (tx) => {
          tx.staging(storage, cache).delete(leaseId, ownerNonce);
        });
      } catch {}
    throw error;
  } finally {
    for (const item of pending) item.release();
    pending.length = 0;
    for (let index = releases.length - 1; index >= 0; index -= 1) releases[index]!();
  }
}

function finalizeStagedManifest(
  port: OperationsStorage,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  leaseId: string,
  ownerNonce: Uint8Array,
  workBudget: {
    readonly maxRows: number;
    readonly maxBytes: number;
    readonly maxStatements?: number;
    readonly maxElapsedMs?: number;
  },
  parameters: ManifestParameters,
  total: number,
  entryIndex: number,
  reservedIngest: boolean,
  cache: ContentCache,
): StreamPreparedManifest {
  const rootNode = buildManifestLevels(
    port,
    storage,
    runtime,
    leaseId,
    ownerNonce,
    workBudget,
    reservedIngest,
    cache,
  );
  const root = encodeManifestRoot({
    parameters,
    fileSize: total,
    entryCount: entryIndex,
    rootNodeHash: rootNode.hash,
  });
  const rootHash = port.hashBytes(root);
  const certificate = port.transaction("write", workBudget, (tx) => {
    const staging = tx.staging(storage, cache);
    if (reservedIngest)
      staging.consumeIngestReservation(leaseId, ownerNonce, intrinsicByteLength(root));
    if (reservedIngest)
      staging.consumeMetadataReservation(
        leaseId,
        ownerNonce,
        DURABLE_METADATA_ROW_BYTES,
      );
    tx.content(storage, cache).putManifestRoot(rootHash, root);
    staging.appendBatch(leaseId, ownerNonce, [
      Object.freeze({
        kind: "manifest-root",
        hash: rootHash,
        size: intrinsicByteLength(root),
      }),
    ]);
    staging.beginReconciliation(leaseId, ownerNonce, rootHash);
    return Object.freeze({
      ...staging.snapshot(leaseId, ownerNonce),
      manifestHash: rootHash,
    });
  });
  let complete = false;
  while (!complete)
    complete = port.transaction(
      "write",
      workBudget,
      (tx) =>
        tx
          .staging(storage, cache)
          .reconcileBatch(
            leaseId,
            ownerNonce,
            Math.max(
              1,
              Math.min(
                storage.maxQueryBatchSize,
                Math.floor((storage.maxFinalTransactionRows - 8) / 4),
                Math.floor(
                  (storage.maxFinalTransactionRows * 4 - 16) /
                    (storage.maxManifestDepth * 2 + 12),
                ),
              ),
            ),
          ).complete,
    );
  port.transaction("write", workBudget, (tx) => {
    const staging = tx.staging(storage, cache);
    staging.seal(certificate);
  });
  return Object.freeze({ hash: rootHash, size: total, certificate });
}

function buildManifestLevels(
  port: OperationsStorage,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  leaseId: string,
  ownerNonce: Uint8Array,
  budget: { readonly maxRows: number; readonly maxBytes: number },
  reservedIngest: boolean,
  cache: ContentCache,
): PreparedNode {
  let level = 0;
  let sourceKind: "entries" | "level" = "entries";
  while (true) {
    let cursor = -1;
    let state = 0n;
    let group: Array<ManifestEntry | ManifestChild> = [];
    let outputIndex = 0;
    let single: PreparedNode | undefined;
    const pendingNodes: PreparedNode[] = [];
    const durableBatchLimit = durableWriteBatchLimit(storage);
    const flushNodes = (): void => {
      if (!pendingNodes.length) return;
      const nodes = pendingNodes.splice(0);
      port.transaction("write", budget, (tx) => {
        const staging = tx.staging(storage, cache);
        if (reservedIngest)
          staging.consumeIngestReservation(
            leaseId,
            ownerNonce,
            nodes.reduce(
              (sum, node) => checkedAdd(sum, intrinsicByteLength(node.encoded)),
              0,
            ),
          );
        if (reservedIngest)
          staging.consumeMetadataReservation(
            leaseId,
            ownerNonce,
            nodes.length * DURABLE_METADATA_ROW_BYTES,
          );
        tx.content(storage, cache).putManifestNodesBatch(
          nodes.map((node) => ({ hash: node.hash, encoded: node.encoded })),
        );
        staging.putLevelRecordsBatch(
          leaseId,
          level,
          nodes.map((node) =>
            Object.freeze({
              recordIndex: outputIndex++,
              nodeHash: node.hash,
              span: node.span,
              entryCount: node.entryCount,
            }),
          ),
        );
        const unique = [
          ...new Map(nodes.map((node) => [bytesToHex(node.hash), node])).values(),
        ];
        staging.appendBatch(
          leaseId,
          ownerNonce,
          unique.map((node) =>
            Object.freeze({
              kind: "manifest-node" as const,
              hash: node.hash,
              size: intrinsicByteLength(node.encoded),
            }),
          ),
        );
      });
    };
    const emit = (): void => {
      const node =
        level === 0
          ? Object.freeze({
              kind: "leaf",
              span: group.reduce(
                (sum, entry) => checkedAdd(sum, (entry as ManifestEntry).length),
                0,
              ),
              entryCount: group.length,
              entries: Object.freeze(group as ManifestEntry[]),
            } satisfies ManifestLeaf)
          : Object.freeze({
              kind: "internal",
              span: group.reduce(
                (sum, child) => checkedAdd(sum, (child as ManifestChild).span),
                0,
              ),
              entryCount: group.reduce(
                (sum, child) => checkedAdd(sum, (child as ManifestChild).entryCount),
                0,
              ),
              children: Object.freeze(group as ManifestChild[]),
            } satisfies ManifestInternal);
      const encoded = encodeManifestNode(node);
      const prepared = Object.freeze({
        hash: port.hashBytes(encoded),
        encoded,
        span: node.span,
        entryCount: node.entryCount,
      });
      single = prepared;
      pendingNodes.push(prepared);
      group = [];
      state = 0n;
      if (
        pendingNodes.length >= Math.min(durableBatchLimit, 64) ||
        pendingNodes.reduce(
          (sum, item) => checkedAdd(sum, intrinsicByteLength(item.encoded)),
          0,
        ) >= Math.floor(storage.maxFinalTransactionBytes / 2)
      )
        flushNodes();
    };
    const minimum = level === 0 ? 64 : 32;
    const target = level === 0 ? 128 : 64;
    const maximum = level === 0 ? 256 : 128;
    while (true) {
      const rows = port.transaction("read", budget, (tx) => {
        const staging = tx.staging(storage, cache);
        return sourceKind === "entries"
          ? staging.entriesAfter(
              leaseId,
              cursor,
              storage.maxQueryBatchSize,
              runtime.maxQueryBatchBytes,
            )
          : staging.levelRecordsAfter(
              leaseId,
              level - 1,
              cursor,
              storage.maxQueryBatchSize,
              runtime.maxQueryBatchBytes,
            );
      });
      if (!rows.length) break;
      for (const row of rows) {
        cursor =
          sourceKind === "entries"
            ? (row as EntryRow).entry_index
            : (row as LevelRow).record_index;
        const record: ManifestEntry | ManifestChild =
          sourceKind === "entries"
            ? Object.freeze({
                hash: (row as EntryRow).object_hash,
                length: (row as EntryRow).length,
              })
            : Object.freeze({
                hash: (row as LevelRow).node_hash,
                span: (row as LevelRow).span,
                entryCount: (row as LevelRow).entry_count,
              });
        group.push(record);
        state = advanceManifestGroupingState(state, record);
        if (isManifestGroupBoundary(group.length, state, minimum, target, maximum))
          emit();
      }
      if (rows.length < storage.maxQueryBatchSize) break;
    }
    if (group.length || (level === 0 && outputIndex === 0 && pendingNodes.length === 0))
      emit();
    flushNodes();
    if (outputIndex === 1 && single) return single;
    if (outputIndex <= 0) throw new Error("ECORRUPT: manifest level produced no node");
    sourceKind = "level";
    level += 1;
    if (level >= storage.maxManifestDepth)
      throw new RangeError("manifest depth exceeds configured limit");
  }
}

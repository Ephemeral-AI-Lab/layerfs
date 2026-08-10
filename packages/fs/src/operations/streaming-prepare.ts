import { sha256 } from "../cas/sha256.js";
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
  type RuntimeLimits,
  type StorageLimits,
} from "../resources/limits.js";
import type { ContentCache } from "../cache/content-cache.js";
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

function ingestReservationBytes(declaredBytes: number, storage: StorageLimits): number {
  const maximumEntries = checkedAdd(
    Math.ceil(declaredBytes / DEFAULT_FASTCDC.minimum),
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
  const leaseId = globalThis.crypto.randomUUID();
  const ownerId = globalThis.crypto.randomUUID();
  const ownerNonce = randomNonce();
  const now = clock();
  if (!Number.isSafeInteger(now) || now < 0)
    throw new Error("clock must return a nonnegative safe integer");
  const workBudget = {
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
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
  const flushObjects = (): void => {
    if (!pending.length) return;
    const batch = pending.splice(0);
    pendingBytes = 0;
    port.transaction("write", workBudget, (tx) => {
      const staging = tx.staging(storage);
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
      tx.content(storage).putObjectsBatch(batch);
      for (const item of batch)
        staging.putEntry(
          leaseId,
          entryIndex++,
          item.hash,
          intrinsicByteLength(item.bytes),
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
      staging.bumpRoot(5, leaseId);
    });
  };
  const acceptChunk = (chunk: Uint8Array): void => {
    const chunkLength = intrinsicByteLength(chunk);
    chunk = copyBytes(chunk);
    total = checkedAdd(total, chunkLength);
    if (total > storage.maxFileBytes) throw new RangeError("file exceeds maxFileBytes");
    if (
      pending.length >= storage.maxQueryBatchSize ||
      checkedAdd(pendingBytes, chunkLength) > pendingLimit
    )
      flushObjects();
    pending.push(Object.freeze({ hash: sha256(chunk), bytes: chunk }));
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
      const staging = tx.staging(storage);
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: now + storage.stagingLeaseMs,
        ingestReservationBytes: durableIngestReservation,
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
    );
  } catch (error) {
    if (leaseBegun)
      try {
        port.transaction("write", workBudget, (tx) => {
          tx.staging(storage).delete(leaseId, ownerNonce);
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
  if (
    !Number.isSafeInteger(expectedSize) ||
    expectedSize < 0 ||
    expectedSize > storage.maxFileBytes
  )
    throw new RangeError("staged manifest size exceeds configured limit");
  const leaseId = globalThis.crypto.randomUUID();
  const ownerId = globalThis.crypto.randomUUID();
  const ownerNonce = randomNonce();
  const now = clock();
  if (!Number.isSafeInteger(now) || now < 0)
    throw new Error("clock must return a nonnegative safe integer");
  const workBudget = Object.freeze({
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
    maxStatements: storage.maxFinalTransactionRows,
    maxElapsedMs: 250,
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
        if (objects.length) tx.content(storage).putObjectsBatch(objects);
        const staging = tx.staging(storage);
        for (const item of batch)
          staging.putEntry(leaseId, entryIndex++, item.hash, item.length);
        const unique = [
          ...new Map(batch.map((item) => [bytesToHex(item.hash), item])).values(),
        ];
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
        staging.bumpRoot(5, leaseId);
      });
    } finally {
      for (const item of batch) item.release();
    }
  };
  try {
    cache?.makeRoom(entrySnapshotBudget + builderBudget);
    port.transaction("write", workBudget, (tx) => {
      const staging = tx.staging(storage);
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: now + storage.stagingLeaseMs,
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
        pending.length >= storage.maxQueryBatchSize ||
        pendingBytes + bytesLength > pendingLimit
      )
        flush();
      const release = admission.reserve(checkedAdd(32, bytesLength));
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
      false,
    );
  } catch (error) {
    if (leaseBegun)
      try {
        port.transaction("write", workBudget, (tx) => {
          tx.staging(storage).delete(leaseId, ownerNonce);
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
): StreamPreparedManifest {
  const rootNode = buildManifestLevels(
    port,
    storage,
    runtime,
    leaseId,
    ownerNonce,
    workBudget,
    reservedIngest,
  );
  const root = encodeManifestRoot({
    parameters,
    fileSize: total,
    entryCount: entryIndex,
    rootNodeHash: rootNode.hash,
  });
  const rootHash = sha256(root);
  const certificate = port.transaction("write", workBudget, (tx) => {
    const staging = tx.staging(storage);
    if (reservedIngest)
      staging.consumeIngestReservation(leaseId, ownerNonce, intrinsicByteLength(root));
    tx.content(storage).putManifestRoot(rootHash, root);
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
          .staging(storage)
          .reconcileBatch(
            leaseId,
            ownerNonce,
            Math.max(
              1,
              Math.min(
                storage.maxQueryBatchSize,
                Math.floor((storage.maxFinalTransactionRows - 8) / 4),
              ),
            ),
          ).complete,
    );
  port.transaction("write", workBudget, (tx) => {
    const staging = tx.staging(storage);
    staging.seal(certificate);
    staging.bumpRoot(5, leaseId);
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
    const flushNodes = (): void => {
      if (!pendingNodes.length) return;
      const nodes = pendingNodes.splice(0);
      port.transaction("write", budget, (tx) => {
        const staging = tx.staging(storage);
        if (reservedIngest)
          staging.consumeIngestReservation(
            leaseId,
            ownerNonce,
            nodes.reduce(
              (sum, node) => checkedAdd(sum, intrinsicByteLength(node.encoded)),
              0,
            ),
          );
        tx.content(storage).putManifestNodesBatch(
          nodes.map((node) => ({ hash: node.hash, encoded: node.encoded })),
        );
        for (const node of nodes)
          staging.putLevelRecord(
            leaseId,
            level,
            outputIndex++,
            node.hash,
            node.span,
            node.entryCount,
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
        staging.bumpRoot(5, leaseId);
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
        hash: sha256(encoded),
        encoded,
        span: node.span,
        entryCount: node.entryCount,
      });
      single = prepared;
      pendingNodes.push(prepared);
      group = [];
      state = 0n;
      if (
        pendingNodes.length >= Math.min(storage.maxQueryBatchSize, 64) ||
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
        const staging = tx.staging(storage);
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

import { bytesToHex, copyBytes, intrinsicByteLength } from "../cas/bytes.js";
import { sha256 } from "../cas/sha256.js";
import { StreamingFastCdc } from "../cdc/fastcdc.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
  type ManifestNode,
  type ManifestParameters,
} from "../manifests/codec.js";
import { validateCanonicalManifestNode } from "../manifests/cursor.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";
import type {
  AdmissionController,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
import type { ContentCache } from "../cache/content-cache.js";
import type {
  AuthenticatedManifestTreePath,
  ClosureCertificate,
  OperationsStorage,
  StorageTransactionPorts,
  StorageTransactionMode,
  StorageWorkBudget,
} from "./storage-ports.js";
import {
  prepareContentStreaming,
  type StreamPreparedManifest,
} from "./streaming-prepare.js";

export interface DurableEditSource {
  readonly manifestHash: Uint8Array;
  readonly size: number;
  readonly parameters: ManifestParameters;
  /** Exact storage transactions performed by one synchronous `read` call. */
  readonly readStorageTransactions?: number;
  /** Largest byte window one bounded source-read transaction can materialize. */
  readonly maxReadWindowBytes?: number;
  read(offset: number, length: number): Uint8Array;
}

export interface DurableContentEdit {
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertLength: number;
  /** Library-owned insertion bytes retained while preparation is in flight. */
  readonly retainedBytes?: number;
  readInsert(offset: number, length: number): Uint8Array;
}

export interface DurablePathCopyMetrics {
  readonly authenticatedNodesRead: number;
  readonly manifestRecordsRead: number;
  readonly emittedNodes: number;
  readonly emittedEntries: number;
  readonly emittedObjectBytes: number;
  readonly reusedSubtrees: number;
  readonly storageTransactions: number;
  readonly sourceReadCalls: number;
  readonly sourceReadTransactions: number;
  readonly sourceBytesRead: number;
}

export interface DurableEditPreparedManifest extends StreamPreparedManifest {
  readonly mode: "durable-path-copy" | "streamed-fallback";
  readonly pathCopyReason?: string;
  readonly pathCopyMetrics?: DurablePathCopyMetrics;
}

interface PreparedNode {
  readonly hash: Uint8Array;
  readonly encoded: Uint8Array;
  readonly node: ManifestNode;
}

interface ReusedClaim {
  readonly sourcePath: readonly number[];
  readonly nodeHash: Uint8Array;
  readonly span: number;
  readonly entryCount: number;
}

interface PathCopyCandidate {
  readonly path: AuthenticatedManifestTreePath;
  readonly entries: readonly {
    readonly hash: Uint8Array;
    readonly bytes: Uint8Array;
  }[];
  readonly nodes: readonly PreparedNode[];
  readonly reused: readonly ReusedClaim[];
  readonly root: Uint8Array;
  readonly rootHash: Uint8Array;
  readonly entryCount: number;
  readonly sourceBytesRead: number;
  readonly sourceReadCalls: number;
  readonly sourceReadTransactions: number;
  readonly authenticatedNodesRead: number;
  readonly manifestRecordsRead: number;
  release(): void;
}

class DurablePathCopyFallbackError extends Error {}

const MAX_PATH_COPY_LEAF_ENTRIES = 256;
const MAX_PATH_COPY_TRANSACTIONS = 64;

function validateInputs(source: DurableEditSource, edit: DurableContentEdit): number {
  if (
    intrinsicByteLength(source.manifestHash) !== 32 ||
    !Number.isSafeInteger(source.size) ||
    source.size < 0 ||
    (source.readStorageTransactions !== undefined &&
      (!Number.isSafeInteger(source.readStorageTransactions) ||
        source.readStorageTransactions < 0)) ||
    (source.maxReadWindowBytes !== undefined &&
      (!Number.isSafeInteger(source.maxReadWindowBytes) ||
        source.maxReadWindowBytes <= 0))
  )
    throw new RangeError("durable edit source identity or size is invalid");
  if (
    !Number.isSafeInteger(edit.offset) ||
    edit.offset < 0 ||
    !Number.isSafeInteger(edit.deleteLength) ||
    edit.deleteLength < 0 ||
    !Number.isSafeInteger(edit.insertLength) ||
    edit.insertLength < 0 ||
    (edit.retainedBytes !== undefined &&
      (!Number.isSafeInteger(edit.retainedBytes) ||
        edit.retainedBytes < 0 ||
        edit.retainedBytes > edit.insertLength)) ||
    edit.offset > source.size ||
    edit.deleteLength > source.size - edit.offset
  )
    throw new RangeError("durable edit is outside the source");
  return checkedAdd(source.size - edit.deleteLength, edit.insertLength);
}

function exactRead(
  read: (offset: number, length: number) => Uint8Array,
  offset: number,
  length: number,
  label: string,
): Uint8Array {
  const borrowed = read(offset, length);
  if (intrinsicByteLength(borrowed) !== length)
    throw new Error(`ECORRUPT: ${label} returned a partial range`);
  return copyBytes(borrowed);
}

function readEditedRange(
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  position: number,
  length: number,
): Uint8Array {
  if (
    !Number.isSafeInteger(position) ||
    !Number.isSafeInteger(length) ||
    position < 0 ||
    length < 0 ||
    position + length > newSize
  )
    throw new RangeError("edited read is outside the result");
  const output = new Uint8Array(length);
  const dirtyNewEnd = checkedAdd(edit.offset, edit.insertLength);
  const delta = edit.insertLength - edit.deleteLength;
  let cursor = position;
  let written = 0;
  while (written < length) {
    if (cursor < edit.offset) {
      const count = Math.min(length - written, edit.offset - cursor);
      output.set(exactRead(source.read.bind(source), cursor, count, "source"), written);
      cursor += count;
      written += count;
      continue;
    }
    if (cursor < dirtyNewEnd) {
      const insertionOffset = cursor - edit.offset;
      const count = Math.min(length - written, edit.insertLength - insertionOffset);
      output.set(
        exactRead(edit.readInsert.bind(edit), insertionOffset, count, "insertion"),
        written,
      );
      cursor += count;
      written += count;
      continue;
    }
    const oldOffset = cursor - delta;
    const count = length - written;
    output.set(
      exactRead(source.read.bind(source), oldOffset, count, "source"),
      written,
    );
    cursor += count;
    written += count;
  }
  return output;
}

function editedContentStream(
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  readWindowBytes: number,
  admission: AdmissionController,
  cache?: ContentCache,
): ReadableStream<Uint8Array> {
  let position = 0;
  let queuedRelease: (() => void) | undefined;
  return new ReadableStream<Uint8Array>(
    {
      pull(controller) {
        queuedRelease?.();
        queuedRelease = undefined;
        if (position === newSize) {
          controller.close();
          return;
        }
        const length = Math.min(readWindowBytes, newSize - position);
        // Account for the output, the source-owned return value, and the detached
        // snapshot while exactRead is copying. Only the output survives enqueue,
        // but all three may coexist during a hostile or uncached source read.
        const workingBytes = checkedMultiply(
          length,
          3,
          "durable edit streamed read windows",
        );
        cache?.makeRoom(workingBytes);
        const release = admission.reserve(workingBytes);
        try {
          const bytes = readEditedRange(source, edit, newSize, position, length);
          position += length;
          controller.enqueue(bytes);
          queuedRelease = release;
        } catch (error) {
          release();
          throw error;
        }
      },
      cancel() {
        queuedRelease?.();
        queuedRelease = undefined;
      },
    },
    {
      // Prevent construction from starting source work before the streaming
      // preparation pipeline has admitted its own working set.
      highWaterMark: 0,
    },
  );
}

function makeNode(node: ManifestNode): PreparedNode {
  const encoded = encodeManifestNode(node);
  return Object.freeze({ hash: sha256(encoded), encoded, node });
}

function buildCandidate(
  path: AuthenticatedManifestTreePath,
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  cache?: ContentCache,
): PathCopyCandidate {
  if (edit.deleteLength !== edit.insertLength)
    throw new DurablePathCopyFallbackError(
      "bounded one-path copy requires an equal-length replacement",
    );
  const leafFrame = path.nodes.at(-1);
  if (!leafFrame || leafFrame.node.kind !== "leaf" || path.fileSize === 0)
    throw new DurablePathCopyFallbackError(
      "bounded one-path copy requires a nonempty authenticated leaf",
    );
  const leafEnd = checkedAdd(path.leafOffset, leafFrame.node.span);
  const editEnd = checkedAdd(edit.offset, edit.deleteLength);
  if (edit.offset < path.leafOffset || editEnd > leafEnd)
    throw new DurablePathCopyFallbackError(
      "edit crosses an authenticated leaf boundary",
    );
  const maxAffectedBytes = Math.min(
    runtime.maxWriteSessionBytes,
    runtime.maxPendingWriteBytes,
    // A cold edit may overlap the authenticated source BLOB, its normalized
    // cache value, the edited window, CDC carry/output, and replacement-node
    // ownership. Nine full leaf capacities is the conservative no-double-
    // buffer envelope. Larger leaves use the bounded streamed fallback.
    Math.floor(runtime.maxManagedResidentBytes / 9),
  );
  if (leafFrame.node.span > maxAffectedBytes)
    throw new DurablePathCopyFallbackError(
      "authenticated leaf exceeds the bounded durable edit window",
    );
  let attemptBytes = checkedMultiply(
    leafFrame.node.span,
    9,
    "durable path-copy byte windows",
  );
  attemptBytes = checkedAdd(
    attemptBytes,
    checkedMultiply(
      path.nodes.length + 2,
      storage.maxManifestNodeBytes,
      "durable path-copy node windows",
    ),
    "durable path-copy working set",
  );
  let releaseAttempt: () => void;
  try {
    cache?.makeRoom(attemptBytes);
    releaseAttempt = admission.reserve(attemptBytes);
  } catch (error) {
    if (error instanceof RangeError)
      throw new DurablePathCopyFallbackError(
        "durable path-copy working set cannot be admitted",
      );
    throw error;
  }
  let keepReservation = false;
  try {
    let sourceBytesRead = 0;
    let sourceReadCalls = 0;
    const measuredSource: DurableEditSource = Object.freeze({
      ...source,
      read(offset: number, length: number): Uint8Array {
        sourceReadCalls = checkedAdd(sourceReadCalls, 1);
        sourceBytesRead = checkedAdd(sourceBytesRead, length);
        return source.read(offset, length);
      },
    });
    const editedLeaf = readEditedRange(
      measuredSource,
      edit,
      newSize,
      path.leafOffset,
      leafFrame.node.span,
    );
    const entries: Array<{ readonly hash: Uint8Array; readonly bytes: Uint8Array }> =
      [];
    let emittedObjectBytes = 0;
    const maxOutputBytes = Math.min(
      maxAffectedBytes,
      runtime.maxPendingWriteBytes,
      storage.maxFinalTransactionBytes,
    );
    const chunker = new StreamingFastCdc(source.parameters);
    chunker.drain(
      editedLeaf,
      (borrowed) => {
        const borrowedLength = intrinsicByteLength(borrowed);
        if (entries.length >= MAX_PATH_COPY_LEAF_ENTRIES)
          throw new DurablePathCopyFallbackError(
            "edited leaf exceeds the bounded path-copy entry output",
          );
        const projectedBytes = checkedAdd(
          emittedObjectBytes,
          borrowedLength,
          "durable path-copy object output",
        );
        if (projectedBytes > maxOutputBytes)
          throw new DurablePathCopyFallbackError(
            "edited leaf exceeds the bounded path-copy byte output",
          );
        const bytes = copyBytes(borrowed);
        entries.push(Object.freeze({ hash: sha256(bytes), bytes }));
        emittedObjectBytes = projectedBytes;
      },
      leafFrame.finalAtLevel,
    );
    if (chunker.bufferedBytes !== 0)
      throw new DurablePathCopyFallbackError(
        "edited FastCDC stream did not reconnect at the authenticated leaf boundary",
      );
    const manifestEntries: ManifestEntry[] = entries.map((entry) =>
      Object.freeze({
        hash: copyBytes(entry.hash),
        length: intrinsicByteLength(entry.bytes),
      }),
    );
    const leaf: ManifestLeaf = Object.freeze({
      kind: "leaf",
      span: manifestEntries.reduce((sum, entry) => checkedAdd(sum, entry.length), 0),
      entryCount: manifestEntries.length,
      entries: Object.freeze(manifestEntries),
    });
    if (leaf.span !== leafFrame.node.span)
      throw new DurablePathCopyFallbackError(
        "edited leaf span did not reconnect to the source tree",
      );
    try {
      validateCanonicalManifestNode(
        leaf,
        source.parameters,
        leafFrame.finalAtLevel,
        path.nodes.length === 1,
      );
    } catch (error) {
      throw new DurablePathCopyFallbackError(
        `edited leaf requires regrouping beyond one authenticated path: ${String(error)}`,
      );
    }
    const prepared: PreparedNode[] = [makeNode(leaf)];
    const reused = new Map<string, ReusedClaim>();
    let replacement: ManifestChild = Object.freeze({
      hash: copyBytes(prepared[0]!.hash),
      span: leaf.span,
      entryCount: leaf.entryCount,
    });
    for (let level = path.nodes.length - 2; level >= 0; level -= 1) {
      const frame = path.nodes[level]!;
      if (frame.node.kind !== "internal" || frame.selectedChildIndex === undefined)
        throw new Error("ECORRUPT: authenticated path lost an internal child");
      const children = frame.node.children.map((child, index) => {
        if (index === frame.selectedChildIndex) return replacement;
        const claim = Object.freeze({
          sourcePath: Object.freeze([...frame.path, index]),
          nodeHash: copyBytes(child.hash),
          span: child.span,
          entryCount: child.entryCount,
        });
        reused.set(bytesToHex(child.hash), claim);
        return Object.freeze({
          hash: copyBytes(child.hash),
          span: child.span,
          entryCount: child.entryCount,
        });
      });
      const internal: ManifestInternal = Object.freeze({
        kind: "internal",
        span: children.reduce((sum, child) => checkedAdd(sum, child.span), 0),
        entryCount: children.reduce(
          (sum, child) => checkedAdd(sum, child.entryCount),
          0,
        ),
        children: Object.freeze(children),
      });
      try {
        validateCanonicalManifestNode(
          internal,
          source.parameters,
          frame.finalAtLevel,
          level === 0,
        );
      } catch (error) {
        throw new DurablePathCopyFallbackError(
          `copied ancestor requires bounded sibling regrouping: ${String(error)}`,
        );
      }
      const next = makeNode(internal);
      prepared.push(next);
      replacement = Object.freeze({
        hash: copyBytes(next.hash),
        span: internal.span,
        entryCount: internal.entryCount,
      });
    }
    if (replacement.span !== newSize)
      throw new Error("ECORRUPT: copied manifest root span differs from edited size");
    const entryCount = path.entryCount - leafFrame.node.entryCount + leaf.entryCount;
    const root = encodeManifestRoot({
      parameters: source.parameters,
      fileSize: newSize,
      entryCount,
      rootNodeHash: replacement.hash,
    });
    const newNodeHashes = new Set(prepared.map((node) => bytesToHex(node.hash)));
    const reusedValues = [...reused.values()].filter(
      (claim) => !newNodeHashes.has(bytesToHex(claim.nodeHash)),
    );
    const parentPaths = new Map<string, readonly number[]>();
    for (const claim of reusedValues) {
      const parent = Object.freeze(claim.sourcePath.slice(0, -1));
      parentPaths.set(parent.join("/"), parent);
    }
    const registeredNodesRead = [...parentPaths.values()].reduce(
      (sum, parent) => checkedAdd(sum, parent.length + 1),
      0,
    );
    const reconciledNodesRead = reusedValues.reduce(
      (sum, claim) => checkedAdd(sum, claim.sourcePath.length + 1),
      0,
    );
    const authenticatedNodesRead = checkedAdd(
      path.nodesRead,
      checkedAdd(registeredNodesRead, reconciledNodesRead),
    );
    const authenticationRootReads = checkedAdd(
      1,
      checkedAdd(parentPaths.size, reusedValues.length),
    );
    const manifestRecordsRead = checkedAdd(
      checkedAdd(authenticatedNodesRead, authenticationRootReads),
      checkedAdd(
        2,
        checkedAdd(
          checkedMultiply(prepared.length, 2),
          checkedMultiply(reusedValues.length, 2),
        ),
      ),
    );
    const candidate = Object.freeze({
      path,
      entries: Object.freeze(entries),
      nodes: Object.freeze(prepared),
      reused: Object.freeze(reusedValues),
      root,
      rootHash: sha256(root),
      entryCount,
      sourceBytesRead,
      sourceReadCalls,
      sourceReadTransactions: checkedMultiply(
        sourceReadCalls,
        source.readStorageTransactions ?? 0,
        "durable source read transactions",
      ),
      authenticatedNodesRead,
      manifestRecordsRead,
      release: releaseAttempt,
    });
    keepReservation = true;
    return candidate;
  } finally {
    if (!keepReservation) releaseAttempt();
  }
}

function batchesByBytes<T>(
  values: readonly T[],
  rowLimit: number,
  byteLimit: number,
  size: (value: T) => number,
): readonly (readonly T[])[] {
  const batches: T[][] = [];
  let batch: T[] = [];
  let bytes = 0;
  for (const value of values) {
    const valueBytes = checkedAdd(size(value), 256);
    if (batch.length && (batch.length >= rowLimit || bytes + valueBytes > byteLimit)) {
      batches.push(batch);
      batch = [];
      bytes = 0;
    }
    if (valueBytes > byteLimit)
      throw new RangeError("one durable path-copy value exceeds transaction envelope");
    batch.push(value);
    bytes = checkedAdd(bytes, valueBytes);
  }
  if (batch.length) batches.push(batch);
  return Object.freeze(batches.map((value) => Object.freeze(value)));
}

function reconciliationWorkLimit(storage: StorageLimits): number {
  return Math.max(
    1,
    Math.min(
      storage.maxQueryBatchSize,
      Math.floor((storage.maxFinalTransactionRows - 8) / 4),
    ),
  );
}

function projectedPersistenceTransactions(
  candidate: PathCopyCandidate,
  storage: StorageLimits,
): number {
  const objectBatches = batchesByBytes(
    candidate.entries,
    storage.maxQueryBatchSize,
    storage.maxFinalTransactionBytes,
    (entry) => intrinsicByteLength(entry.bytes),
  ).length;
  const reusedBatches = Math.ceil(candidate.reused.length / storage.maxQueryBatchSize);
  const reconciliationWork = checkedAdd(
    1,
    candidate.nodes.reduce((sum, prepared) => {
      const edges =
        prepared.node.kind === "leaf"
          ? prepared.node.entries.length
          : prepared.node.children.length;
      return checkedAdd(sum, edges);
    }, 0),
  );
  const reconciliationTransactions = checkedAdd(
    Math.ceil(reconciliationWork / reconciliationWorkLimit(storage)),
    1,
  );
  return (
    5 +
    objectBatches +
    reusedBatches +
    (candidate.reused.length ? 1 : 0) +
    reconciliationTransactions
  );
}

function persistCandidate(
  port: OperationsStorage,
  source: DurableEditSource,
  candidate: PathCopyCandidate,
  storage: StorageLimits,
  cache: ContentCache | undefined,
  clock: () => number,
  transactionLimit: number,
): StreamPreparedManifest & { readonly storageTransactions: number } {
  const leaseId = globalThis.crypto.randomUUID();
  const ownerId = globalThis.crypto.randomUUID();
  const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
  const now = clock();
  if (!Number.isSafeInteger(now) || now < 0)
    throw new Error("clock must return a nonnegative safe integer");
  const budget: StorageWorkBudget = Object.freeze({
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
    maxStatements: storage.maxFinalTransactionRows,
    maxElapsedMs: 250,
  });
  let storageTransactions = 0;
  let begun = false;
  const transact = <T>(
    mode: StorageTransactionMode,
    callback: (tx: StorageTransactionPorts) => T,
  ): T => {
    if (storageTransactions >= transactionLimit)
      throw new DurablePathCopyFallbackError(
        "durable path-copy exceeds its aggregate storage transaction cap",
      );
    storageTransactions += 1;
    return port.transaction(mode, budget, callback);
  };
  try {
    transact<void>("write", (tx) => {
      const staging = tx.staging(storage);
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: checkedAdd(now, storage.stagingLeaseMs),
      });
      tx.manifestTree(storage, cache).protectSourceManifest(
        leaseId,
        ownerNonce,
        source.manifestHash,
      );
      staging.bumpRoot(5, leaseId);
    });
    begun = true;

    const uniqueObjects = [
      ...new Map(
        candidate.entries.map((entry) => [bytesToHex(entry.hash), entry]),
      ).values(),
    ];
    for (const batch of batchesByBytes(
      uniqueObjects,
      storage.maxQueryBatchSize,
      storage.maxFinalTransactionBytes,
      (entry) => intrinsicByteLength(entry.bytes),
    ))
      transact<void>("write", (tx) => {
        tx.content(storage, cache).putObjectsBatch(
          batch.map((entry) => ({ hash: entry.hash, bytes: entry.bytes })),
        );
        tx.staging(storage).appendBatch(
          leaseId,
          ownerNonce,
          batch.map((entry) => ({
            kind: "object" as const,
            hash: entry.hash,
            size: intrinsicByteLength(entry.bytes),
          })),
        );
      });

    transact<void>("write", (tx) => {
      const content = tx.content(storage, cache);
      content.putManifestNodesBatch(
        candidate.nodes.map((node) => ({ hash: node.hash, encoded: node.encoded })),
      );
      tx.staging(storage).appendBatch(
        leaseId,
        ownerNonce,
        candidate.nodes.map((node) => ({
          kind: "manifest-node" as const,
          hash: node.hash,
          size: intrinsicByteLength(node.encoded),
        })),
      );
    });

    const reused = candidate.reused;
    for (let start = 0; start < reused.length; start += storage.maxQueryBatchSize) {
      const batch = reused.slice(start, start + storage.maxQueryBatchSize);
      transact<void>("write", (tx) => {
        const content = tx.content(storage, cache);
        const members = batch.map((claim) => {
          const encoded = content.getManifestNode(claim.nodeHash);
          if (!encoded) throw new Error("ECORRUPT: reused subtree node is missing");
          return Object.freeze({
            kind: "manifest-node" as const,
            hash: claim.nodeHash,
            size: intrinsicByteLength(encoded),
          });
        });
        tx.staging(storage).appendBatch(leaseId, ownerNonce, members);
      });
    }
    if (reused.length)
      transact<void>("write", (tx) => {
        tx.manifestTree(storage, cache).registerReusedSubtrees(
          leaseId,
          ownerNonce,
          source.manifestHash,
          reused,
        );
      });

    const certificate = transact<ClosureCertificate>("write", (tx) => {
      tx.content(storage, cache).putManifestRoot(candidate.rootHash, candidate.root);
      const staging = tx.staging(storage);
      staging.appendBatch(leaseId, ownerNonce, [
        {
          kind: "manifest-root",
          hash: candidate.rootHash,
          size: intrinsicByteLength(candidate.root),
        },
      ]);
      staging.beginReconciliation(leaseId, ownerNonce, candidate.rootHash);
      return Object.freeze({
        ...staging.snapshot(leaseId, ownerNonce),
        manifestHash: copyBytes(candidate.rootHash),
      });
    });
    let complete = false;
    while (!complete)
      complete = transact<{ readonly complete: boolean }>("write", (tx) =>
        tx
          .staging(storage)
          .reconcileBatch(leaseId, ownerNonce, reconciliationWorkLimit(storage)),
      ).complete;
    transact<void>("write", (tx) => {
      const staging = tx.staging(storage);
      staging.seal(certificate);
      staging.bumpRoot(5, leaseId);
    });
    return Object.freeze({
      hash: copyBytes(candidate.rootHash),
      size: candidate.path.fileSize,
      certificate,
      storageTransactions,
    });
  } catch (error) {
    if (begun)
      try {
        port.transaction("write", budget, (tx) => {
          tx.staging(storage).delete(leaseId, ownerNonce);
        });
      } catch {}
    throw error;
  }
}

export async function prepareDurableEditedContent(
  port: OperationsStorage,
  source: DurableEditSource,
  edit: DurableContentEdit,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  cache?: ContentCache,
  clock: () => number = Date.now,
): Promise<DurableEditPreparedManifest> {
  const newSize = validateInputs(source, edit);
  if (newSize > storage.maxFileBytes)
    throw new RangeError("edited file exceeds maxFileBytes");
  let reason: string | undefined;
  try {
    const pathCapacity = checkedMultiply(
      storage.maxManifestDepth + 1,
      checkedMultiply(
        storage.maxManifestNodeBytes,
        4,
        "authenticated path node ownership",
      ),
      "authenticated manifest path ownership",
    );
    let releasePath: () => void;
    try {
      cache?.makeRoom(pathCapacity);
      releasePath = admission.reserve(pathCapacity);
    } catch (error) {
      if (error instanceof RangeError)
        throw new DurablePathCopyFallbackError(
          "authenticated path working set cannot be admitted",
        );
      throw error;
    }
    try {
      const path = port.transaction(
        "read",
        {
          maxRows: storage.maxQueryBatchSize,
          maxBytes: runtime.maxQueryBatchBytes,
          maxStatements: storage.maxManifestDepth * 4 + 8,
          maxElapsedMs: 250,
        },
        (tx) =>
          tx
            .manifestTree(storage, cache)
            .pathAtOffset(source.manifestHash, edit.offset),
      );
      if (
        path.fileSize !== source.size ||
        path.parameters.minimum !== source.parameters.minimum ||
        path.parameters.average !== source.parameters.average ||
        path.parameters.maximum !== source.parameters.maximum
      )
        throw new Error("ECORRUPT: durable edit source disagrees with manifest root");
      const candidate = buildCandidate(
        path,
        source,
        edit,
        newSize,
        storage,
        runtime,
        admission,
        cache,
      );
      try {
        const projectedTransactions = checkedAdd(
          1,
          checkedAdd(
            candidate.sourceReadTransactions,
            projectedPersistenceTransactions(candidate, storage),
          ),
          "durable path-copy aggregate transactions",
        );
        if (projectedTransactions > MAX_PATH_COPY_TRANSACTIONS)
          throw new DurablePathCopyFallbackError(
            "durable path-copy exceeds its aggregate storage transaction cap",
          );
        const persistenceLimit =
          MAX_PATH_COPY_TRANSACTIONS - 1 - candidate.sourceReadTransactions;
        const prepared = persistCandidate(
          port,
          source,
          candidate,
          storage,
          cache,
          clock,
          persistenceLimit,
        );
        return Object.freeze({
          hash: prepared.hash,
          size: newSize,
          certificate: prepared.certificate,
          mode: "durable-path-copy",
          pathCopyMetrics: Object.freeze({
            authenticatedNodesRead: candidate.authenticatedNodesRead,
            manifestRecordsRead: candidate.manifestRecordsRead,
            emittedNodes: candidate.nodes.length,
            emittedEntries: candidate.entries.length,
            emittedObjectBytes: candidate.entries.reduce(
              (sum, entry) => checkedAdd(sum, intrinsicByteLength(entry.bytes)),
              0,
            ),
            reusedSubtrees: candidate.reused.length,
            storageTransactions:
              prepared.storageTransactions + 1 + candidate.sourceReadTransactions,
            sourceReadCalls: candidate.sourceReadCalls,
            sourceReadTransactions: candidate.sourceReadTransactions,
            sourceBytesRead: candidate.sourceBytesRead,
          }),
        });
      } finally {
        candidate.release();
      }
    } finally {
      releasePath();
    }
  } catch (error) {
    if (!(error instanceof DurablePathCopyFallbackError)) throw error;
    reason = error.message;
  }
  const readWindowBytes = Math.max(
    1,
    Math.min(
      1024 * 1024,
      source.maxReadWindowBytes ?? 32 * 1024,
      runtime.maxWriteSessionBytes,
      runtime.maxQueryBatchBytes,
      Math.floor(storage.maxFinalTransactionBytes / 2),
    ),
  );
  const prepared = await prepareContentStreaming(
    port,
    editedContentStream(source, edit, newSize, readWindowBytes, admission, cache),
    storage,
    runtime,
    admission,
    undefined,
    cache,
    clock,
    newSize,
  );
  return Object.freeze({
    ...prepared,
    mode: "streamed-fallback",
    pathCopyReason: reason,
  });
}

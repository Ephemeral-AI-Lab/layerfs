import {
  bytesToHex,
  copyBytes,
  equalBytes,
  intrinsicByteLength,
} from "../cas/bytes.js";
import { manifestIdFromHash, type HashFunction } from "../cas/sha256.js";
import { StreamingFastCdc } from "../cdc/fastcdc.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
  validateSupportedManifestParameters,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
  type ManifestNode,
  type ManifestParameters,
} from "../manifests/codec.js";
import { validateCanonicalManifestNode } from "../manifests/cursor.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";
import {
  DURABLE_METADATA_ROW_BYTES,
  type AdmissionController,
  type RuntimeLimits,
  type StorageLimits,
} from "../resources/limits.js";
import { ContentCache } from "../cache/content-cache.js";
import type {
  AuthenticatedManifestTreePath,
  ClosureCertificate,
  ContentStore,
  OperationsStorage,
  StorageTransactionPorts,
  StorageTransactionMode,
  StorageWorkBudget,
  ValidatedSealedLease,
} from "./storage-ports.js";
import {
  prepareContentStreaming,
  type StreamPreparedManifest,
} from "./streaming-prepare.js";
import type { DiagnosticBuiltManifest } from "./full-rebuild.js";
import type { EncodedManifestNode } from "../manifests/builder.js";
import {
  DEFAULT_LOCAL_REBUILD_LIMITS,
  LocalRebuildLimitError,
  rebuildManifestLocallyWithParametersOwned,
  type LocalRebuildLimits,
  type LocallyRebuiltManifest,
} from "./local-rebuild.js";
import {
  BoundedRebuildFallbackError,
  assembleBoundedManifestState,
  rebuildManifestBoundedOwned,
  type BoundedManifestState,
  type BoundedPathFrame,
} from "./bounded-local-rebuild.js";

export interface DurableEditSource {
  readonly manifestHash: Uint8Array;
  /** Exact root bytes authenticated during source selection, when available. */
  readonly rootBytes?: Uint8Array;
  /** Root mutation generation from the same source snapshot, when available. */
  readonly rootMutationGeneration?: number;
  readonly size: number;
  readonly parameters: ManifestParameters;
  /** Exact storage transactions performed by one synchronous `read` call. */
  readonly readStorageTransactions?: number;
  /** Dynamic transaction count for sources that coalesce multiple reads. */
  readonly getReadStorageTransactions?: () => number;
  /** Largest byte window one bounded source-read transaction can materialize. */
  readonly maxReadWindowBytes?: number;
  /** Optional transaction-bound source read used to coalesce the bounded loader. */
  readonly readInTransaction?: (
    content: ContentStore,
    offset: number,
    length: number,
  ) => Uint8Array;
  /** Releases any source-owned bounded read window retained for this edit. */
  readonly releaseReadWindow?: () => void;
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
  readonly mode: "durable-path-copy" | "local-rebuild" | "streamed-fallback";
  readonly pathCopyReason?: string;
  readonly pathCopyMetrics?: DurablePathCopyMetrics;
  readonly localRebuildReason?: string;
  readonly localRebuildMetrics?: {
    readonly storageTransactions: number;
    readonly persistenceMerged: boolean;
    readonly persistenceRows: number;
    readonly persistenceBytes: number;
    readonly persistenceUnits: number;
    readonly sourceReadCalls: number;
    readonly sourceReadTransactions: number;
    readonly sourceBytesRead: number;
    readonly authenticatedNodesRead: number;
    readonly loadedEntries: number;
    readonly loadedNodes: number;
    readonly affectedEntries: number;
    readonly newObjectCount: number;
    readonly newManifestNodeCount: number;
    readonly reusedSubtrees: number;
    readonly reusedManifestNodeCount: number;
    readonly scanWindowBytes: number;
    readonly reconnectOldOffset: number;
    readonly reconnectNewOffset: number;
    readonly pathAuthenticationTransactions: number;
    readonly phaseMs: LocalRebuildPhaseTimings;
  };
  readonly fallbackMetrics?: {
    readonly sourceReadCalls: number;
    readonly sourceReadTransactions: number;
    readonly sourceBytesRead: number;
    readonly pathAuthenticationTransactions: number;
    readonly persistenceTransactions: number;
    readonly storageTransactions: number;
    readonly readWindowBytes: number;
  };
}

export interface LocalRebuildPhaseTimings {
  sourceReadMs: number;
  manifestLoadMs: number;
  rebuildMs: number;
  persistenceMs: number;
  reconciliationMs: number;
  finalizeMs: number;
}

/**
 * Read-only state loaded by the filesystem adapter while it still owns the
 * mutation-source read transaction. This is intentionally an internal
 * hand-off; the public prepareDurableEditedContent API remains unchanged.
 */
export interface DurableEditReadSnapshot {
  readonly state: BoundedManifestState;
}

type DurableEditValidatedLease = ValidatedSealedLease & {
  /** Internal expiry carried across the same write transaction only. */
  readonly expiresAtMs?: number;
};

type DurableEditFinalizer = (
  tx: StorageTransactionPorts,
  certificate: ClosureCertificate,
  hash: Uint8Array,
  size: number,
  validatedLease?: DurableEditValidatedLease,
) => void;

function emptyLocalRebuildPhaseTimings(): LocalRebuildPhaseTimings {
  return {
    sourceReadMs: 0,
    manifestLoadMs: 0,
    rebuildMs: 0,
    persistenceMs: 0,
    reconciliationMs: 0,
    finalizeMs: 0,
  };
}

interface PreparedNode {
  readonly hash: Uint8Array;
  readonly encoded: Uint8Array;
  readonly node: ManifestNode;
}

interface ReusedClaim {
  readonly sourcePath: readonly number[];
  readonly sourceFinalAtLevel: boolean;
  /** Present when the source path was authenticated by the bounded loader. */
  readonly sourceLeafDelta?: number;
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
    (source.getReadStorageTransactions !== undefined &&
      typeof source.getReadStorageTransactions !== "function") ||
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

function measuredSourceReadTransactions(
  source: DurableEditSource,
  readCalls: number,
  label: string,
): number {
  const dynamic = source.getReadStorageTransactions?.();
  if (dynamic !== undefined) {
    if (!Number.isSafeInteger(dynamic) || dynamic < 0)
      throw new Error(`${label} must return a nonnegative safe integer`);
    return dynamic;
  }
  return checkedMultiply(readCalls, source.readStorageTransactions ?? 0, label);
}

interface BoundedSourceWindow {
  readonly source: DurableEditSource;
  release(): void;
}

/**
 * Gives bounded local rebuilds the same one-window behavior as the filesystem
 * source for adapters that expose a window limit but do not provide a
 * transaction-bound read callback. The first slice materializes one centered
 * bounded window; later slices are detached views over that window. The
 * retained bytes stay admitted until the attempt finishes.
 */
function boundedSourceWindow(
  source: DurableEditSource,
  cache: ContentCache | undefined,
): BoundedSourceWindow {
  const maximum = source.maxReadWindowBytes;
  if (source.readInTransaction || maximum === undefined || source.size === 0)
    return Object.freeze({ source, release: () => {} });
  let physicalReads = 0;
  let cached:
    | {
        readonly offset: number;
        readonly bytes: Uint8Array;
        readonly release: () => void;
      }
    | undefined;
  const release = (): void => {
    cached?.release();
    cached = undefined;
  };
  const windowed = Object.freeze({
    ...source,
    readStorageTransactions: 0,
    getReadStorageTransactions: () => physicalReads,
    read(offset: number, length: number): Uint8Array {
      if (length === 0) return new Uint8Array(0);
      if (length > maximum)
        throw new LocalRebuildLimitError(
          "bounded source slice exceeds its admitted read window",
        );
      const end = checkedAdd(offset, length, "bounded source slice end");
      const cachedEnd = cached
        ? checkedAdd(cached.offset, cached.bytes.byteLength)
        : -1;
      if (!cached || offset < cached.offset || end > cachedEnd) {
        release();
        const windowLength = Math.min(maximum, source.size);
        const maxOffset = Math.max(0, source.size - windowLength);
        const centered = Math.max(0, offset - Math.floor((windowLength - length) / 2));
        const windowOffset = Math.min(centered, maxOffset);
        const available = Math.min(windowLength, source.size - windowOffset);
        const bytes = source.read(windowOffset, available);
        if (!(bytes instanceof Uint8Array) || bytes.byteLength !== available)
          throw new Error("random-access source returned a partial range");
        physicalReads = checkedAdd(
          physicalReads,
          source.readStorageTransactions ?? 1,
          "bounded source window transactions",
        );
        const retained = cache?.reserveOperation(bytes.byteLength) ?? (() => {});
        cached = Object.freeze({ offset: windowOffset, bytes, release: retained });
      }
      return copyBytes(
        cached.bytes.subarray(offset - cached.offset, end - cached.offset),
      );
    },
  });
  return Object.freeze({ source: windowed, release });
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

function makeNode(node: ManifestNode, hashBytes: HashFunction): PreparedNode {
  const encoded = encodeManifestNode(node);
  return Object.freeze({ hash: hashBytes(encoded), encoded, node });
}

function buildCandidate(
  path: AuthenticatedManifestTreePath,
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  cache: ContentCache | undefined,
  hashBytes: HashFunction,
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
        // StreamingFastCdc#emitChunk returns a fresh, detached slice, so the
        // borrowed chunk is already library-owned.
        entries.push(Object.freeze({ hash: hashBytes(borrowed), bytes: borrowed }));
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
    const prepared: PreparedNode[] = [makeNode(leaf, hashBytes)];
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
      const sourceParent = frame.node;
      const children = sourceParent.children.map((child, index) => {
        if (index === frame.selectedChildIndex) return replacement;
        const sourceFinalAtLevel =
          frame.finalAtLevel && index === sourceParent.children.length - 1;
        const claim = Object.freeze({
          sourcePath: Object.freeze([...frame.path, index]),
          sourceFinalAtLevel,
          sourceLeafDelta: path.nodes.length - (frame.path.length + 2),
          nodeHash: copyBytes(child.hash),
          span: child.span,
          entryCount: child.entryCount,
        });
        const key = bytesToHex(child.hash);
        const prior = reused.get(key);
        // A non-final source proof is strictly stronger: it is valid in both
        // non-final and final destination contexts. Never let a later final
        // occurrence overwrite that bounded authentication path.
        if (!prior || (prior.sourceFinalAtLevel && !sourceFinalAtLevel))
          reused.set(key, claim);
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
      const next = makeNode(internal, hashBytes);
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
    const rootHash = hashBytes(root);
    const unchangedRoot = bytesToHex(rootHash) === bytesToHex(source.manifestHash);
    const persistedEntries = unchangedRoot ? [] : entries;
    const persistedNodes = unchangedRoot ? [] : prepared;
    const newNodeHashes = new Set(persistedNodes.map((node) => bytesToHex(node.hash)));
    const reusedValues = unchangedRoot
      ? [
          Object.freeze({
            sourcePath: Object.freeze([] as number[]),
            sourceFinalAtLevel: true,
            sourceLeafDelta: path.nodes.length - 1,
            nodeHash: copyBytes(replacement.hash),
            span: replacement.span,
            entryCount: replacement.entryCount,
          }),
        ]
      : [...reused.values()].filter(
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
      checkedAdd(
        registeredNodesRead,
        checkedMultiply(
          reconciledNodesRead,
          2,
          "reused closure and validation authentication",
        ),
      ),
    );
    const authenticationRootReads = checkedAdd(
      1,
      checkedAdd(
        parentPaths.size,
        checkedMultiply(reusedValues.length, 2, "reused authentication roots"),
      ),
    );
    const manifestRecordsRead = checkedAdd(
      checkedAdd(authenticatedNodesRead, authenticationRootReads),
      checkedAdd(
        18,
        checkedAdd(
          checkedMultiply(prepared.length, 4),
          checkedMultiply(reusedValues.length, 4),
        ),
      ),
    );
    const candidate = Object.freeze({
      path,
      entries: Object.freeze(persistedEntries),
      nodes: Object.freeze(persistedNodes),
      reused: Object.freeze(reusedValues),
      root,
      rootHash,
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
  return Math.max(1, Math.floor((storage.maxFinalTransactionRows - 12) / 5));
}

type ReusedClaimPath = { readonly sourcePath: readonly number[] };

function reusedClaimBatches<T extends ReusedClaimPath>(
  claims: readonly T[],
  storage: StorageLimits,
): readonly (readonly T[])[] {
  const batches: T[][] = [];
  let batch: T[] = [];
  let resultRows = 2;
  let statements = 3;
  let parents = new Set<string>();
  const flush = (): void => {
    if (batch.length) batches.push(batch);
    batch = [];
    resultRows = 2;
    statements = 3;
    parents = new Set<string>();
  };
  for (const claim of claims) {
    const parentKey = claim.sourcePath.slice(0, -1).join("/");
    const pathAuthenticationCost = checkedAdd(
      3,
      checkedMultiply(claim.sourcePath.length, 2, "reused path query envelope"),
      "reused path query envelope",
    );
    let authenticationCost = parents.has(parentKey) ? 0 : pathAuthenticationCost;
    const nextResultRows = checkedAdd(
      resultRows,
      checkedAdd(authenticationCost, 1),
      "reused claim result rows",
    );
    const nextStatements = checkedAdd(
      statements,
      checkedAdd(authenticationCost, 2),
      "reused claim statements",
    );
    if (
      batch.length &&
      (batch.length >=
        Math.min(
          storage.maxQueryBatchSize,
          Math.floor((storage.maxFinalTransactionRows - 8) / 3),
        ) ||
        nextResultRows > storage.maxFinalTransactionRows ||
        nextStatements > storage.maxFinalTransactionRows * 4)
    )
      flush();
    authenticationCost = parents.has(parentKey) ? 0 : pathAuthenticationCost;
    resultRows = checkedAdd(
      resultRows,
      checkedAdd(authenticationCost, 1),
      "reused claim result rows",
    );
    statements = checkedAdd(
      statements,
      checkedAdd(authenticationCost, 2),
      "reused claim statements",
    );
    if (
      resultRows > storage.maxFinalTransactionRows ||
      statements > storage.maxFinalTransactionRows * 4
    )
      throw new DurablePathCopyFallbackError(
        "one reused subtree authentication exceeds the transaction query envelope",
      );
    parents.add(parentKey);
    batch.push(claim);
  }
  flush();
  return Object.freeze(batches.map((values) => Object.freeze(values)));
}

function durableWriteBatchLimit(storage: StorageLimits): number {
  return Math.max(
    1,
    Math.min(
      storage.maxQueryBatchSize,
      Math.floor((storage.maxFinalTransactionRows - 24) / 6),
    ),
  );
}

function candidateValidationRows(candidate: PathCopyCandidate): number {
  return candidate.nodes.reduce(
    (rows, prepared) =>
      prepared.node.kind === "internal"
        ? checkedAdd(rows, prepared.node.children.length, "path-copy validation rows")
        : rows,
    1,
  );
}

function projectedPersistenceTransactions(
  candidate: PathCopyCandidate,
  storage: StorageLimits,
): number {
  const uniqueEntries = [
    ...new Map(
      candidate.entries.map((entry) => [bytesToHex(entry.hash), entry]),
    ).values(),
  ];
  const objectBatches = batchesByBytes(
    uniqueEntries,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    (entry) => intrinsicByteLength(entry.bytes),
  ).length;
  const nodeBatches = batchesByBytes(
    candidate.nodes,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    (node) => intrinsicByteLength(node.encoded),
  ).length;
  const reusedBatches = reusedClaimBatches(candidate.reused, storage).length;
  const seenEdges = new Set<string>();
  let reconciliationUnits = 4;
  for (const prepared of candidate.nodes) {
    const edges =
      prepared.node.kind === "leaf"
        ? prepared.node.entries.map((entry) => ({ kind: 0, hash: entry.hash }))
        : prepared.node.children.map((child) => ({ kind: 2, hash: child.hash }));
    for (const edge of edges) {
      const key = `${edge.kind}:${bytesToHex(edge.hash)}`;
      reconciliationUnits = checkedAdd(reconciliationUnits, seenEdges.has(key) ? 1 : 4);
      seenEdges.add(key);
    }
  }
  const reconciliationWork = Math.ceil(reconciliationUnits / 4);
  const validationWork = checkedAdd(
    candidate.reused.length,
    candidate.nodes.reduce((sum, prepared) => {
      const work = prepared.node.kind === "leaf" ? 1 : prepared.node.children.length;
      return checkedAdd(sum, work);
    }, 0),
  );
  const reconciliationTransactions = checkedAdd(
    Math.ceil(
      checkedAdd(reconciliationWork, validationWork) / reconciliationWorkLimit(storage),
    ),
    1,
  );
  return (
    5 + objectBatches + nodeBatches + reusedBatches * 2 + reconciliationTransactions
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
    maxStatements: storage.maxFinalTransactionRows * 4,
    maxElapsedMs: 5_000,
  });
  const uniqueObjects = [
    ...new Map(
      candidate.entries.map((entry) => [bytesToHex(entry.hash), entry]),
    ).values(),
  ];
  const newObjectBytes = uniqueObjects.reduce(
    (sum, entry) => checkedAdd(sum, intrinsicByteLength(entry.bytes)),
    0,
  );
  const newNodeBytes = candidate.nodes.reduce(
    (sum, node) => checkedAdd(sum, intrinsicByteLength(node.encoded)),
    0,
  );
  const newPayloadBytes = checkedAdd(
    checkedAdd(newObjectBytes, newNodeBytes),
    intrinsicByteLength(candidate.root),
  );
  const durablePayloadReservation = checkedAdd(
    checkedMultiply(newPayloadBytes, 2, "path-copy physical and staging payload"),
    checkedMultiply(
      candidate.reused.length,
      storage.maxManifestNodeBytes,
      "path-copy reused staging envelope",
    ),
  );
  const metadataRows = checkedAdd(
    checkedAdd(
      checkedMultiply(uniqueObjects.length, 3, "path-copy object metadata"),
      checkedMultiply(candidate.nodes.length, 4, "path-copy node metadata"),
    ),
    checkedAdd(
      checkedAdd(
        checkedMultiply(candidate.reused.length, 3, "path-copy reused metadata"),
        candidateValidationRows(candidate),
        "path-copy validation metadata",
      ),
      16,
      "path-copy fixed metadata",
    ),
  );
  const durableMetadataReservation = checkedMultiply(
    metadataRows,
    DURABLE_METADATA_ROW_BYTES,
    "path-copy metadata reservation",
  );
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
  const certificateHolder: { value?: ClosureCertificate } = {};
  const steps: PersistenceStep[] = [];
  steps.push({
    rows: 8,
    bytes: 4096,
    units: 1,
    run: (tx) => {
      const staging = tx.staging(storage, cache);
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: checkedAdd(now, storage.stagingLeaseMs),
        ingestReservationBytes: durablePayloadReservation,
        metadataReservationBytes: durableMetadataReservation,
      });
      staging.consumeMetadataReservation(
        leaseId,
        ownerNonce,
        DURABLE_METADATA_ROW_BYTES,
      );
      const manifestTree = tx.manifestTree(storage, cache) as ReturnType<
        StorageTransactionPorts["manifestTree"]
      > & {
        protectTrustedSourceManifest?: (
          leaseId: string,
          ownerNonce: Uint8Array,
          manifestHash: Uint8Array,
          rootBytes: Uint8Array,
        ) => void;
      };
      if (manifestTree.protectTrustedSourceManifest && source.rootBytes)
        manifestTree.protectTrustedSourceManifest(
          leaseId,
          ownerNonce,
          source.manifestHash,
          source.rootBytes,
        );
      else manifestTree.protectSourceManifest(leaseId, ownerNonce, source.manifestHash);
      const snapshotStaging = staging as typeof staging & {
        bumpRootFromSnapshot?: (
          kind: number,
          id: string,
          expectedGeneration: number,
        ) => void;
      };
      if (
        source.rootMutationGeneration !== undefined &&
        snapshotStaging.bumpRootFromSnapshot
      )
        snapshotStaging.bumpRootFromSnapshot(5, leaseId, source.rootMutationGeneration);
      else staging.bumpRoot(5, leaseId);
    },
  });
  for (const batch of batchesByBytes(
    uniqueObjects,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    (entry) => intrinsicByteLength(entry.bytes),
  )) {
    const batchBytes = batch.reduce(
      (sum, entry) => checkedAdd(sum, intrinsicByteLength(entry.bytes)),
      0,
    );
    steps.push({
      rows: checkedAdd(batch.length * 3, 8),
      bytes: batchBytes,
      units: 1,
      run: (tx) => {
        const staging = tx.staging(storage, cache);
        staging.consumeIngestReservation(leaseId, ownerNonce, batchBytes);
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          batch.length * DURABLE_METADATA_ROW_BYTES,
        );
        tx.content(storage, cache).putObjectsBatch(
          batch.map((entry) => ({ hash: entry.hash, bytes: entry.bytes })),
        );
        staging.appendBatch(
          leaseId,
          ownerNonce,
          batch.map((entry) => ({
            kind: "object" as const,
            hash: entry.hash,
            size: intrinsicByteLength(entry.bytes),
          })),
        );
      },
    });
  }
  for (const batch of batchesByBytes(
    candidate.nodes,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    (node) => intrinsicByteLength(node.encoded),
  )) {
    const batchBytes = batch.reduce(
      (sum, node) => checkedAdd(sum, intrinsicByteLength(node.encoded)),
      0,
    );
    steps.push({
      rows: checkedAdd(batch.length * 3, 8),
      bytes: batchBytes,
      units: 1,
      run: (tx) => {
        const staging = tx.staging(storage, cache);
        staging.consumeIngestReservation(leaseId, ownerNonce, batchBytes);
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          batch.length * 2 * DURABLE_METADATA_ROW_BYTES,
        );
        const encodedNodes = batch.map((node) => ({
          hash: node.hash,
          encoded: node.encoded,
        }));
        const content = tx.content(storage, cache) as LocalFreshContentStore;
        const inserted = content.putFreshManifestNodesBatch
          ? content.putFreshManifestNodesBatch(encodedNodes)
          : content.putManifestNodesBatch(encodedNodes);
        staging.appendBatch(
          leaseId,
          ownerNonce,
          batch.map((node) => ({
            kind: "manifest-node" as const,
            hash: node.hash,
            size: intrinsicByteLength(node.encoded),
          })),
        );
      },
    });
  }
  const reused = candidate.reused;
  const reusedBatches = reusedClaimBatches(reused, storage);
  const verifiedReusedNodeSizes = new Map<string, number>();
  for (const batch of reusedBatches) {
    steps.push({
      rows: checkedAdd(batch.length * 12, 8),
      bytes: 0,
      units: checkedAdd(batch.length * 2, 1),
      run: (tx) => {
        const staging = tx.staging(storage, cache) as ReturnType<
          StorageTransactionPorts["staging"]
        > & {
          appendTrustedReusedManifestBatch?: (
            leaseId: string,
            ownerNonce: Uint8Array,
            nodeHashes: readonly Uint8Array[],
          ) => ClosureCertificate & {
            readonly verifiedNodeSizes: ReadonlyMap<string, number>;
          };
          appendReusedManifestBatch?: (
            leaseId: string,
            ownerNonce: Uint8Array,
            nodeHashes: readonly Uint8Array[],
          ) => ClosureCertificate & {
            readonly verifiedNodeSizes: ReadonlyMap<string, number>;
          };
        };
        if (staging.appendTrustedReusedManifestBatch) {
          const appended = staging.appendTrustedReusedManifestBatch(
            leaseId,
            ownerNonce,
            batch.map((claim) => claim.nodeHash),
          );
          for (const [hash, size] of appended.verifiedNodeSizes)
            verifiedReusedNodeSizes.set(hash, size);
        } else if (staging.appendReusedManifestBatch) {
          const appended = staging.appendReusedManifestBatch(
            leaseId,
            ownerNonce,
            batch.map((claim) => claim.nodeHash),
          );
          for (const [hash, size] of appended.verifiedNodeSizes)
            verifiedReusedNodeSizes.set(hash, size);
        } else {
          const content = tx.content(storage, cache);
          const members = batch.map((claim) => {
            const size = content.withManifestNode(claim.nodeHash, intrinsicByteLength);
            if (size === undefined)
              throw new Error("ECORRUPT: reused subtree node is missing");
            return Object.freeze({
              kind: "manifest-node" as const,
              hash: claim.nodeHash,
              size,
            });
          });
          staging.appendBatch(leaseId, ownerNonce, members);
        }
      },
    });
  }
  for (const batch of reusedBatches) {
    steps.push({
      rows: checkedAdd(batch.length * 12, 8),
      bytes: 0,
      units: checkedAdd(batch.length * 2, 1),
      run: (tx) => {
        const staging = tx.staging(storage, cache);
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          batch.length * DURABLE_METADATA_ROW_BYTES,
        );
        const certificateState = staging.snapshot(leaseId, ownerNonce);
        const certificatePatch: {
          value?: {
            readonly chainDigest: Uint8Array;
            readonly chainFold: Uint8Array;
            readonly objectCount: number;
            readonly objectBytes: number;
            readonly nodeCount: number;
            readonly nodeBytes: number;
            readonly membershipCount: number;
          };
        } = {};
        const manifestTree = tx.manifestTree(storage, cache) as ReturnType<
          StorageTransactionPorts["manifestTree"]
        > & {
          preloadSubtreeSummaries?: (nodeHashes: readonly Uint8Array[]) => void;
        };
        manifestTree.preloadSubtreeSummaries?.(batch.map((claim) => claim.nodeHash));
        const registered = manifestTree.registerReusedSubtrees(
          leaseId,
          ownerNonce,
          source.manifestHash,
          batch,
          {
            // Path-copy writes can share content-addressed members with a
            // reused summary just like local rebuilds. Keep summary overlap
            // detection active for both durable edit paths so a shared fresh
            // object/node is never counted twice.
            knownObjectHashes: candidate.entries.map((entry) => entry.hash),
            knownNodeHashes: candidate.nodes.map((node) => node.hash),
            sourceManifestProtected: true,
            allowSummaries: reusedBatches.length === 1,
            certificateState,
            deferCertificateWrite: true,
            certificatePatch,
            ...(batch.every(
              (claim) =>
                claim.sourceLeafDelta !== undefined &&
                claim.sourceFinalAtLevel !== undefined,
            )
              ? {
                  authenticatedClaims: batch.map((claim) => ({
                    sourcePath: claim.sourcePath,
                    nodeHash: claim.nodeHash,
                    span: claim.span,
                    entryCount: claim.entryCount,
                    sourceFinalAtLevel: claim.sourceFinalAtLevel,
                    sourceLeafDelta: claim.sourceLeafDelta!,
                  })),
                }
              : {}),
          },
        );
        if (verifiedReusedNodeSizes.size && registered.length)
          staging.cacheReusedSubtreeMetadata(
            leaseId,
            batch.map((claim) => claim.nodeHash),
            registered,
            verifiedReusedNodeSizes,
          );
        else
          staging.cacheReusedSubtreeMetadata(
            leaseId,
            batch.map((claim) => claim.nodeHash),
          );
        if (certificatePatch.value)
          staging.applyCertificatePatch(leaseId, certificatePatch.value);
      },
    });
  }
  steps.push({
    rows: 8,
    bytes: intrinsicByteLength(candidate.root),
    units: 1,
    run: (tx) => {
      const staging = tx.staging(storage, cache);
      staging.consumeIngestReservation(
        leaseId,
        ownerNonce,
        intrinsicByteLength(candidate.root),
      );
      staging.consumeMetadataReservation(
        leaseId,
        ownerNonce,
        DURABLE_METADATA_ROW_BYTES,
      );
      const content = tx.content(storage, cache) as LocalFreshContentStore;
      if (content.putFreshManifestRoot)
        content.putFreshManifestRoot(candidate.rootHash, candidate.root);
      else content.putManifestRoot(candidate.rootHash, candidate.root);
      const rootMember = {
        kind: "manifest-root" as const,
        hash: candidate.rootHash,
        size: intrinsicByteLength(candidate.root),
      };
      const trustedStaging = staging as typeof staging & {
        appendFreshBatch?: (
          leaseId: string,
          ownerNonce: Uint8Array,
          members: readonly {
            readonly kind: "object" | "manifest-root" | "manifest-node";
            readonly hash: Uint8Array;
            readonly size: number;
          }[],
          verifiedBacking?: { readonly rootSizes?: ReadonlyMap<string, number> },
        ) => ClosureCertificate;
      };
      if (trustedStaging.appendFreshBatch)
        trustedStaging.appendFreshBatch(leaseId, ownerNonce, [rootMember], {
          rootSizes: new Map([[bytesToHex(candidate.rootHash), rootMember.size]]),
        });
      else staging.appendBatch(leaseId, ownerNonce, [rootMember]);
      staging.beginReconciliation(leaseId, ownerNonce, candidate.rootHash);
      certificateHolder.value = Object.freeze({
        ...staging.snapshot(leaseId, ownerNonce),
        manifestHash: copyBytes(candidate.rootHash),
      });
    },
  });
  const reconcileUnits = checkedAdd(
    checkedMultiply(candidate.entries.length, 4, "path-copy closure edges"),
    checkedAdd(
      checkedMultiply(candidate.nodes.length, 2, "path-copy closure nodes"),
      checkedAdd(candidate.reused.length, 10, "path-copy closure validation"),
    ),
  );
  steps.push({
    rows: reconcileUnits,
    bytes: 0,
    units: reconcileUnits,
    selfTransacting: true,
    run: (tx, merged) => {
      let complete = false;
      while (!complete) {
        if (merged) {
          complete = tx
            .staging(storage, cache)
            .reconcileBatch(
              leaseId,
              ownerNonce,
              reconciliationWorkLimit(storage),
            ).complete;
        } else {
          complete = transact<{ readonly complete: boolean }>("write", (inner) =>
            inner
              .staging(storage, cache)
              .reconcileBatch(leaseId, ownerNonce, reconciliationWorkLimit(storage)),
          ).complete;
        }
      }
    },
  });
  steps.push({
    rows: 8,
    bytes: 0,
    units: 1,
    run: (tx) => {
      tx.staging(storage, cache).seal(certificateHolder.value!);
    },
  });
  try {
    runPersistenceSteps(
      steps,
      storage,
      port,
      budget,
      transact,
      undefined,
      undefined,
      false,
    );
    begun = true;
    const certificate = certificateHolder.value!;
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
          tx.staging(storage, cache).delete(leaseId, ownerNonce);
        });
      } catch {}
    throw error;
  }
}

interface LoadedManifestState {
  readonly manifest: DiagnosticBuiltManifest;
  /** Child-index path from the root to each retained node, keyed by hex hash. */
  readonly paths: ReadonlyMap<string, readonly number[]>;
  release(): void;
}

/**
 * Loads the bounded Merkle-descent state for the durable local rebuild: the
 * validation certificate, the manifest root, the affected and dirty-end
 * paths via `pathAtOffset`, and a capped right-fringe crawl - all in one read
 * transaction. The loaded neighborhood is bounded by the local-rebuild
 * limits; anything larger throws `BoundedRebuildFallbackError`.
 */
export function boundedManifestStateReadBudget(
  sourceWindowBytes: number,
  storage: StorageLimits,
  limits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
): StorageWorkBudget {
  if (!Number.isSafeInteger(sourceWindowBytes) || sourceWindowBytes < 0)
    throw new RangeError("bounded source window bytes must be nonnegative");
  const windowCap = limits.maxAffectedEntries;
  return Object.freeze({
    maxRows: checkedAdd(24, checkedMultiply(windowCap, 2)),
    maxBytes: Math.max(
      8_192,
      checkedAdd(
        checkedMultiply(windowCap, 36),
        checkedMultiply(windowCap, checkedAdd(storage.maxManifestNodeBytes, 512)),
      ),
      checkedAdd(sourceWindowBytes, 8_192, "bounded source window envelope"),
    ),
    maxStatements: checkedAdd(
      checkedMultiply(windowCap, 4),
      checkedAdd(8, checkedMultiply(storage.maxManifestDepth, 2)),
    ),
    maxElapsedMs: 5_000,
  });
}

function pathAtOffsetWithinLeaf(
  path: AuthenticatedManifestTreePath,
  offset: number,
): AuthenticatedManifestTreePath {
  const leafFrame = path.nodes.at(-1);
  if (!leafFrame || leafFrame.node.kind !== "leaf")
    throw new Error("ECORRUPT: bounded path does not terminate at a leaf");
  const selectedOffset = path.fileSize === 0 ? 0 : Math.min(offset, path.fileSize - 1);
  const leafEnd = checkedAdd(path.leafOffset, leafFrame.node.span);
  if (path.fileSize === 0 && path.leafOffset === 0 && leafEnd === 0)
    return Object.freeze({ ...path, entryIndex: -1, entryOffset: 0 });
  if (selectedOffset < path.leafOffset || selectedOffset >= leafEnd)
    throw new Error("ECORRUPT: bounded endpoint escaped its authenticated leaf");
  let entryOffset = path.leafOffset;
  let entryIndex = 0;
  if (path.fileSize !== 0) {
    let relative = selectedOffset - path.leafOffset;
    entryIndex = -1;
    for (let index = 0; index < leafFrame.node.entries.length; index += 1) {
      const entry = leafFrame.node.entries[index]!;
      if (relative < entry.length) {
        entryIndex = index;
        break;
      }
      relative -= entry.length;
      entryOffset = checkedAdd(entryOffset, entry.length);
    }
    if (entryIndex < 0)
      throw new Error("ECORRUPT: bounded leaf lacks its endpoint entry");
  }
  return Object.freeze({ ...path, entryIndex, entryOffset });
}

/**
 * Read budget for the filesystem adapter's coalesced mutation snapshot. The
 * bounded loader remains the authority for its own rows and bytes; the small
 * envelope below accounts for namespace resolution and the source-root check
 * performed in the same read transaction.
 */
export function durableEditReadSnapshotBudget(
  sourceWindowBytes: number,
  storage: StorageLimits,
  limits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
): StorageWorkBudget {
  const bounded = boundedManifestStateReadBudget(sourceWindowBytes, storage, limits);
  return Object.freeze({
    maxRows: checkedAdd(bounded.maxRows, 16, "durable edit snapshot rows"),
    maxBytes: checkedAdd(bounded.maxBytes, 64 * 1024, "durable edit snapshot bytes"),
    maxStatements: checkedAdd(
      bounded.maxStatements ?? bounded.maxRows,
      16,
      "durable edit snapshot statements",
    ),
    maxElapsedMs: bounded.maxElapsedMs ?? 5_000,
  });
}

/**
 * Loads bounded state against a transaction supplied by the caller. The
 * caller may use this to combine mutation-source selection and bounded
 * manifest loading in one read snapshot; no write work is performed here.
 */
export function loadBoundedManifestStateInTransaction(
  tx: StorageTransactionPorts,
  source: DurableEditSource,
  manifestHash: Uint8Array,
  edit: { readonly offset: number; readonly deleteLength: number },
  storage: StorageLimits,
  limits: LocalRebuildLimits,
  cache: ContentCache | undefined,
  allowTruncatedFringe: boolean,
  suppliedRootBytes?: Uint8Array,
): BoundedManifestState {
  if (intrinsicByteLength(manifestHash) !== 32)
    throw new RangeError("manifest hash must contain exactly 32 bytes");
  let releaseState: (() => void) | undefined;
  try {
    const content = tx.content(storage, cache);
    const tree = tx.manifestTree(storage, cache);
    const ownedHash = copyBytes(manifestHash);
    // The validation certificate row is read by `pathAtOffset` inside the
    // same transaction; the budget accounts for that extra row.
    const rootBytes = suppliedRootBytes
      ? copyBytes(suppliedRootBytes)
      : content.withManifestRoot(ownedHash, (encoded) => copyBytes(encoded));
    if (!rootBytes) throw new Error("ECORRUPT: missing manifest root");
    const decoded = decodeManifestRoot(rootBytes, ownedHash);
    validateSupportedManifestParameters(decoded.parameters);
    const dirtyOldEnd = checkedAdd(
      edit.offset,
      edit.deleteLength,
      "bounded dirty old end",
    );
    const affected = tree.pathAtOffset(ownedHash, edit.offset);
    const affectedLeaf = affected.nodes.at(-1);
    const affectedLeafEnd = affectedLeaf
      ? checkedAdd(affected.leafOffset, affectedLeaf.node.span)
      : -1;
    const sameLeaf =
      affectedLeaf?.node.kind === "leaf" &&
      (dirtyOldEnd < affectedLeafEnd ||
        (dirtyOldEnd === decoded.fileSize && affectedLeafEnd === decoded.fileSize));
    const dirty = sameLeaf
      ? pathAtOffsetWithinLeaf(affected, dirtyOldEnd)
      : tree.pathAtOffset(ownedHash, dirtyOldEnd);
    const frameOf = (node: (typeof affected.nodes)[number]): BoundedPathFrame =>
      Object.freeze({
        hash: copyBytes(node.hash),
        path: Object.freeze([...node.path]),
        offset: node.offset,
        finalAtLevel: node.finalAtLevel,
        node: node.node,
        ...(node.selectedChildIndex === undefined
          ? {}
          : { selectedChildIndex: node.selectedChildIndex }),
      });
    const state = assembleBoundedManifestState(
      Object.freeze({
        rootHash: ownedHash,
        root: rootBytes,
        parameters: Object.freeze({ ...decoded.parameters }),
        fileSize: decoded.fileSize,
        entryCount: decoded.entryCount,
      }),
      affected.nodes.map(frameOf),
      affected.entryIndex,
      dirty.nodes.map(frameOf),
      dirtyOldEnd,
      (hash) =>
        content.withManifestNode(hash, (encoded) => decodeManifestNode(encoded, hash)),
      (hash, node, finalAtLevel, rootNode) => {
        void hash;
        validateCanonicalManifestNode(node, decoded.parameters, finalAtLevel, rootNode);
      },
      limits,
      allowTruncatedFringe,
    );
    const sourceWindowBytes = Math.min(
      source.size,
      Math.max(0, source.maxReadWindowBytes ?? 0),
    );
    if (source.readInTransaction && sourceWindowBytes > 0) {
      const windowOffset = Math.min(
        Math.max(0, edit.offset - Math.floor((sourceWindowBytes - 1) / 2)),
        Math.max(0, source.size - sourceWindowBytes),
      );
      source.readInTransaction(
        content,
        windowOffset,
        Math.min(sourceWindowBytes, source.size - windowOffset),
      );
    }
    const loadedEntries =
      state.affectedLeaf.entries.length +
      (state.dirtyEndLeaf === state.affectedLeaf
        ? 0
        : state.dirtyEndLeaf.entries.length) +
      state.fringeLeaves.reduce((sum, leaf) => sum + leaf.entries.length, 0);
    const loadedChildren = state.levelWindows.reduce(
      (sum, window) =>
        sum +
        (window ? window.affectedChildren.length : 0) +
        window!.fringe.reduce((inner, group) => inner + group.children.length, 0),
      0,
    );
    const stateBytes = checkedAdd(
      checkedMultiply(loadedEntries, 64, "bounded loaded entry state"),
      checkedMultiply(
        loadedChildren,
        checkedAdd(
          checkedMultiply(storage.maxManifestNodeBytes, 4),
          4096,
          "bounded loaded node envelope",
        ),
        "bounded loaded node state",
      ),
    );
    releaseState = cache!.reserveOperation(stateBytes);
    return Object.freeze({ ...state, release: () => releaseState?.() });
  } catch (error) {
    releaseState?.();
    throw error;
  }
}

function loadBoundedManifestState(
  port: OperationsStorage,
  source: DurableEditSource,
  manifestHash: Uint8Array,
  edit: { readonly offset: number; readonly deleteLength: number },
  storage: StorageLimits,
  limits: LocalRebuildLimits,
  cache: ContentCache | undefined,
  allowTruncatedFringe: boolean,
): BoundedManifestState {
  const sourceWindowBytes = Math.min(
    source.size,
    Math.max(0, source.maxReadWindowBytes ?? 0),
  );
  return port.transaction(
    "read",
    boundedManifestStateReadBudget(sourceWindowBytes, storage, limits),
    (tx) =>
      loadBoundedManifestStateInTransaction(
        tx,
        source,
        manifestHash,
        edit,
        storage,
        limits,
        cache,
        allowTruncatedFringe,
      ),
  );
}

/** Returns undefined only for the normal bounded-loader optimization miss. */
export function tryLoadBoundedManifestStateInTransaction(
  tx: StorageTransactionPorts,
  source: DurableEditSource,
  manifestHash: Uint8Array,
  edit: { readonly offset: number; readonly deleteLength: number },
  storage: StorageLimits,
  limits: LocalRebuildLimits,
  cache: ContentCache | undefined,
  allowTruncatedFringe: boolean,
  suppliedRootBytes?: Uint8Array,
): BoundedManifestState | undefined {
  try {
    return loadBoundedManifestStateInTransaction(
      tx,
      source,
      manifestHash,
      edit,
      storage,
      limits,
      cache,
      allowTruncatedFringe,
      suppliedRootBytes,
    );
  } catch (error) {
    if (error instanceof BoundedRebuildFallbackError || error instanceof RangeError)
      return undefined;
    throw error;
  }
}

function estimatedManifestNodeCount(entryCount: number): number {
  let leaves = Math.max(1, Math.ceil(entryCount / 64));
  let estimate = 1 + leaves;
  let levelNodes = leaves;
  for (let depth = 1; levelNodes > 1 && depth < 32; depth += 1) {
    levelNodes = Math.max(1, Math.ceil(levelNodes / 32));
    estimate += levelNodes;
  }
  return estimate;
}

/**
 * Loads the complete authenticated manifest state for the durable local
 * rebuild: every reachable node (with its source child-index path) and the
 * ordered entry stream, in one read transaction. The retained state is
 * bounded by the local-rebuild limits; anything larger falls back.
 */
function loadAuthenticatedManifestState(
  port: OperationsStorage,
  manifestHash: Uint8Array,
  storage: StorageLimits,
  limits: LocalRebuildLimits,
  cache: ContentCache | undefined,
  hashBytes: HashFunction,
): LoadedManifestState {
  if (intrinsicByteLength(manifestHash) !== 32)
    throw new RangeError("manifest hash must contain exactly 32 bytes");
  const retainedEntriesCap = limits.maxRetainedEntries;
  const retainedNodesCap = limits.maxRetainedNodes;
  const budget: StorageWorkBudget = Object.freeze({
    maxRows: checkedAdd(24, checkedMultiply(retainedNodesCap, 2)),
    maxBytes: Math.max(
      8_192,
      checkedAdd(
        retainedEntriesCap * 36,
        checkedMultiply(
          retainedNodesCap,
          checkedAdd(storage.maxManifestNodeBytes, 512),
        ),
      ),
    ),
    maxStatements: checkedAdd(
      retainedNodesCap * 4,
      checkedAdd(8, checkedMultiply(storage.maxManifestDepth, 2)),
    ),
    maxElapsedMs: 5_000,
  });
  let releaseState: (() => void) | undefined;
  try {
    const loaded = port.transaction<{
      readonly root: Uint8Array;
      readonly rootHash: Uint8Array;
      readonly rootNodeHash: Uint8Array;
      readonly fileSize: number;
      readonly entryCount: number;
      readonly parameters: ManifestParameters;
      readonly nodes: Map<string, EncodedManifestNode>;
      readonly paths: Map<string, readonly number[]>;
      readonly entries: ManifestEntry[];
      readonly nodesRead: number;
    }>("read", budget, (tx) => {
      const content = tx.content(storage, cache);
      const ownedHash = copyBytes(manifestHash);
      const root = content.withManifestRoot(ownedHash, (encoded) => copyBytes(encoded));
      if (!root) throw new Error("ECORRUPT: missing manifest root");
      const decoded = decodeManifestRoot(root, ownedHash);
      validateSupportedManifestParameters(decoded.parameters);
      if (decoded.entryCount > retainedEntriesCap)
        throw new LocalRebuildLimitError(
          "durable local rebuild exceeds its retained-entry limit; use the streamed workspace fallback",
        );
      const retainedEntries = Math.min(decoded.entryCount, retainedEntriesCap);
      const retainedNodes = Math.min(
        estimatedManifestNodeCount(retainedEntries),
        retainedNodesCap,
      );
      const stateBytes = checkedAdd(
        checkedMultiply(retainedEntries, 64, "loaded entry state"),
        checkedMultiply(
          retainedNodes,
          checkedAdd(
            checkedMultiply(storage.maxManifestNodeBytes, 4, "loaded node state"),
            4096,
            "loaded node envelope",
          ),
          "loaded manifest state",
        ),
      );
      releaseState = cache!.reserveOperation(stateBytes);
      const nodes = new Map<string, EncodedManifestNode>();
      const paths = new Map<string, readonly number[]>();
      const entries: ManifestEntry[] = [];
      let nodeVisits = 0;
      let leafDepth: number | undefined;
      const visit = (
        hash: Uint8Array,
        path: readonly number[],
        depth: number,
        finalAtLevel: boolean,
        rootNode: boolean,
        expected: ManifestChild | undefined,
      ): void => {
        nodeVisits += 1;
        if (nodeVisits > retainedNodesCap)
          throw new LocalRebuildLimitError(
            "durable local rebuild exceeds its retained-node limit; use the streamed workspace fallback",
          );
        const authoritativeHash = copyBytes(hash);
        const encoded = content.withManifestNode(authoritativeHash, (bytes) =>
          copyBytes(bytes),
        );
        if (!encoded) throw new Error("ECORRUPT: missing manifest node");
        const node = decodeManifestNode(encoded, authoritativeHash);
        if (
          expected &&
          (node.span !== expected.span || node.entryCount !== expected.entryCount)
        )
          throw new Error("ECORRUPT: manifest child totals mismatch");
        validateCanonicalManifestNode(node, decoded.parameters, finalAtLevel, rootNode);
        nodes.set(
          bytesToHex(authoritativeHash),
          Object.freeze({
            hash: authoritativeHash,
            encoded,
            node,
          }),
        );
        paths.set(bytesToHex(authoritativeHash), Object.freeze([...path]));
        if (node.kind === "leaf") {
          if (leafDepth === undefined) leafDepth = depth;
          else if (leafDepth !== depth)
            throw new Error("ECORRUPT: unbalanced manifest tree");
          for (const entry of node.entries) {
            if (entries.length >= limits.maxRetainedEntries)
              throw new LocalRebuildLimitError(
                "durable local rebuild exceeds its retained-entry limit; use the streamed workspace fallback",
              );
            entries.push(
              Object.freeze({ hash: copyBytes(entry.hash), length: entry.length }),
            );
          }
          return;
        }
        for (let index = 0; index < node.children.length; index += 1) {
          const child = node.children[index]!;
          visit(
            child.hash,
            [...path, index],
            depth + 1,
            finalAtLevel && index === node.children.length - 1,
            false,
            child,
          );
        }
      };
      visit(decoded.rootNodeHash, [], 1, true, true, undefined);
      const rootNode = nodes.get(bytesToHex(decoded.rootNodeHash));
      if (!rootNode || rootNode.node.span !== decoded.fileSize)
        throw new Error("ECORRUPT: manifest root totals mismatch");
      if (rootNode.node.entryCount !== decoded.entryCount)
        throw new Error("ECORRUPT: manifest root totals mismatch");
      if (entries.length !== decoded.entryCount)
        throw new Error("ECORRUPT: manifest entry stream count mismatch");
      if ((decoded.fileSize === 0) !== (decoded.entryCount === 0))
        throw new Error("ECORRUPT: manifest empty root totals mismatch");
      return Object.freeze({
        root,
        rootHash: ownedHash,
        rootNodeHash: copyBytes(decoded.rootNodeHash),
        fileSize: decoded.fileSize,
        entryCount: decoded.entryCount,
        parameters: decoded.parameters,
        nodes,
        paths,
        entries,
        nodesRead: nodeVisits,
      });
    });
    const manifest: DiagnosticBuiltManifest = Object.freeze({
      id: manifestIdFromHash(loaded.rootHash),
      rootHash: loaded.rootHash,
      root: loaded.root,
      nodes: loaded.nodes,
      entries: Object.freeze(loaded.entries),
    });
    return Object.freeze({
      manifest,
      paths: loaded.paths,
      release: releaseState!,
    });
  } catch (error) {
    releaseState?.();
    throw error;
  }
}

interface RebuiltSpine {
  readonly newNodes: readonly EncodedManifestNode[];
  /** Balanced leaf depth proved while walking the rebuilt spine. */
  readonly leafDepth: number;
  readonly reused: readonly {
    readonly sourcePath: readonly number[];
    readonly sourceFinalAtLevel?: boolean;
    readonly sourceLeafDelta?: number;
    readonly nodeHash: Uint8Array;
    readonly span: number;
    readonly entryCount: number;
  }[];
  /**
   * New chunks referenced by the rebuilt leaves: full staged members appended
   * with their put batches.
   */
  readonly fullObjects: readonly {
    readonly hash: Uint8Array;
    readonly length: number;
  }[];
  /**
   * Already-durable boundary records referenced by the rebuilt leaves:
   * count-only staged members (chain + counts, no membership row).
   */
  readonly countedObjects: readonly {
    readonly hash: Uint8Array;
    readonly length: number;
  }[];
  readonly validationRows: number;
}
function walkRebuiltSpine(
  rebuilt: LocallyRebuiltManifest,
  oldManifest: DiagnosticBuiltManifest,
  paths: ReadonlyMap<string, readonly number[]>,
  limits: LocalRebuildLimits,
): RebuiltSpine {
  const decodedRoot = decodeManifestRoot(rebuilt.root, rebuilt.rootHash);
  validateSupportedManifestParameters(decodedRoot.parameters);
  if (decodedRoot.entryCount > limits.maxRetainedEntries)
    throw new LocalRebuildLimitError(
      "durable local rebuild result exceeds its retained-entry limit",
    );
  if (decodedRoot.rootNodeHash === undefined)
    throw new Error("ECORRUPT: rebuilt manifest root lost its node");
  const newNodes: EncodedManifestNode[] = [];
  const reused: RebuiltSpine["reused"][number][] = [];
  const objects = new Map<
    string,
    { readonly hash: Uint8Array; readonly length: number }
  >();
  let visited = 0;
  let validationRows = 1;
  let leafDepth: number | undefined;
  const visit = (
    hash: Uint8Array,
    depth: number,
    finalAtLevel: boolean,
    rootNode: boolean,
    expected: ManifestChild | undefined,
  ): void => {
    visited += 1;
    if (visited > limits.maxRetainedNodes)
      throw new LocalRebuildLimitError(
        "durable local rebuild result exceeds its retained-node limit",
      );
    const hex = bytesToHex(hash);
    const fresh = rebuilt.newNodes.get(hex);
    const old = fresh ? undefined : oldManifest.nodes.get(hex);
    const encoded = fresh ?? old;
    if (!encoded) throw new Error("ECORRUPT: rebuilt manifest lost a spine node");
    if (
      expected &&
      (encoded.node.span !== expected.span ||
        encoded.node.entryCount !== expected.entryCount)
    )
      throw new Error("ECORRUPT: rebuilt manifest child totals mismatch");
    if (fresh) {
      newNodes.push(fresh);
    } else {
      const sourcePath = paths.get(hex);
      if (!sourcePath)
        throw new Error("ECORRUPT: reused spine node lost its source path");
      reused.push(
        Object.freeze({
          sourcePath,
          nodeHash: copyBytes(hash),
          span: encoded.node.span,
          entryCount: encoded.node.entryCount,
        }),
      );
    }
    if (encoded.node.kind === "leaf") {
      if (leafDepth === undefined) leafDepth = depth;
      else if (leafDepth !== depth)
        throw new Error("ECORRUPT: rebuilt manifest is unbalanced");
      if (fresh) {
        for (const entry of encoded.node.entries) {
          const key = bytesToHex(entry.hash);
          if (!objects.has(key))
            objects.set(
              key,
              Object.freeze({ hash: copyBytes(entry.hash), length: entry.length }),
            );
        }
      }
      return;
    }
    validationRows = checkedAdd(
      validationRows,
      encoded.node.children.length,
      "rebuilt validation rows",
    );
    for (let index = 0; index < encoded.node.children.length; index += 1) {
      const child = encoded.node.children[index]!;
      visit(
        child.hash,
        depth + 1,
        finalAtLevel && index === encoded.node.children.length - 1,
        false,
        child,
      );
    }
  };
  if (bytesToHex(rebuilt.rootHash) === bytesToHex(oldManifest.rootHash)) {
    const rootNode = oldManifest.nodes.get(bytesToHex(decodedRoot.rootNodeHash));
    if (!rootNode) throw new Error("ECORRUPT: unchanged root lost its spine node");
    return Object.freeze({
      newNodes: Object.freeze([]),
      reused: Object.freeze([
        Object.freeze({
          sourcePath: Object.freeze([] as number[]),
          nodeHash: copyBytes(decodedRoot.rootNodeHash),
          span: rootNode.node.span,
          entryCount: rootNode.node.entryCount,
        }),
      ]),
      fullObjects: Object.freeze([]),
      countedObjects: Object.freeze([]),
      validationRows: 1,
      leafDepth: (() => {
        let depth = 1;
        let node = rootNode.node;
        while (node.kind === "internal") {
          depth += 1;
          node = oldManifest.nodes.get(bytesToHex(node.children[0]!.hash))!.node;
        }
        return depth;
      })(),
    });
  }
  visit(decodedRoot.rootNodeHash, 1, true, true, undefined);
  if (leafDepth === undefined)
    throw new Error("ECORRUPT: rebuilt manifest has no leaf level");
  const spliceHashes = new Set(
    rebuilt.entrySplice.entries.map((entry) => bytesToHex(entry.hash)),
  );
  const fullObjects: RebuiltSpine["fullObjects"][number][] = [];
  const countedObjects: RebuiltSpine["countedObjects"][number][] = [];
  for (const object of objects.values()) {
    if (spliceHashes.has(bytesToHex(object.hash))) fullObjects.push(object);
    else countedObjects.push(object);
  }
  return Object.freeze({
    newNodes,
    reused,
    fullObjects: Object.freeze(fullObjects),
    countedObjects: Object.freeze(countedObjects),
    validationRows,
    leafDepth,
  });
}

/**
 * Walks the bounded rebuilt spine: only the fresh (segment and ancestor)
 * nodes are visited; every old node directly referenced by a fresh node
 * becomes a reused-subtree claim whose source path comes from the loaded
 * path frames and the fringe. A rebuilt segment node that hash-matches an
 * old node whose source path is not on the loaded path cannot be claimed and
 * falls back (repeated-content dedup is only representable when the old node
 * sits on the loaded neighborhood).
 */
function walkRebuiltSpineBounded(
  rebuilt: LocallyRebuiltManifest,
  state: BoundedManifestState,
  limits: LocalRebuildLimits,
): RebuiltSpine {
  const decodedRoot = decodeManifestRoot(rebuilt.root, rebuilt.rootHash);
  validateSupportedManifestParameters(decodedRoot.parameters);
  if (decodedRoot.entryCount > limits.maxRetainedEntries)
    throw new LocalRebuildLimitError(
      "durable local rebuild result exceeds its retained-entry limit",
    );
  if (decodedRoot.rootNodeHash === undefined)
    throw new Error("ECORRUPT: rebuilt manifest root lost its node");
  if (bytesToHex(rebuilt.rootHash) === bytesToHex(state.root.rootHash)) {
    return Object.freeze({
      newNodes: Object.freeze([]),
      reused: Object.freeze([
        Object.freeze({
          sourcePath: Object.freeze([] as number[]),
          sourceFinalAtLevel: true,
          sourceLeafDelta: state.rootDepth - 1,
          nodeHash: copyBytes(decodedRoot.rootNodeHash),
          span: state.root.fileSize,
          entryCount: state.root.entryCount,
        }),
      ]),
      fullObjects: Object.freeze([]),
      countedObjects: Object.freeze([]),
      validationRows: 1,
      leafDepth: state.rootDepth,
    });
  }
  const newNodes: EncodedManifestNode[] = [];
  const reused: RebuiltSpine["reused"][number][] = [];
  const objects = new Map<
    string,
    { readonly hash: Uint8Array; readonly length: number }
  >();
  let visited = 0;
  let validationRows = 1;
  let leafDepth: number | undefined;
  const visit = (
    hash: Uint8Array,
    depth: number,
    finalAtLevel: boolean,
    rootNode: boolean,
    expected: ManifestChild | undefined,
  ): void => {
    visited += 1;
    if (visited > limits.maxRetainedNodes)
      throw new LocalRebuildLimitError(
        "durable local rebuild result exceeds its retained-node limit",
      );
    const hex = bytesToHex(hash);
    const fresh = rebuilt.newNodes.get(hex);
    if (!fresh) {
      const proof = state.claimProofs.get(hex);
      if (!proof)
        throw new BoundedRebuildFallbackError(
          "bounded rebuilt segment node hash-matches an old node outside the loaded path",
        );
      if (!expected) throw new Error("ECORRUPT: rebuilt root claim lacks child totals");
      reused.push(
        Object.freeze({
          sourcePath: proof.sourcePath,
          sourceFinalAtLevel: proof.sourceFinalAtLevel,
          sourceLeafDelta: proof.sourceLeafDelta,
          nodeHash: copyBytes(hash),
          span: expected.span,
          entryCount: expected.entryCount,
        }),
      );
      return;
    }
    const encoded = fresh;
    if (
      expected &&
      (encoded.node.span !== expected.span ||
        encoded.node.entryCount !== expected.entryCount)
    )
      throw new Error("ECORRUPT: rebuilt manifest child totals mismatch");
    newNodes.push(fresh);
    if (encoded.node.kind === "leaf") {
      if (leafDepth === undefined) leafDepth = depth;
      else if (leafDepth !== depth)
        throw new Error("ECORRUPT: rebuilt manifest is unbalanced");
      for (const entry of encoded.node.entries) {
        const key = bytesToHex(entry.hash);
        if (!objects.has(key))
          objects.set(
            key,
            Object.freeze({ hash: copyBytes(entry.hash), length: entry.length }),
          );
      }
      return;
    }
    validationRows = checkedAdd(
      validationRows,
      encoded.node.children.length,
      "bounded rebuilt validation rows",
    );
    for (let index = 0; index < encoded.node.children.length; index += 1) {
      const child = encoded.node.children[index]!;
      visit(
        child.hash,
        depth + 1,
        finalAtLevel && index === encoded.node.children.length - 1,
        false,
        child,
      );
    }
  };
  visit(decodedRoot.rootNodeHash, 1, true, true, undefined);
  if (leafDepth === undefined)
    throw new Error("ECORRUPT: rebuilt manifest has no leaf level");
  const spliceHashes = new Set(
    rebuilt.entrySplice.entries.map((entry) => bytesToHex(entry.hash)),
  );
  const fullObjects: RebuiltSpine["fullObjects"][number][] = [];
  const countedObjects: RebuiltSpine["countedObjects"][number][] = [];
  for (const object of objects.values()) {
    if (spliceHashes.has(bytesToHex(object.hash))) fullObjects.push(object);
    else countedObjects.push(object);
  }
  return Object.freeze({
    newNodes,
    reused,
    fullObjects: Object.freeze(fullObjects),
    countedObjects: Object.freeze(countedObjects),
    validationRows,
    leafDepth,
  });
}

function materializeEditInsertion(
  edit: DurableContentEdit,
  limits: LocalRebuildLimits,
): Uint8Array | undefined {
  if (edit.insertLength === 0) return new Uint8Array(0);
  if (edit.insertLength > limits.maxAffectedBytes) return undefined;
  const bytes = edit.readInsert(0, edit.insertLength);
  if (intrinsicByteLength(bytes) !== edit.insertLength)
    throw new Error("ECORRUPT: edit insertion returned a partial range");
  return copyBytes(bytes);
}

interface PersistenceStep {
  /** Projected result rows this step adds inside one transaction. */
  readonly rows: number;
  /** Projected binding bytes this step adds inside one transaction. */
  readonly bytes: number;
  /** Projected reconciliation work units this step adds inside one transaction. */
  readonly units: number;
  /**
   * Runs the step against the given transaction. When `merged` is false the
   * step executes in its own transaction (the bounded-work fallback), so
   * steps that must repeat work - the reconciliation loop - use one
   * transaction per call instead of accumulating rows in a single one.
   */
  run(tx: StorageTransactionPorts, merged: boolean): unknown;
  /**
   * When true the step manages its own transactions in the fallback branch
   * (the reconciliation loop must commit per bounded call); the merged branch
   * still runs it inside the single transaction.
   */
  readonly selfTransacting?: boolean;
}

interface PersistenceRun {
  readonly merged: boolean;
  readonly rows: number;
  readonly bytes: number;
  readonly units: number;
}

type LocalFreshContentStore = ContentStore & {
  reserveAllocationSequence?: (count: number) => void;
  putFreshManifestRoot?: (hash: Uint8Array, encoded: Uint8Array) => boolean;
  putFreshObjectsBatch?: (
    input: readonly { readonly hash: Uint8Array; readonly bytes: Uint8Array }[],
  ) => { readonly verifiedSizes: ReadonlyMap<string, number> };
  putFreshManifestNodesBatch?: (
    nodes: readonly { readonly hash: Uint8Array; readonly encoded: Uint8Array }[],
  ) => { readonly verifiedSizes: ReadonlyMap<string, number> };
};

/**
 * Executes the durable persistence steps. When the projected row, binding-byte,
 * and reconciliation-unit budgets all fit within one transaction, every step
 * runs in a single write transaction so the WAL/fsync floor collapses to one
 * commit for small edits; otherwise each step keeps its own transaction (the
 * established bounded-work shape). The single-transaction branch never applies
 * to large closures or tight-budget profiles, whose per-step behavior is
 * unchanged.
 */
function runPersistenceSteps(
  steps: readonly PersistenceStep[],
  storage: StorageLimits,
  port: OperationsStorage,
  budget: StorageWorkBudget,
  transact: <T>(
    mode: StorageTransactionMode,
    callback: (tx: StorageTransactionPorts) => T,
  ) => T,
  afterMerged?: (tx: StorageTransactionPorts) => void,
  beforeMerged?: (tx: StorageTransactionPorts) => void,
  allowMerged = true,
): PersistenceRun {
  let rows = 0;
  let bytes = 0;
  let units = 0;
  for (const step of steps) {
    rows = checkedAdd(rows, step.rows, "persistence step rows");
    bytes = checkedAdd(bytes, step.bytes, "persistence step bytes");
    units = checkedAdd(units, step.units, "persistence step units");
  }
  if (
    allowMerged &&
    rows <= storage.maxFinalTransactionRows - 16 &&
    bytes <= storage.maxFinalTransactionBytes &&
    units <= reconciliationWorkLimit(storage)
  ) {
    transact<void>("write", (tx) => {
      beforeMerged?.(tx);
      for (const step of steps) step.run(tx, true);
      afterMerged?.(tx);
    });
    return Object.freeze({ merged: true, rows, bytes, units });
  }
  for (const step of steps) {
    if (step.selfTransacting) {
      step.run(undefined as unknown as StorageTransactionPorts, false);
    } else {
      transact<void>("write", (tx) => step.run(tx, false));
    }
  }
  return Object.freeze({ merged: false, rows, bytes, units });
}

function persistLocallyRebuilt(
  port: OperationsStorage,
  source: DurableEditSource,
  rebuilt: LocallyRebuiltManifest,
  spine: RebuiltSpine,
  authenticatedExistingRoot: Uint8Array | undefined,
  storage: StorageLimits,
  cache: ContentCache | undefined,
  clock: () => number,
  transactionLimit: number,
  phaseMs: LocalRebuildPhaseTimings,
  finalizePrepared?: DurableEditFinalizer,
): StreamPreparedManifest & {
  readonly storageTransactions: number;
  readonly persistenceMerged: boolean;
  readonly persistenceRows: number;
  readonly persistenceBytes: number;
  readonly persistenceUnits: number;
  readonly newObjectCount: number;
  readonly newManifestNodeCount: number;
  readonly reusedSubtrees: number;
  readonly validationRows: number;
} {
  const leaseId = globalThis.crypto.randomUUID();
  const ownerId = globalThis.crypto.randomUUID();
  const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
  const now = clock();
  if (!Number.isSafeInteger(now) || now < 0)
    throw new Error("clock must return a nonnegative safe integer");
  const budget: StorageWorkBudget = Object.freeze({
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
    maxStatements: storage.maxFinalTransactionRows * 4,
    maxElapsedMs: 5_000,
  });
  const uniqueObjects = [
    ...new Map(
      spine.fullObjects.map((object) => [bytesToHex(object.hash), object]),
    ).values(),
  ];
  const putObjects: Array<{ readonly hash: Uint8Array; readonly bytes: Uint8Array }> =
    [];
  for (const entry of uniqueObjects) {
    const bytes = rebuilt.affectedObjects.get(bytesToHex(entry.hash));
    if (!bytes)
      throw new Error("ECORRUPT: rebuilt splice entry lost its affected object");
    putObjects.push(Object.freeze({ hash: entry.hash, bytes }));
  }
  const spineObjectBytes = spine.fullObjects.reduce(
    (sum, object) => checkedAdd(sum, object.length),
    0,
  );
  const newNodeBytes = spine.newNodes.reduce(
    (sum, node) => checkedAdd(sum, intrinsicByteLength(node.encoded)),
    0,
  );
  const newPayloadBytes = checkedAdd(
    checkedAdd(spineObjectBytes, newNodeBytes),
    intrinsicByteLength(rebuilt.root),
  );
  // Membership appends consume the ingest envelope once per newly inserted
  // member on top of the payload puts, so the envelope doubles the total
  // membership payload and adds a per-node envelope for reused subtrees.
  // Count-only members are already durable: they consume no ingest at all.
  const durablePayloadReservation = checkedAdd(
    checkedMultiply(newPayloadBytes, 2, "local-rebuild physical and staging payload"),
    checkedMultiply(
      spine.reused.length,
      storage.maxManifestNodeBytes,
      "local-rebuild reused staging envelope",
    ),
  );
  const metadataRows = checkedAdd(
    checkedAdd(
      checkedMultiply(spine.newNodes.length, 4, "local-rebuild node metadata"),
      checkedMultiply(spine.reused.length, 3, "local-rebuild reused metadata"),
    ),
    checkedAdd(
      checkedAdd(spine.validationRows, 16, "local-rebuild fixed metadata"),
      checkedMultiply(spine.fullObjects.length, 3, "local-rebuild object metadata"),
      "local-rebuild validation metadata",
    ),
  );
  const durableMetadataReservation = checkedMultiply(
    metadataRows,
    DURABLE_METADATA_ROW_BYTES,
    "local-rebuild metadata reservation",
  );
  let storageTransactions = 0;
  let begun = false;
  const transact = <T>(
    mode: StorageTransactionMode,
    callback: (tx: StorageTransactionPorts) => T,
  ): T => {
    if (storageTransactions >= transactionLimit)
      throw new DurablePathCopyFallbackError(
        "durable local rebuild exceeds its aggregate storage transaction cap",
      );
    storageTransactions += 1;
    return port.transaction(mode, budget, callback);
  };
  const certificateHolder: { value?: ClosureCertificate } = {};
  let stagingTransaction: StorageTransactionPorts | undefined;
  let stagingRepository:
    | (ReturnType<StorageTransactionPorts["staging"]> & {
        enableBatchedIngestAccounting?: () => void;
        flushBatchedIngestAccounting?: () => void;
        flushBatchedUsageAccounting?: () => void;
        sealAndValidate?: (
          certificate: ClosureCertificate,
          now: number,
        ) => DurableEditValidatedLease;
        appendFreshBatch?: (
          leaseId: string,
          ownerNonce: Uint8Array,
          members: readonly {
            readonly kind: "object" | "manifest-root" | "manifest-node";
            readonly hash: Uint8Array;
            readonly size: number;
          }[],
          verifiedBacking?: {
            readonly objectSizes?: ReadonlyMap<string, number>;
            readonly nodeSizes?: ReadonlyMap<string, number>;
            readonly rootSizes?: ReadonlyMap<string, number>;
          },
        ) => ClosureCertificate;
        appendTrustedReusedManifestBatch?: (
          leaseId: string,
          ownerNonce: Uint8Array,
          nodeHashes: readonly Uint8Array[],
        ) => ClosureCertificate & {
          readonly verifiedNodeSizes: ReadonlyMap<string, number>;
        };
        appendReusedManifestBatch?: (
          leaseId: string,
          ownerNonce: Uint8Array,
          nodeHashes: readonly Uint8Array[],
        ) => ClosureCertificate & {
          readonly verifiedNodeSizes: ReadonlyMap<string, number>;
        };
      })
    | undefined;
  const stagingFor = (tx: StorageTransactionPorts) => {
    if (stagingTransaction !== tx) {
      stagingTransaction = tx;
      stagingRepository = tx.staging(storage, cache);
    }
    return stagingRepository!;
  };
  const sealedLeaseHolder: { value?: DurableEditValidatedLease } = {};
  const steps: PersistenceStep[] = [];
  steps.push({
    rows: 8,
    bytes: 4096,
    units: 1,
    run: (tx) => {
      const staging = stagingFor(tx);
      (
        tx.content(storage, cache) as LocalFreshContentStore
      ).reserveAllocationSequence?.(
        checkedAdd(
          checkedAdd(putObjects.length, spine.newNodes.length),
          1,
          "local-rebuild allocation range",
        ),
      );
      staging.begin({
        leaseId,
        ownerId,
        ownerNonce,
        now,
        expiresAt: checkedAdd(now, storage.stagingLeaseMs),
        ingestReservationBytes: durablePayloadReservation,
        metadataReservationBytes: durableMetadataReservation,
      });
      staging.consumeMetadataReservation(
        leaseId,
        ownerNonce,
        DURABLE_METADATA_ROW_BYTES,
      );
      const manifestTree = tx.manifestTree(storage, cache) as ReturnType<
        StorageTransactionPorts["manifestTree"]
      > & {
        protectTrustedSourceManifest?: (
          leaseId: string,
          ownerNonce: Uint8Array,
          manifestHash: Uint8Array,
          rootBytes: Uint8Array,
        ) => void;
      };
      if (manifestTree.protectTrustedSourceManifest && source.rootBytes)
        manifestTree.protectTrustedSourceManifest(
          leaseId,
          ownerNonce,
          source.manifestHash,
          source.rootBytes,
        );
      else manifestTree.protectSourceManifest(leaseId, ownerNonce, source.manifestHash);
      const snapshotStaging = staging as typeof staging & {
        bumpRootFromSnapshot?: (
          kind: number,
          id: string,
          expectedGeneration: number,
        ) => void;
      };
      if (
        source.rootMutationGeneration !== undefined &&
        snapshotStaging.bumpRootFromSnapshot
      )
        snapshotStaging.bumpRootFromSnapshot(5, leaseId, source.rootMutationGeneration);
      else staging.bumpRoot(5, leaseId);
    },
  });
  const objectBatches = batchesByBytes(
    putObjects,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    (object) => intrinsicByteLength(object.bytes),
  );
  for (const batch of objectBatches) {
    const batchBytes = batch.reduce(
      (sum, object) => checkedAdd(sum, intrinsicByteLength(object.bytes)),
      0,
    );
    steps.push({
      rows: checkedAdd(batch.length * 3, 8),
      bytes: batchBytes,
      units: 1,
      run: (tx) => {
        const staging = stagingFor(tx);
        staging.consumeIngestReservation(leaseId, ownerNonce, batchBytes);
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          batch.length * DURABLE_METADATA_ROW_BYTES,
        );
        // The bounded local rebuild hashes these owned bytes while producing
        // the authenticated manifest entry. Preserve the generic and
        // streamed paths' in-transaction digest verification, but avoid
        // hashing the same local payload a second time here.
        const content = tx.content(storage, cache) as LocalFreshContentStore;
        const inserted = content.putFreshObjectsBatch
          ? content.putFreshObjectsBatch(batch)
          : content.putObjectsBatch(batch, false);
        const members = batch.map((object) => ({
          kind: "object" as const,
          hash: object.hash,
          size: intrinsicByteLength(object.bytes),
        }));
        if (staging.appendFreshBatch) {
          const verifiedSizes =
            "verifiedSizes" in inserted ? inserted.verifiedSizes : undefined;
          staging.appendFreshBatch(
            leaseId,
            ownerNonce,
            members,
            verifiedSizes ? { objectSizes: verifiedSizes } : {},
          );
        } else staging.appendBatch(leaseId, ownerNonce, members);
      },
    });
  }
  // Every object referenced by the rebuilt (non-claimed) leaves needs staged
  // membership for the closure; the splice objects were appended with their
  // put batches, and the already-durable boundary records are appended here
  // as count-only members (chain + counts, no membership row).
  const countedObjects = spine.countedObjects;
  for (const batch of batchesByBytes(
    countedObjects,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    () => 0,
  )) {
    steps.push({
      rows: 8,
      bytes: 0,
      units: 1,
      run: (tx) => {
        stagingFor(tx).appendCountedBatch(
          leaseId,
          ownerNonce,
          batch.map((object) => ({
            kind: "object" as const,
            hash: object.hash,
            size: object.length,
            counted: true as const,
          })),
        );
      },
    });
  }
  for (const batch of batchesByBytes(
    spine.newNodes,
    durableWriteBatchLimit(storage),
    storage.maxFinalTransactionBytes,
    (node) => intrinsicByteLength(node.encoded),
  )) {
    const batchBytes = batch.reduce(
      (sum, node) => checkedAdd(sum, intrinsicByteLength(node.encoded)),
      0,
    );
    steps.push({
      rows: checkedAdd(batch.length * 3, 8),
      bytes: batchBytes,
      units: 1,
      run: (tx) => {
        const staging = stagingFor(tx);
        staging.consumeIngestReservation(leaseId, ownerNonce, batchBytes);
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          batch.length * 2 * DURABLE_METADATA_ROW_BYTES,
        );
        const encodedNodes = batch.map((node) => ({
          hash: node.hash,
          encoded: node.encoded,
        }));
        const content = tx.content(storage, cache) as LocalFreshContentStore;
        const inserted = content.putFreshManifestNodesBatch
          ? content.putFreshManifestNodesBatch(encodedNodes)
          : content.putManifestNodesBatch(encodedNodes);
        tx.manifestTree(storage, cache).recordSubtreeSummaries(encodedNodes);
        const members = batch.map((node) => ({
          kind: "manifest-node" as const,
          hash: node.hash,
          size: intrinsicByteLength(node.encoded),
        }));
        if (staging.appendFreshBatch) {
          const verifiedSizes =
            "verifiedSizes" in inserted ? inserted.verifiedSizes : undefined;
          staging.appendFreshBatch(
            leaseId,
            ownerNonce,
            members,
            verifiedSizes ? { nodeSizes: verifiedSizes } : {},
          );
        } else staging.appendBatch(leaseId, ownerNonce, members);
      },
    });
  }
  const reusedBatches = reusedClaimBatches(spine.reused, storage);
  const verifiedReusedNodeSizes = new Map<string, number>();
  for (const batch of reusedBatches) {
    steps.push({
      rows: checkedAdd(batch.length * 12, 8),
      bytes: 0,
      units: checkedAdd(batch.length * 2, 1),
      run: (tx) => {
        const staging = stagingFor(tx);
        if (staging.appendTrustedReusedManifestBatch) {
          const appended = staging.appendTrustedReusedManifestBatch(
            leaseId,
            ownerNonce,
            batch.map((claim) => claim.nodeHash),
          );
          for (const [hash, size] of appended.verifiedNodeSizes)
            verifiedReusedNodeSizes.set(hash, size);
        } else if (staging.appendReusedManifestBatch) {
          const appended = staging.appendReusedManifestBatch(
            leaseId,
            ownerNonce,
            batch.map((claim) => claim.nodeHash),
          );
          for (const [hash, size] of appended.verifiedNodeSizes)
            verifiedReusedNodeSizes.set(hash, size);
        } else {
          const content = tx.content(storage, cache);
          const members = batch.map((claim) => {
            const size = content.withManifestNode(claim.nodeHash, intrinsicByteLength);
            if (size === undefined)
              throw new Error("ECORRUPT: reused subtree node is missing");
            return Object.freeze({
              kind: "manifest-node" as const,
              hash: claim.nodeHash,
              size,
            });
          });
          staging.appendBatch(leaseId, ownerNonce, members);
        }
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          batch.length * DURABLE_METADATA_ROW_BYTES,
        );
        const certificateState = staging.snapshot(leaseId, ownerNonce);
        const certificatePatch: {
          value?: {
            readonly chainDigest: Uint8Array;
            readonly chainFold: Uint8Array;
            readonly objectCount: number;
            readonly objectBytes: number;
            readonly nodeCount: number;
            readonly nodeBytes: number;
            readonly membershipCount: number;
          };
        } = {};
        const manifestTree = tx.manifestTree(storage, cache) as ReturnType<
          StorageTransactionPorts["manifestTree"]
        > & {
          preloadSubtreeSummaries?: (nodeHashes: readonly Uint8Array[]) => void;
        };
        manifestTree.preloadSubtreeSummaries?.(batch.map((claim) => claim.nodeHash));
        const registered = manifestTree.registerReusedSubtrees(
          leaseId,
          ownerNonce,
          source.manifestHash,
          batch,
          {
            knownObjectHashes: [...spine.fullObjects, ...spine.countedObjects].map(
              (object) => object.hash,
            ),
            knownNodeHashes: spine.newNodes.map((node) => node.hash),
            sourceManifestProtected: true,
            allowSummaries: reusedBatches.length === 1,
            certificateState,
            deferCertificateWrite: true,
            certificatePatch,
            ...(batch.every(
              (claim) =>
                claim.sourceLeafDelta !== undefined &&
                claim.sourceFinalAtLevel !== undefined,
            )
              ? {
                  authenticatedClaims: batch.map((claim) => ({
                    sourcePath: claim.sourcePath,
                    nodeHash: claim.nodeHash,
                    span: claim.span,
                    entryCount: claim.entryCount,
                    sourceFinalAtLevel: claim.sourceFinalAtLevel!,
                    sourceLeafDelta: claim.sourceLeafDelta!,
                  })),
                }
              : {}),
          },
        );
        if (verifiedReusedNodeSizes.size && registered.length)
          staging.cacheReusedSubtreeMetadata(
            leaseId,
            batch.map((claim) => claim.nodeHash),
            registered,
            verifiedReusedNodeSizes,
          );
        else
          staging.cacheReusedSubtreeMetadata(
            leaseId,
            batch.map((claim) => claim.nodeHash),
          );
        if (certificatePatch.value)
          staging.applyCertificatePatch(leaseId, certificatePatch.value);
      },
    });
  }
  steps.push({
    rows: 8,
    bytes: intrinsicByteLength(rebuilt.root),
    units: 1,
    run: (tx, merged) => {
      const staging = stagingFor(tx);
      staging.consumeIngestReservation(
        leaseId,
        ownerNonce,
        intrinsicByteLength(rebuilt.root),
      );
      staging.consumeMetadataReservation(
        leaseId,
        ownerNonce,
        DURABLE_METADATA_ROW_BYTES,
      );
      const content = tx.content(storage, cache) as LocalFreshContentStore;
      // An unchanged local rebuild already has the exact root bytes from the
      // authenticated source snapshot. Keep the generic collision-checking
      // insertion for every other case, but avoid allocating and probing a
      // duplicate root on this narrow no-op path.
      const rootAlreadyPersisted =
        authenticatedExistingRoot !== undefined &&
        equalBytes(rebuilt.rootHash, source.manifestHash) &&
        equalBytes(rebuilt.root, authenticatedExistingRoot);
      if (!rootAlreadyPersisted) {
        if (content.putFreshManifestRoot)
          content.putFreshManifestRoot(rebuilt.rootHash, rebuilt.root);
        else content.putManifestRoot(rebuilt.rootHash, rebuilt.root);
      }
      const rootMember = {
        kind: "manifest-root" as const,
        hash: rebuilt.rootHash,
        size: intrinsicByteLength(rebuilt.root),
      };
      if (staging.appendFreshBatch)
        staging.appendFreshBatch(leaseId, ownerNonce, [rootMember], {
          rootSizes: new Map([
            [bytesToHex(rebuilt.rootHash), intrinsicByteLength(rebuilt.root)],
          ]),
        });
      else staging.appendBatch(leaseId, ownerNonce, [rootMember]);
      staging.registerTrustedObjects(
        [...spine.fullObjects, ...countedObjects].map((object) => ({
          hash: object.hash,
          length: object.length,
        })),
      );
      if (merged && staging.beginTrustedReconciliation)
        staging.beginTrustedReconciliation(leaseId, ownerNonce, rebuilt.rootHash);
      else staging.beginReconciliation(leaseId, ownerNonce, rebuilt.rootHash);
      certificateHolder.value = Object.freeze({
        ...staging.snapshot(leaseId, ownerNonce),
        manifestHash: copyBytes(rebuilt.rootHash),
      });
    },
  });
  const reconcileUnits = checkedAdd(
    checkedMultiply(
      spine.fullObjects.length + spine.countedObjects.length,
      4,
      "local-rebuild closure edges",
    ),
    checkedAdd(
      checkedMultiply(spine.newNodes.length, 2, "local-rebuild closure nodes"),
      checkedAdd(spine.reused.length, 10, "local-rebuild closure validation"),
    ),
  );
  steps.push({
    rows: reconcileUnits,
    bytes: 0,
    units: reconcileUnits,
    selfTransacting: true,
    run: (tx, merged) => {
      const started = performance.now();
      try {
        const trusted = stagingFor(tx).completeTrustedLocalReconciliation;
        if (merged && trusted) {
          const progress = trusted.call(
            stagingFor(tx),
            leaseId,
            ownerNonce,
            rebuilt.rootHash,
            spine.newNodes.map((node) => node.hash),
            intrinsicByteLength(rebuilt.root),
            spine.leafDepth,
          );
          if (!progress.complete)
            throw new Error("ECORRUPT: trusted local reconciliation did not complete");
        } else {
          let complete = false;
          while (!complete) {
            if (merged) {
              complete = stagingFor(tx).reconcileBatch(
                leaseId,
                ownerNonce,
                reconciliationWorkLimit(storage),
                {
                  skipObjectBackingCheck: true,
                },
              ).complete;
            } else {
              complete = transact<{ readonly complete: boolean }>("write", (inner) =>
                stagingFor(inner).reconcileBatch(
                  leaseId,
                  ownerNonce,
                  reconciliationWorkLimit(storage),
                  {
                    skipObjectBackingCheck: true,
                  },
                ),
              ).complete;
            }
          }
        }
      } finally {
        phaseMs.reconciliationMs += performance.now() - started;
      }
    },
  });
  steps.push({
    rows: 8,
    bytes: 0,
    units: 1,
    run: (tx) => {
      const staging = stagingFor(tx);
      const certificate = certificateHolder.value!;
      if (staging.sealAndValidate)
        sealedLeaseHolder.value = staging.sealAndValidate(certificate, now);
      else staging.seal(certificate);
    },
  });
  try {
    const started = performance.now();
    const reconciliationBeforePersistence = phaseMs.reconciliationMs;
    const finalizeBeforePersistence = phaseMs.finalizeMs;
    const persistence = runPersistenceSteps(
      steps,
      storage,
      port,
      budget,
      transact,
      (tx) => {
        stagingFor(tx).flushBatchedIngestAccounting?.();
        if (finalizePrepared)
          finalizePrepared(
            tx,
            certificateHolder.value!,
            rebuilt.rootHash,
            rebuilt.fileSize,
            sealedLeaseHolder.value,
          );
        stagingFor(tx).flushBatchedUsageAccounting?.();
      },
      (tx) => stagingFor(tx).enableBatchedIngestAccounting?.(),
    );
    phaseMs.persistenceMs += Math.max(
      0,
      performance.now() -
        started -
        (phaseMs.reconciliationMs - reconciliationBeforePersistence) -
        (phaseMs.finalizeMs - finalizeBeforePersistence),
    );
    begun = true;
    const certificate = certificateHolder.value!;
    return Object.freeze({
      hash: copyBytes(rebuilt.rootHash),
      size: rebuilt.fileSize,
      certificate,
      storageTransactions,
      persistenceMerged: persistence.merged,
      persistenceRows: persistence.rows,
      persistenceBytes: persistence.bytes,
      persistenceUnits: persistence.units,
      newObjectCount: rebuilt.metrics.newObjectCount,
      newManifestNodeCount: rebuilt.metrics.newManifestNodeCount,
      reusedSubtrees: spine.reused.length,
      validationRows: spine.validationRows,
    });
  } catch (error) {
    if (begun)
      try {
        port.transaction("write", budget, (tx) => {
          tx.staging(storage, cache).delete(leaseId, ownerNonce);
        });
      } catch {}
    throw error;
  }
}

type LocalRebuildAttemptOutcome =
  | { readonly outcome: "prepared"; readonly prepared: DurableEditPreparedManifest }
  | { readonly outcome: "fell-back"; readonly reason: string };

function tryLocallyRebuiltContent(
  port: OperationsStorage,
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  cache: ContentCache | undefined,
  clock: () => number,
  finalizePrepared?: DurableEditFinalizer,
  readSnapshot?: DurableEditReadSnapshot,
): LocalRebuildAttemptOutcome {
  const limits = DEFAULT_LOCAL_REBUILD_LIMITS;
  if (edit.insertLength > limits.maxAffectedBytes)
    return Object.freeze({
      outcome: "fell-back",
      reason: "edit insertion exceeds the local rebuild affected-byte window",
    });
  // Storage-free preflight: every manifest entry spans at most the FastCDC
  // maximum, so the minimum possible entry count is ceil(size / maximum).
  // When even that exceeds the retained-entry budget the manifest cannot be
  // loaded, so skip the loader transaction entirely.
  const minimumEntries = Math.max(
    1,
    Math.ceil(source.size / source.parameters.maximum),
  );
  if (minimumEntries > limits.maxRetainedEntries)
    return Object.freeze({
      outcome: "fell-back",
      reason:
        "durable local rebuild exceeds its retained-entry limit; use the streamed workspace fallback",
    });
  let sourceReadCalls = 0;
  let sourceBytesRead = 0;
  let storageTransactions = 0;
  const phaseMs = emptyLocalRebuildPhaseTimings();
  const boundedWindow = boundedSourceWindow(source, cache);
  const sourceForAttempt = boundedWindow.source;
  const measuredSource: DurableEditSource = Object.freeze({
    ...sourceForAttempt,
    read(offset: number, length: number): Uint8Array {
      sourceReadCalls = checkedAdd(
        sourceReadCalls,
        1,
        "durable local rebuild source reads",
      );
      sourceBytesRead = checkedAdd(
        sourceBytesRead,
        length,
        "durable local rebuild source bytes",
      );
      const started = performance.now();
      try {
        return sourceForAttempt.read(offset, length);
      } finally {
        phaseMs.sourceReadMs += performance.now() - started;
      }
    },
    ...(sourceForAttempt.readInTransaction
      ? {
          readInTransaction(
            content: ContentStore,
            offset: number,
            length: number,
          ): Uint8Array {
            sourceReadCalls = checkedAdd(
              sourceReadCalls,
              1,
              "bounded durable local rebuild source reads",
            );
            sourceBytesRead = checkedAdd(
              sourceBytesRead,
              length,
              "bounded durable local rebuild source bytes",
            );
            const started = performance.now();
            try {
              return sourceForAttempt.readInTransaction!(content, offset, length);
            } finally {
              phaseMs.sourceReadMs += performance.now() - started;
            }
          },
        }
      : {}),
  });
  const measuredPort: OperationsStorage = Object.freeze({
    ...port,
    transaction<T>(
      mode: StorageTransactionMode,
      budget: StorageWorkBudget,
      callback: (ports: StorageTransactionPorts) => T,
    ): T {
      storageTransactions = checkedAdd(
        storageTransactions,
        1,
        "durable local rebuild transactions",
      );
      return port.transaction(mode, budget, callback);
    },
  });
  // Admission envelope: the retained manifest state plus the affected window
  // (chunker input/output, affected objects, rebuilt nodes) plus the
  // materialized insertion and a fixed slack. The path-copy envelope capped
  // at maxManagedResidentBytes/9; the local rebuild admits its state and
  // window together instead. The envelope is reserved before the insertion
  // is materialized so no caller work happens under an unadmittable profile.
  const windowBytes = checkedAdd(
    checkedMultiply(
      limits.maxAffectedBytes,
      2,
      "durable local rebuild affected window",
    ),
    checkedAdd(edit.insertLength, 1024 * 1024, "durable local rebuild slack"),
    "durable local rebuild working set",
  );
  let releaseWindow: (() => void) | undefined;
  try {
    cache?.makeRoom(windowBytes);
    releaseWindow = admission.reserve(windowBytes);
  } catch (error) {
    if (error instanceof RangeError)
      return Object.freeze({
        outcome: "fell-back",
        reason: "durable local rebuild working set cannot be admitted",
      });
    throw error;
  }
  try {
    const insertBytes = materializeEditInsertion(edit, limits);
    if (insertBytes === undefined)
      return Object.freeze({
        outcome: "fell-back",
        reason: "edit insertion exceeds the local rebuild affected-byte window",
      });
    // The bounded Merkle descent is attempted first: one read transaction
    // loads the validation certificate, the root, both paths, and a capped
    // fringe; the rebuild and the frontier claims stay inside that
    // neighborhood. Anything the bounded path cannot represent falls back to
    // the full-state loader below.
    const bounded = tryBoundedLocalRebuild(
      measuredPort,
      measuredSource,
      edit,
      insertBytes,
      newSize,
      storage,
      cache,
      clock,
      limits,
      finalizePrepared,
      true,
      readSnapshot?.state,
    );
    if (bounded.outcome === "prepared") return bounded;
    const loadStarted = performance.now();
    const loaded = loadAuthenticatedManifestState(
      measuredPort,
      source.manifestHash,
      storage,
      limits,
      cache,
      port.hashBytes,
    );
    phaseMs.manifestLoadMs += performance.now() - loadStarted;
    try {
      const rebuildStarted = performance.now();
      const sourceReadBeforeRebuild = phaseMs.sourceReadMs;
      const rebuilt = rebuildManifestLocallyWithParametersOwned(
        measuredSource,
        loaded.manifest,
        {
          offset: edit.offset,
          deleteLength: edit.deleteLength,
          insertBytes,
        },
        source.parameters,
        limits,
      );
      const spine = walkRebuiltSpine(rebuilt, loaded.manifest, loaded.paths, limits);
      phaseMs.rebuildMs += Math.max(
        0,
        performance.now() -
          rebuildStarted -
          (phaseMs.sourceReadMs - sourceReadBeforeRebuild),
      );
      const sourceReadTransactions = measuredSourceReadTransactions(
        sourceForAttempt,
        sourceReadCalls,
        "durable local rebuild source transactions",
      );
      const persistenceLimit = Math.max(
        1,
        MAX_PATH_COPY_TRANSACTIONS - 1 - sourceReadTransactions,
      );
      const prepared = persistLocallyRebuilt(
        measuredPort,
        source,
        rebuilt,
        spine,
        loaded.manifest.root,
        storage,
        cache,
        clock,
        persistenceLimit,
        phaseMs,
        finalizePrepared,
      );
      return Object.freeze({
        outcome: "prepared",
        prepared: Object.freeze({
          hash: prepared.hash,
          size: newSize,
          certificate: prepared.certificate,
          mode: "local-rebuild",
          localRebuildMetrics: Object.freeze({
            storageTransactions: checkedAdd(
              prepared.storageTransactions,
              sourceReadTransactions,
              "durable local rebuild aggregate transactions",
            ),
            persistenceMerged: prepared.persistenceMerged,
            persistenceRows: prepared.persistenceRows,
            persistenceBytes: prepared.persistenceBytes,
            persistenceUnits: prepared.persistenceUnits,
            sourceReadCalls,
            sourceReadTransactions,
            sourceBytesRead,
            authenticatedNodesRead: loaded.manifest.nodes.size,
            loadedEntries: loaded.manifest.entries.length,
            loadedNodes: loaded.manifest.nodes.size,
            affectedEntries: rebuilt.metrics.affectedEntryCount,
            newObjectCount: prepared.newObjectCount,
            newManifestNodeCount: prepared.newManifestNodeCount,
            reusedSubtrees: prepared.reusedSubtrees,
            reusedManifestNodeCount: rebuilt.metrics.reusedManifestNodeCount,
            scanWindowBytes: rebuilt.metrics.scanWindowBytes,
            reconnectOldOffset: rebuilt.metrics.reconnectOldOffset,
            reconnectNewOffset: rebuilt.metrics.reconnectNewOffset,
            pathAuthenticationTransactions: 2,
            phaseMs,
          }),
        }),
      });
    } finally {
      loaded.release();
    }
  } catch (error) {
    if (
      !(error instanceof RangeError) &&
      !(error instanceof DurablePathCopyFallbackError)
    )
      throw error;
    return Object.freeze({
      outcome: "fell-back",
      reason: String(error instanceof Error ? error.message : error),
    });
  } finally {
    releaseWindow?.();
    boundedWindow.release();
  }
}

/**
 * The bounded Merkle-descent local rebuild: loads the neighborhood state in
 * one read transaction, rebuilds with the relative regroup, derives the
 * frontier claims from the loaded paths, and persists. Falls back without
 * side effects on `BoundedRebuildFallbackError`.
 */
function tryBoundedLocalRebuild(
  port: OperationsStorage,
  source: DurableEditSource,
  edit: DurableContentEdit,
  insertBytes: Uint8Array,
  newSize: number,
  storage: StorageLimits,
  cache: ContentCache | undefined,
  clock: () => number,
  limits: LocalRebuildLimits,
  finalizePrepared?: DurableEditFinalizer,
  allowTruncatedFringe = true,
  preloadedState?: BoundedManifestState,
): LocalRebuildAttemptOutcome {
  let sourceReadCalls = 0;
  let sourceBytesRead = 0;
  let storageTransactions = 0;
  const phaseMs = emptyLocalRebuildPhaseTimings();
  const measuredSource: DurableEditSource = Object.freeze({
    ...source,
    read(offset: number, length: number): Uint8Array {
      sourceReadCalls = checkedAdd(
        sourceReadCalls,
        1,
        "bounded durable local rebuild source reads",
      );
      sourceBytesRead = checkedAdd(
        sourceBytesRead,
        length,
        "bounded durable local rebuild source bytes",
      );
      const started = performance.now();
      try {
        return source.read(offset, length);
      } finally {
        phaseMs.sourceReadMs += performance.now() - started;
      }
    },
    ...(source.readInTransaction
      ? {
          readInTransaction(
            content: ContentStore,
            offset: number,
            length: number,
          ): Uint8Array {
            sourceReadCalls = checkedAdd(
              sourceReadCalls,
              1,
              "bounded durable local rebuild source reads",
            );
            sourceBytesRead = checkedAdd(
              sourceBytesRead,
              length,
              "bounded durable local rebuild source bytes",
            );
            const started = performance.now();
            try {
              return source.readInTransaction!(content, offset, length);
            } finally {
              phaseMs.sourceReadMs += performance.now() - started;
            }
          },
        }
      : {}),
  });
  const measuredPort: OperationsStorage = Object.freeze({
    ...port,
    transaction<T>(
      mode: StorageTransactionMode,
      budget: StorageWorkBudget,
      callback: (ports: StorageTransactionPorts) => T,
    ): T {
      storageTransactions = checkedAdd(
        storageTransactions,
        1,
        "bounded durable local rebuild transactions",
      );
      return port.transaction(mode, budget, callback);
    },
  });
  try {
    const loadStarted = performance.now();
    const sourceReadBeforeLoad = phaseMs.sourceReadMs;
    const state =
      preloadedState ??
      loadBoundedManifestState(
        measuredPort,
        measuredSource,
        source.manifestHash,
        { offset: edit.offset, deleteLength: edit.deleteLength },
        storage,
        limits,
        cache,
        allowTruncatedFringe && edit.insertLength === edit.deleteLength,
      );
    phaseMs.manifestLoadMs += Math.max(
      0,
      preloadedState
        ? 0
        : performance.now() -
            loadStarted -
            (phaseMs.sourceReadMs - sourceReadBeforeLoad),
    );
    try {
      const rebuildStarted = performance.now();
      const sourceReadBeforeRebuild = phaseMs.sourceReadMs;
      const rebuilt = rebuildManifestBoundedOwned(
        state,
        measuredSource,
        { offset: edit.offset, deleteLength: edit.deleteLength, insertBytes },
        limits,
        port.hashBytes,
      );
      const spine = walkRebuiltSpineBounded(rebuilt, state, limits);
      phaseMs.rebuildMs += Math.max(
        0,
        performance.now() -
          rebuildStarted -
          (phaseMs.sourceReadMs - sourceReadBeforeRebuild),
      );
      const sourceReadTransactions = measuredSourceReadTransactions(
        source,
        sourceReadCalls,
        "bounded durable local rebuild source transactions",
      );
      const persistenceLimit = Math.max(
        1,
        MAX_PATH_COPY_TRANSACTIONS - 1 - sourceReadTransactions,
      );
      const prepared = persistLocallyRebuilt(
        measuredPort,
        source,
        rebuilt,
        spine,
        state.root.root,
        storage,
        cache,
        clock,
        persistenceLimit,
        phaseMs,
        finalizePrepared,
      );
      const loadedEntries =
        state.affectedLeaf.entries.length +
        (state.dirtyEndLeaf === state.affectedLeaf
          ? 0
          : state.dirtyEndLeaf.entries.length) +
        state.fringeLeaves.reduce((sum, leaf) => sum + leaf.entries.length, 0);
      const loadedNodes =
        state.rootDepth +
        (state.dirtyEndLeaf === state.affectedLeaf ? 0 : 1) +
        state.fringeLeaves.length +
        state.levelWindows.reduce(
          (sum, window) => sum + (window ? window.fringe.length : 0),
          0,
        );
      return Object.freeze({
        outcome: "prepared",
        prepared: Object.freeze({
          hash: prepared.hash,
          size: newSize,
          certificate: prepared.certificate,
          mode: "local-rebuild",
          localRebuildMetrics: Object.freeze({
            storageTransactions: checkedAdd(
              prepared.storageTransactions,
              sourceReadTransactions,
              "bounded durable local rebuild aggregate transactions",
            ),
            persistenceMerged: prepared.persistenceMerged,
            persistenceRows: prepared.persistenceRows,
            persistenceBytes: prepared.persistenceBytes,
            persistenceUnits: prepared.persistenceUnits,
            sourceReadCalls,
            sourceReadTransactions,
            sourceBytesRead,
            authenticatedNodesRead: loadedNodes,
            loadedEntries,
            loadedNodes,
            affectedEntries: rebuilt.metrics.affectedEntryCount,
            newObjectCount: prepared.newObjectCount,
            newManifestNodeCount: prepared.newManifestNodeCount,
            reusedSubtrees: prepared.reusedSubtrees,
            reusedManifestNodeCount: state.claimPaths.size,
            scanWindowBytes: rebuilt.metrics.scanWindowBytes,
            reconnectOldOffset: rebuilt.metrics.reconnectOldOffset,
            reconnectNewOffset: rebuilt.metrics.reconnectNewOffset,
            pathAuthenticationTransactions: 2,
            phaseMs,
          }),
        }),
      });
    } finally {
      state.release();
    }
  } catch (error) {
    if (error instanceof BoundedRebuildFallbackError) {
      // An equal-length edit can usually stop at the first authenticated
      // boundary, but a partial final group may still need the unchanged
      // suffix to prove its canonical grouping. Retry once with the complete
      // fringe before falling through to the older authenticated loader.
      if (allowTruncatedFringe)
        return tryBoundedLocalRebuild(
          port,
          source,
          edit,
          insertBytes,
          newSize,
          storage,
          cache,
          clock,
          limits,
          finalizePrepared,
          false,
        );
      return Object.freeze({
        outcome: "fell-back",
        reason: error.message,
      });
    }
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
  retainedBytesAlreadyAdmitted = false,
  finalizePrepared?: DurableEditFinalizer,
  readSnapshot?: DurableEditReadSnapshot,
): Promise<DurableEditPreparedManifest> {
  const ownsCache = cache === undefined;
  const operationCache =
    cache ??
    new ContentCache(Math.min(runtime.maxCacheBytes, 4 * 1024 ** 2), admission);
  cache = operationCache;
  const newSize = validateInputs(source, edit);
  if (newSize > storage.maxFileBytes)
    throw new RangeError("edited file exceeds maxFileBytes");
  let releaseRetained: (() => void) | undefined;
  if ((edit.retainedBytes ?? 0) > 0 && !retainedBytesAlreadyAdmitted) {
    cache.makeRoom(edit.retainedBytes!);
    releaseRetained = admission.reserve(edit.retainedBytes!);
  }
  try {
    // The bounded local rebuild is attempted first: for equal-length edits the
    // FastCDC gear stream reconverges within one chunk, so re-chunking the
    // affected window (with native hashing) beats re-chunking the whole
    // authenticated leaf. The durable path-copy remains the fallback for
    // shapes the local rebuild cannot represent, and the O(file) streamed
    // workspace rebuild is the final fallback.
    const localAttempt = tryLocallyRebuiltContent(
      port,
      source,
      edit,
      newSize,
      storage,
      runtime,
      admission,
      cache,
      clock,
      finalizePrepared,
      readSnapshot,
    );
    if (localAttempt.outcome === "prepared") return localAttempt.prepared;
    const localRebuildReason = localAttempt.reason;
    let reason: string | undefined;
    let pathAuthenticationTransactions = 0;
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
        pathAuthenticationTransactions = checkedAdd(
          pathAuthenticationTransactions,
          1,
          "durable edit path transactions",
        );
        const path = port.transaction(
          "read",
          {
            maxRows: checkedAdd(
              8,
              checkedMultiply(
                storage.maxManifestDepth,
                2,
                "authenticated path result rows",
              ),
              "authenticated path result rows",
            ),
            maxBytes: Math.max(
              runtime.maxQueryBatchBytes,
              checkedAdd(
                1_024,
                checkedMultiply(
                  storage.maxManifestDepth,
                  checkedAdd(
                    storage.maxManifestNodeBytes,
                    512,
                    "authenticated path row bytes",
                  ),
                  "authenticated path result bytes",
                ),
                "authenticated path result bytes",
              ),
            ),
            maxStatements: storage.maxManifestDepth * 4 + 8,
            maxElapsedMs: 5_000,
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
          port.hashBytes,
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
              `durable path-copy exceeds its aggregate storage transaction cap (${projectedTransactions} projected for ${candidate.reused.length} reused subtrees)`,
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
    let sourceReadCalls = 0;
    let sourceBytesRead = 0;
    let persistenceTransactions = 0;
    const measuredSource: DurableEditSource = Object.freeze({
      ...source,
      read(offset: number, length: number): Uint8Array {
        sourceReadCalls = checkedAdd(
          sourceReadCalls,
          1,
          "streamed fallback source reads",
        );
        sourceBytesRead = checkedAdd(
          sourceBytesRead,
          length,
          "streamed fallback source bytes",
        );
        return source.read(offset, length);
      },
    });
    const measuredPort: OperationsStorage = Object.freeze({
      ...port,
      transaction<T>(
        mode: StorageTransactionMode,
        budget: StorageWorkBudget,
        callback: (ports: StorageTransactionPorts) => T,
      ): T {
        persistenceTransactions = checkedAdd(
          persistenceTransactions,
          1,
          "streamed fallback persistence transactions",
        );
        return port.transaction(mode, budget, callback);
      },
    });
    const prepared = await prepareContentStreaming(
      measuredPort,
      editedContentStream(
        measuredSource,
        edit,
        newSize,
        readWindowBytes,
        admission,
        cache,
      ),
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
      ...(localRebuildReason === undefined ? {} : { localRebuildReason }),
      fallbackMetrics: Object.freeze({
        sourceReadCalls,
        sourceReadTransactions: measuredSourceReadTransactions(
          source,
          sourceReadCalls,
          "streamed fallback source transactions",
        ),
        sourceBytesRead,
        pathAuthenticationTransactions,
        persistenceTransactions,
        storageTransactions: checkedAdd(
          checkedAdd(
            pathAuthenticationTransactions,
            persistenceTransactions,
            "streamed fallback storage transactions",
          ),
          measuredSourceReadTransactions(
            source,
            sourceReadCalls,
            "streamed fallback source transactions",
          ),
          "streamed fallback storage transactions",
        ),
        readWindowBytes,
      }),
    });
  } finally {
    if (ownsCache) operationCache.clear();
    releaseRetained?.();
  }
}

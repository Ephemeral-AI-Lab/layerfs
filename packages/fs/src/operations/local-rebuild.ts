import {
  bytesToHex,
  copyBytes,
  equalBytes,
  intrinsicByteLength,
} from "../cas/bytes.js";
import { manifestIdFromHash, sha256 } from "../cas/sha256.js";
import { StreamingFastCdc, type FastCdcConfiguration } from "../cdc/fastcdc.js";
import {
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
  type ManifestNode,
  decodeManifestNode,
  MAX_MANIFEST_NODE_BYTES,
  ROOT_ENVELOPE_BYTES,
  validateSupportedManifestParameters,
} from "../manifests/codec.js";
import type { EncodedManifestNode } from "../manifests/builder.js";
import { ManifestSequentialCursor } from "../manifests/cursor.js";
import {
  MAX_DIAGNOSTIC_CONTENT_BYTES,
  type DiagnosticBuiltManifest,
} from "./full-rebuild.js";
import { checkedAdd } from "../resources/safe-integers.js";
import { MAX_CONTENT_OBJECT_BYTES } from "../resources/limits.js";
import {
  advanceManifestGroupingState,
  isManifestGroupBoundary,
} from "../manifests/grouping.js";

export interface RandomAccessContentSource {
  readonly size: number;
  read(offset: number, length: number): Uint8Array;
}

export interface LocalContentEdit {
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertBytes: Uint8Array;
}

export interface OwnedLocalContentInputs {
  readonly source: RandomAccessContentSource;
  readonly edit: LocalContentEdit;
}

export interface ValidatedLocalContentInputs {
  readonly source: RandomAccessContentSource;
  readonly edit: LocalContentEdit;
}

/** Validates and snapshots scalars without copying caller payload bytes. */
export function validateLocalContentInputs(
  source: RandomAccessContentSource,
  edit: LocalContentEdit,
): ValidatedLocalContentInputs {
  const size = source.size;
  const read = source.read;
  const offset = edit.offset;
  const deleteLength = edit.deleteLength;
  const insertBytes = edit.insertBytes;
  if (!Number.isSafeInteger(size) || size < 0)
    throw new RangeError("source size must be a nonnegative safe integer");
  if (
    !Number.isSafeInteger(offset) ||
    offset < 0 ||
    !Number.isSafeInteger(deleteLength) ||
    deleteLength < 0 ||
    offset > size ||
    deleteLength > size - offset
  )
    throw new RangeError("local edit is outside the source");
  if (!(insertBytes instanceof Uint8Array))
    throw new TypeError("local edit insertion must be a Uint8Array");
  const insertionByteLength = intrinsicByteLength(insertBytes);
  if (insertionByteLength > MAX_CONTENT_OBJECT_BYTES)
    throw new RangeError("edit insertion exceeds the supported object limit");
  checkedAdd(size - deleteLength, insertionByteLength, "rebuilt content size");
  return Object.freeze({
    source: Object.freeze({
      size,
      read(offsetValue: number, length: number): Uint8Array {
        return read.call(source, offsetValue, length);
      },
    }),
    edit: Object.freeze({
      offset,
      deleteLength,
      insertBytes,
    }),
  });
}

/** Internal ownership boundary; scalar validation must already have succeeded. */
export function ownLocalContentInputs(
  validated: ValidatedLocalContentInputs,
): OwnedLocalContentInputs {
  return Object.freeze({
    source: validated.source,
    edit: Object.freeze({
      offset: validated.edit.offset,
      deleteLength: validated.edit.deleteLength,
      insertBytes: copyBytes(validated.edit.insertBytes),
    }),
  });
}

export function snapshotLocalRebuildLimits(
  limits: LocalRebuildLimits,
): Readonly<LocalRebuildLimits> {
  const owned = Object.freeze({
    maxRetainedEntries: limits.maxRetainedEntries,
    maxRetainedNodes: limits.maxRetainedNodes,
    maxAffectedEntries: limits.maxAffectedEntries,
    maxAffectedBytes: limits.maxAffectedBytes,
  });
  for (const [name, value] of Object.entries(owned))
    if (!Number.isSafeInteger(value) || value <= 0)
      throw new RangeError(`${name} must be a positive safe integer`);
  for (const name of Object.keys(owned) as Array<keyof LocalRebuildLimits>)
    if (owned[name] > DEFAULT_LOCAL_REBUILD_LIMITS[name])
      throw new RangeError(`${name} may only lower the fixed diagnostic cap`);
  if (owned.maxAffectedEntries > owned.maxRetainedEntries)
    throw new RangeError("maxAffectedEntries exceeds maxRetainedEntries");
  return owned;
}

export interface LocalRebuildLimits {
  readonly maxRetainedEntries: number;
  readonly maxRetainedNodes: number;
  readonly maxAffectedEntries: number;
  readonly maxAffectedBytes: number;
}
export const DEFAULT_LOCAL_REBUILD_LIMITS: Readonly<LocalRebuildLimits> = Object.freeze(
  {
    maxRetainedEntries: 16_384,
    maxRetainedNodes: 32_768,
    maxAffectedEntries: 4096,
    maxAffectedBytes: 16 * 1024 * 1024,
  },
);
export interface LocalRebuildAttemptMetrics {
  readonly sourceBytesRead: number;
  readonly bytesHashed: number;
  readonly largestSourceRead: number;
  readonly chunkerInputBytesCopied: number;
  readonly chunkerOutputBytesCopied: number;
  readonly chunkerBoundaryBytesScanned: number;
  readonly editedInputBytesPrepared: number;
}
export class LocalRebuildLimitError extends RangeError {
  readonly name = "LocalRebuildLimitError";
  constructor(
    message: string,
    readonly attemptMetrics: Readonly<LocalRebuildAttemptMetrics> = Object.freeze({
      sourceBytesRead: 0,
      bytesHashed: 0,
      largestSourceRead: 0,
      chunkerInputBytesCopied: 0,
      chunkerOutputBytesCopied: 0,
      chunkerBoundaryBytesScanned: 0,
      editedInputBytesPrepared: 0,
    }),
  ) {
    super(message);
  }
}

export interface ManifestEntrySplice {
  readonly start: number;
  readonly deleteCount: number;
  readonly entries: readonly ManifestEntry[];
}

export interface LocalRebuildMetrics {
  readonly sourceBytesRead: number;
  readonly bytesHashed: number;
  readonly scanWindowBytes: number;
  readonly reconnectOldOffset: number;
  readonly reconnectNewOffset: number;
  readonly reusedPrefixEntries: number;
  readonly reusedSuffixEntries: number;
  readonly affectedEntryCount: number;
  readonly newObjectCount: number;
  readonly newManifestNodeCount: number;
  readonly reusedManifestNodeCount: number;
  readonly fellBackToEnd: boolean;
  readonly insertionCopyCount: 1;
  readonly insertionBytesCopied: number;
  readonly chunkerInputBytesCopied: number;
  readonly chunkerOutputBytesCopied: number;
  readonly chunkerBoundaryBytesScanned: number;
  readonly editedInputBytesPrepared: number;
}

export interface LocallyRebuiltManifest {
  readonly rootHash: Uint8Array;
  readonly root: Uint8Array;
  readonly fileSize: number;
  readonly entryCount: number;
  readonly entrySplice: ManifestEntrySplice;
  /** Only chunks produced inside the local rechunking window. */
  readonly affectedObjects: ReadonlyMap<string, Uint8Array>;
  /** Only authenticated nodes absent from the old manifest. */
  readonly newNodes: ReadonlyMap<string, EncodedManifestNode>;
  readonly metrics: LocalRebuildMetrics;
}

type RecordValue = ManifestEntry | ManifestChild;

interface GroupBounds {
  readonly start: number;
  readonly end: number;
  readonly node: EncodedManifestNode;
}

interface RegroupedLevel {
  readonly oldGroups: readonly GroupBounds[];
  readonly prefixGroupCount: number;
  readonly reconnectGroup: number;
  readonly segment: readonly EncodedManifestNode[];
  readonly totalGroupCount: number;
}

function toChild(node: EncodedManifestNode): ManifestChild {
  return Object.freeze({
    hash: node.hash,
    span: node.node.span,
    entryCount: node.node.entryCount,
  });
}

function nodeRecordCount(node: ManifestNode, leaf: boolean): number {
  if (leaf && node.kind !== "leaf")
    throw new Error("old manifest level contains a non-leaf node");
  if (!leaf && node.kind !== "internal")
    throw new Error("old manifest level contains a leaf node");
  return node.kind === "leaf" ? node.entries.length : node.children.length;
}

function groupBounds(
  nodes: readonly EncodedManifestNode[],
  leaf: boolean,
  recordCount: number,
): GroupBounds[] {
  const result: GroupBounds[] = [];
  let cursor = 0;
  for (const node of nodes) {
    const count = nodeRecordCount(node.node, leaf);
    result.push(Object.freeze({ start: cursor, end: checkedAdd(cursor, count), node }));
    cursor += count;
  }
  if (cursor !== recordCount)
    throw new Error("old manifest level record count mismatch");
  return result;
}

function startGroupFor(
  groups: readonly GroupBounds[],
  recordIndex: number,
  recordCount: number,
): number {
  if (
    groups.length === 1 &&
    groups[0]!.start === 0 &&
    groups[0]!.end === 0 &&
    recordIndex === 0
  )
    return 0;
  for (let index = 0; index < groups.length; index += 1) {
    const group = groups[index]!;
    if (
      recordIndex === group.start ||
      (recordIndex > group.start && recordIndex < group.end)
    )
      return index;
  }
  if (recordIndex === recordCount) return groups.length;
  throw new RangeError("manifest splice does not lie inside the old level");
}

function reconnectGroupFor(
  groups: readonly GroupBounds[],
  recordIndex: number,
  recordCount: number,
): number | undefined {
  if (recordIndex === recordCount) return groups.length;
  for (let index = 0; index < groups.length; index += 1)
    if (groups[index]!.start === recordIndex) return index;
  return undefined;
}

function makeNode(
  level: number,
  records: readonly RecordValue[],
  old: DiagnosticBuiltManifest,
  newNodes: Map<string, EncodedManifestNode>,
): EncodedManifestNode {
  let node: ManifestNode;
  if (level === 0) {
    const entries = records as readonly ManifestEntry[];
    node = Object.freeze({
      kind: "leaf",
      span: entries.reduce((sum, entry) => checkedAdd(sum, entry.length), 0),
      entryCount: entries.length,
      entries: Object.freeze([...entries]),
    } satisfies ManifestLeaf);
  } else {
    const children = records as readonly ManifestChild[];
    node = Object.freeze({
      kind: "internal",
      span: children.reduce((sum, child) => checkedAdd(sum, child.span), 0),
      entryCount: children.reduce((sum, child) => checkedAdd(sum, child.entryCount), 0),
      children: Object.freeze([...children]),
    } satisfies ManifestInternal);
  }
  const encoded = encodeManifestNode(node);
  const hash = sha256(encoded);
  const key = bytesToHex(hash);
  const existing = old.nodes.get(key) ?? newNodes.get(key);
  if (existing) return existing;
  const created = Object.freeze({ hash, encoded, node });
  newNodes.set(key, created);
  return created;
}

function regroupLevel(
  level: number,
  oldRecords: readonly RecordValue[],
  oldNodes: readonly EncodedManifestNode[],
  spliceStart: number,
  spliceEnd: number,
  replacement: readonly RecordValue[],
  old: DiagnosticBuiltManifest,
  newNodes: Map<string, EncodedManifestNode>,
): RegroupedLevel {
  if (spliceStart < 0 || spliceEnd < spliceStart || spliceEnd > oldRecords.length)
    throw new RangeError("invalid manifest level splice");
  const bounds = groupBounds(oldNodes, level === 0, oldRecords.length);
  const startGroup = startGroupFor(bounds, spliceStart, oldRecords.length);
  const groupStart =
    startGroup === bounds.length ? oldRecords.length : bounds[startGroup]!.start;
  const minimumReconnect =
    bounds[startGroup]?.start === bounds[startGroup]?.end ? startGroup + 1 : startGroup;
  let oldCursor = groupStart;
  let replacementCursor = 0;
  let group: RecordValue[] = [];
  let state = 0n;
  let reconnectGroup = bounds.length;
  const segment: EncodedManifestNode[] = [];
  const minimum = level === 0 ? 64 : 32;
  const target = level === 0 ? 128 : 64;
  const maximum = level === 0 ? 256 : 128;

  const emit = (): boolean => {
    if (group.length === 0) return false;
    segment.push(makeNode(level, group, old, newNodes));
    group = [];
    state = 0n;
    const logicalOldCursor =
      replacementCursor === replacement.length && oldCursor <= spliceEnd
        ? spliceEnd
        : oldCursor;
    if (replacementCursor === replacement.length && logicalOldCursor >= spliceEnd) {
      const candidate = reconnectGroupFor(bounds, logicalOldCursor, oldRecords.length);
      if (candidate !== undefined && candidate >= minimumReconnect) {
        reconnectGroup = candidate;
        return true;
      }
    }
    return false;
  };

  let stopped = false;
  while (!stopped) {
    let record: RecordValue | undefined;
    if (oldCursor < spliceStart) record = oldRecords[oldCursor++];
    else if (replacementCursor < replacement.length)
      record = replacement[replacementCursor++];
    else {
      if (oldCursor < spliceEnd) oldCursor = spliceEnd;
      if (oldCursor < oldRecords.length) record = oldRecords[oldCursor++];
    }
    if (!record) break;
    group.push(record);
    state = advanceManifestGroupingState(state, record);
    if (isManifestGroupBoundary(group.length, state, minimum, target, maximum))
      stopped = emit();
  }
  if (!stopped && group.length > 0) emit();
  if (
    level === 0 &&
    startGroup === 0 &&
    reconnectGroup === bounds.length &&
    segment.length === 0
  )
    segment.push(makeNode(0, [], old, newNodes));
  const totalGroupCount =
    startGroup + segment.length + (bounds.length - reconnectGroup);
  return Object.freeze({
    oldGroups: bounds,
    prefixGroupCount: startGroup,
    reconnectGroup,
    segment: Object.freeze(segment),
    totalGroupCount,
  });
}

function onlyNode(level: RegroupedLevel): EncodedManifestNode {
  if (level.totalGroupCount !== 1) throw new Error("manifest level is not singular");
  if (level.prefixGroupCount === 1) return level.oldGroups[0]!.node;
  if (level.segment.length === 1) return level.segment[0]!;
  if (level.reconnectGroup < level.oldGroups.length)
    return level.oldGroups[level.reconnectGroup]!.node;
  throw new Error("manifest level lost its root node");
}

function orderedLevels(old: DiagnosticBuiltManifest): EncodedManifestNode[][] {
  const root = decodeManifestRoot(old.root, old.rootHash);
  const levels: EncodedManifestNode[][] = [];
  const visit = (hash: Uint8Array, depth: number): number => {
    if (depth > 32) throw new Error("manifest tree is too deep");
    const node = old.nodes.get(bytesToHex(hash));
    if (!node) throw new Error("old manifest is missing an authenticated node");
    if (node.node.kind === "leaf") {
      (levels[0] ??= []).push(node);
      return 0;
    }
    let height: number | undefined;
    for (const child of node.node.children) {
      const childHeight = visit(child.hash, depth + 1);
      if (height !== undefined && height !== childHeight)
        throw new Error("manifest tree levels are unbalanced");
      height = childHeight;
    }
    const actual = (height ?? -1) + 1;
    (levels[actual] ??= []).push(node);
    return actual;
  };
  visit(root.rootNodeHash, 1);
  return levels;
}

function entryOffsets(entries: readonly ManifestEntry[]): {
  readonly offsets: readonly number[];
  readonly boundary: ReadonlyMap<number, number>;
  readonly size: number;
} {
  const offsets = [0];
  const boundary = new Map<number, number>([[0, 0]]);
  let size = 0;
  for (let index = 0; index < entries.length; index += 1) {
    size = checkedAdd(size, entries[index]!.length);
    offsets.push(size);
    boundary.set(size, index + 1);
  }
  return { offsets: Object.freeze(offsets), boundary, size };
}

function containingEntry(offsets: readonly number[], offset: number): number {
  let low = 0;
  let high = offsets.length - 1;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (offsets[middle]! < offset) low = middle + 1;
    else high = middle;
  }
  if (offsets[low] === offset) return low;
  return Math.max(0, low - 1);
}

function authenticateDiagnosticManifest(
  old: DiagnosticBuiltManifest,
  limits: LocalRebuildLimits,
  sourceSize: number,
): DiagnosticBuiltManifest {
  if (intrinsicByteLength(old.root) !== ROOT_ENVELOPE_BYTES)
    throw new Error("diagnostic manifest root must contain exactly 68 bytes");
  if (intrinsicByteLength(old.rootHash) !== 32)
    throw new Error("diagnostic manifest root hash must contain 32 bytes");
  const root = copyBytes(old.root);
  const rootHash = copyBytes(old.rootHash);
  const decodedRoot = decodeManifestRoot(root, rootHash);
  validateSupportedManifestParameters(decodedRoot.parameters);
  if (decodedRoot.fileSize !== sourceSize)
    throw new Error("source size does not match old manifest root");
  if (
    decodedRoot.entryCount > limits.maxRetainedEntries ||
    old.entries.length > limits.maxRetainedEntries ||
    old.nodes.size > limits.maxRetainedNodes
  )
    throw new LocalRebuildLimitError(
      "diagnostic local-rebuild state exceeds its fixed retained-entry/node limit; use the streamed workspace fallback",
    );
  const sourceNodes = old.nodes;
  const authenticatedBytes = new Map<string, Uint8Array>();
  const reachable = new Set<string>();
  let nodeVisits = 0;
  const reader = {
    get(hash: Uint8Array): Uint8Array | undefined {
      nodeVisits = checkedAdd(nodeVisits, 1, "diagnostic manifest node visits");
      if (nodeVisits > limits.maxRetainedNodes)
        throw new LocalRebuildLimitError(
          "diagnostic manifest traversal exceeds its node-visit limit; use the streamed workspace fallback",
        );
      const key = bytesToHex(hash);
      const cached = sourceNodes.get(key);
      if (!cached) return undefined;
      if (intrinsicByteLength(cached.hash) !== 32)
        throw new Error("diagnostic manifest cached node hash must contain 32 bytes");
      if (intrinsicByteLength(cached.encoded) > MAX_MANIFEST_NODE_BYTES)
        throw new Error("diagnostic manifest cached node exceeds the v1 byte maximum");
      const encoded = copyBytes(cached.encoded);
      if (bytesToHex(cached.hash) !== key)
        throw new Error("diagnostic manifest cached node hash differs from its key");
      let cachedEncoding: Uint8Array;
      try {
        cachedEncoding = encodeManifestNode(cached.node);
      } catch {
        throw new Error("diagnostic manifest cached node is malformed");
      }
      if (!equalBytes(cachedEncoding, encoded))
        throw new Error(
          "diagnostic manifest cached node differs from authenticated bytes",
        );
      reachable.add(key);
      authenticatedBytes.set(key, encoded);
      return copyBytes(encoded);
    },
  };
  const authenticatedEntries: ManifestEntry[] = [];
  const cursor = new ManifestSequentialCursor(root, 0, reader, rootHash, 32);
  for (let record = cursor.next(); record; record = cursor.next())
    authenticatedEntries.push(
      Object.freeze({
        hash: copyBytes(record.entry.hash),
        length: record.entry.length,
      }),
    );
  if (authenticatedEntries.length !== decodedRoot.entryCount)
    throw new Error("diagnostic manifest authenticated entry count mismatch");
  if (reachable.size !== sourceNodes.size)
    throw new Error("diagnostic manifest contains unreachable cached nodes");
  if (old.entries.length !== authenticatedEntries.length)
    throw new Error("diagnostic manifest cached entry stream length mismatch");
  for (let index = 0; index < authenticatedEntries.length; index += 1) {
    const cached = old.entries[index]!;
    const authenticated = authenticatedEntries[index]!;
    if (
      cached.length !== authenticated.length ||
      !equalBytes(cached.hash, authenticated.hash)
    )
      throw new Error(
        "diagnostic manifest cached entry differs from authenticated tree",
      );
  }
  if (old.id !== manifestIdFromHash(rootHash))
    throw new Error("diagnostic manifest cached identifier differs from root hash");
  const authenticatedNodes = new Map<string, EncodedManifestNode>();
  for (const [key, encoded] of authenticatedBytes) {
    const cached = sourceNodes.get(key)!;
    const hash = copyBytes(cached.hash);
    authenticatedNodes.set(
      key,
      Object.freeze({ hash, encoded, node: decodeManifestNode(encoded, hash) }),
    );
  }
  return Object.freeze({
    id: old.id,
    rootHash,
    root,
    nodes: authenticatedNodes,
    entries: Object.freeze(authenticatedEntries),
  });
}

/** Fixed-size diagnostic helper; it is not a storage-scale incremental-edit path. */
function rebuildDiagnosticManifestLocallyOwned(
  source: RandomAccessContentSource,
  old: DiagnosticBuiltManifest,
  edit: LocalContentEdit,
  limits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
): LocallyRebuiltManifest {
  const sourceSize = source.size;
  const editOffset = edit.offset;
  const deleteLength = edit.deleteLength;
  const callerInsertBytes = edit.insertBytes;
  if (!Number.isSafeInteger(sourceSize) || sourceSize < 0)
    throw new RangeError("source size must be a nonnegative safe integer");
  if (
    !Number.isSafeInteger(editOffset) ||
    editOffset < 0 ||
    !Number.isSafeInteger(deleteLength) ||
    deleteLength < 0 ||
    editOffset > sourceSize ||
    deleteLength > sourceSize - editOffset
  )
    throw new RangeError("local edit is outside the source");
  if (!(callerInsertBytes instanceof Uint8Array))
    throw new TypeError("local edit insertion must be a Uint8Array");
  const newSize = checkedAdd(sourceSize - deleteLength, callerInsertBytes.byteLength);
  if (
    sourceSize > MAX_DIAGNOSTIC_CONTENT_BYTES ||
    newSize > MAX_DIAGNOSTIC_CONTENT_BYTES
  )
    throw new LocalRebuildLimitError(
      "diagnostic local rebuild exceeds its fixed content-size cap; use the streamed workspace fallback",
    );
  old = authenticateDiagnosticManifest(old, limits, sourceSize);
  const oldRoot = decodeManifestRoot(old.root, old.rootHash);
  validateSupportedManifestParameters(oldRoot.parameters);
  if (callerInsertBytes.byteLength > limits.maxAffectedBytes)
    throw new LocalRebuildLimitError(
      "local edit insertion exceeds the affected-byte window; use the streamed workspace fallback",
    );
  const insertBytes = callerInsertBytes;
  edit = Object.freeze({ offset: editOffset, deleteLength, insertBytes });
  const oldLayout = entryOffsets(old.entries);
  if (oldLayout.size !== sourceSize)
    throw new Error("source size does not match old manifest entries");
  if (oldRoot.fileSize !== sourceSize || oldRoot.entryCount !== old.entries.length)
    throw new Error("old manifest totals do not match its entry stream");
  if (edit.deleteLength === 0 && insertBytes.byteLength === 0) {
    return Object.freeze({
      rootHash: old.rootHash,
      root: old.root,
      fileSize: sourceSize,
      entryCount: old.entries.length,
      entrySplice: Object.freeze({
        start: 0,
        deleteCount: 0,
        entries: Object.freeze([]),
      }),
      affectedObjects: new Map(),
      newNodes: new Map(),
      metrics: Object.freeze({
        sourceBytesRead: 0,
        bytesHashed: 0,
        scanWindowBytes: 0,
        reconnectOldOffset: edit.offset,
        reconnectNewOffset: edit.offset,
        reusedPrefixEntries: old.entries.length,
        reusedSuffixEntries: 0,
        affectedEntryCount: 0,
        newObjectCount: 0,
        newManifestNodeCount: 0,
        reusedManifestNodeCount: old.nodes.size,
        fellBackToEnd: false,
        insertionCopyCount: 1,
        insertionBytesCopied: insertBytes.byteLength,
        chunkerInputBytesCopied: 0,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 0,
      }),
    });
  }

  const delta = insertBytes.byteLength - edit.deleteLength;
  const locatedStart = containingEntry(oldLayout.offsets, edit.offset);
  // EOF is a forced FastCDC boundary. Appending must reopen the final chunk
  // because more bytes can move that boundary.
  const startEntry =
    edit.offset === sourceSize && insertBytes.byteLength > 0 && old.entries.length > 0
      ? old.entries.length - 1
      : locatedStart;
  const scanStart = oldLayout.offsets[startEntry]!;
  const dirtyOldEnd = edit.offset + edit.deleteLength;
  const dirtyNewEnd = edit.offset + insertBytes.byteLength;
  let sourceBytesRead = 0;
  let largestSourceRead = 0;
  let editedInputBytesPrepared = 0;
  const readOld = (offset: number, length: number): Uint8Array => {
    if (length === 0) return new Uint8Array();
    const bytes = source.read(offset, length);
    if (!(bytes instanceof Uint8Array) || intrinsicByteLength(bytes) !== length)
      throw new Error("random-access source returned a partial range");
    sourceBytesRead = checkedAdd(sourceBytesRead, length);
    largestSourceRead = Math.max(largestSourceRead, length);
    return bytes;
  };
  const readEdited = (position: number, length: number): Uint8Array => {
    editedInputBytesPrepared = checkedAdd(
      editedInputBytesPrepared,
      length,
      "prepared edited-input bytes",
    );
    const output = new Uint8Array(length);
    let written = 0;
    let cursor = position;
    while (written < length) {
      if (cursor < edit.offset) {
        const count = Math.min(length - written, edit.offset - cursor);
        output.set(readOld(cursor, count), written);
        cursor += count;
        written += count;
      } else if (cursor < dirtyNewEnd) {
        const insertionOffset = cursor - edit.offset;
        const count = Math.min(
          length - written,
          insertBytes.byteLength - insertionOffset,
        );
        output.set(
          insertBytes.subarray(insertionOffset, insertionOffset + count),
          written,
        );
        cursor += count;
        written += count;
      } else {
        const oldOffset = cursor - delta;
        const count = length - written;
        output.set(readOld(oldOffset, count), written);
        cursor += count;
        written += count;
      }
    }
    return output;
  };

  const oldObjectIds = new Set(old.entries.map((entry) => bytesToHex(entry.hash)));
  const affectedEntries: ManifestEntry[] = [];
  const affectedObjects = new Map<string, Uint8Array>();
  let bytesHashed = 0;
  let newObjectCount = 0;
  let newCursor = scanStart;
  let feedCursor = scanStart;
  let reconnectOldOffset: number | undefined;
  let reconnectEntry: number | undefined;
  const acceptReconnect = (): boolean => {
    if (newCursor < dirtyNewEnd) return false;
    const mappedOld = newCursor - delta;
    if (mappedOld < dirtyOldEnd) return false;
    const entry = oldLayout.boundary.get(mappedOld);
    if (entry === undefined) return false;
    reconnectOldOffset = mappedOld;
    reconnectEntry = entry;
    return true;
  };
  acceptReconnect();
  const chunker = new StreamingFastCdc(oldRoot.parameters);
  const attemptMetrics = (): Readonly<LocalRebuildAttemptMetrics> => {
    const metrics = chunker.metrics;
    return Object.freeze({
      sourceBytesRead,
      bytesHashed,
      largestSourceRead,
      chunkerInputBytesCopied: metrics.inputBytesCopied,
      chunkerOutputBytesCopied: metrics.outputBytesCopied,
      chunkerBoundaryBytesScanned: metrics.boundaryBytesScanned,
      editedInputBytesPrepared,
    });
  };
  const reconnected = Object.freeze({ kind: "reconnected" });
  const affectedLimit = Object.freeze({ kind: "affected-limit" });
  let affectedLimitMessage = "";
  const acceptChunk = (chunk: Uint8Array): void => {
    if (affectedEntries.length >= limits.maxAffectedEntries) {
      affectedLimitMessage =
        "local reconnection exceeds its affected-entry limit; use the streamed workspace fallback";
      throw affectedLimit;
    }
    if (chunk.byteLength > limits.maxAffectedBytes - bytesHashed) {
      affectedLimitMessage =
        "local reconnection exceeds its affected-byte limit; use the streamed workspace fallback";
      throw affectedLimit;
    }
    const hash = sha256(chunk);
    const key = bytesToHex(hash);
    bytesHashed = checkedAdd(bytesHashed, chunk.byteLength);
    affectedEntries.push(Object.freeze({ hash, length: chunk.byteLength }));
    const firstAffectedOccurrence = !affectedObjects.has(key);
    if (firstAffectedOccurrence) affectedObjects.set(key, chunk);
    if (firstAffectedOccurrence && !oldObjectIds.has(key)) newObjectCount += 1;
    newCursor += chunk.byteLength;
    if (acceptReconnect()) throw reconnected;
  };
  while (feedCursor < newSize && reconnectEntry === undefined) {
    if (affectedEntries.length >= limits.maxAffectedEntries)
      throw new LocalRebuildLimitError(
        "local reconnection exceeds its affected-entry limit; use the streamed workspace fallback",
        attemptMetrics(),
      );
    const remainingByteBudget = limits.maxAffectedBytes - bytesHashed;
    const budgetProbe =
      remainingByteBudget === Number.MAX_SAFE_INTEGER
        ? remainingByteBudget
        : remainingByteBudget + 1;
    const inputLength = Math.min(
      oldRoot.parameters.maximum,
      newSize - feedCursor,
      budgetProbe,
    );
    if (inputLength <= 0)
      throw new LocalRebuildLimitError(
        "local reconnection exceeds its affected-byte limit; use the streamed workspace fallback",
        attemptMetrics(),
      );
    const input = readEdited(feedCursor, inputLength);
    feedCursor += inputLength;
    try {
      chunker.drain(input, acceptChunk, feedCursor === newSize);
    } catch (error) {
      if (error === reconnected) break;
      if (error === affectedLimit)
        throw new LocalRebuildLimitError(affectedLimitMessage, attemptMetrics());
      throw error;
    }
    if (bytesHashed + chunker.bufferedBytes > limits.maxAffectedBytes)
      throw new LocalRebuildLimitError(
        "local reconnection exceeds its affected-byte limit; use the streamed workspace fallback",
        attemptMetrics(),
      );
  }
  if (reconnectEntry === undefined || reconnectOldOffset === undefined)
    throw new Error("local FastCDC scan did not reconnect at end of file");

  const entrySplice = Object.freeze({
    start: startEntry,
    deleteCount: reconnectEntry - startEntry,
    entries: Object.freeze(affectedEntries),
  });
  const finalEntryCount = checkedAdd(
    old.entries.length - entrySplice.deleteCount,
    entrySplice.entries.length,
    "locally rebuilt entry count",
  );
  if (finalEntryCount > limits.maxRetainedEntries)
    throw new LocalRebuildLimitError(
      "local result exceeds its retained-entry limit; use the streamed workspace fallback",
      attemptMetrics(),
    );
  const levels = orderedLevels(old);
  const newNodes = new Map<string, EncodedManifestNode>();
  let levelIndex = 0;
  let oldRecords: readonly RecordValue[] = old.entries;
  let oldNodes = levels[0] ?? [];
  let spliceStart = entrySplice.start;
  let spliceEnd = entrySplice.start + entrySplice.deleteCount;
  let replacement: readonly RecordValue[] = entrySplice.entries;
  let rebuilt = regroupLevel(
    levelIndex,
    oldRecords,
    oldNodes,
    spliceStart,
    spliceEnd,
    replacement,
    old,
    newNodes,
  );
  let reusedManifestNodeCount =
    rebuilt.prefixGroupCount +
    (rebuilt.oldGroups.length - rebuilt.reconnectGroup) +
    rebuilt.segment.filter((node) => old.nodes.has(bytesToHex(node.hash))).length;
  while (rebuilt.totalGroupCount > 1) {
    levelIndex += 1;
    oldRecords = (levels[levelIndex - 1] ?? []).map(toChild);
    oldNodes = levels[levelIndex] ?? [];
    spliceStart = rebuilt.prefixGroupCount;
    spliceEnd = rebuilt.reconnectGroup;
    replacement = rebuilt.segment.map(toChild);
    if (oldNodes.length === 0) {
      if (spliceStart !== 0 || spliceEnd !== oldRecords.length)
        throw new Error(
          `local manifest height growth retained an unexpected outer segment at level ${levelIndex}: ${spliceStart}:${spliceEnd}/${oldRecords.length}`,
        );
      oldRecords = [];
      spliceStart = 0;
      spliceEnd = 0;
    }
    rebuilt = regroupLevel(
      levelIndex,
      oldRecords,
      oldNodes,
      spliceStart,
      spliceEnd,
      replacement,
      old,
      newNodes,
    );
    reusedManifestNodeCount +=
      rebuilt.prefixGroupCount +
      (rebuilt.oldGroups.length - rebuilt.reconnectGroup) +
      rebuilt.segment.filter((node) => old.nodes.has(bytesToHex(node.hash))).length;
  }
  const rootNode = onlyNode(rebuilt);
  const entryCount = finalEntryCount;
  const root = encodeManifestRoot({
    parameters: oldRoot.parameters,
    fileSize: newSize,
    entryCount,
    rootNodeHash: rootNode.hash,
  });
  const rootHash = sha256(root);
  return Object.freeze({
    rootHash,
    root,
    fileSize: newSize,
    entryCount,
    entrySplice,
    affectedObjects,
    newNodes,
    metrics: Object.freeze({
      sourceBytesRead,
      bytesHashed,
      scanWindowBytes: oldRoot.parameters.maximum,
      reconnectOldOffset,
      reconnectNewOffset: newCursor,
      reusedPrefixEntries: startEntry,
      reusedSuffixEntries: old.entries.length - reconnectEntry,
      affectedEntryCount: affectedEntries.length,
      newObjectCount,
      newManifestNodeCount: newNodes.size,
      reusedManifestNodeCount,
      fellBackToEnd: reconnectOldOffset === sourceSize && dirtyOldEnd < sourceSize,
      insertionCopyCount: 1,
      insertionBytesCopied: insertBytes.byteLength,
      chunkerInputBytesCopied: chunker.metrics.inputBytesCopied,
      chunkerOutputBytesCopied: chunker.metrics.outputBytesCopied,
      chunkerBoundaryBytesScanned: chunker.metrics.boundaryBytesScanned,
      editedInputBytesPrepared,
    }),
  });
}

export function rebuildDiagnosticManifestLocally(
  source: RandomAccessContentSource,
  old: DiagnosticBuiltManifest,
  edit: LocalContentEdit,
  limits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
): LocallyRebuiltManifest {
  limits = snapshotLocalRebuildLimits(limits);
  const validated = validateLocalContentInputs(source, edit);
  const preflightRoot = decodeManifestRoot(old.root, old.rootHash);
  validateSupportedManifestParameters(preflightRoot.parameters);
  if (preflightRoot.fileSize !== validated.source.size)
    throw new Error("source size does not match old manifest root");
  if (
    preflightRoot.entryCount > limits.maxRetainedEntries ||
    old.entries.length > limits.maxRetainedEntries ||
    old.nodes.size > limits.maxRetainedNodes
  )
    throw new LocalRebuildLimitError(
      "diagnostic local-rebuild state exceeds its fixed retained-entry/node limit; use the streamed workspace fallback",
    );
  const owned = ownLocalContentInputs(validated);
  return rebuildDiagnosticManifestLocallyOwned(owned.source, old, owned.edit, limits);
}

export function applyEntrySplice(
  entries: readonly ManifestEntry[],
  splice: ManifestEntrySplice,
  maxEntries = DEFAULT_LOCAL_REBUILD_LIMITS.maxRetainedEntries,
): ManifestEntry[] {
  if (
    !Number.isSafeInteger(splice.start) ||
    splice.start < 0 ||
    !Number.isSafeInteger(splice.deleteCount) ||
    splice.deleteCount < 0 ||
    splice.start + splice.deleteCount > entries.length
  )
    throw new RangeError("invalid manifest entry splice");
  if (entries.length - splice.deleteCount + splice.entries.length > maxEntries)
    throw new LocalRebuildLimitError(
      "entry splice exceeds its fixed in-memory limit; use a streamed workspace",
    );
  return [
    ...entries.slice(0, splice.start),
    ...splice.entries,
    ...entries.slice(splice.start + splice.deleteCount),
  ];
}

export function rebuildManifestLocallyWithParameters(
  source: RandomAccessContentSource,
  old: DiagnosticBuiltManifest,
  edit: LocalContentEdit,
  parameters: FastCdcConfiguration,
  limits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
): LocallyRebuiltManifest {
  parameters = snapshotMatchingLocalParameters(old, parameters);
  limits = snapshotLocalRebuildLimits(limits);
  const owned = ownLocalContentInputs(validateLocalContentInputs(source, edit));
  return rebuildManifestLocallyWithParametersOwned(
    owned.source,
    old,
    owned.edit,
    parameters,
    limits,
  );
}

/** Operations-internal entry: source/edit have already crossed the ownership boundary. */
export function rebuildManifestLocallyWithParametersOwned(
  source: RandomAccessContentSource,
  old: DiagnosticBuiltManifest,
  edit: LocalContentEdit,
  parameters: FastCdcConfiguration,
  limits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
): LocallyRebuiltManifest {
  snapshotMatchingLocalParameters(old, parameters);
  limits = snapshotLocalRebuildLimits(limits);
  return rebuildDiagnosticManifestLocallyOwned(source, old, edit, limits);
}

export function snapshotMatchingLocalParameters(
  old: DiagnosticBuiltManifest,
  parameters: FastCdcConfiguration,
): Readonly<FastCdcConfiguration> {
  const owned = Object.freeze({
    minimum: parameters.minimum,
    average: parameters.average,
    maximum: parameters.maximum,
  });
  validateSupportedManifestParameters(owned);
  const root = decodeManifestRoot(old.root, old.rootHash);
  if (
    root.parameters.minimum !== owned.minimum ||
    root.parameters.average !== owned.average ||
    root.parameters.maximum !== owned.maximum
  )
    throw new Error("local rebuild parameters must match the old manifest");
  return owned;
}

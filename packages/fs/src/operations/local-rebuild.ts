import { sha256 } from "../cas/sha256.js";
import { findFastCdcBoundary, type FastCdcConfiguration } from "../cdc/fastcdc.js";
import { decodeManifestRoot, encodeManifestNode, encodeManifestRoot, type ManifestChild, type ManifestEntry, type ManifestInternal, type ManifestLeaf, type ManifestNode } from "../manifests/codec.js";
import type { BuiltManifest, EncodedManifestNode } from "../manifests/builder.js";
import { bytesToHex } from "../cas/bytes.js";
import { checkedAdd } from "../resources/safe-integers.js";
import { advanceManifestGroupingState, isManifestGroupBoundary } from "../manifests/grouping.js";

export interface RandomAccessContentSource {
  readonly size: number;
  read(offset: number, length: number): Uint8Array;
}

export interface LocalContentEdit {
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertBytes: Uint8Array;
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
  return Object.freeze({ hash: node.hash, span: node.node.span, entryCount: node.node.entryCount });
}

function nodeRecordCount(node: ManifestNode, leaf: boolean): number {
  if (leaf && node.kind !== "leaf") throw new Error("old manifest level contains a non-leaf node");
  if (!leaf && node.kind !== "internal") throw new Error("old manifest level contains a leaf node");
  return node.kind === "leaf" ? node.entries.length : node.children.length;
}

function groupBounds(nodes: readonly EncodedManifestNode[], leaf: boolean, recordCount: number): GroupBounds[] {
  const result: GroupBounds[] = [];
  let cursor = 0;
  for (const node of nodes) {
    const count = nodeRecordCount(node.node, leaf);
    result.push(Object.freeze({ start: cursor, end: checkedAdd(cursor, count), node }));
    cursor += count;
  }
  if (cursor !== recordCount) throw new Error("old manifest level record count mismatch");
  return result;
}

function startGroupFor(groups: readonly GroupBounds[], recordIndex: number, recordCount: number): number {
  if (groups.length === 1 && groups[0]!.start === 0 && groups[0]!.end === 0 && recordIndex === 0) return 0;
  for (let index = 0; index < groups.length; index += 1) {
    const group = groups[index]!;
    if (recordIndex === group.start || (recordIndex > group.start && recordIndex < group.end)) return index;
  }
  if (recordIndex === recordCount) return groups.length;
  throw new RangeError("manifest splice does not lie inside the old level");
}

function reconnectGroupFor(groups: readonly GroupBounds[], recordIndex: number, recordCount: number): number | undefined {
  if (recordIndex === recordCount) return groups.length;
  for (let index = 0; index < groups.length; index += 1) if (groups[index]!.start === recordIndex) return index;
  return undefined;
}

function makeNode(level: number, records: readonly RecordValue[], old: BuiltManifest, newNodes: Map<string, EncodedManifestNode>): EncodedManifestNode {
  let node: ManifestNode;
  if (level === 0) {
    const entries = records as readonly ManifestEntry[];
    node = Object.freeze({ kind: "leaf", span: entries.reduce((sum, entry) => checkedAdd(sum, entry.length), 0), entryCount: entries.length, entries: Object.freeze([...entries]) } satisfies ManifestLeaf);
  } else {
    const children = records as readonly ManifestChild[];
    node = Object.freeze({ kind: "internal", span: children.reduce((sum, child) => checkedAdd(sum, child.span), 0), entryCount: children.reduce((sum, child) => checkedAdd(sum, child.entryCount), 0), children: Object.freeze([...children]) } satisfies ManifestInternal);
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
  old: BuiltManifest,
  newNodes: Map<string, EncodedManifestNode>,
): RegroupedLevel {
  if (spliceStart < 0 || spliceEnd < spliceStart || spliceEnd > oldRecords.length) throw new RangeError("invalid manifest level splice");
  const bounds = groupBounds(oldNodes, level === 0, oldRecords.length);
  const startGroup = startGroupFor(bounds, spliceStart, oldRecords.length);
  const groupStart = startGroup === bounds.length ? oldRecords.length : bounds[startGroup]!.start;
  const minimumReconnect = bounds[startGroup]?.start === bounds[startGroup]?.end ? startGroup + 1 : startGroup;
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
    group = []; state = 0n;
    const logicalOldCursor = replacementCursor === replacement.length && oldCursor <= spliceEnd ? spliceEnd : oldCursor;
    if (replacementCursor === replacement.length && logicalOldCursor >= spliceEnd) {
      const candidate = reconnectGroupFor(bounds, logicalOldCursor, oldRecords.length);
      if (candidate !== undefined && candidate >= minimumReconnect) { reconnectGroup = candidate; return true; }
    }
    return false;
  };

  let stopped = false;
  while (!stopped) {
    let record: RecordValue | undefined;
    if (oldCursor < spliceStart) record = oldRecords[oldCursor++];
    else if (replacementCursor < replacement.length) record = replacement[replacementCursor++];
    else {
      if (oldCursor < spliceEnd) oldCursor = spliceEnd;
      if (oldCursor < oldRecords.length) record = oldRecords[oldCursor++];
    }
    if (!record) break;
    group.push(record);
    state = advanceManifestGroupingState(state, record);
    if (isManifestGroupBoundary(group.length, state, minimum, target, maximum)) stopped = emit();
  }
  if (!stopped && group.length > 0) emit();
  if (level === 0 && startGroup === 0 && reconnectGroup === bounds.length && segment.length === 0) segment.push(makeNode(0, [], old, newNodes));
  const totalGroupCount = startGroup + segment.length + (bounds.length - reconnectGroup);
  return Object.freeze({ oldGroups: bounds, prefixGroupCount: startGroup, reconnectGroup, segment: Object.freeze(segment), totalGroupCount });
}

function onlyNode(level: RegroupedLevel): EncodedManifestNode {
  if (level.totalGroupCount !== 1) throw new Error("manifest level is not singular");
  if (level.prefixGroupCount === 1) return level.oldGroups[0]!.node;
  if (level.segment.length === 1) return level.segment[0]!;
  if (level.reconnectGroup < level.oldGroups.length) return level.oldGroups[level.reconnectGroup]!.node;
  throw new Error("manifest level lost its root node");
}

function orderedLevels(old: BuiltManifest): EncodedManifestNode[][] {
  const root = decodeManifestRoot(old.root, old.rootHash);
  const levels: EncodedManifestNode[][] = [];
  const visit = (hash: Uint8Array, depth: number): number => {
    if (depth > 32) throw new Error("manifest tree is too deep");
    const node = old.nodes.get(bytesToHex(hash));
    if (!node) throw new Error("old manifest is missing an authenticated node");
    if (node.node.kind === "leaf") { (levels[0] ??= []).push(node); return 0; }
    let height: number | undefined;
    for (const child of node.node.children) {
      const childHeight = visit(child.hash, depth + 1);
      if (height !== undefined && height !== childHeight) throw new Error("manifest tree levels are unbalanced");
      height = childHeight;
    }
    const actual = (height ?? -1) + 1;
    (levels[actual] ??= []).push(node);
    return actual;
  };
  visit(root.rootNodeHash, 1);
  return levels;
}

function entryOffsets(entries: readonly ManifestEntry[]): { readonly offsets: readonly number[]; readonly boundary: ReadonlyMap<number, number>; readonly size: number } {
  const offsets = [0];
  const boundary = new Map<number, number>([[0, 0]]);
  let size = 0;
  for (let index = 0; index < entries.length; index += 1) {
    size = checkedAdd(size, entries[index]!.length);
    offsets.push(size); boundary.set(size, index + 1);
  }
  return { offsets: Object.freeze(offsets), boundary, size };
}

function containingEntry(offsets: readonly number[], offset: number): number {
  let low = 0; let high = offsets.length - 1;
  while (low < high) { const middle = Math.floor((low + high) / 2); if (offsets[middle]! < offset) low = middle + 1; else high = middle; }
  if (offsets[low] === offset) return low;
  return Math.max(0, low - 1);
}

export function rebuildManifestLocally(source: RandomAccessContentSource, old: BuiltManifest, edit: LocalContentEdit): LocallyRebuiltManifest {
  if (!Number.isSafeInteger(source.size) || source.size < 0) throw new RangeError("source size must be a nonnegative safe integer");
  if (!Number.isSafeInteger(edit.offset) || edit.offset < 0 || !Number.isSafeInteger(edit.deleteLength) || edit.deleteLength < 0 || edit.offset > source.size || edit.deleteLength > source.size - edit.offset) throw new RangeError("local edit is outside the source");
  const insertBytes = edit.insertBytes.slice();
  const oldLayout = entryOffsets(old.entries);
  if (oldLayout.size !== source.size) throw new Error("source size does not match old manifest entries");
  const oldRoot = decodeManifestRoot(old.root, old.rootHash);
  if (oldRoot.fileSize !== source.size || oldRoot.entryCount !== old.entries.length) throw new Error("old manifest totals do not match its entry stream");
  if (edit.deleteLength === 0 && insertBytes.byteLength === 0) {
    return Object.freeze({ rootHash: old.rootHash, root: old.root, fileSize: source.size, entryCount: old.entries.length, entrySplice: Object.freeze({ start: 0, deleteCount: 0, entries: Object.freeze([]) }), affectedObjects: new Map(), newNodes: new Map(), metrics: Object.freeze({ sourceBytesRead: 0, bytesHashed: 0, scanWindowBytes: 0, reconnectOldOffset: edit.offset, reconnectNewOffset: edit.offset, reusedPrefixEntries: old.entries.length, reusedSuffixEntries: 0, affectedEntryCount: 0, newObjectCount: 0, newManifestNodeCount: 0, reusedManifestNodeCount: old.nodes.size, fellBackToEnd: false }) });
  }

  const delta = insertBytes.byteLength - edit.deleteLength;
  const newSize = checkedAdd(source.size - edit.deleteLength, insertBytes.byteLength);
  const locatedStart = containingEntry(oldLayout.offsets, edit.offset);
  // EOF is a forced FastCDC boundary. Appending must reopen the final chunk
  // because more bytes can move that boundary.
  const startEntry = edit.offset === source.size && insertBytes.byteLength > 0 && old.entries.length > 0 ? old.entries.length - 1 : locatedStart;
  const scanStart = oldLayout.offsets[startEntry]!;
  const dirtyOldEnd = edit.offset + edit.deleteLength;
  const dirtyNewEnd = edit.offset + insertBytes.byteLength;
  let sourceBytesRead = 0;
  const readOld = (offset: number, length: number): Uint8Array => {
    if (length === 0) return new Uint8Array();
    const bytes = source.read(offset, length);
    if (!(bytes instanceof Uint8Array) || bytes.byteLength !== length) throw new Error("random-access source returned a partial range");
    sourceBytesRead = checkedAdd(sourceBytesRead, length);
    return bytes;
  };
  const readEdited = (position: number, length: number): Uint8Array => {
    const output = new Uint8Array(length);
    let written = 0; let cursor = position;
    while (written < length) {
      if (cursor < edit.offset) {
        const count = Math.min(length - written, edit.offset - cursor);
        output.set(readOld(cursor, count), written); cursor += count; written += count;
      } else if (cursor < dirtyNewEnd) {
        const insertionOffset = cursor - edit.offset;
        const count = Math.min(length - written, insertBytes.byteLength - insertionOffset);
        output.set(insertBytes.subarray(insertionOffset, insertionOffset + count), written); cursor += count; written += count;
      } else {
        const oldOffset = cursor - delta;
        const count = length - written;
        output.set(readOld(oldOffset, count), written); cursor += count; written += count;
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
  let reconnectOldOffset: number | undefined;
  let reconnectEntry: number | undefined;
  const acceptReconnect = (): boolean => {
    if (newCursor < dirtyNewEnd) return false;
    const mappedOld = newCursor - delta;
    if (mappedOld < dirtyOldEnd) return false;
    const entry = oldLayout.boundary.get(mappedOld);
    if (entry === undefined) return false;
    reconnectOldOffset = mappedOld; reconnectEntry = entry; return true;
  };
  acceptReconnect();
  while (newCursor < newSize && reconnectEntry === undefined) {
    const window = readEdited(newCursor, Math.min(oldRoot.parameters.maximum, newSize - newCursor));
    const boundary = findFastCdcBoundary(window, 0, oldRoot.parameters);
    const chunk = window.slice(0, boundary);
    const hash = sha256(chunk); const key = bytesToHex(hash);
    bytesHashed = checkedAdd(bytesHashed, chunk.byteLength);
    affectedEntries.push(Object.freeze({ hash, length: chunk.byteLength }));
    const firstAffectedOccurrence = !affectedObjects.has(key);
    if (firstAffectedOccurrence) affectedObjects.set(key, chunk);
    if (firstAffectedOccurrence && !oldObjectIds.has(key)) newObjectCount += 1;
    newCursor += boundary;
    acceptReconnect();
  }
  if (reconnectEntry === undefined || reconnectOldOffset === undefined) throw new Error("local FastCDC scan did not reconnect at end of file");

  const entrySplice = Object.freeze({ start: startEntry, deleteCount: reconnectEntry - startEntry, entries: Object.freeze(affectedEntries) });
  const levels = orderedLevels(old);
  const newNodes = new Map<string, EncodedManifestNode>();
  let levelIndex = 0;
  let oldRecords: readonly RecordValue[] = old.entries;
  let oldNodes = levels[0] ?? [];
  let spliceStart = entrySplice.start;
  let spliceEnd = entrySplice.start + entrySplice.deleteCount;
  let replacement: readonly RecordValue[] = entrySplice.entries;
  let rebuilt = regroupLevel(levelIndex, oldRecords, oldNodes, spliceStart, spliceEnd, replacement, old, newNodes);
  let reusedManifestNodeCount = rebuilt.prefixGroupCount + (rebuilt.oldGroups.length - rebuilt.reconnectGroup) + rebuilt.segment.filter((node) => old.nodes.has(bytesToHex(node.hash))).length;
  while (rebuilt.totalGroupCount > 1) {
    levelIndex += 1;
    oldRecords = (levels[levelIndex - 1] ?? []).map(toChild);
    oldNodes = levels[levelIndex] ?? [];
    spliceStart = rebuilt.prefixGroupCount;
    spliceEnd = rebuilt.reconnectGroup;
    replacement = rebuilt.segment.map(toChild);
    if (oldNodes.length === 0) {
      if (spliceStart !== 0 || spliceEnd !== oldRecords.length) throw new Error(`local manifest height growth retained an unexpected outer segment at level ${levelIndex}: ${spliceStart}:${spliceEnd}/${oldRecords.length}`);
      oldRecords = [];
      spliceStart = 0; spliceEnd = 0;
    }
    rebuilt = regroupLevel(levelIndex, oldRecords, oldNodes, spliceStart, spliceEnd, replacement, old, newNodes);
    reusedManifestNodeCount += rebuilt.prefixGroupCount + (rebuilt.oldGroups.length - rebuilt.reconnectGroup) + rebuilt.segment.filter((node) => old.nodes.has(bytesToHex(node.hash))).length;
  }
  const rootNode = onlyNode(rebuilt);
  const entryCount = old.entries.length - entrySplice.deleteCount + entrySplice.entries.length;
  const root = encodeManifestRoot({ parameters: oldRoot.parameters, fileSize: newSize, entryCount, rootNodeHash: rootNode.hash });
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
      fellBackToEnd: reconnectOldOffset === source.size && dirtyOldEnd < source.size,
    }),
  });
}

export function applyEntrySplice(entries: readonly ManifestEntry[], splice: ManifestEntrySplice): ManifestEntry[] {
  if (!Number.isSafeInteger(splice.start) || splice.start < 0 || !Number.isSafeInteger(splice.deleteCount) || splice.deleteCount < 0 || splice.start + splice.deleteCount > entries.length) throw new RangeError("invalid manifest entry splice");
  return [...entries.slice(0, splice.start), ...splice.entries, ...entries.slice(splice.start + splice.deleteCount)];
}

export function rebuildManifestLocallyWithParameters(source: RandomAccessContentSource, old: BuiltManifest, edit: LocalContentEdit, parameters: FastCdcConfiguration): LocallyRebuiltManifest {
  const root = decodeManifestRoot(old.root, old.rootHash);
  if (root.parameters.minimum !== parameters.minimum || root.parameters.average !== parameters.average || root.parameters.maximum !== parameters.maximum) throw new Error("local rebuild parameters must match the old manifest");
  return rebuildManifestLocally(source, old, edit);
}

import { bytesToHex, copyBytes, intrinsicByteLength } from "../cas/bytes.js";
import { type HashFunction } from "../cas/sha256.js";
import { StreamingFastCdc } from "../cdc/fastcdc.js";
import {
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
import type { EncodedManifestNode } from "../manifests/builder.js";
import {
  advanceManifestGroupingState,
  isManifestGroupBoundary,
} from "../manifests/grouping.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";
import {
  type LocalRebuildLimits,
  LocalRebuildLimitError,
  type LocallyRebuiltManifest,
  type LocalRebuildMetrics,
  type RandomAccessContentSource,
} from "./local-rebuild.js";
import type { DiagnosticBuiltManifest } from "./full-rebuild.js";

/**
 * The bounded Merkle-descent rebuild could not complete inside its windowed
 * neighborhood: the FastCDC reconnect ran past the loaded fringe, the regroup
 * outgrew the level windows, or a rebuilt segment node hash-matches an old
 * node whose source path is not on the loaded path. The caller falls back to
 * the full-state local rebuild; this is only a lost optimization, never a
 * correctness path change.
 */
export class BoundedRebuildFallbackError extends Error {
  readonly name = "BoundedRebuildFallbackError";
}

export interface BoundedManifestRoot {
  readonly rootHash: Uint8Array;
  readonly root: Uint8Array;
  readonly parameters: ManifestParameters;
  readonly fileSize: number;
  readonly entryCount: number;
}

/** One frame of an authenticated root-to-leaf descent. */
export interface BoundedPathFrame {
  readonly hash: Uint8Array;
  readonly path: readonly number[];
  readonly offset: number;
  readonly finalAtLevel: boolean;
  readonly node: ManifestNode;
  readonly selectedChildIndex?: number;
}

/**
 * One loaded leaf of the old manifest: its entries, its global start entry
 * index and byte offset, and the leaf-local entry offsets used by the
 * FastCDC reconnection scan.
 */
export interface BoundedLeaf {
  readonly node: ManifestNode;
  readonly path: readonly number[];
  readonly finalAtLevel: boolean;
  readonly startEntryIndex: number;
  readonly leafOffset: number;
  readonly entries: readonly ManifestEntry[];
  /** Leaf-local cumulative byte offsets; `entryOffsets[0] === 0`. */
  readonly entryOffsets: readonly number[];
}

/** A fully deleted leaf between the affected leaf and the dirty-end leaf. */
export interface BoundedLeafBound {
  readonly startEntryIndex: number;
  readonly entryCount: number;
}

/** A loaded internal sibling (the fringe) with its full children array. */
export interface BoundedFringeGroup {
  readonly path: readonly number[];
  readonly children: readonly ManifestChild[];
}

/**
 * The windowed regroup input for one internal level (level 1..rootDepth-1):
 * the affected node's children plus the loaded right-fringe siblings. The
 * regroup only ever touches records inside this window; the reconnect search
 * is bounded by it.
 */
export interface BoundedLevelWindow {
  readonly level: number;
  readonly affectedChildren: readonly ManifestChild[];
  /** Index of the affected level-(level-1) group inside `affectedChildren`. */
  readonly affectedChildIndex: number;
  readonly fringe: readonly BoundedFringeGroup[];
  /** The loaded records cover the true end of the level's record stream. */
  readonly coversTrueEnd: boolean;
}

/** Immutable snapshot of the old-manifest neighborhood for one bounded edit. */
export interface BoundedManifestState {
  readonly root: BoundedManifestRoot;
  /** Number of levels in the old tree (the leaf level is level 0). */
  readonly rootDepth: number;
  /** Leaf-local index of the entry containing `edit.offset`. */
  readonly affectedEntryIndex: number;
  readonly affectedLeaf: BoundedLeaf;
  readonly dirtyEndLeaf: BoundedLeaf;
  /** Fully deleted leaves between the affected leaf and the dirty-end leaf. */
  readonly deletedLeafBounds: readonly BoundedLeafBound[];
  /** Leaves after the dirty-end leaf, loaded for the reconnect window. */
  readonly fringeLeaves: readonly BoundedLeaf[];
  /** True when the fringe crawl reached the last leaf of the file. */
  readonly fringeCoversToEnd: boolean;
  /** Absolute byte offset -> global entry index for every loaded boundary. */
  readonly boundary: ReadonlyMap<number, number>;
  /** Windows for levels 1..rootDepth-1 (indexed by level; index 0 unused). */
  readonly levelWindows: readonly (BoundedLevelWindow | undefined)[];
  /** Old node hash -> source path for every loaded path/fringe child. */
  readonly claimPaths: ReadonlyMap<string, readonly number[]>;
  /** Source-authenticated proof metadata for each retained claim path. */
  readonly claimProofs: ReadonlyMap<
    string,
    {
      readonly sourcePath: readonly number[];
      readonly sourceFinalAtLevel: boolean;
      readonly sourceLeafDelta: number;
    }
  >;
  release(): void;
}

type RecordValue = ManifestEntry | ManifestChild;

interface BoundedGroupBounds {
  /** Window-relative start record indices of the loaded groups. */
  readonly starts: readonly number[];
  /** Record counts of the loaded groups (children/entry counts). */
  readonly counts: readonly number[];
}

interface BoundedRegroupInput {
  readonly oldRecords: readonly RecordValue[];
  /** Window-relative splice start (the first replacement record). */
  readonly spliceStart: number;
  /** Window-relative splice end (the first record after the splice). */
  readonly spliceEnd: number;
  readonly replacement: readonly RecordValue[];
  readonly groupBounds: BoundedGroupBounds;
  /** The window covers the true end of the level's record stream. */
  readonly coversTrueEnd: boolean;
  /** The affected group is the first group of its level. */
  readonly prefixEmpty: boolean;
  /** The affected group contains no records (the empty manifest leaf). */
  readonly emptyAffectedGroup: boolean;
}

interface BoundedRegroupedLevel {
  readonly prefixEmpty: boolean;
  readonly segment: readonly EncodedManifestNode[];
  /**
   * Window position of the reconnect group inside `groupBounds.starts`, or
   * `counts.length` when the reconnect reached the end of the stream.
   */
  readonly reconnectWindowIndex: number;
  /** The reconnect reached the true end of the level's record stream. */
  readonly suffixEmpty: boolean;
}

interface BoundedEditGeometry {
  readonly startEntry: number;
  readonly scanStart: number;
  readonly delta: number;
  readonly dirtyOldEnd: number;
  readonly dirtyNewEnd: number;
}

function toChild(node: EncodedManifestNode): ManifestChild {
  return Object.freeze({
    hash: copyBytes(node.hash),
    span: node.node.span,
    entryCount: node.node.entryCount,
  });
}

function makeNode(
  level: number,
  records: readonly RecordValue[],
  newNodes: Map<string, EncodedManifestNode>,
  hashBytes: HashFunction,
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
  const hash = hashBytes(encoded);
  const key = bytesToHex(hash);
  const existing = newNodes.get(key);
  if (existing) return existing;
  const created = Object.freeze({ hash, encoded, node });
  newNodes.set(key, created);
  return created;
}

/**
 * Mirrors `regroupLevel` in local-rebuild.ts but operates on the windowed
 * record stream and windowed group bounds with window-relative splice
 * positions. The reconnect search is bounded by the loaded groups; when the
 * window runs out before a reconnect and it does not cover the true end of
 * the level, the attempt falls back.
 */
function regroupLevelBounded(
  level: number,
  input: BoundedRegroupInput,
  newNodes: Map<string, EncodedManifestNode>,
  hashBytes: HashFunction,
): BoundedRegroupedLevel {
  if (input.spliceStart < 0 || input.spliceEnd < input.spliceStart)
    throw new BoundedRebuildFallbackError("invalid bounded level splice");
  const minimum = level === 0 ? 64 : 32;
  const target = level === 0 ? 128 : 64;
  const maximum = level === 0 ? 256 : 128;
  const starts = input.groupBounds.starts;
  const counts = input.groupBounds.counts;
  const recordCount = input.oldRecords.length;
  const minimumReconnect = input.emptyAffectedGroup ? 1 : 0;
  const reconnectSearch = (recordIndex: number): number | undefined => {
    for (let index = 0; index < starts.length; index += 1)
      if (starts[index] === recordIndex && index >= minimumReconnect) return index;
    return undefined;
  };
  let oldCursor = 0;
  let replacementCursor = 0;
  let group: RecordValue[] = [];
  let state = 0n;
  let reconnectGroup: number | undefined;
  const segment: EncodedManifestNode[] = [];
  const emit = (): boolean => {
    if (group.length === 0) return false;
    const canonicalBoundary = isManifestGroupBoundary(
      group.length,
      state,
      minimum,
      target,
      maximum,
    );
    segment.push(makeNode(level, group, newNodes, hashBytes));
    group = [];
    state = 0n;
    const logicalOldCursor =
      replacementCursor === input.replacement.length && oldCursor <= input.spliceEnd
        ? input.spliceEnd
        : oldCursor;
    if (
      replacementCursor === input.replacement.length &&
      logicalOldCursor >= input.spliceEnd
    ) {
      if (logicalOldCursor >= recordCount) {
        const truncatedEnd = reconnectSearch(logicalOldCursor);
        if (truncatedEnd !== undefined && canonicalBoundary) {
          reconnectGroup = truncatedEnd;
          return true;
        }
        if (!input.coversTrueEnd)
          throw new BoundedRebuildFallbackError(
            "bounded reconnect ran past the loaded fringe",
          );
        reconnectGroup = counts.length;
        return true;
      }
      const candidate = reconnectSearch(logicalOldCursor);
      if (candidate !== undefined) {
        reconnectGroup = candidate;
        return true;
      }
    }
    return false;
  };

  let stopped = false;
  while (!stopped) {
    let record: RecordValue | undefined;
    if (oldCursor < input.spliceStart) record = input.oldRecords[oldCursor++];
    else if (replacementCursor < input.replacement.length)
      record = input.replacement[replacementCursor++];
    else {
      if (oldCursor < input.spliceEnd) oldCursor = input.spliceEnd;
      if (oldCursor < recordCount) record = input.oldRecords[oldCursor++];
    }
    if (!record) break;
    group.push(record);
    state = advanceManifestGroupingState(state, record);
    if (isManifestGroupBoundary(group.length, state, minimum, target, maximum))
      stopped = emit();
  }
  if (!stopped && group.length > 0) emit();
  if (reconnectGroup === undefined)
    throw new BoundedRebuildFallbackError(
      "bounded regroup exhausted its loaded window before reconnecting",
    );
  if (
    level === 0 &&
    input.prefixEmpty &&
    reconnectGroup === counts.length &&
    segment.length === 0
  )
    segment.push(makeNode(0, [], newNodes, hashBytes));
  return Object.freeze({
    prefixEmpty: input.prefixEmpty,
    segment: Object.freeze(segment),
    reconnectWindowIndex: reconnectGroup,
    suffixEmpty: reconnectGroup === counts.length,
  });
}

/** Number of entries before a path frame's node (its global start entry). */
function prefixEntryCount(
  path: readonly BoundedPathFrame[],
  frame: BoundedPathFrame,
): number {
  let prefix = 0;
  for (const ancestor of path) {
    if (ancestor === frame) break;
    if (ancestor.node.kind !== "internal" || ancestor.selectedChildIndex === undefined)
      break;
    const children = ancestor.node.children;
    for (let index = 0; index < ancestor.selectedChildIndex; index += 1)
      prefix = checkedAdd(prefix, children[index]!.entryCount);
  }
  return prefix;
}

/**
 * Pure root-to-leaf descent over an in-memory diagnostic manifest, mirroring
 * the SQLite `pathAtOffset` shape so both share the same bounded assembly.
 */
export function boundedPathAtOffset(
  old: DiagnosticBuiltManifest,
  offset: number,
): { readonly frames: readonly BoundedPathFrame[]; readonly entryIndex: number } {
  if (intrinsicByteLength(old.rootHash) !== 32)
    throw new Error("bounded manifest root hash must contain 32 bytes");
  if (!Number.isSafeInteger(offset) || offset < 0)
    throw new RangeError("manifest tree offset must be a nonnegative safe integer");
  const root = decodeManifestRoot(old.root, old.rootHash);
  validateSupportedManifestParameters(root.parameters);
  if (offset > root.fileSize)
    throw new RangeError("manifest tree offset is outside the file");
  const selectedOffset = root.fileSize === 0 ? 0 : Math.min(offset, root.fileSize - 1);
  const frames: BoundedPathFrame[] = [];
  let path: number[] = [];
  let nodeOffset = 0;
  let remaining = selectedOffset;
  let finalAtLevel = true;
  let expected: ManifestChild | undefined;
  let hash = copyBytes(root.rootNodeHash);
  for (let depth = 1; ; depth += 1) {
    const cached = old.nodes.get(bytesToHex(hash));
    if (!cached) throw new Error("old manifest is missing an authenticated node");
    const node = cached.node;
    if (
      expected &&
      (node.span !== expected.span || node.entryCount !== expected.entryCount)
    )
      throw new Error("ECORRUPT: manifest child totals mismatch");
    validateCanonicalManifestNode(node, root.parameters, finalAtLevel, depth === 1);
    if (depth === 1) {
      if (
        node.span !== root.fileSize ||
        node.entryCount !== root.entryCount ||
        (root.fileSize === 0) !== (root.entryCount === 0)
      )
        throw new Error("ECORRUPT: manifest root totals mismatch");
    }
    if (node.kind === "leaf") {
      let entryIndex = 0;
      if (root.fileSize !== 0) {
        let relative = remaining;
        let found = false;
        for (let index = 0; index < node.entries.length; index += 1) {
          const entry = node.entries[index]!;
          if (relative < entry.length) {
            entryIndex = index;
            found = true;
            break;
          }
          relative -= entry.length;
        }
        if (!found)
          throw new Error("ECORRUPT: leaf span does not contain requested offset");
      }
      frames.push(
        Object.freeze({
          hash: copyBytes(hash),
          path: Object.freeze([...path]),
          offset: nodeOffset,
          finalAtLevel,
          node,
        }),
      );
      return Object.freeze({ frames: Object.freeze(frames), entryIndex });
    }
    let childOffset = nodeOffset;
    let selected = -1;
    for (let index = 0; index < node.children.length; index += 1) {
      const child = node.children[index]!;
      if (remaining < child.span) {
        selected = index;
        break;
      }
      remaining -= child.span;
      childOffset = checkedAdd(childOffset, child.span);
    }
    if (selected < 0)
      throw new Error("ECORRUPT: internal span does not contain requested offset");
    frames.push(
      Object.freeze({
        hash: copyBytes(hash),
        path: Object.freeze([...path]),
        offset: nodeOffset,
        finalAtLevel,
        node,
        selectedChildIndex: selected,
      }),
    );
    expected = node.children[selected]!;
    hash = copyBytes(expected.hash);
    finalAtLevel = finalAtLevel && selected === node.children.length - 1;
    nodeOffset = childOffset;
    path = [...path, selected];
  }
}

/** Start entry index of a child within its parent. */
function childStartIndex(
  parentStart: number,
  children: readonly ManifestChild[],
  index: number,
): number {
  let start = parentStart;
  for (let i = 0; i < index; i += 1) start = checkedAdd(start, children[i]!.entryCount);
  return start;
}

/** Byte offset of a child within its parent. */
function childStartOffset(
  parentOffset: number,
  children: readonly ManifestChild[],
  index: number,
): number {
  let offset = parentOffset;
  for (let i = 0; i < index; i += 1) offset = checkedAdd(offset, children[i]!.span);
  return offset;
}

interface BoundedChainItem {
  readonly child: ManifestChild;
  readonly path: readonly number[];
  readonly parentChildren: readonly ManifestChild[];
  readonly parentStart: number;
  readonly parentOffset: number;
  readonly parentFinal: boolean;
}

/**
 * Assembles the bounded manifest state from the affected and dirty-end path
 * frames plus the capped right-fringe crawl. Shared by the pure in-memory
 * state builder and the SQLite-backed bounded loader so both produce
 * byte-identical inputs for the bounded rebuild.
 */
export function assembleBoundedManifestState(
  root: BoundedManifestRoot,
  affectedPath: readonly BoundedPathFrame[],
  affectedEntryIndex: number,
  dirtyEndPath: readonly BoundedPathFrame[],
  dirtyOldEnd: number,
  readNode: (hash: Uint8Array) => ManifestNode | undefined,
  validateNode: (
    hash: Uint8Array,
    node: ManifestNode,
    finalAtLevel: boolean,
    rootNode: boolean,
  ) => void,
  limits: LocalRebuildLimits,
  allowTruncatedFringe = false,
): BoundedManifestState {
  const rootDepth = affectedPath.length;
  if (dirtyEndPath.length !== rootDepth)
    throw new Error("ECORRUPT: bounded manifest paths disagree on depth");
  const affectedFrame = affectedPath.at(-1)!;
  const dirtyFrame = dirtyEndPath.at(-1)!;
  if (affectedFrame.node.kind !== "leaf" || dirtyFrame.node.kind !== "leaf")
    throw new Error("ECORRUPT: bounded manifest path does not end at a leaf");
  const dirtyHash = bytesToHex(dirtyFrame.hash);
  const sameLeaf = bytesToHex(affectedFrame.hash) === dirtyHash;
  const truncateFringe = sameLeaf && allowTruncatedFringe;

  const leafFromParts = (
    node: ManifestNode,
    path: readonly number[],
    finalAtLevel: boolean,
    startEntryIndex: number,
    leafOffset: number,
  ): BoundedLeaf => {
    if (node.kind !== "leaf") throw new Error("ECORRUPT: bounded leaf is not a leaf");
    const entryOffsets: number[] = [0];
    let size = 0;
    for (const entry of node.entries) {
      size = checkedAdd(size, entry.length);
      entryOffsets.push(size);
    }
    return Object.freeze({
      node,
      path: Object.freeze([...path]),
      finalAtLevel,
      startEntryIndex,
      leafOffset,
      entries: node.entries,
      entryOffsets: Object.freeze(entryOffsets),
    });
  };
  const leafFromFrame = (
    path: readonly BoundedPathFrame[],
    frame: BoundedPathFrame,
  ): BoundedLeaf =>
    leafFromParts(
      frame.node,
      frame.path,
      frame.finalAtLevel,
      prefixEntryCount(path, frame),
      frame.offset,
    );
  const affectedLeaf = leafFromFrame(affectedPath, affectedFrame);
  const dirtyEndLeaf = sameLeaf
    ? affectedLeaf
    : leafFromFrame(dirtyEndPath, dirtyFrame);

  const memo = new Map<string, ManifestNode>();
  let loadedRecords = 0;
  const countRecords = (records: number): void => {
    loadedRecords = checkedAdd(loadedRecords, records);
    if (loadedRecords > limits.maxAffectedEntries)
      throw new BoundedRebuildFallbackError(
        "bounded local rebuild exceeds its loaded fringe window",
      );
  };

  const boundary = new Map<number, number>();
  const addLeafBoundaries = (leaf: BoundedLeaf): void => {
    boundary.set(leaf.leafOffset, leaf.startEntryIndex);
    let offset = leaf.leafOffset;
    for (let index = 0; index < leaf.entries.length; index += 1) {
      offset = checkedAdd(offset, leaf.entries[index]!.length);
      boundary.set(offset, leaf.startEntryIndex + index + 1);
    }
  };
  addLeafBoundaries(affectedLeaf);
  if (!sameLeaf) addLeafBoundaries(dirtyEndLeaf);

  const claimPaths = new Map<string, readonly number[]>();
  const claimProofs = new Map<
    string,
    {
      readonly sourcePath: readonly number[];
      readonly sourceFinalAtLevel: boolean;
      readonly sourceLeafDelta: number;
    }
  >();
  const addClaims = (
    children: readonly ManifestChild[],
    path: readonly number[],
    parentFinalAtLevel: boolean,
  ): void => {
    for (let index = 0; index < children.length; index += 1) {
      const sourcePath = Object.freeze([...path, index]);
      const sourceFinalAtLevel = parentFinalAtLevel && index === children.length - 1;
      claimPaths.set(bytesToHex(children[index]!.hash), sourcePath);
      claimProofs.set(
        bytesToHex(children[index]!.hash),
        Object.freeze({
          sourcePath,
          sourceFinalAtLevel,
          sourceLeafDelta: rootDepth - (sourcePath.length + 1),
        }),
      );
    }
  };

  const loadSibling = (item: BoundedChainItem): ManifestNode => {
    const key = bytesToHex(item.child.hash);
    const cached = memo.get(key);
    const node = cached ?? readNode(item.child.hash);
    if (!node) throw new Error("ECORRUPT: bounded fringe node is missing");
    validateNode(
      item.child.hash,
      node,
      item.parentFinal && item.path.at(-1)! === item.parentChildren.length - 1,
      false,
    );
    if (node.span !== item.child.span || node.entryCount !== item.child.entryCount)
      throw new Error("ECORRUPT: bounded fringe child totals mismatch");
    if (!cached) memo.set(key, node);
    return node;
  };

  /**
   * Yields the nodes at `depth` after the affected path node at that depth:
   * the parent's remaining children, then the parent's right siblings'
   * children. The recursion loads and memoizes the ancestor siblings.
   */
  const chain = function* (
    depth: number,
  ): Generator<BoundedChainItem, void, undefined> {
    if (depth <= 0) return;
    const parent = affectedPath[depth - 1]!;
    if (parent.node.kind !== "internal" || parent.selectedChildIndex === undefined)
      throw new Error("ECORRUPT: bounded manifest path lost an internal child");
    const parentStart = prefixEntryCount(affectedPath, parent);
    for (
      let index = parent.selectedChildIndex + 1;
      index < parent.node.children.length;
      index += 1
    )
      yield Object.freeze({
        child: parent.node.children[index]!,
        path: Object.freeze([...parent.path, index]),
        parentChildren: parent.node.children,
        parentStart,
        parentOffset: parent.offset,
        parentFinal: parent.finalAtLevel,
      });
    for (const sibling of chain(depth - 1)) {
      const node = loadSibling(sibling);
      if (node.kind !== "internal")
        throw new Error("ECORRUPT: bounded fringe ancestor is not internal");
      const start = childStartIndex(
        sibling.parentStart,
        sibling.parentChildren,
        sibling.path.at(-1)!,
      );
      const offset = childStartOffset(
        sibling.parentOffset,
        sibling.parentChildren,
        sibling.path.at(-1)!,
      );
      const children = node.children;
      const finalAtLevel =
        sibling.parentFinal &&
        sibling.path.at(-1)! === sibling.parentChildren.length - 1;
      for (let index = 0; index < children.length; index += 1)
        yield Object.freeze({
          child: children[index]!,
          path: Object.freeze([...sibling.path, index]),
          parentChildren: children,
          parentStart: start,
          parentOffset: offset,
          parentFinal: finalAtLevel,
        });
    }
  };

  // The leaf chain: the affected leaf, the fully deleted leaves, the dirty-end
  // leaf, and the fringe leaves after it. Deleted leaves contribute bounds
  // only; the dirty-end leaf and the fringe leaves carry entries.
  const deletedLeafBounds: BoundedLeafBound[] = [];
  const fringeLeaves: BoundedLeaf[] = [];
  let fringeCoversToEnd: boolean = truncateFringe ? affectedLeaf.finalAtLevel : true;
  if (rootDepth >= 2 && !truncateFringe) {
    let foundDirty: boolean = sameLeaf;
    for (const item of chain(rootDepth - 1)) {
      const key = bytesToHex(item.child.hash);
      if (!foundDirty && key !== dirtyHash) {
        deletedLeafBounds.push(
          Object.freeze({
            startEntryIndex: childStartIndex(
              item.parentStart,
              item.parentChildren,
              item.path.at(-1)!,
            ),
            entryCount: item.child.entryCount,
          }),
        );
        countRecords(item.child.entryCount);
        continue;
      }
      if (!foundDirty) {
        foundDirty = true;
        continue;
      }
      const node = loadSibling(item);
      if (node.kind !== "leaf")
        throw new Error("ECORRUPT: bounded fringe leaf is not a leaf");
      const leaf = leafFromParts(
        node,
        item.path,
        item.parentFinal && item.path.at(-1)! === item.parentChildren.length - 1,
        childStartIndex(item.parentStart, item.parentChildren, item.path.at(-1)!),
        childStartOffset(item.parentOffset, item.parentChildren, item.path.at(-1)!),
      );
      fringeLeaves.push(leaf);
      addLeafBoundaries(leaf);
      countRecords(node.entries.length);
    }
  }

  // The level windows for levels 1..rootDepth-1.
  const levelWindows: (BoundedLevelWindow | undefined)[] = [];
  for (let level = 1; level < rootDepth; level += 1) {
    const frame = affectedPath[rootDepth - 1 - level]!;
    if (frame.node.kind !== "internal" || frame.selectedChildIndex === undefined)
      throw new Error("ECORRUPT: bounded manifest path lost an internal child");
    addClaims(frame.node.children, frame.path, frame.finalAtLevel);
    const fringe: BoundedFringeGroup[] = [];
    for (const item of chain(rootDepth - 1 - level)) {
      const itemStart = childStartIndex(
        item.parentStart,
        item.parentChildren,
        item.path.at(-1)!,
      );
      const node = loadSibling(item);
      if (node.kind !== "internal")
        throw new Error("ECORRUPT: bounded fringe internal is not internal");
      fringe.push(
        Object.freeze({ path: item.path, children: Object.freeze([...node.children]) }),
      );
      countRecords(node.children.length);
    }
    levelWindows[level] = Object.freeze({
      level,
      affectedChildren: Object.freeze([...frame.node.children]),
      affectedChildIndex: frame.selectedChildIndex,
      fringe: Object.freeze(fringe),
      coversTrueEnd: truncateFringe ? frame.finalAtLevel : fringeCoversToEnd,
    });
  }

  return Object.freeze({
    root,
    rootDepth,
    affectedEntryIndex,
    affectedLeaf,
    dirtyEndLeaf,
    deletedLeafBounds: Object.freeze(deletedLeafBounds),
    fringeLeaves: Object.freeze(fringeLeaves),
    fringeCoversToEnd,
    boundary,
    levelWindows: Object.freeze(levelWindows),
    claimPaths,
    claimProofs,
    release: (): void => {},
  });
}

/** Pure state builder over an in-memory diagnostic manifest (golden oracle). */
export function buildBoundedManifestState(
  old: DiagnosticBuiltManifest,
  offset: number,
  deleteLength: number,
  limits: LocalRebuildLimits,
  allowTruncatedFringe = false,
): BoundedManifestState {
  const root = decodeManifestRoot(old.root, old.rootHash);
  validateSupportedManifestParameters(root.parameters);
  const dirtyOldEnd = checkedAdd(offset, deleteLength, "bounded dirty old end");
  const affected = boundedPathAtOffset(old, offset);
  const dirty = boundedPathAtOffset(old, dirtyOldEnd);
  const readNode = (hash: Uint8Array): ManifestNode | undefined =>
    old.nodes.get(bytesToHex(hash))?.node;
  const validateNode = (
    hash: Uint8Array,
    node: ManifestNode,
    finalAtLevel: boolean,
    rootNode: boolean,
  ): void => {
    void hash;
    validateCanonicalManifestNode(node, root.parameters, finalAtLevel, rootNode);
  };
  return assembleBoundedManifestState(
    Object.freeze({
      rootHash: copyBytes(old.rootHash),
      root: copyBytes(old.root),
      parameters: Object.freeze({ ...root.parameters }),
      fileSize: root.fileSize,
      entryCount: root.entryCount,
    }),
    affected.frames,
    affected.entryIndex,
    dirty.frames,
    dirtyOldEnd,
    readNode,
    validateNode,
    limits,
    allowTruncatedFringe,
  );
}

interface BoundedEditGeometry {
  readonly startEntry: number;
  readonly scanStart: number;
  readonly delta: number;
  readonly dirtyOldEnd: number;
  readonly dirtyNewEnd: number;
}

function editGeometry(
  state: BoundedManifestState,
  editOffset: number,
  deleteLength: number,
  insertLength: number,
  sourceSize: number,
  emptyInsertion: boolean,
): BoundedEditGeometry {
  const dirtyOldEnd = checkedAdd(editOffset, deleteLength, "bounded dirty old end");
  const dirtyNewEnd = checkedAdd(editOffset, insertLength, "bounded dirty new end");
  const locatedStart = checkedAdd(
    state.affectedLeaf.startEntryIndex,
    state.affectedEntryIndex,
    "bounded affected entry index",
  );
  const startEntry =
    editOffset === sourceSize && !emptyInsertion && state.root.entryCount > 0
      ? state.root.entryCount - 1
      : locatedStart;
  const scanStart = checkedAdd(
    state.affectedLeaf.leafOffset,
    state.affectedLeaf.entryOffsets[state.affectedEntryIndex]!,
    "bounded scan start",
  );
  return Object.freeze({
    startEntry,
    scanStart,
    delta: insertLength - deleteLength,
    dirtyOldEnd,
    dirtyNewEnd,
  });
}

function isSingular(level: BoundedRegroupedLevel): boolean {
  return level.prefixEmpty && level.segment.length === 1 && level.suffixEmpty;
}

/** The affected level-`level` group is the first group of its level. */
function prefixEmptyAtLevel(state: BoundedManifestState, level: number): boolean {
  // The affected level-`level` node sits at depth `rootDepth - 1 - level`
  // from the root; it is the first node at its level iff every ancestor from
  // the root's selected child down to its parent is the first child.
  const lastDepth = state.rootDepth - 2 - level;
  if (lastDepth < 0) return true;
  for (let depth = 0; depth <= lastDepth; depth += 1)
    if (state.affectedLeaf.path[depth] !== 0) return false;
  return true;
}

function regroupBoundedLevel(
  level: number,
  state: BoundedManifestState,
  geometry: BoundedEditGeometry,
  entrySplice: {
    readonly start: number;
    readonly deleteCount: number;
    readonly entries: readonly ManifestEntry[];
  },
  newNodes: Map<string, EncodedManifestNode>,
  hashBytes: HashFunction,
  previous?: BoundedRegroupedLevel,
): BoundedRegroupedLevel {
  if (level === 0) {
    const affected = state.affectedLeaf;
    const dirty = state.dirtyEndLeaf;
    const a = entrySplice.start - affected.startEntryIndex;
    const spliceEnd = entrySplice.start + entrySplice.deleteCount;
    if (a < 0 || a > affected.entries.length)
      throw new BoundedRebuildFallbackError(
        "bounded splice start leaves the affected leaf",
      );
    const dirtyLocal = spliceEnd - dirty.startEntryIndex;
    if (dirtyLocal < 0 || dirtyLocal > dirty.entries.length)
      throw new BoundedRebuildFallbackError(
        "bounded splice end leaves the loaded leaves",
      );
    const replacement = entrySplice.entries;
    const records: RecordValue[] = [];
    for (let index = 0; index < a; index += 1) records.push(affected.entries[index]!);
    if (dirty !== affected)
      for (let index = dirtyLocal; index < dirty.entries.length; index += 1)
        records.push(dirty.entries[index]!);
    else
      for (let index = dirtyLocal; index < affected.entries.length; index += 1)
        records.push(affected.entries[index]!);
    for (const leaf of state.fringeLeaves)
      for (const entry of leaf.entries) records.push(entry);
    const spliceEndRelative = spliceEnd - affected.startEntryIndex;
    // Window coordinate of the record at full-relative position p: records
    // at or after the splice map linearly past the prefix; records inside
    // the splice range (and the fully deleted leaves) are not in the window
    // and clamp below the after-splice start so the reconnect search never
    // matches them.
    const windowCoord = (p: number): number =>
      p >= spliceEndRelative ? a + (p - spliceEndRelative) : Math.min(p, a - 1);
    const starts: number[] = [0];
    const counts: number[] = [affected.entries.length];
    for (const bound of state.deletedLeafBounds) {
      starts.push(windowCoord(bound.startEntryIndex - affected.startEntryIndex));
      counts.push(bound.entryCount);
    }
    if (dirty !== affected) {
      starts.push(windowCoord(dirty.startEntryIndex - affected.startEntryIndex));
      counts.push(dirty.entries.length);
    }
    for (const leaf of state.fringeLeaves) {
      starts.push(windowCoord(leaf.startEntryIndex - affected.startEntryIndex));
      counts.push(leaf.entries.length);
    }
    if (!state.fringeCoversToEnd) {
      starts.push(records.length);
      counts.push(0);
    }
    return regroupLevelBounded(
      0,
      {
        oldRecords: Object.freeze(records),
        spliceStart: a,
        spliceEnd: windowCoord(spliceEndRelative),
        replacement,
        groupBounds: Object.freeze({
          starts: Object.freeze(starts),
          counts: Object.freeze(counts),
        }),
        coversTrueEnd: spliceEnd === state.root.entryCount || state.fringeCoversToEnd,
        prefixEmpty: prefixEmptyAtLevel(state, 0),
        emptyAffectedGroup: affected.entries.length === 0,
      },
      newNodes,
      hashBytes,
    );
  }
  const window = state.levelWindows[level];
  if (!window) throw new BoundedRebuildFallbackError("bounded level window is missing");
  if (previous === undefined)
    throw new BoundedRebuildFallbackError(
      "bounded level regroup lacks its predecessor",
    );
  const c = window.affectedChildIndex;
  const replacement = previous.segment.map(toChild);
  const w = previous.suffixEmpty
    ? window.affectedChildren.length - c
    : previous.reconnectWindowIndex;
  if (w < 0 || w > window.affectedChildren.length - c)
    throw new BoundedRebuildFallbackError(
      "bounded splice crosses the loaded level window",
    );
  const records: RecordValue[] = [];
  for (const child of window.affectedChildren) records.push(child);
  for (const group of window.fringe)
    for (const child of group.children) records.push(child);
  const starts: number[] = [0];
  const counts: number[] = [window.affectedChildren.length];
  let cursor = window.affectedChildren.length;
  for (const group of window.fringe) {
    starts.push(cursor);
    counts.push(group.children.length);
    cursor += group.children.length;
  }
  if (!window.coversTrueEnd) {
    starts.push(cursor);
    counts.push(0);
  }
  return regroupLevelBounded(
    level,
    {
      oldRecords: Object.freeze(records),
      spliceStart: c,
      spliceEnd: c + w,
      replacement,
      groupBounds: Object.freeze({
        starts: Object.freeze(starts),
        counts: Object.freeze(counts),
      }),
      coversTrueEnd: previous.suffixEmpty || window.coversTrueEnd,
      prefixEmpty: prefixEmptyAtLevel(state, level),
      emptyAffectedGroup: false,
    },
    newNodes,
    hashBytes,
  );
}

function regroupBoundedHeight(
  level: number,
  segment: readonly EncodedManifestNode[],
  newNodes: Map<string, EncodedManifestNode>,
  hashBytes: HashFunction,
): BoundedRegroupedLevel {
  const replacement = segment.map(toChild);
  return regroupLevelBounded(
    level,
    {
      oldRecords: Object.freeze([]),
      spliceStart: 0,
      spliceEnd: 0,
      replacement,
      groupBounds: Object.freeze({
        starts: Object.freeze([]),
        counts: Object.freeze([]),
      }),
      coversTrueEnd: true,
      prefixEmpty: true,
      emptyAffectedGroup: false,
    },
    newNodes,
    hashBytes,
  );
}

function rebuildBoundedSpine(
  state: BoundedManifestState,
  geometry: BoundedEditGeometry,
  entrySplice: {
    readonly start: number;
    readonly deleteCount: number;
    readonly entries: readonly ManifestEntry[];
  },
  newNodes: Map<string, EncodedManifestNode>,
  hashBytes: HashFunction,
): EncodedManifestNode {
  let rebuilt = regroupBoundedLevel(
    0,
    state,
    geometry,
    entrySplice,
    newNodes,
    hashBytes,
  );
  if (isSingular(rebuilt)) return rebuilt.segment[0]!;
  for (let level = 1; level < state.rootDepth; level += 1) {
    rebuilt = regroupBoundedLevel(
      level,
      state,
      geometry,
      entrySplice,
      newNodes,
      hashBytes,
      rebuilt,
    );
    if (isSingular(rebuilt)) return rebuilt.segment[0]!;
  }
  if (!rebuilt.prefixEmpty || !rebuilt.suffixEmpty)
    throw new BoundedRebuildFallbackError(
      "bounded local manifest height growth retained an unexpected outer segment",
    );
  let grown = rebuilt;
  for (let level = state.rootDepth; !isSingular(grown); level += 1)
    grown = regroupBoundedHeight(level, grown.segment, newNodes, hashBytes);
  return grown.segment[0]!;
}

/**
 * The bounded local rebuild: re-chunks the edited window against the loaded
 * old-manifest neighborhood (the affected leaf, the dirty-end leaf, and the
 * capped fringe) and rebuilds the manifest spine with the relative regroup.
 * The output is byte-identical to `rebuildDiagnosticManifestLocallyOwned`
 * whenever it completes; anything the bounded neighborhood cannot represent
 * throws `BoundedRebuildFallbackError` (or the established
 * `LocalRebuildLimitError`).
 */
export function rebuildManifestBoundedOwned(
  state: BoundedManifestState,
  source: RandomAccessContentSource,
  edit: {
    readonly offset: number;
    readonly deleteLength: number;
    readonly insertBytes: Uint8Array;
  },
  limits: LocalRebuildLimits,
  hashBytes: HashFunction,
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
  const root = state.root;
  validateSupportedManifestParameters(root.parameters);
  const contentCeiling = checkedMultiply(
    limits.maxRetainedEntries,
    root.parameters.maximum,
    "bounded retained-entry content ceiling",
  );
  if (sourceSize > contentCeiling || newSize > contentCeiling)
    throw new LocalRebuildLimitError(
      "local rebuild exceeds its retained-entry content ceiling; use the streamed workspace fallback",
    );
  if (callerInsertBytes.byteLength > limits.maxAffectedBytes)
    throw new LocalRebuildLimitError(
      "local edit insertion exceeds the affected-byte window; use the streamed workspace fallback",
    );
  if (root.fileSize !== sourceSize)
    throw new Error("source size does not match old manifest root");
  const insertBytes = callerInsertBytes;
  if (deleteLength === 0 && insertBytes.byteLength === 0) {
    return Object.freeze({
      rootHash: root.rootHash,
      root: root.root,
      fileSize: sourceSize,
      entryCount: root.entryCount,
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
        reusedPrefixEntries: root.entryCount,
        reusedSuffixEntries: 0,
        affectedEntryCount: 0,
        newObjectCount: 0,
        newManifestNodeCount: 0,
        reusedManifestNodeCount: state.claimPaths.size,
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
  const geometry = editGeometry(
    state,
    editOffset,
    deleteLength,
    insertBytes.byteLength,
    sourceSize,
    insertBytes.byteLength === 0,
  );
  const oldObjectIds = new Set<string>();
  for (const leaf of [
    state.affectedLeaf,
    ...(state.dirtyEndLeaf === state.affectedLeaf ? [] : [state.dirtyEndLeaf]),
    ...state.fringeLeaves,
  ])
    for (const entry of leaf.entries) oldObjectIds.add(bytesToHex(entry.hash));
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
      "prepared bounded edited-input bytes",
    );
    const end = checkedAdd(position, length, "bounded edited input end");
    if (end <= editOffset) return readOld(position, length);
    if (position >= geometry.dirtyNewEnd)
      return readOld(position - geometry.delta, length);
    const output = new Uint8Array(length);
    let written = 0;
    let cursor = position;
    while (written < length) {
      if (cursor < editOffset) {
        const count = Math.min(length - written, editOffset - cursor);
        output.set(readOld(cursor, count), written);
        cursor += count;
        written += count;
      } else if (cursor < geometry.dirtyNewEnd) {
        const insertionOffset = cursor - editOffset;
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
        const oldOffset = cursor - geometry.delta;
        const count = length - written;
        output.set(readOld(oldOffset, count), written);
        cursor += count;
        written += count;
      }
    }
    return output;
  };

  const affectedEntries: ManifestEntry[] = [];
  const affectedObjects = new Map<string, Uint8Array>();
  let bytesHashed = 0;
  let newObjectCount = 0;
  let newCursor = geometry.scanStart;
  let feedCursor = geometry.scanStart;
  let reconnectOldOffset: number | undefined;
  let reconnectEntry: number | undefined;
  const acceptReconnect = (): boolean => {
    if (newCursor < geometry.dirtyNewEnd) return false;
    const mappedOld = newCursor - geometry.delta;
    if (mappedOld < geometry.dirtyOldEnd) return false;
    const entry = state.boundary.get(mappedOld);
    if (entry === undefined) return false;
    reconnectOldOffset = mappedOld;
    reconnectEntry = entry;
    return true;
  };
  acceptReconnect();
  const chunker = new StreamingFastCdc(root.parameters);
  const attemptMetrics = (): Readonly<{
    readonly sourceBytesRead: number;
    readonly bytesHashed: number;
    readonly largestSourceRead: number;
    readonly chunkerInputBytesCopied: number;
    readonly chunkerOutputBytesCopied: number;
    readonly chunkerBoundaryBytesScanned: number;
    readonly editedInputBytesPrepared: number;
  }> => {
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
    const hash = hashBytes(chunk);
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
    const editBoundary =
      feedCursor < editOffset
        ? editOffset
        : feedCursor < geometry.dirtyNewEnd
          ? geometry.dirtyNewEnd
          : newSize;
    const postEditReadMaximum =
      feedCursor >= geometry.dirtyNewEnd
        ? Math.min(root.parameters.maximum, 64 * 1024)
        : root.parameters.maximum;
    const inputLength = Math.min(
      postEditReadMaximum,
      newSize - feedCursor,
      budgetProbe,
      editBoundary - feedCursor,
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
    start: geometry.startEntry,
    deleteCount: reconnectEntry - geometry.startEntry,
    entries: Object.freeze(affectedEntries),
  });
  const finalEntryCount = checkedAdd(
    root.entryCount - entrySplice.deleteCount,
    entrySplice.entries.length,
    "bounded rebuilt entry count",
  );
  if (finalEntryCount > limits.maxRetainedEntries)
    throw new LocalRebuildLimitError(
      "local result exceeds its retained-entry limit; use the streamed workspace fallback",
      attemptMetrics(),
    );

  const newNodes = new Map<string, EncodedManifestNode>();
  const rootNode = rebuildBoundedSpine(
    state,
    geometry,
    entrySplice,
    newNodes,
    hashBytes,
  );
  const encodedRoot = encodeManifestRoot({
    parameters: root.parameters,
    fileSize: newSize,
    entryCount: finalEntryCount,
    rootNodeHash: rootNode.hash,
  });
  const rootHash = hashBytes(encodedRoot);
  const metrics: LocalRebuildMetrics = Object.freeze({
    sourceBytesRead,
    bytesHashed,
    scanWindowBytes: root.parameters.maximum,
    reconnectOldOffset,
    reconnectNewOffset: newCursor,
    reusedPrefixEntries: geometry.startEntry,
    reusedSuffixEntries: root.entryCount - reconnectEntry,
    affectedEntryCount: affectedEntries.length,
    newObjectCount,
    newManifestNodeCount: newNodes.size,
    reusedManifestNodeCount: state.claimPaths.size,
    fellBackToEnd:
      reconnectOldOffset === sourceSize && geometry.dirtyOldEnd < sourceSize,
    insertionCopyCount: 1,
    insertionBytesCopied: insertBytes.byteLength,
    chunkerInputBytesCopied: chunker.metrics.inputBytesCopied,
    chunkerOutputBytesCopied: chunker.metrics.outputBytesCopied,
    chunkerBoundaryBytesScanned: chunker.metrics.boundaryBytesScanned,
    editedInputBytesPrepared,
  });
  return Object.freeze({
    rootHash,
    root: encodedRoot,
    fileSize: newSize,
    entryCount: finalEntryCount,
    entrySplice,
    affectedObjects,
    newNodes,
    metrics,
  });
}

import { sha256 } from "../cas/sha256.js";
import { copyBytes, intrinsicByteLength } from "../cas/bytes.js";
import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
  MAX_MANIFEST_ENTRY_COUNT,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
  type ManifestNode,
  type ManifestParameters,
  validateSupportedManifestParameters,
} from "./codec.js";
import {
  advanceManifestGroupingState,
  INTERNAL_MANIFEST_GROUPING,
  isManifestGroupBoundary,
  LEAF_MANIFEST_GROUPING,
} from "./grouping.js";

export interface EncodedManifestNode {
  readonly hash: Uint8Array;
  readonly encoded: Uint8Array;
  readonly node: ManifestNode;
}
export interface ManifestBuildRecord {
  readonly index: number;
  readonly child: ManifestChild;
}
export interface ManifestNodeWrite extends ManifestBuildRecord {
  readonly level: number;
  readonly value: EncodedManifestNode;
}
export interface ManifestBuildWorkspace {
  /** Borrowed views passed here are valid only for this synchronous call. */
  writeNode(record: ManifestNodeWrite): void;
  /** Returned rows and byte views are borrowed until the next workspace call. */
  readLevel(
    level: number,
    afterIndex: number,
    limit: number,
  ): readonly ManifestBuildRecord[];
}
export interface ManifestBuildOptions {
  readonly readBatchRecords?: number;
  readonly maxDepth?: number;
  readonly maxEntries?: number;
}
export interface BuiltManifestRoot {
  readonly rootHash: Uint8Array;
  readonly root: Uint8Array;
  readonly fileSize: number;
  readonly entryCount: number;
  readonly nodeCount: number;
  readonly depth: number;
  /** Peak count of borrowed plus owned record references retained concurrently. */
  readonly peakRetainedRecords: number;
  /** Serialized-size capacity proxy, not measured JavaScript heap usage. */
  readonly peakRetainedSerializedRecordBytes: number;
  readonly groupingRecordCount: number;
  readonly groupingRecordBytesProcessed: number;
}

function preparedNode(node: ManifestNode): EncodedManifestNode {
  const encoded = encodeManifestNode(node);
  return Object.freeze({ hash: sha256(encoded), encoded, node });
}

function writeNode(
  workspace: ManifestBuildWorkspace,
  level: number,
  index: number,
  node: ManifestNode,
): ManifestChild {
  const value = preparedNode(node);
  const retainedChild = Object.freeze({
    hash: copyBytes(value.hash),
    span: node.span,
    entryCount: node.entryCount,
  });
  const borrowedChild = Object.freeze({
    hash: copyBytes(retainedChild.hash),
    span: retainedChild.span,
    entryCount: retainedChild.entryCount,
  });
  workspace.writeNode(Object.freeze({ level, index, child: borrowedChild, value }));
  return retainedChild;
}

/**
 * Canonically builds a manifest while retaining only one grouping window and
 * one keyset page. Level records and encoded nodes belong to the supplied
 * workspace, which may be durable; this function never retains the manifest.
 */
export function buildManifestFromEntries(
  entries: Iterable<ManifestEntry>,
  parameters: ManifestParameters,
  workspace: ManifestBuildWorkspace,
  options: ManifestBuildOptions = {},
): BuiltManifestRoot {
  parameters = Object.freeze({
    minimum: parameters.minimum,
    average: parameters.average,
    maximum: parameters.maximum,
  });
  validateSupportedManifestParameters(parameters);
  const readBatchRecords = checkedInteger(
    options.readBatchRecords ?? 64,
    "readBatchRecords",
    4096,
  );
  const maxDepth = checkedInteger(options.maxDepth ?? 8, "maxDepth", 64);
  const maxEntries = checkedInteger(
    options.maxEntries ?? MAX_MANIFEST_ENTRY_COUNT,
    "maxEntries",
    MAX_MANIFEST_ENTRY_COUNT,
  );
  if (readBatchRecords === 0 || maxDepth === 0 || maxEntries === 0)
    throw new RangeError("manifest builder limits must be positive");
  let fileSize = 0;
  let entryCount = 0;
  let nodeCount = 0;
  let peakRetainedRecords = 0;
  let peakRetainedSerializedRecordBytes = 0;
  let groupingRecordCount = 0;
  let groupingRecordBytesProcessed = 0;
  let observedEntryCount = 0;
  let leafGroup: ManifestEntry[] = [];
  let leafState = 0n;
  let leafCount = 0;
  let onlyChild: ManifestChild | undefined;
  const observeRetained = (records: number, serializedBytes: number): void => {
    const rootRecord = onlyChild ? 1 : 0;
    peakRetainedRecords = Math.max(peakRetainedRecords, records + rootRecord);
    peakRetainedSerializedRecordBytes = Math.max(
      peakRetainedSerializedRecordBytes,
      serializedBytes + rootRecord * 48,
    );
  };
  const emitLeaf = (): void => {
    const span = leafGroup.reduce((sum, entry) => checkedAdd(sum, entry.length), 0);
    const node = Object.freeze({
      kind: "leaf",
      span,
      entryCount: leafGroup.length,
      entries: Object.freeze(leafGroup),
    } satisfies ManifestLeaf);
    onlyChild = writeNode(workspace, 0, leafCount++, node);
    nodeCount += 1;
    leafGroup = [];
    leafState = 0n;
  };
  const appendEntry = (entry: ManifestEntry, final: boolean): void => {
    if (entry.length === 0)
      throw new RangeError("zero-length manifest entries are forbidden");
    if (entry.length > parameters.maximum)
      throw new RangeError("manifest entry exceeds the FastCDC maximum");
    if (!final && entry.length < parameters.minimum)
      throw new RangeError("non-final manifest entry is below the FastCDC minimum");
    fileSize = checkedAdd(fileSize, entry.length);
    entryCount = checkedInteger(
      checkedAdd(entryCount, 1, "manifest entry count"),
      "manifest entry count",
      maxEntries,
    );
    leafGroup.push(entry);
    observeRetained(
      leafGroup.length + (pendingEntry ? 1 : 0),
      (leafGroup.length + (pendingEntry ? 1 : 0)) * 36,
    );
    leafState = advanceManifestGroupingState(leafState, entry);
    groupingRecordCount = checkedAdd(groupingRecordCount, 1);
    groupingRecordBytesProcessed = checkedAdd(groupingRecordBytesProcessed, 36);
    if (
      isManifestGroupBoundary(
        leafGroup.length,
        leafState,
        LEAF_MANIFEST_GROUPING.minimum,
        LEAF_MANIFEST_GROUPING.target,
        LEAF_MANIFEST_GROUPING.maximum,
      )
    )
      emitLeaf();
  };
  let pendingEntry: ManifestEntry | undefined;
  for (const borrowedEntry of entries) {
    observedEntryCount = checkedInteger(
      checkedAdd(observedEntryCount, 1, "observed manifest entry count"),
      "observed manifest entry count",
      maxEntries,
    );
    if (intrinsicByteLength(borrowedEntry.hash) !== 32)
      throw new RangeError("manifest entry hash must contain 32 bytes");
    checkedInteger(borrowedEntry.length, "manifest entry length", 0xffff_ffff);
    const entry = Object.freeze({
      hash: copyBytes(borrowedEntry.hash),
      length: borrowedEntry.length,
    });
    observeRetained(
      leafGroup.length + (pendingEntry ? 1 : 0) + 2,
      (leafGroup.length + (pendingEntry ? 1 : 0) + 2) * 36,
    );
    if (pendingEntry) appendEntry(pendingEntry, false);
    pendingEntry = entry;
  }
  if (pendingEntry) appendEntry(pendingEntry, true);
  if (leafGroup.length || leafCount === 0) emitLeaf();

  let inputLevel = 0;
  let inputCount = leafCount;
  let depth = 1;
  while (inputCount > 1) {
    if (depth >= maxDepth)
      throw new RangeError("manifest depth exceeds configured limit");
    let cursor = -1;
    let expectedIndex = 0;
    let outputCount = 0;
    let group: ManifestChild[] = [];
    let state = 0n;
    const emitInternal = (): void => {
      const node = Object.freeze({
        kind: "internal",
        span: group.reduce((sum, child) => checkedAdd(sum, child.span), 0),
        entryCount: group.reduce(
          (sum, child) =>
            checkedInteger(
              checkedAdd(sum, child.entryCount, "manifest entry count"),
              "manifest entry count",
              MAX_MANIFEST_ENTRY_COUNT,
            ),
          0,
        ),
        children: Object.freeze(group),
      } satisfies ManifestInternal);
      onlyChild = writeNode(workspace, inputLevel + 1, outputCount++, node);
      nodeCount += 1;
      group = [];
      state = 0n;
    };
    const readOwnedPage = (afterIndex: number): readonly ManifestBuildRecord[] => {
      const borrowedRows = workspace.readLevel(
        inputLevel,
        afterIndex,
        readBatchRecords,
      );
      if (borrowedRows.length > readBatchRecords)
        throw new Error("manifest workspace exceeded the requested keyset page");
      const rows = borrowedRows.map((row) => {
        checkedInteger(row.child.span, "manifest child span");
        checkedInteger(
          row.child.entryCount,
          "manifest child entry count",
          MAX_MANIFEST_ENTRY_COUNT,
        );
        if (
          intrinsicByteLength(row.child.hash) !== 32 ||
          row.child.span === 0 ||
          row.child.entryCount === 0
        )
          throw new RangeError("invalid manifest workspace child");
        return Object.freeze({
          index: row.index,
          child: Object.freeze({
            hash: copyBytes(row.child.hash),
            span: row.child.span,
            entryCount: row.child.entryCount,
          }),
        });
      });
      observeRetained(
        group.length + borrowedRows.length + rows.length,
        (group.length + borrowedRows.length + rows.length) * 48,
      );
      return rows;
    };
    while (true) {
      // readOwnedPage's borrowed views leave scope before grouping can invoke a
      // workspace write or grow the retained canonical group.
      const rows = readOwnedPage(cursor);
      if (!rows.length) break;
      for (const row of rows) {
        if (row.index !== expectedIndex || row.index <= cursor)
          throw new Error(
            "manifest workspace returned discontinuous or unordered level records",
          );
        const child = row.child;
        cursor = row.index;
        expectedIndex += 1;
        group.push(child);
        state = advanceManifestGroupingState(state, child);
        groupingRecordCount = checkedAdd(groupingRecordCount, 1);
        groupingRecordBytesProcessed = checkedAdd(groupingRecordBytesProcessed, 48);
        observeRetained(group.length + rows.length, (group.length + rows.length) * 48);
        if (
          isManifestGroupBoundary(
            group.length,
            state,
            INTERNAL_MANIFEST_GROUPING.minimum,
            INTERNAL_MANIFEST_GROUPING.target,
            INTERNAL_MANIFEST_GROUPING.maximum,
          )
        )
          emitInternal();
      }
      if (rows.length < readBatchRecords) break;
    }
    if (expectedIndex !== inputCount)
      throw new Error("manifest workspace level count mismatch");
    if (group.length) emitInternal();
    inputLevel += 1;
    inputCount = outputCount;
    depth += 1;
  }
  if (!onlyChild) throw new Error("manifest builder did not produce a root node");
  if (onlyChild.span !== fileSize || onlyChild.entryCount !== entryCount)
    throw new Error("manifest workspace root totals differ from the built content");
  const root = encodeManifestRoot({
    parameters,
    fileSize,
    entryCount,
    rootNodeHash: onlyChild.hash,
  });
  return Object.freeze({
    rootHash: sha256(root),
    root,
    fileSize,
    entryCount,
    nodeCount,
    depth,
    peakRetainedRecords,
    peakRetainedSerializedRecordBytes,
    groupingRecordCount,
    groupingRecordBytesProcessed,
  });
}

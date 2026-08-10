import { copyBytes, equalBytes, intrinsicByteLength } from "../cas/bytes.js";
import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import { readU64, writeU64 } from "./binary.js";
import { sha256 } from "../cas/sha256.js";
import { MAX_CONTENT_OBJECT_BYTES } from "../resources/limits.js";

export const ROOT_ENVELOPE_BYTES = 68;
export const NODE_HEADER_BYTES = 32;
export const LEAF_RECORD_BYTES = 36;
export const INTERNAL_RECORD_BYTES = 48;
export const MAX_MANIFEST_ENTRY_COUNT = 0xffff_ffff;
export const MAX_MANIFEST_NODE_BYTES = NODE_HEADER_BYTES + 256 * LEAF_RECORD_BYTES;
export interface ManifestParameters {
  readonly minimum: number;
  readonly average: number;
  readonly maximum: number;
}
export interface ManifestRoot {
  readonly parameters: ManifestParameters;
  readonly fileSize: number;
  readonly entryCount: number;
  readonly rootNodeHash: Uint8Array;
}
export interface ManifestEntry {
  readonly hash: Uint8Array;
  readonly length: number;
}
export interface ManifestChild {
  readonly hash: Uint8Array;
  readonly span: number;
  readonly entryCount: number;
}
export interface ManifestLeaf {
  readonly kind: "leaf";
  readonly span: number;
  readonly entryCount: number;
  readonly entries: readonly ManifestEntry[];
}
export interface ManifestInternal {
  readonly kind: "internal";
  readonly span: number;
  readonly entryCount: number;
  readonly children: readonly ManifestChild[];
}
export type ManifestNode = ManifestLeaf | ManifestInternal;

export function snapshotManifestParameters(
  parameters: ManifestParameters,
): Readonly<ManifestParameters> {
  const minimum = parameters.minimum;
  const average = parameters.average;
  const maximum = parameters.maximum;
  return Object.freeze({ minimum, average, maximum });
}

function validateOwnedManifestParameters(parameters: ManifestParameters): void {
  checkedInteger(parameters.minimum, "manifest minimum chunk size", 0xffff_ffff);
  checkedInteger(parameters.average, "manifest average chunk size", 0xffff_ffff);
  checkedInteger(parameters.maximum, "manifest maximum chunk size", 0xffff_ffff);
  if (
    parameters.minimum === 0 ||
    parameters.minimum > parameters.average ||
    parameters.average > parameters.maximum
  ) {
    throw new RangeError(
      "manifest FastCDC parameters require 0 < minimum <= average <= maximum",
    );
  }
  if ((parameters.average & (parameters.average - 1)) !== 0)
    throw new RangeError("manifest FastCDC average must be a power of two");
}

export function validateManifestParameters(parameters: ManifestParameters): void {
  validateOwnedManifestParameters(snapshotManifestParameters(parameters));
}

/**
 * Validates parameters that this runtime may use to construct or materialize
 * content. Binary inspection remains format-complete for valid uint32 values.
 */
export function validateSupportedManifestParameters(
  parameters: ManifestParameters,
): void {
  const owned = snapshotManifestParameters(parameters);
  validateOwnedManifestParameters(owned);
  if (owned.maximum > MAX_CONTENT_OBJECT_BYTES)
    throw new RangeError(
      `manifest FastCDC maximum exceeds the effective content-object limit (${MAX_CONTENT_OBJECT_BYTES})`,
    );
}

function magic(bytes: Uint8Array, expected: string): void {
  if (bytes.byteLength < 4 || String.fromCharCode(...bytes.subarray(0, 4)) !== expected)
    throw new Error(`invalid ${expected} magic`);
}

export function encodeManifestRoot(root: ManifestRoot): Uint8Array {
  const callerRootNodeHash = root.rootNodeHash;
  if (intrinsicByteLength(callerRootNodeHash) !== 32)
    throw new RangeError("root node hash must contain 32 bytes");
  const rootNodeHash = copyBytes(callerRootNodeHash);
  const parameters = snapshotManifestParameters(root.parameters);
  const fileSize = root.fileSize;
  const entryCount = root.entryCount;
  validateOwnedManifestParameters(parameters);
  checkedInteger(fileSize, "manifest root file size");
  checkedInteger(entryCount, "manifest root entry count", MAX_MANIFEST_ENTRY_COUNT);
  const bytes = new Uint8Array(ROOT_ENVELOPE_BYTES);
  bytes.set([0x45, 0x41, 0x46, 0x52]);
  const view = new DataView(bytes.buffer);
  view.setUint16(4, 1, true);
  bytes[6] = 1;
  bytes[7] = 1;
  view.setUint32(8, parameters.minimum, true);
  view.setUint32(12, parameters.average, true);
  view.setUint32(16, parameters.maximum, true);
  writeU64(view, 20, fileSize);
  writeU64(view, 28, entryCount);
  bytes.set(rootNodeHash, 36);
  return bytes;
}

export function decodeManifestRoot(
  bytes: Uint8Array,
  expectedHash?: Uint8Array,
): ManifestRoot {
  if (intrinsicByteLength(bytes) !== ROOT_ENVELOPE_BYTES)
    throw new Error("manifest root envelope must contain exactly 68 bytes");
  bytes = copyBytes(bytes);
  if (expectedHash !== undefined) {
    if (intrinsicByteLength(expectedHash) !== 32)
      throw new RangeError("expected manifest root hash must contain 32 bytes");
    expectedHash = copyBytes(expectedHash);
  }
  magic(bytes, "EAFR");
  if (expectedHash && !equalBytes(sha256(bytes), expectedHash))
    throw new Error("manifest root digest mismatch");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint16(4, true) !== 1 || bytes[6] !== 1 || bytes[7] !== 1)
    throw new Error("unsupported manifest algorithm or version");
  const minimum = view.getUint32(8, true);
  const average = view.getUint32(12, true);
  const maximum = view.getUint32(16, true);
  validateManifestParameters({ minimum, average, maximum });
  const entryCount = checkedInteger(
    readU64(view, 28, "entry count"),
    "manifest root entry count",
    MAX_MANIFEST_ENTRY_COUNT,
  );
  return Object.freeze({
    parameters: Object.freeze({ minimum, average, maximum }),
    fileSize: readU64(view, 20, "file size"),
    entryCount,
    rootNodeHash: copyBytes(bytes, 36, 68),
  });
}

function snapshotManifestNodeForEncoding(node: ManifestNode): ManifestNode {
  const kind = node.kind;
  const span = node.span;
  const entryCount = node.entryCount;
  if (kind !== "leaf" && kind !== "internal")
    throw new RangeError("invalid manifest node kind");
  checkedInteger(span, "manifest node span");
  checkedInteger(entryCount, "manifest node entry count", MAX_MANIFEST_ENTRY_COUNT);
  if (kind === "leaf") {
    const callerEntries = (node as ManifestLeaf).entries;
    const count = callerEntries.length;
    if (count > 256) throw new RangeError("manifest node capacity exceeded");
    const entries: ManifestEntry[] = [];
    let computedSpan = 0;
    for (let index = 0; index < count; index += 1) {
      const callerEntry = callerEntries[index]!;
      const callerHash = callerEntry.hash;
      if (intrinsicByteLength(callerHash) !== 32)
        throw new RangeError("invalid manifest leaf entry");
      const hash = copyBytes(callerHash);
      const length = callerEntry.length;
      if (length === 0 || length > 0xffff_ffff)
        throw new RangeError("invalid manifest leaf entry");
      checkedInteger(length, "manifest entry length", 0xffff_ffff);
      entries.push(Object.freeze({ hash, length }));
      computedSpan = checkedAdd(computedSpan, length);
    }
    if (computedSpan !== span || entryCount !== count)
      throw new Error("manifest leaf totals mismatch");
    return Object.freeze({
      kind,
      span,
      entryCount,
      entries: Object.freeze(entries),
    });
  }
  const callerChildren = (node as ManifestInternal).children;
  const count = callerChildren.length;
  if (count > 128) throw new RangeError("manifest node capacity exceeded");
  if (count === 0) throw new RangeError("empty internal manifest nodes are forbidden");
  const children: ManifestChild[] = [];
  let computedSpan = 0;
  let computedCount = 0;
  for (let index = 0; index < count; index += 1) {
    const callerChild = callerChildren[index]!;
    const callerHash = callerChild.hash;
    if (intrinsicByteLength(callerHash) !== 32)
      throw new RangeError("invalid manifest internal child");
    const hash = copyBytes(callerHash);
    const childSpan = callerChild.span;
    const childEntryCount = callerChild.entryCount;
    checkedInteger(childSpan, "manifest child span");
    checkedInteger(
      childEntryCount,
      "manifest child entry count",
      MAX_MANIFEST_ENTRY_COUNT,
    );
    if (childSpan === 0 || childEntryCount === 0)
      throw new RangeError("invalid manifest internal child");
    children.push(
      Object.freeze({
        hash,
        span: childSpan,
        entryCount: childEntryCount,
      }),
    );
    computedSpan = checkedAdd(computedSpan, childSpan);
    computedCount = checkedInteger(
      checkedAdd(computedCount, childEntryCount, "manifest node entry count"),
      "manifest node entry count",
      MAX_MANIFEST_ENTRY_COUNT,
    );
  }
  if (computedSpan !== span || computedCount !== entryCount)
    throw new Error("manifest internal totals mismatch");
  return Object.freeze({
    kind,
    span,
    entryCount,
    children: Object.freeze(children),
  });
}

export function encodeManifestNode(node: ManifestNode): Uint8Array {
  node = snapshotManifestNodeForEncoding(node);
  const count = node.kind === "leaf" ? node.entries.length : node.children.length;
  const capacity = node.kind === "leaf" ? 256 : 128;
  if (count > capacity) throw new RangeError("manifest node capacity exceeded");
  if (node.kind === "internal" && count === 0)
    throw new RangeError("empty internal manifest nodes are forbidden");
  const recordBytes = node.kind === "leaf" ? LEAF_RECORD_BYTES : INTERNAL_RECORD_BYTES;
  const bytes = new Uint8Array(NODE_HEADER_BYTES + count * recordBytes);
  bytes.set([0x45, 0x41, 0x46, 0x4e]);
  const view = new DataView(bytes.buffer);
  view.setUint16(4, 1, true);
  bytes[6] = node.kind === "leaf" ? 0 : 1;
  bytes[7] = 1;
  view.setUint32(8, count, true);
  view.setUint32(12, 0, true);
  writeU64(view, 16, node.span);
  writeU64(view, 24, node.entryCount);
  if (node.kind === "leaf") {
    let computedSpan = 0;
    for (let index = 0; index < node.entries.length; index += 1) {
      const entry = node.entries[index]!;
      const offset = NODE_HEADER_BYTES + index * LEAF_RECORD_BYTES;
      bytes.set(entry.hash, offset);
      view.setUint32(offset + 32, entry.length, true);
      computedSpan = checkedAdd(computedSpan, entry.length);
    }
    if (computedSpan !== node.span || node.entryCount !== count)
      throw new Error("manifest leaf totals mismatch");
  } else {
    let computedSpan = 0;
    let computedCount = 0;
    for (let index = 0; index < node.children.length; index += 1) {
      const child = node.children[index]!;
      const offset = NODE_HEADER_BYTES + index * INTERNAL_RECORD_BYTES;
      bytes.set(child.hash, offset);
      writeU64(view, offset + 32, child.span);
      writeU64(view, offset + 40, child.entryCount);
      computedSpan = checkedAdd(computedSpan, child.span);
      computedCount = checkedInteger(
        checkedAdd(computedCount, child.entryCount, "manifest node entry count"),
        "manifest node entry count",
        MAX_MANIFEST_ENTRY_COUNT,
      );
    }
    if (computedSpan !== node.span || computedCount !== node.entryCount)
      throw new Error("manifest internal totals mismatch");
  }
  return bytes;
}

export function decodeManifestNode(
  bytes: Uint8Array,
  expectedHash?: Uint8Array,
): ManifestNode {
  const suppliedBytes = intrinsicByteLength(bytes);
  if (suppliedBytes < NODE_HEADER_BYTES) throw new Error("truncated manifest node");
  if (suppliedBytes > MAX_MANIFEST_NODE_BYTES)
    throw new RangeError("manifest node exceeds the absolute v1 byte maximum");
  bytes = copyBytes(bytes);
  if (expectedHash !== undefined) {
    if (intrinsicByteLength(expectedHash) !== 32)
      throw new RangeError("expected manifest node hash must contain 32 bytes");
    expectedHash = copyBytes(expectedHash);
  }
  magic(bytes, "EAFN");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (
    view.getUint16(4, true) !== 1 ||
    bytes[7] !== 1 ||
    (bytes[6] !== 0 && bytes[6] !== 1) ||
    view.getUint32(12, true) !== 0
  )
    throw new Error("unsupported or malformed manifest node header");
  const count = view.getUint32(8, true);
  const span = readU64(view, 16, "node span");
  const entryCount = checkedInteger(
    readU64(view, 24, "node entry count"),
    "manifest node entry count",
    MAX_MANIFEST_ENTRY_COUNT,
  );
  const leaf = bytes[6] === 0;
  const recordBytes = leaf ? LEAF_RECORD_BYTES : INTERNAL_RECORD_BYTES;
  const capacity = leaf ? 256 : 128;
  if (count > capacity || bytes.byteLength !== NODE_HEADER_BYTES + count * recordBytes)
    throw new Error("noncanonical manifest node size");
  if (!leaf && count === 0) throw new Error("empty internal manifest node");
  if (expectedHash && !equalBytes(sha256(bytes), expectedHash))
    throw new Error("manifest node digest mismatch");
  if (leaf) {
    const entries: ManifestEntry[] = [];
    let computedSpan = 0;
    for (let index = 0; index < count; index += 1) {
      const offset = NODE_HEADER_BYTES + index * recordBytes;
      const length = view.getUint32(offset + 32, true);
      if (length === 0) throw new Error("zero-length manifest entry");
      entries.push(
        Object.freeze({ hash: copyBytes(bytes, offset, offset + 32), length }),
      );
      computedSpan = checkedAdd(computedSpan, length);
    }
    if (computedSpan !== span || entryCount !== count)
      throw new Error("manifest leaf totals mismatch");
    return Object.freeze({
      kind: "leaf",
      span,
      entryCount,
      entries: Object.freeze(entries),
    });
  }
  const children: ManifestChild[] = [];
  let computedSpan = 0;
  let computedCount = 0;
  for (let index = 0; index < count; index += 1) {
    const offset = NODE_HEADER_BYTES + index * recordBytes;
    const childSpan = readU64(view, offset + 32, "child span");
    const childCount = checkedInteger(
      readU64(view, offset + 40, "child entry count"),
      "manifest child entry count",
      MAX_MANIFEST_ENTRY_COUNT,
    );
    if (childSpan === 0 || childCount === 0)
      throw new Error("empty internal manifest child");
    children.push(
      Object.freeze({
        hash: copyBytes(bytes, offset, offset + 32),
        span: childSpan,
        entryCount: childCount,
      }),
    );
    computedSpan = checkedAdd(computedSpan, childSpan);
    computedCount = checkedInteger(
      checkedAdd(computedCount, childCount, "manifest node entry count"),
      "manifest node entry count",
      MAX_MANIFEST_ENTRY_COUNT,
    );
  }
  if (computedSpan !== span || computedCount !== entryCount)
    throw new Error("manifest internal totals mismatch");
  return Object.freeze({
    kind: "internal",
    span,
    entryCount,
    children: Object.freeze(children),
  });
}

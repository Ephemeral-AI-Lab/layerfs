import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import { copyBytes } from "../cas/bytes.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  validateSupportedManifestParameters,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
  type ManifestParameters,
} from "./codec.js";
import {
  INTERNAL_MANIFEST_GROUPING,
  LEAF_MANIFEST_GROUPING,
  validateCanonicalManifestGroup,
} from "./grouping.js";

export const MAX_MANIFEST_DEPTH = 64;

export interface ManifestNodeReader {
  get(hash: Uint8Array): Uint8Array | undefined;
}
export interface ManifestLookup {
  readonly entry: ManifestEntry | null;
  readonly entryOffset: number;
  readonly nodesRead: number;
}
export interface ManifestCursorEntry {
  readonly entry: ManifestEntry;
  readonly offset: number;
}

interface CursorFrame {
  readonly node: ManifestInternal;
  readonly finalAtLevel: boolean;
  childIndex: number;
  childOffset: number;
}

function validateDepthLimit(maxDepth: number): void {
  checkedInteger(maxDepth, "maxDepth", MAX_MANIFEST_DEPTH);
  if (maxDepth === 0) throw new RangeError("maxDepth must be positive");
}

function validateNodeCanonicality(
  node: ManifestLeaf | ManifestInternal,
  parameters: ManifestParameters,
  finalAtLevel: boolean,
  rootNode: boolean,
): void {
  if (node.kind === "leaf") {
    if (node.entries.length === 0) {
      if (!rootNode || node.span !== 0 || node.entryCount !== 0)
        throw new Error("empty manifest leaf is only canonical as the empty root");
      return;
    }
    for (let index = 0; index < node.entries.length; index += 1) {
      const entry = node.entries[index]!;
      if (entry.length > parameters.maximum)
        throw new Error("manifest entry exceeds root FastCDC maximum");
      const finalEntry = finalAtLevel && index === node.entries.length - 1;
      if (!finalEntry && entry.length < parameters.minimum)
        throw new Error("non-final manifest entry is below root FastCDC minimum");
    }
    validateCanonicalManifestGroup(node.entries, LEAF_MANIFEST_GROUPING, finalAtLevel);
    return;
  }
  if (node.children.length === 0) throw new Error("empty internal manifest node");
  if (rootNode && node.children.length === 1)
    throw new Error("unary internal root wrapper is noncanonical");
  validateCanonicalManifestGroup(
    node.children,
    INTERNAL_MANIFEST_GROUPING,
    finalAtLevel,
  );
}

export class ManifestSequentialCursor {
  readonly #reader: ManifestNodeReader;
  readonly #maxDepth: number;
  readonly #parameters: ManifestParameters;
  readonly #stack: CursorFrame[] = [];
  #leaf: ManifestLeaf | undefined;
  #leafDepth: number | undefined;
  #entryIndex = 0;
  #entryOffset = 0;
  #nodesRead = 0;

  constructor(
    rootBytes: Uint8Array,
    offset: number,
    reader: ManifestNodeReader,
    expectedRootHash?: Uint8Array,
    maxDepth = 8,
  ) {
    const root = decodeManifestRoot(rootBytes, expectedRootHash);
    validateSupportedManifestParameters(root.parameters);
    if (!Number.isSafeInteger(offset) || offset < 0 || offset > root.fileSize)
      throw new RangeError("manifest offset is outside the file");
    validateDepthLimit(maxDepth);
    this.#reader = reader;
    this.#maxDepth = maxDepth;
    this.#parameters = root.parameters;
    const rootNode = this.#load(root.rootNodeHash, 1, undefined, true, true);
    if (rootNode.span !== root.fileSize || rootNode.entryCount !== root.entryCount)
      throw new Error("manifest root totals mismatch");
    if ((root.fileSize === 0) !== (root.entryCount === 0))
      throw new Error("manifest empty root totals mismatch");
    if (offset === root.fileSize) {
      this.#entryOffset = root.fileSize;
      return;
    }
    this.#descend(rootNode, 0, offset, 1, true);
  }

  peek(): ManifestCursorEntry | null {
    const entry = this.#leaf?.entries[this.#entryIndex];
    return entry
      ? Object.freeze({
          entry: Object.freeze({ hash: copyBytes(entry.hash), length: entry.length }),
          offset: this.#entryOffset,
        })
      : null;
  }

  next(): ManifestCursorEntry | null {
    const entry = this.#leaf?.entries[this.#entryIndex];
    if (!entry) return null;
    const current = Object.freeze({
      entry: Object.freeze({ hash: copyBytes(entry.hash), length: entry.length }),
      offset: this.#entryOffset,
    });
    this.#entryOffset = checkedAdd(this.#entryOffset, entry.length);
    this.#entryIndex += 1;
    if (this.#entryIndex >= this.#leaf!.entries.length) this.#advanceLeaf();
    return current;
  }

  get nodesRead(): number {
    return this.#nodesRead;
  }
  get retainedNodeCount(): number {
    return this.#stack.length + (this.#leaf ? 1 : 0);
  }

  #load(
    hash: Uint8Array,
    depth: number,
    expected?: ManifestChild,
    finalAtLevel = false,
    rootNode = false,
  ): ManifestLeaf | ManifestInternal {
    if (depth > this.#maxDepth)
      throw new Error("manifest depth exceeds configured maximum");
    const authoritativeHash = copyBytes(hash);
    const encoded = this.#reader.get(copyBytes(authoritativeHash));
    if (!encoded) throw new Error("missing manifest node");
    const node = decodeManifestNode(encoded, authoritativeHash);
    this.#nodesRead += 1;
    if (
      expected &&
      (node.span !== expected.span || node.entryCount !== expected.entryCount)
    )
      throw new Error("manifest child totals mismatch");
    validateNodeCanonicality(node, this.#parameters, finalAtLevel, rootNode);
    return node;
  }

  #descend(
    initial: ManifestLeaf | ManifestInternal,
    nodeOffset: number,
    relative: number,
    depth: number,
    finalAtLevel: boolean,
  ): void {
    let node = initial;
    let start = nodeOffset;
    let remaining = relative;
    let currentDepth = depth;
    while (node.kind === "internal") {
      let childOffset = start;
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
        throw new Error("manifest internal span does not contain requested offset");
      this.#stack.push({ node, childIndex: selected, childOffset, finalAtLevel });
      const child = node.children[selected]!;
      const childFinal = finalAtLevel && selected === node.children.length - 1;
      currentDepth += 1;
      node = this.#load(child.hash, currentDepth, child, childFinal);
      finalAtLevel = childFinal;
      start = childOffset;
    }
    if (this.#leafDepth === undefined) this.#leafDepth = currentDepth;
    else if (this.#leafDepth !== currentDepth)
      throw new Error("unbalanced manifest tree");
    let entryOffset = start;
    for (let index = 0; index < node.entries.length; index += 1) {
      const entry = node.entries[index]!;
      if (remaining < entry.length) {
        this.#leaf = node;
        this.#entryIndex = index;
        this.#entryOffset = entryOffset;
        return;
      }
      remaining -= entry.length;
      entryOffset = checkedAdd(entryOffset, entry.length);
    }
    throw new Error("manifest leaf span does not contain requested offset");
  }

  #advanceLeaf(): void {
    this.#leaf = undefined;
    this.#entryIndex = 0;
    while (this.#stack.length > 0) {
      const frame = this.#stack.at(-1)!;
      const completed = frame.node.children[frame.childIndex]!;
      const nextIndex = frame.childIndex + 1;
      if (nextIndex >= frame.node.children.length) {
        this.#stack.pop();
        continue;
      }
      frame.childIndex = nextIndex;
      frame.childOffset = checkedAdd(frame.childOffset, completed.span);
      const child = frame.node.children[nextIndex]!;
      const finalAtLevel =
        frame.finalAtLevel && nextIndex === frame.node.children.length - 1;
      const depth = this.#stack.length + 1;
      const node = this.#load(child.hash, depth, child, finalAtLevel);
      this.#descend(node, frame.childOffset, 0, depth, finalAtLevel);
      return;
    }
  }
}

export function lookupManifest(
  rootBytes: Uint8Array,
  offset: number,
  reader: ManifestNodeReader,
  expectedRootHash?: Uint8Array,
  maxDepth = 8,
): ManifestLookup {
  const cursor = new ManifestSequentialCursor(
    rootBytes,
    offset,
    reader,
    expectedRootHash,
    maxDepth,
  );
  const selected = cursor.peek();
  return {
    entry: selected?.entry ?? null,
    entryOffset: selected?.offset ?? offset,
    nodesRead: cursor.nodesRead,
  };
}

export function validateManifestTree(
  rootBytes: Uint8Array,
  reader: ManifestNodeReader,
  expectedRootHash?: Uint8Array,
  maxDepth = 8,
): void {
  const root = decodeManifestRoot(rootBytes, expectedRootHash);
  validateDepthLimit(maxDepth);
  let leafDepth: number | undefined;
  const visit = (
    hash: Uint8Array,
    depth: number,
    finalAtLevel: boolean,
    rootNode: boolean,
  ): { span: number; count: number } => {
    if (depth > maxDepth) throw new Error("manifest depth exceeds configured maximum");
    const authoritativeHash = copyBytes(hash);
    const bytes = reader.get(copyBytes(authoritativeHash));
    if (!bytes) throw new Error("missing manifest node");
    const node = decodeManifestNode(bytes, authoritativeHash);
    validateNodeCanonicality(node, root.parameters, finalAtLevel, rootNode);
    if (node.kind === "leaf") {
      if (leafDepth === undefined) leafDepth = depth;
      else if (leafDepth !== depth) throw new Error("unbalanced manifest tree");
      return { span: node.span, count: node.entryCount };
    }
    let span = 0;
    let count = 0;
    for (let index = 0; index < node.children.length; index += 1) {
      const child = node.children[index]!;
      const actual = visit(
        child.hash,
        depth + 1,
        finalAtLevel && index === node.children.length - 1,
        false,
      );
      if (actual.span !== child.span || actual.count !== child.entryCount)
        throw new Error("manifest child totals mismatch");
      span = checkedAdd(span, actual.span);
      count = checkedAdd(count, actual.count);
    }
    return { span, count };
  };
  const totals = visit(root.rootNodeHash, 1, true, true);
  if (totals.span !== root.fileSize || totals.count !== root.entryCount)
    throw new Error("manifest root totals mismatch");
  if ((root.fileSize === 0) !== (root.entryCount === 0))
    throw new Error("manifest empty root totals mismatch");
}

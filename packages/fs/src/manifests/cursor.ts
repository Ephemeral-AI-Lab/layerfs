import {
  decodeManifestNode,
  decodeManifestRoot,
  type ManifestChild,
  type ManifestEntry,
  type ManifestInternal,
  type ManifestLeaf,
} from "./codec.js";

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
  childIndex: number;
  childOffset: number;
}

export class ManifestSequentialCursor {
  readonly #reader: ManifestNodeReader;
  readonly #maxDepth: number;
  readonly #stack: CursorFrame[] = [];
  #leaf: ManifestLeaf | undefined;
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
    if (!Number.isSafeInteger(offset) || offset < 0 || offset > root.fileSize)
      throw new RangeError("manifest offset is outside the file");
    if (!Number.isSafeInteger(maxDepth) || maxDepth <= 0)
      throw new RangeError("maxDepth must be a positive safe integer");
    this.#reader = reader;
    this.#maxDepth = maxDepth;
    if (offset === root.fileSize) {
      this.#entryOffset = root.fileSize;
      return;
    }
    const rootNode = this.#load(root.rootNodeHash, 1);
    if (rootNode.span !== root.fileSize || rootNode.entryCount !== root.entryCount)
      throw new Error("manifest root totals mismatch");
    this.#descend(rootNode, 0, offset, 1);
  }

  peek(): ManifestCursorEntry | null {
    const entry = this.#leaf?.entries[this.#entryIndex];
    return entry ? Object.freeze({ entry, offset: this.#entryOffset }) : null;
  }

  next(): ManifestCursorEntry | null {
    const current = this.peek();
    if (!current) return null;
    this.#entryOffset += current.entry.length;
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
  ): ManifestLeaf | ManifestInternal {
    if (depth > this.#maxDepth)
      throw new Error("manifest depth exceeds configured maximum");
    const encoded = this.#reader.get(hash);
    if (!encoded) throw new Error("missing manifest node");
    const node = decodeManifestNode(encoded, hash);
    this.#nodesRead += 1;
    if (
      expected &&
      (node.span !== expected.span || node.entryCount !== expected.entryCount)
    )
      throw new Error("manifest child totals mismatch");
    return node;
  }

  #descend(
    initial: ManifestLeaf | ManifestInternal,
    nodeOffset: number,
    relative: number,
    depth: number,
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
        childOffset += child.span;
      }
      if (selected < 0)
        throw new Error("manifest internal span does not contain requested offset");
      this.#stack.push({ node, childIndex: selected, childOffset });
      const child = node.children[selected]!;
      currentDepth += 1;
      node = this.#load(child.hash, currentDepth, child);
      start = childOffset;
    }
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
      entryOffset += entry.length;
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
      frame.childOffset += completed.span;
      const child = frame.node.children[nextIndex]!;
      const node = this.#load(child.hash, this.#stack.length + 1, child);
      this.#descend(node, frame.childOffset, 0, this.#stack.length + 1);
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
  const visit = (hash: Uint8Array, depth: number): { span: number; count: number } => {
    if (depth > maxDepth) throw new Error("manifest depth exceeds configured maximum");
    const bytes = reader.get(hash);
    if (!bytes) throw new Error("missing manifest node");
    const node = decodeManifestNode(bytes, hash);
    if (node.kind === "leaf") return { span: node.span, count: node.entryCount };
    let span = 0;
    let count = 0;
    for (const child of node.children) {
      const actual = visit(child.hash, depth + 1);
      if (actual.span !== child.span || actual.count !== child.entryCount)
        throw new Error("manifest child totals mismatch");
      span += actual.span;
      count += actual.count;
    }
    return { span, count };
  };
  const totals = visit(root.rootNodeHash, 1);
  if (totals.span !== root.fileSize || totals.count !== root.entryCount)
    throw new Error("manifest root totals mismatch");
}

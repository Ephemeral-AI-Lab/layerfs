import { equalBytes } from "../utils/bytes.js";
import { decodeManifestNode, decodeManifestRoot, type ManifestEntry } from "./codec.js";

export interface ManifestNodeReader { get(hash: Uint8Array): Uint8Array | undefined }
export interface ManifestLookup { readonly entry: ManifestEntry | null; readonly entryOffset: number; readonly nodesRead: number }

export function lookupManifest(rootBytes: Uint8Array, offset: number, reader: ManifestNodeReader, expectedRootHash?: Uint8Array, maxDepth = 8): ManifestLookup {
  const root = decodeManifestRoot(rootBytes, expectedRootHash);
  if (!Number.isSafeInteger(offset) || offset < 0 || offset > root.fileSize) throw new RangeError("manifest offset is outside the file");
  if (offset === root.fileSize) return { entry: null, entryOffset: root.fileSize, nodesRead: 0 };
  let hash = root.rootNodeHash; let relative = offset; let absolute = 0; let nodesRead = 0;
  for (let depth = 0; depth < maxDepth; depth += 1) {
    const encoded = reader.get(hash); if (!encoded) throw new Error("missing manifest node");
    const node = decodeManifestNode(encoded, hash); nodesRead += 1;
    if (node.kind === "leaf") {
      for (const entry of node.entries) {
        if (relative < entry.length) return { entry, entryOffset: absolute, nodesRead };
        relative -= entry.length; absolute += entry.length;
      }
      throw new Error("manifest leaf span does not contain requested offset");
    }
    let found = false;
    for (const child of node.children) {
      if (relative < child.span) { hash = child.hash; found = true; break; }
      relative -= child.span; absolute += child.span;
    }
    if (!found) throw new Error("manifest internal span does not contain requested offset");
  }
  throw new Error("manifest depth exceeds configured maximum");
}

export function validateManifestTree(rootBytes: Uint8Array, reader: ManifestNodeReader, expectedRootHash?: Uint8Array, maxDepth = 8): void {
  const root = decodeManifestRoot(rootBytes, expectedRootHash);
  const visit = (hash: Uint8Array, depth: number): { span: number; count: number } => {
    if (depth > maxDepth) throw new Error("manifest depth exceeds configured maximum");
    const bytes = reader.get(hash); if (!bytes) throw new Error("missing manifest node");
    const node = decodeManifestNode(bytes, hash);
    if (node.kind === "leaf") return { span: node.span, count: node.entryCount };
    let span = 0; let count = 0;
    for (const child of node.children) {
      const actual = visit(child.hash, depth + 1);
      if (actual.span !== child.span || actual.count !== child.entryCount) throw new Error("manifest child totals mismatch");
      span += actual.span; count += actual.count;
    }
    return { span, count };
  };
  const totals = visit(root.rootNodeHash, 1);
  if (totals.span !== root.fileSize || totals.count !== root.entryCount) throw new Error("manifest root totals mismatch");
}


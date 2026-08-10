import {
  decodeManifestNode,
  decodeManifestRoot,
  type ManifestEntry,
} from "../manifests/codec.js";
import { checkedAdd } from "../resources/safe-integers.js";
import type { RuntimeLimits, StorageLimits } from "../resources/limits.js";
import { AdmissionController } from "../resources/limits.js";
import { prepareContentStreaming } from "./streaming-prepare.js";
import type {
  ClosureCertificate,
  ContentStore,
  OperationsStorage,
} from "./storage-ports.js";
import type { ContentCache } from "../cache/content-cache.js";

function concatParts(parts: readonly Uint8Array[]): Uint8Array {
  const length = parts.reduce((sum, part) => checkedAdd(sum, part.byteLength), 0);
  const output = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.byteLength;
  }
  return output;
}

export interface PreparedManifest {
  readonly hash: Uint8Array;
  readonly size: number;
  readonly certificate: ClosureCertificate;
}

export async function prepareContent(
  port: OperationsStorage,
  input: Uint8Array | ReadableStream<Uint8Array>,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  signal?: AbortSignal,
  cache?: ContentCache,
  clock?: () => number,
): Promise<PreparedManifest> {
  return prepareContentStreaming(
    port,
    input,
    storage,
    runtime,
    admission,
    signal,
    cache,
    clock,
  );
}

export function readManifestRange(
  repository: ContentStore,
  manifestHash: Uint8Array,
  offset: number,
  length: number,
): Uint8Array {
  const rootBytes = repository.getManifestRoot(manifestHash);
  if (!rootBytes) throw new Error("ECORRUPT: missing manifest root");
  const root = decodeManifestRoot(rootBytes, manifestHash);
  if (length === 0 || offset >= root.fileSize) return new Uint8Array();
  const end = Math.min(root.fileSize, offset + length);
  const parts: Uint8Array[] = [];
  const visit = (hash: Uint8Array, nodeStart: number, depth: number): void => {
    if (depth > 8) throw new Error("ECORRUPT: manifest depth exceeded");
    const encoded = repository.getManifestNode(hash);
    if (!encoded) throw new Error("ECORRUPT: missing manifest node");
    const node = decodeManifestNode(encoded, hash);
    if (nodeStart >= end || nodeStart + node.span <= offset) return;
    if (node.kind === "leaf") {
      let position = nodeStart;
      for (const entry of node.entries) {
        const entryEnd = position + entry.length;
        if (position < end && entryEnd > offset) {
          const object = repository.getObject(entry.hash, entry.length);
          if (!object) throw new Error("ECORRUPT: missing CAS object");
          parts.push(
            object.slice(
              Math.max(0, offset - position),
              Math.min(entry.length, end - position),
            ),
          );
        }
        position = entryEnd;
        if (position >= end) break;
      }
    } else {
      let position = nodeStart;
      for (const child of node.children) {
        if (position < end && position + child.span > offset)
          visit(child.hash, position, depth + 1);
        position += child.span;
        if (position >= end) break;
      }
    }
  };
  const rootNodeBytes = repository.getManifestNode(root.rootNodeHash);
  if (!rootNodeBytes) throw new Error("ECORRUPT: missing root manifest node");
  const rootNode = decodeManifestNode(rootNodeBytes, root.rootNodeHash);
  if (rootNode.span !== root.fileSize || rootNode.entryCount !== root.entryCount)
    throw new Error("ECORRUPT: manifest root totals mismatch");
  visit(root.rootNodeHash, 0, 1);
  return concatParts(parts);
}

export function readManifestInto(
  repository: ContentStore,
  manifestHash: Uint8Array,
  position: number,
  destination: Uint8Array,
  destinationOffset: number,
  length: number,
): number {
  if (
    !Number.isSafeInteger(position) ||
    position < 0 ||
    !Number.isSafeInteger(destinationOffset) ||
    destinationOffset < 0 ||
    !Number.isSafeInteger(length) ||
    length < 0 ||
    destinationOffset + length > destination.byteLength
  )
    throw new RangeError("invalid direct manifest read range");
  const rootBytes = repository.getManifestRoot(manifestHash);
  if (!rootBytes) throw new Error("ECORRUPT: missing manifest root");
  const root = decodeManifestRoot(rootBytes, manifestHash);
  if (!length || position >= root.fileSize) return 0;
  const end = Math.min(root.fileSize, position + length);
  let written = 0;
  const visit = (hash: Uint8Array, nodeStart: number, depth: number): void => {
    if (depth > 8) throw new Error("ECORRUPT: manifest depth exceeded");
    if (nodeStart >= end || nodeStart >= position + length) return;
    const encoded = repository.getManifestNode(hash);
    if (!encoded) throw new Error("ECORRUPT: missing manifest node");
    const node = decodeManifestNode(encoded, hash);
    if (nodeStart + node.span <= position) return;
    if (node.kind === "leaf") {
      let cursor = nodeStart;
      for (const entry of node.entries) {
        const entryEnd = cursor + entry.length;
        if (cursor < end && entryEnd > position) {
          const object = repository.getObject(entry.hash, entry.length);
          if (!object) throw new Error("ECORRUPT: missing CAS object");
          const start = Math.max(0, position - cursor);
          const stop = Math.min(entry.length, end - cursor);
          destination.set(object.subarray(start, stop), destinationOffset + written);
          written += stop - start;
        }
        cursor = entryEnd;
        if (cursor >= end) break;
      }
    } else {
      let cursor = nodeStart;
      for (const child of node.children) {
        if (cursor < end && cursor + child.span > position)
          visit(child.hash, cursor, depth + 1);
        cursor += child.span;
        if (cursor >= end) break;
      }
    }
  };
  visit(root.rootNodeHash, 0, 1);
  return written;
}

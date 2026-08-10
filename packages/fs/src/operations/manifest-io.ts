import { decodeManifestNode, decodeManifestRoot, type ManifestEntry } from "../manifests/codec.js";
import { concatBytes } from "../utils/bytes.js";
import { ContentRepository } from "../sqlite/content-repository.js";
import { runUnitOfWork } from "../sqlite/unit-of-work.js";
import type { FilesystemSQLiteDriver } from "../sqlite-driver.js";
import type { RuntimeLimits, StorageLimits } from "../resources/limits.js";
import { AdmissionController } from "../resources/limits.js";
import { prepareContentStreaming } from "./streaming-prepare.js";
import type { ClosureCertificate } from "../sqlite/staging-repository.js";
import type { ContentCache } from "../resources/content-cache.js";

export interface PreparedManifest { readonly hash: Uint8Array; readonly size: number; readonly certificate: ClosureCertificate }

export async function prepareContent(driver: FilesystemSQLiteDriver, input: Uint8Array | ReadableStream<Uint8Array>, storage: StorageLimits, runtime: RuntimeLimits, admission: AdmissionController, signal?: AbortSignal, cache?: ContentCache): Promise<PreparedManifest> {
  return prepareContentStreaming(driver, input, storage, runtime, admission, signal, cache);
}

export function readManifestRange(repository: ContentRepository, manifestHash: Uint8Array, offset: number, length: number): Uint8Array {
  const rootBytes = repository.getManifestRoot(manifestHash); if (!rootBytes) throw new Error("ECORRUPT: missing manifest root");
  const root = decodeManifestRoot(rootBytes, manifestHash); if (length === 0 || offset >= root.fileSize) return new Uint8Array();
  const end = Math.min(root.fileSize, offset + length); const parts: Uint8Array[] = [];
  const visit = (hash: Uint8Array, nodeStart: number, depth: number): void => {
    if (depth > 8) throw new Error("ECORRUPT: manifest depth exceeded");
    const encoded = repository.getManifestNode(hash); if (!encoded) throw new Error("ECORRUPT: missing manifest node");
    const node = decodeManifestNode(encoded, hash); if (nodeStart >= end || nodeStart + node.span <= offset) return;
    if (node.kind === "leaf") {
      let position = nodeStart;
      for (const entry of node.entries) {
        const entryEnd = position + entry.length;
        if (position < end && entryEnd > offset) { const object = repository.getObject(entry.hash, entry.length); if (!object) throw new Error("ECORRUPT: missing CAS object"); parts.push(object.slice(Math.max(0, offset - position), Math.min(entry.length, end - position))); }
        position = entryEnd; if (position >= end) break;
      }
    } else { let position = nodeStart; for (const child of node.children) { if (position < end && position + child.span > offset) visit(child.hash, position, depth + 1); position += child.span; if (position >= end) break; } }
  };
  const rootNodeBytes = repository.getManifestNode(root.rootNodeHash); if (!rootNodeBytes) throw new Error("ECORRUPT: missing root manifest node");
  const rootNode = decodeManifestNode(rootNodeBytes, root.rootNodeHash); if (rootNode.span !== root.fileSize || rootNode.entryCount !== root.entryCount) throw new Error("ECORRUPT: manifest root totals mismatch");
  visit(root.rootNodeHash, 0, 1); return concatBytes(parts);
}

export function readManifestInto(repository: ContentRepository, manifestHash: Uint8Array, position: number, destination: Uint8Array, destinationOffset: number, length: number): number {
  if (!Number.isSafeInteger(position) || position < 0 || !Number.isSafeInteger(destinationOffset) || destinationOffset < 0 || !Number.isSafeInteger(length) || length < 0 || destinationOffset + length > destination.byteLength) throw new RangeError("invalid direct manifest read range");
  const rootBytes = repository.getManifestRoot(manifestHash); if (!rootBytes) throw new Error("ECORRUPT: missing manifest root"); const root = decodeManifestRoot(rootBytes, manifestHash); if (!length || position >= root.fileSize) return 0;
  const end = Math.min(root.fileSize, position + length); let written = 0;
  const visit = (hash: Uint8Array, nodeStart: number, depth: number): void => {
    if (depth > 8) throw new Error("ECORRUPT: manifest depth exceeded"); if (nodeStart >= end || nodeStart >= position + length) return;
    const encoded = repository.getManifestNode(hash); if (!encoded) throw new Error("ECORRUPT: missing manifest node"); const node = decodeManifestNode(encoded, hash); if (nodeStart + node.span <= position) return;
    if (node.kind === "leaf") { let cursor = nodeStart; for (const entry of node.entries) { const entryEnd = cursor + entry.length; if (cursor < end && entryEnd > position) { const object = repository.getObject(entry.hash, entry.length); if (!object) throw new Error("ECORRUPT: missing CAS object"); const start = Math.max(0, position - cursor); const stop = Math.min(entry.length, end - cursor); destination.set(object.subarray(start, stop), destinationOffset + written); written += stop - start; } cursor = entryEnd; if (cursor >= end) break; } }
    else { let cursor = nodeStart; for (const child of node.children) { if (cursor < end && cursor + child.span > position) visit(child.hash, cursor, depth + 1); cursor += child.span; if (cursor >= end) break; } }
  };
  visit(root.rootNodeHash, 0, 1); return written;
}

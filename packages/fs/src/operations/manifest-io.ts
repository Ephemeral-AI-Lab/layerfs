import { buildManifestFromEntries } from "../manifests/builder.js";
import { decodeManifestNode, decodeManifestRoot, type ManifestEntry } from "../manifests/codec.js";
import { StreamingFastCdc, DEFAULT_FASTCDC } from "../cdc/fastcdc.js";
import { sha256 } from "../cas/sha256.js";
import { bytesToHex, concatBytes } from "../utils/bytes.js";
import { ContentRepository } from "../sqlite/content-repository.js";
import { runUnitOfWork } from "../sqlite/unit-of-work.js";
import type { FilesystemSQLiteDriver } from "../sqlite-driver.js";
import type { RuntimeLimits, StorageLimits } from "../resources/limits.js";
import { AdmissionController } from "../resources/limits.js";

export interface PreparedManifest { readonly hash: Uint8Array; readonly size: number }

export async function prepareContent(driver: FilesystemSQLiteDriver, input: Uint8Array | ReadableStream<Uint8Array>, storage: StorageLimits, runtime: RuntimeLimits, admission: AdmissionController, signal?: AbortSignal): Promise<PreparedManifest> {
  const entries: ManifestEntry[] = []; const chunker = new StreamingFastCdc(DEFAULT_FASTCDC); let total = 0;
  const persist = (chunks: readonly Uint8Array[]): void => {
    for (const chunk of chunks) {
      total += chunk.byteLength; if (total > storage.maxFileBytes) throw new RangeError("file exceeds maxFileBytes");
      const hash = sha256(chunk); entries.push(Object.freeze({ hash, length: chunk.byteLength }));
      runUnitOfWork(driver, "write", { maxRows: storage.maxFinalTransactionRows, maxBytes: storage.maxFinalTransactionBytes }, (tx) => new ContentRepository(tx, storage).putObject(hash, chunk));
    }
  };
  if (input instanceof Uint8Array) {
    const release = admission.reserve(input.byteLength); try {
      for (let offset = 0; offset < input.byteLength; offset += runtime.maxWriteSessionBytes) persist(chunker.push(input.subarray(offset, offset + runtime.maxWriteSessionBytes)));
      persist(chunker.finish());
    } finally { release(); }
  } else {
    const reader = input.getReader();
    try {
      while (true) {
        if (signal?.aborted) throw new DOMException("The operation was aborted", "AbortError");
        const { done, value } = await reader.read(); if (done) break; if (!(value instanceof Uint8Array)) throw new TypeError("write stream chunks must be Uint8Array values");
        for (let offset = 0; offset < value.byteLength; offset += runtime.maxWriteSessionBytes) { const part = value.subarray(offset, offset + runtime.maxWriteSessionBytes); const release = admission.reserve(part.byteLength); try { persist(chunker.push(part)); } finally { release(); } }
      }
      persist(chunker.finish());
    } finally { reader.releaseLock(); }
  }
  const manifest = buildManifestFromEntries(entries, DEFAULT_FASTCDC);
  for (const node of manifest.nodes.values()) runUnitOfWork(driver, "write", { maxRows: storage.maxFinalTransactionRows, maxBytes: storage.maxFinalTransactionBytes }, (tx) => new ContentRepository(tx, storage).putManifestNode(node.hash, node.encoded));
  runUnitOfWork(driver, "write", { maxRows: storage.maxFinalTransactionRows, maxBytes: storage.maxFinalTransactionBytes }, (tx) => new ContentRepository(tx, storage).putManifestRoot(manifest.rootHash, manifest.root));
  return Object.freeze({ hash: manifest.rootHash, size: total });
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
        if (position < end && entryEnd > offset) { const object = repository.getObject(entry.hash); if (!object) throw new Error("ECORRUPT: missing CAS object"); parts.push(object.slice(Math.max(0, offset - position), Math.min(entry.length, end - position))); }
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
    if (node.kind === "leaf") { let cursor = nodeStart; for (const entry of node.entries) { const entryEnd = cursor + entry.length; if (cursor < end && entryEnd > position) { const object = repository.getObject(entry.hash); if (!object) throw new Error("ECORRUPT: missing CAS object"); const start = Math.max(0, position - cursor); const stop = Math.min(entry.length, end - cursor); destination.set(object.subarray(start, stop), destinationOffset + written); written += stop - start; } cursor = entryEnd; if (cursor >= end) break; } }
    else { let cursor = nodeStart; for (const child of node.children) { if (cursor < end && cursor + child.span > position) visit(child.hash, cursor, depth + 1); cursor += child.span; if (cursor >= end) break; } }
  };
  visit(root.rootNodeHash, 0, 1); return written;
}

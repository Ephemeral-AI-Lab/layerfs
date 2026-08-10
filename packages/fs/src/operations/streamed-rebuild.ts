import { sha256 } from "../cas/sha256.js";
import { StreamingFastCdc, type FastCdcConfiguration } from "../cdc/fastcdc.js";
import { buildManifestFromEntries, type BuiltManifestRoot, type ManifestBuildWorkspace } from "../manifests/builder.js";
import type { ManifestEntry } from "../manifests/codec.js";
import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import type { DiagnosticBuiltManifest } from "./full-rebuild.js";
import { DEFAULT_LOCAL_REBUILD_LIMITS, LocalRebuildLimitError, rebuildManifestLocally, type LocalContentEdit, type LocalRebuildLimits, type LocallyRebuiltManifest, type RandomAccessContentSource } from "./local-rebuild.js";

export interface StreamedObjectSink { putObject(hash: Uint8Array, bytes: Uint8Array): void }
export interface StreamedRebuildOptions { readonly readWindowBytes?: number; readonly manifestReadBatchRecords?: number; readonly maxManifestDepth?: number }
export interface StreamedRebuildMetrics { readonly sourceBytesRead: number; readonly bytesHashed: number; readonly objectCount: number; readonly largestSourceRead: number; readonly peakRetainedRecords: number }
export interface StreamedRebuildResult { readonly mode: "streamed-fallback"; readonly manifest: BuiltManifestRoot; readonly metrics: StreamedRebuildMetrics; readonly localLimitReason: string }
export interface LocalRebuildResult { readonly mode: "local"; readonly manifest: LocallyRebuiltManifest }

function validateEdit(source: RandomAccessContentSource, edit: LocalContentEdit): void {
  if (!Number.isSafeInteger(source.size) || source.size < 0) throw new RangeError("source size must be a nonnegative safe integer");
  if (!Number.isSafeInteger(edit.offset) || edit.offset < 0 || !Number.isSafeInteger(edit.deleteLength) || edit.deleteLength < 0 || edit.offset > source.size || edit.deleteLength > source.size - edit.offset) throw new RangeError("streamed edit is outside the source");
}

export function rebuildEditedContentStreaming(source: RandomAccessContentSource, edit: LocalContentEdit, parameters: FastCdcConfiguration, workspace: ManifestBuildWorkspace, objects: StreamedObjectSink, reason = "explicit streamed rebuild", options: StreamedRebuildOptions = {}): StreamedRebuildResult {
  validateEdit(source, edit);
  const readWindowBytes = checkedInteger(options.readWindowBytes ?? parameters.maximum, "readWindowBytes", 16 * 1024 * 1024);
  if (readWindowBytes === 0) throw new RangeError("readWindowBytes must be positive");
  let sourceBytesRead = 0; let bytesHashed = 0; let objectCount = 0; let largestSourceRead = 0;
  const read = (offset: number, length: number): Uint8Array => {
    const bytes = source.read(offset, length); if (!(bytes instanceof Uint8Array) || bytes.byteLength !== length) throw new Error("random-access source returned a partial range");
    sourceBytesRead = checkedAdd(sourceBytesRead, length); largestSourceRead = Math.max(largestSourceRead, length); return bytes;
  };
  function* entries(): Generator<ManifestEntry> {
    const chunker = new StreamingFastCdc(parameters);
    const accept = function* (input: Uint8Array): Generator<ManifestEntry> {
      for (const chunk of chunker.push(input)) { const hash = sha256(chunk); bytesHashed = checkedAdd(bytesHashed, chunk.byteLength); objectCount = checkedAdd(objectCount, 1); objects.putObject(hash, chunk); yield Object.freeze({ hash, length: chunk.byteLength }); }
    };
    for (let offset = 0; offset < edit.offset;) { const length = Math.min(readWindowBytes, edit.offset - offset); yield* accept(read(offset, length)); offset += length; }
    for (let offset = 0; offset < edit.insertBytes.byteLength;) { const length = Math.min(readWindowBytes, edit.insertBytes.byteLength - offset); yield* accept(edit.insertBytes.subarray(offset, offset + length)); offset += length; }
    for (let offset = edit.offset + edit.deleteLength; offset < source.size;) { const length = Math.min(readWindowBytes, source.size - offset); yield* accept(read(offset, length)); offset += length; }
    for (const chunk of chunker.finish()) { const hash = sha256(chunk); bytesHashed = checkedAdd(bytesHashed, chunk.byteLength); objectCount = checkedAdd(objectCount, 1); objects.putObject(hash, chunk); yield Object.freeze({ hash, length: chunk.byteLength }); }
  }
  const manifest = buildManifestFromEntries(entries(), parameters, workspace, { ...(options.manifestReadBatchRecords === undefined ? {} : { readBatchRecords: options.manifestReadBatchRecords }), ...(options.maxManifestDepth === undefined ? {} : { maxDepth: options.maxManifestDepth }) });
  return Object.freeze({ mode: "streamed-fallback", manifest, metrics: Object.freeze({ sourceBytesRead, bytesHashed, objectCount, largestSourceRead, peakRetainedRecords: manifest.peakRetainedRecords }), localLimitReason: reason });
}

export function rebuildManifestLocallyOrStream(source: RandomAccessContentSource, old: DiagnosticBuiltManifest, edit: LocalContentEdit, parameters: FastCdcConfiguration, workspace: ManifestBuildWorkspace, objects: StreamedObjectSink, localLimits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS, options: StreamedRebuildOptions = {}): LocalRebuildResult | StreamedRebuildResult {
  try { return Object.freeze({ mode: "local", manifest: rebuildManifestLocally(source, old, edit, localLimits) }); }
  catch (error) {
    if (!(error instanceof LocalRebuildLimitError)) throw error;
    return rebuildEditedContentStreaming(source, edit, parameters, workspace, objects, error.message, options);
  }
}

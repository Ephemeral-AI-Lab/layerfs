import { copyBytes, intrinsicByteLength } from "../cas/bytes.js";
import { sha256 } from "../cas/sha256.js";
import { StreamingFastCdc } from "../cdc/fastcdc.js";
import type { ManifestParameters } from "../manifests/codec.js";
import { checkedAdd } from "../resources/safe-integers.js";
import type {
  AdmissionController,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
import type { ContentCache } from "../cache/content-cache.js";
import type { AuthenticatedManifestEntry, OperationsStorage } from "./storage-ports.js";
import {
  prepareContentEntriesStreaming,
  prepareContentStreaming,
  type StagedManifestEntryInput,
  type StreamPreparedManifest,
} from "./streaming-prepare.js";

export interface DurableEditSource {
  readonly size: number;
  readonly parameters: ManifestParameters;
  read(offset: number, length: number): Uint8Array;
  entries(offset: number, limit: number): readonly AuthenticatedManifestEntry[];
}

export interface DurableContentEdit {
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertLength: number;
  /** Library-owned insertion bytes retained while preparation is in flight. */
  readonly retainedBytes?: number;
  readInsert(offset: number, length: number): Uint8Array;
}

export interface DurableEditPreparedManifest extends StreamPreparedManifest {
  readonly mode: "durable-path-copy" | "streamed-fallback";
  readonly pathCopyReason?: string;
}

class DurablePathCopyFallbackError extends Error {}

function validateInputs(source: DurableEditSource, edit: DurableContentEdit): number {
  if (!Number.isSafeInteger(source.size) || source.size < 0)
    throw new RangeError("source size must be a nonnegative safe integer");
  if (
    !Number.isSafeInteger(edit.offset) ||
    edit.offset < 0 ||
    !Number.isSafeInteger(edit.deleteLength) ||
    edit.deleteLength < 0 ||
    !Number.isSafeInteger(edit.insertLength) ||
    edit.insertLength < 0 ||
    (edit.retainedBytes !== undefined &&
      (!Number.isSafeInteger(edit.retainedBytes) ||
        edit.retainedBytes < 0 ||
        edit.retainedBytes > edit.insertLength)) ||
    edit.offset > source.size ||
    edit.deleteLength > source.size - edit.offset
  )
    throw new RangeError("durable edit is outside the source");
  return checkedAdd(source.size - edit.deleteLength, edit.insertLength);
}

function exactRead(
  read: (offset: number, length: number) => Uint8Array,
  offset: number,
  length: number,
  label: string,
): Uint8Array {
  const bytes = copyBytes(read(offset, length));
  if (intrinsicByteLength(bytes) !== length)
    throw new Error(`ECORRUPT: ${label} returned a partial range`);
  return bytes;
}

function readEditedRange(
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  position: number,
  length: number,
): Uint8Array {
  if (
    !Number.isSafeInteger(position) ||
    !Number.isSafeInteger(length) ||
    position < 0 ||
    length < 0 ||
    position + length > newSize
  )
    throw new RangeError("edited read is outside the result");
  const output = new Uint8Array(length);
  const dirtyNewEnd = checkedAdd(edit.offset, edit.insertLength);
  const delta = edit.insertLength - edit.deleteLength;
  let cursor = position;
  let written = 0;
  while (written < length) {
    if (cursor < edit.offset) {
      const count = Math.min(length - written, edit.offset - cursor);
      output.set(exactRead(source.read.bind(source), cursor, count, "source"), written);
      cursor += count;
      written += count;
      continue;
    }
    if (cursor < dirtyNewEnd) {
      const insertionOffset = cursor - edit.offset;
      const count = Math.min(length - written, edit.insertLength - insertionOffset);
      output.set(
        exactRead(edit.readInsert.bind(edit), insertionOffset, count, "insertion"),
        written,
      );
      cursor += count;
      written += count;
      continue;
    }
    const oldOffset = cursor - delta;
    const count = length - written;
    output.set(
      exactRead(source.read.bind(source), oldOffset, count, "source"),
      written,
    );
    cursor += count;
    written += count;
  }
  return output;
}

function discoverScanStart(
  source: DurableEditSource,
  edit: DurableContentEdit,
): number {
  if (source.size === 0) return 0;
  const probe = edit.offset === source.size ? source.size - 1 : edit.offset;
  const selected = source.entries(probe, 1)[0];
  if (
    !selected ||
    selected.offset > probe ||
    selected.offset + selected.length <= probe
  )
    throw new Error("ECORRUPT: authenticated manifest did not contain edit offset");
  return selected.offset;
}

function isAuthenticatedBoundary(source: DurableEditSource, offset: number): boolean {
  if (offset === source.size) return true;
  if (offset < 0 || offset > source.size) return false;
  return source.entries(offset, 1)[0]?.offset === offset;
}

function affectedPathCopyEntries(
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  scanStart: number,
  maxAffectedBytes: number,
): {
  readonly entries: readonly StagedManifestEntryInput[];
  readonly reconnectOldOffset: number;
} {
  const delta = edit.insertLength - edit.deleteLength;
  const dirtyOldEnd = checkedAdd(edit.offset, edit.deleteLength);
  const dirtyNewEnd = checkedAdd(edit.offset, edit.insertLength);
  const entries: StagedManifestEntryInput[] = [];
  let affectedBytes = 0;
  let newCursor = scanStart;
  let feedCursor = scanStart;
  let reconnectOldOffset: number | undefined;
  const acceptReconnect = (): boolean => {
    if (newCursor < dirtyNewEnd) return false;
    const mappedOld = newCursor - delta;
    if (mappedOld < dirtyOldEnd || !isAuthenticatedBoundary(source, mappedOld))
      return false;
    reconnectOldOffset = mappedOld;
    return true;
  };
  if (acceptReconnect())
    return Object.freeze({
      entries: Object.freeze([]),
      reconnectOldOffset: reconnectOldOffset!,
    });
  const chunker = new StreamingFastCdc(source.parameters);
  const reconnected = Object.freeze({ kind: "reconnected" });
  const capped = Object.freeze({ kind: "capped" });
  const acceptChunk = (borrowed: Uint8Array): void => {
    const chunk = copyBytes(borrowed);
    if (chunk.byteLength > maxAffectedBytes - affectedBytes) throw capped;
    affectedBytes = checkedAdd(affectedBytes, chunk.byteLength);
    entries.push(
      Object.freeze({
        hash: sha256(chunk),
        length: chunk.byteLength,
        bytes: chunk,
      }),
    );
    newCursor = checkedAdd(newCursor, chunk.byteLength);
    if (acceptReconnect()) throw reconnected;
  };
  while (feedCursor < newSize && reconnectOldOffset === undefined) {
    if (affectedBytes + chunker.bufferedBytes >= maxAffectedBytes)
      throw new DurablePathCopyFallbackError(
        "local FastCDC reconnection exceeded its bounded durable window",
      );
    const length = Math.min(
      source.parameters.maximum,
      newSize - feedCursor,
      maxAffectedBytes - affectedBytes - chunker.bufferedBytes,
    );
    if (length <= 0)
      throw new DurablePathCopyFallbackError(
        "local FastCDC reconnection exceeded its bounded durable window",
      );
    const input = readEditedRange(source, edit, newSize, feedCursor, length);
    feedCursor += length;
    try {
      chunker.drain(input, acceptChunk, feedCursor === newSize);
    } catch (error) {
      if (error === reconnected) break;
      if (error === capped)
        throw new DurablePathCopyFallbackError(
          "local FastCDC reconnection exceeded its bounded durable window",
        );
      throw error;
    }
  }
  if (reconnectOldOffset === undefined)
    throw new Error("ECORRUPT: FastCDC rebuild did not reconnect at end of file");
  return Object.freeze({
    entries: Object.freeze(entries),
    reconnectOldOffset,
  });
}

function* copyAuthenticatedEntries(
  source: DurableEditSource,
  start: number,
  end: number,
  batchSize: number,
): Generator<StagedManifestEntryInput> {
  let cursor = start;
  while (cursor < end) {
    const rows = source.entries(cursor, batchSize);
    if (!rows.length || rows[0]!.offset !== cursor)
      throw new DurablePathCopyFallbackError(
        "authenticated entry cursor rejected a stale derived boundary",
      );
    for (const row of rows) {
      if (row.offset !== cursor || row.offset + row.length > end)
        throw new DurablePathCopyFallbackError(
          "authenticated entry cursor rejected a stale derived span",
        );
      yield Object.freeze({ hash: copyBytes(row.hash), length: row.length });
      cursor = checkedAdd(cursor, row.length);
      if (cursor === end) break;
    }
  }
}

function editedContentStream(
  source: DurableEditSource,
  edit: DurableContentEdit,
  newSize: number,
  readWindowBytes: number,
): ReadableStream<Uint8Array> {
  let position = 0;
  return new ReadableStream<Uint8Array>({
    pull(controller) {
      if (position === newSize) {
        controller.close();
        return;
      }
      const length = Math.min(readWindowBytes, newSize - position);
      const bytes = readEditedRange(source, edit, newSize, position, length);
      position += length;
      controller.enqueue(bytes);
    },
  });
}

export async function prepareDurableEditedContent(
  port: OperationsStorage,
  source: DurableEditSource,
  edit: DurableContentEdit,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  cache?: ContentCache,
  clock: () => number = Date.now,
): Promise<DurableEditPreparedManifest> {
  const newSize = validateInputs(source, edit);
  if (newSize > storage.maxFileBytes)
    throw new RangeError("edited file exceeds maxFileBytes");
  const maxAffectedBytes = Math.max(
    source.parameters.maximum,
    Math.min(
      runtime.maxWriteSessionBytes,
      runtime.maxPendingWriteBytes,
      Math.floor(runtime.maxManagedResidentBytes / 8),
    ),
  );
  let reason: string | undefined;
  let releaseAttempt: (() => void) | undefined;
  try {
    cache?.makeRoom(maxAffectedBytes);
    releaseAttempt = admission.reserve(maxAffectedBytes);
    const scanStart = discoverScanStart(source, edit);
    const affected = affectedPathCopyEntries(
      source,
      edit,
      newSize,
      scanStart,
      maxAffectedBytes,
    );
    const entries = (function* (): Generator<StagedManifestEntryInput> {
      yield* copyAuthenticatedEntries(source, 0, scanStart, storage.maxQueryBatchSize);
      yield* affected.entries;
      yield* copyAuthenticatedEntries(
        source,
        affected.reconnectOldOffset,
        source.size,
        storage.maxQueryBatchSize,
      );
    })();
    const prepared = await prepareContentEntriesStreaming(
      port,
      entries,
      source.parameters,
      newSize,
      storage,
      runtime,
      admission,
      cache,
      clock,
    );
    return Object.freeze({ ...prepared, mode: "durable-path-copy" });
  } catch (error) {
    if (!(error instanceof DurablePathCopyFallbackError)) throw error;
    reason = error.message;
  } finally {
    releaseAttempt?.();
  }
  const prepared = await prepareContentStreaming(
    port,
    editedContentStream(
      source,
      edit,
      newSize,
      Math.max(
        1,
        Math.min(
          runtime.maxWriteSessionBytes,
          runtime.maxQueryBatchBytes,
          Math.floor(storage.maxFinalTransactionBytes / 2),
        ),
      ),
    ),
    storage,
    runtime,
    admission,
    undefined,
    cache,
    clock,
  );
  return Object.freeze({
    ...prepared,
    mode: "streamed-fallback",
    pathCopyReason: reason,
  });
}

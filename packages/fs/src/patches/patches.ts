import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import { intrinsicByteRange } from "../resources/byte-capacity.js";

export interface StructuralPatch {
  readonly sequence: number;
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertBytes: Uint8Array;
}
export const MAX_STRUCTURAL_PATCHES = 256;

interface BytePiece {
  readonly bytes: Uint8Array;
  readonly start: number;
  readonly length: number;
}

export interface StructuralPatchMetrics {
  readonly copiedBytes: number;
  readonly peakSegments: number;
  readonly metadataSegmentsCreated: number;
}

export interface StructuralPatchResult {
  readonly bytes: Uint8Array;
  readonly metrics: StructuralPatchMetrics;
}

function pieceRange(
  pieces: readonly BytePiece[],
  start: number,
  end: number,
): BytePiece[] {
  const result: BytePiece[] = [];
  let logical = 0;
  for (const piece of pieces) {
    const pieceEnd = logical + piece.length;
    const overlapStart = Math.max(start, logical);
    const overlapEnd = Math.min(end, pieceEnd);
    if (overlapStart < overlapEnd)
      result.push(
        Object.freeze({
          bytes: piece.bytes,
          start: piece.start + overlapStart - logical,
          length: overlapEnd - overlapStart,
        }),
      );
    logical = pieceEnd;
    if (logical >= end) break;
  }
  return result;
}

/**
 * Applies all edits to bounded piece metadata and copies payload bytes exactly
 * once into the final result. No evolving full-file intermediate is created.
 */
export function applyStructuralPatchesWithMetrics(
  base: Uint8Array,
  patches: readonly StructuralPatch[],
  maxPatches = MAX_STRUCTURAL_PATCHES,
): StructuralPatchResult {
  base = intrinsicByteRange(base);
  checkedInteger(maxPatches, "maxPatches", MAX_STRUCTURAL_PATCHES);
  if (maxPatches === 0) throw new RangeError("maxPatches must be positive");
  if (patches.length > maxPatches)
    throw new RangeError("structural patch count exceeds maxPatches");
  const ordered = patches
    .map((patch) => {
      const snapshot = {
        sequence: patch.sequence,
        offset: patch.offset,
        deleteLength: patch.deleteLength,
        insertBytes: patch.insertBytes,
      };
      return Object.freeze({
        ...snapshot,
        insertBytes: intrinsicByteRange(snapshot.insertBytes),
      });
    })
    .sort((left, right) => left.sequence - right.sequence);
  let validatedSize = base.byteLength;
  let expectedSequence = 0;
  for (const patch of ordered) {
    checkedInteger(patch.sequence, "patch.sequence");
    if (patch.sequence !== expectedSequence)
      throw new Error("structural patch sequence must be contiguous from zero");
    expectedSequence += 1;
    checkedInteger(patch.offset, "patch.offset", validatedSize);
    checkedInteger(
      patch.deleteLength,
      "patch.deleteLength",
      validatedSize - patch.offset,
    );
    validatedSize = checkedAdd(
      validatedSize - patch.deleteLength,
      patch.insertBytes.byteLength,
      "patched size",
    );
  }

  let pieces: BytePiece[] = base.byteLength
    ? [Object.freeze({ bytes: base, start: 0, length: base.byteLength })]
    : [];
  let logicalSize = base.byteLength;
  let peakSegments = pieces.length;
  let metadataSegmentsCreated = pieces.length;
  for (const patch of ordered) {
    const next = [
      ...pieceRange(pieces, 0, patch.offset),
      ...(patch.insertBytes.byteLength
        ? [
            Object.freeze({
              bytes: patch.insertBytes,
              start: 0,
              length: patch.insertBytes.byteLength,
            }),
          ]
        : []),
      ...pieceRange(pieces, patch.offset + patch.deleteLength, logicalSize),
    ];
    pieces = next;
    logicalSize = logicalSize - patch.deleteLength + patch.insertBytes.byteLength;
    peakSegments = Math.max(peakSegments, pieces.length);
    metadataSegmentsCreated = checkedAdd(
      metadataSegmentsCreated,
      pieces.length,
      "piece-table metadata count",
    );
  }
  if (logicalSize !== validatedSize)
    throw new Error("piece-table result size differs from validated patch size");
  const bytes = new Uint8Array(validatedSize);
  let writeOffset = 0;
  for (const piece of pieces) {
    bytes.set(
      intrinsicByteRange(piece.bytes, piece.start, piece.start + piece.length),
      writeOffset,
    );
    writeOffset += piece.length;
  }
  return Object.freeze({
    bytes,
    metrics: Object.freeze({
      copiedBytes: writeOffset,
      peakSegments,
      metadataSegmentsCreated,
    }),
  });
}

export function applyStructuralPatches(
  base: Uint8Array,
  patches: readonly StructuralPatch[],
  maxPatches = MAX_STRUCTURAL_PATCHES,
): Uint8Array {
  return applyStructuralPatchesWithMetrics(base, patches, maxPatches).bytes;
}

export function replaceRange(
  bytes: Uint8Array,
  offset: number,
  deleteLength: number,
  insertBytes: Uint8Array,
): Uint8Array {
  return applyStructuralPatches(bytes, [
    { sequence: 0, offset, deleteLength, insertBytes },
  ]);
}

export function truncateBytes(bytes: Uint8Array, size: number): Uint8Array {
  bytes = intrinsicByteRange(bytes);
  checkedInteger(size, "size");
  if (size <= bytes.byteLength) {
    const result = new Uint8Array(size);
    result.set(intrinsicByteRange(bytes, 0, size));
    return result;
  }
  const result = new Uint8Array(size);
  result.set(bytes);
  return result;
}

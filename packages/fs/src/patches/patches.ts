import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";

function concatBytes(parts: readonly Uint8Array[]): Uint8Array { const length = parts.reduce((sum, part) => checkedAdd(sum, part.byteLength), 0); const result = new Uint8Array(length); let offset = 0; for (const part of parts) { result.set(part, offset); offset += part.byteLength; } return result; }

export interface StructuralPatch { readonly sequence: number; readonly offset: number; readonly deleteLength: number; readonly insertBytes: Uint8Array }

export function applyStructuralPatches(base: Uint8Array, patches: readonly StructuralPatch[]): Uint8Array {
  let result: Uint8Array = base.slice();
  let expectedSequence = 0;
  for (const patch of [...patches].sort((a, b) => a.sequence - b.sequence)) {
    checkedInteger(patch.sequence, "patch.sequence");
    if (patch.sequence !== expectedSequence) throw new Error("structural patch sequence must be contiguous from zero");
    expectedSequence += 1;
    checkedInteger(patch.offset, "patch.offset", result.byteLength);
    checkedInteger(patch.deleteLength, "patch.deleteLength", result.byteLength - patch.offset);
    const end = checkedAdd(patch.offset, patch.deleteLength);
    result = concatBytes([result.subarray(0, patch.offset), patch.insertBytes, result.subarray(end)]);
  }
  return result;
}

export function replaceRange(bytes: Uint8Array, offset: number, deleteLength: number, insertBytes: Uint8Array): Uint8Array {
  return applyStructuralPatches(bytes, [{ sequence: 0, offset, deleteLength, insertBytes }]);
}

export function truncateBytes(bytes: Uint8Array, size: number): Uint8Array {
  checkedInteger(size, "size");
  if (size <= bytes.byteLength) return bytes.slice(0, size);
  const result = new Uint8Array(size);
  result.set(bytes);
  return result;
}

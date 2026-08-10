import type { ManifestChild, ManifestEntry } from "./codec.js";

// Manifest grouping is versioned independently from byte chunking even though
// both v1 algorithms deliberately use the same deterministic gear generator.
const MANIFEST_GROUPING_GEAR_V1: Uint32Array = (() => {
  const table = new Uint32Array(256);
  let seed = 0x9e3779b9;
  for (let index = 0; index < table.length; index += 1) {
    seed = (seed ^ (seed << 13)) >>> 0;
    seed = (seed ^ (seed >>> 17)) >>> 0;
    seed = (seed ^ (seed << 5)) >>> 0;
    table[index] = seed;
  }
  return table;
})();

function recordBytes(record: ManifestEntry | ManifestChild): Uint8Array {
  if ("length" in record) {
    const bytes = new Uint8Array(36);
    bytes.set(record.hash);
    new DataView(bytes.buffer).setUint32(32, record.length, true);
    return bytes;
  }
  const bytes = new Uint8Array(48);
  const view = new DataView(bytes.buffer);
  bytes.set(record.hash);
  view.setBigUint64(32, BigInt(record.span), true);
  view.setBigUint64(40, BigInt(record.entryCount), true);
  return bytes;
}

export function advanceManifestGroupingState(
  state: bigint,
  record: ManifestEntry | ManifestChild,
): bigint {
  let next = state;
  for (const byte of recordBytes(record))
    next =
      ((next << 1n) + BigInt(MANIFEST_GROUPING_GEAR_V1[byte]!)) &
      0xffff_ffff_ffff_ffffn;
  return next;
}

export function isManifestGroupBoundary(
  count: number,
  state: bigint,
  minimum: number,
  target: number,
  maximum: number,
): boolean {
  return count >= maximum || (count >= minimum && (state & BigInt(target - 1)) === 0n);
}

import type { ManifestChild, ManifestEntry } from "./codec.js";

export interface ManifestGroupingConfiguration {
  readonly minimum: number;
  readonly target: number;
  readonly maximum: number;
}

export const LEAF_MANIFEST_GROUPING: ManifestGroupingConfiguration = Object.freeze({
  minimum: 64,
  target: 128,
  maximum: 256,
});
export const INTERNAL_MANIFEST_GROUPING: ManifestGroupingConfiguration = Object.freeze({
  minimum: 32,
  target: 64,
  maximum: 128,
});

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

/** Prove that a stored node is exactly one group from the canonical record scan. */
export function validateCanonicalManifestGroup(
  records: readonly (ManifestEntry | ManifestChild)[],
  configuration: ManifestGroupingConfiguration,
  finalGroup: boolean,
): void {
  if (records.length === 0) throw new Error("empty canonical manifest group");
  if (records.length > configuration.maximum)
    throw new Error("manifest group exceeds its canonical maximum");
  let state = 0n;
  let boundary = false;
  for (let index = 0; index < records.length; index += 1) {
    state = advanceManifestGroupingState(state, records[index]!);
    boundary = isManifestGroupBoundary(
      index + 1,
      state,
      configuration.minimum,
      configuration.target,
      configuration.maximum,
    );
    if (boundary && index + 1 < records.length)
      throw new Error("manifest group continues past its canonical boundary");
  }
  if (!finalGroup && !boundary)
    throw new Error("manifest group ends before its canonical boundary");
}

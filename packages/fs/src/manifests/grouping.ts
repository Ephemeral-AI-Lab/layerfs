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

// Each record updates all 64 state bits. Canonical boundaries use the high
// bits: low bits are biased by the trailing zero bytes in ordinary records.
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

function advanceByte(state: bigint, byte: number): bigint {
  return (
    ((state << 1n) + BigInt(MANIFEST_GROUPING_GEAR_V1[byte]!)) & 0xffff_ffff_ffff_ffffn
  );
}

function advanceLittleEndian(state: bigint, value: number, bytes: number): bigint {
  let next = state;
  let remaining = BigInt(value);
  for (let index = 0; index < bytes; index += 1) {
    next = advanceByte(next, Number(remaining & 0xffn));
    remaining >>= 8n;
  }
  return next;
}

export function advanceManifestGroupingState(
  state: bigint,
  record: ManifestEntry | ManifestChild,
): bigint {
  let next = state;
  for (const byte of record.hash) next = advanceByte(next, byte);
  return "length" in record
    ? advanceLittleEndian(next, record.length, 4)
    : advanceLittleEndian(
        advanceLittleEndian(next, record.span, 8),
        record.entryCount,
        8,
      );
}

export function isManifestGroupBoundary(
  count: number,
  state: bigint,
  minimum: number,
  target: number,
  maximum: number,
): boolean {
  const bits = Math.log2(target);
  const high = state >> BigInt(64 - bits);
  return count >= maximum || (count >= minimum && high === 0n);
}

/** Prove that a stored node is exactly one group from the canonical record scan. */
export function validateCanonicalManifestGroup(
  records: readonly (ManifestEntry | ManifestChild)[],
  configuration: ManifestGroupingConfiguration,
  finalGroup: boolean,
): void {
  configuration = Object.freeze({
    minimum: configuration.minimum,
    target: configuration.target,
    maximum: configuration.maximum,
  });
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

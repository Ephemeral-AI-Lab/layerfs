import { ReplicationError } from "./errors.js";

export const MAX_CANONICAL_TEXT_BYTES = 256;
export const MAX_CANONICAL_ARRAY_ENTRIES = 64;
export const MAX_CANONICAL_ERROR_TEXT_BYTES = 4096;
export const PRE_NEGOTIATION_ENVELOPE_BYTES = 64 * 1024;

const TEXT_ENCODER = new TextEncoder();

function containsUnpairedSurrogate(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return true;
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return true;
    }
  }
  return false;
}

export function canonicalUtf8(
  value: string,
  name: string,
  maximumBytes = MAX_CANONICAL_TEXT_BYTES,
  allowEmpty = false,
): Uint8Array {
  if (typeof value !== "string") throw new TypeError(`${name} must be a string`);
  if (containsUnpairedSurrogate(value))
    throw new ReplicationError(
      "ProtocolMismatch",
      `${name} contains an unpaired UTF-16 surrogate`,
    );
  const bytes = TEXT_ENCODER.encode(value);
  if ((!allowEmpty && bytes.byteLength === 0) || bytes.byteLength > maximumBytes)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${name} must contain ${allowEmpty ? "at most" : "between 1 and"} ${maximumBytes} UTF-8 bytes`,
    );
  return bytes;
}

export function positiveSafeInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name} must be a positive safe integer`,
    );
  return value;
}

export function nonnegativeSafeInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${name} must be a nonnegative safe integer`,
    );
  return value;
}

export function exactDigest(value: Uint8Array, name: string): Uint8Array {
  if (!(value instanceof Uint8Array) || value.byteLength !== 32)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${name} must contain exactly 32 bytes`,
    );
  return new Uint8Array(value);
}

export function boundedArray<T>(
  value: readonly T[],
  name: string,
  maximum = MAX_CANONICAL_ARRAY_ENTRIES,
): readonly T[] {
  if (!Array.isArray(value) || value.length > maximum)
    throw new ReplicationError(
      "ProtocolMismatch",
      `${name} must contain at most ${maximum} entries`,
    );
  return value;
}

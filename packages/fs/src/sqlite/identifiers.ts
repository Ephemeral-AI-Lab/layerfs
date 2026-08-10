import { utf8ByteLength } from "../namespace/utf8.js";

export const MAX_DURABLE_IDENTIFIER_BYTES = 128;

export function validateDurableIdentifier(value: string, label: string): void {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0"))
    throw new RangeError(`${label} is invalid`);
  if (utf8ByteLength(value) > MAX_DURABLE_IDENTIFIER_BYTES)
    throw new RangeError(
      `${label} exceeds ${MAX_DURABLE_IDENTIFIER_BYTES} UTF-8 bytes`,
    );
}

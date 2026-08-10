export function encodeUtf8(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

/** Exact TextEncoder UTF-8 length without allocating the encoded value. */
export function utf8ByteLength(value: string): number {
  let bytes = 0;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code <= 0x7f) bytes += 1;
    else if (code <= 0x7ff) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else bytes += 3;
    } else bytes += 3;
    if (!Number.isSafeInteger(bytes)) throw new RangeError("UTF-8 length overflow");
  }
  return bytes;
}
export function decodeUtf8(value: Uint8Array): string {
  return new TextDecoder("utf-8", { fatal: true }).decode(value);
}

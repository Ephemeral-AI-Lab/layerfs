const TYPED_ARRAY_PROTOTYPE = Object.getPrototypeOf(Uint8Array.prototype) as object;
const typedArrayBuffer = Object.getOwnPropertyDescriptor(
  TYPED_ARRAY_PROTOTYPE,
  "buffer",
)!.get!;
const typedArrayByteOffset = Object.getOwnPropertyDescriptor(
  TYPED_ARRAY_PROTOTYPE,
  "byteOffset",
)!.get!;
const typedArrayByteLength = Object.getOwnPropertyDescriptor(
  TYPED_ARRAY_PROTOTYPE,
  "byteLength",
)!.get!;

export function intrinsicByteLength(value: Uint8Array): number {
  try {
    return Reflect.apply(typedArrayByteLength, value, []) as number;
  } catch {
    throw new TypeError("expected a Uint8Array");
  }
}

export function intrinsicByteRange(
  value: Uint8Array,
  start = 0,
  end?: number,
): Uint8Array {
  const byteLength = intrinsicByteLength(value);
  end ??= byteLength;
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(end) ||
    start < 0 ||
    end < start ||
    end > byteLength
  )
    throw new RangeError("byte range is outside the input");
  const buffer = Reflect.apply(typedArrayBuffer, value, []) as ArrayBufferLike;
  const byteOffset = Reflect.apply(typedArrayByteOffset, value, []) as number;
  return new Uint8Array(buffer, byteOffset + start, end - start);
}

/** Always returns a detached, plain Uint8Array, including for Buffer/subclass inputs. */
export function copyBytes(value: Uint8Array, start = 0, end?: number): Uint8Array {
  const source = intrinsicByteRange(value, start, end);
  const output = new Uint8Array(source.byteLength);
  output.set(source);
  return output;
}

export function freezeBytes(value: Uint8Array): Uint8Array {
  return copyBytes(value);
}

export function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  left = intrinsicByteRange(left);
  right = intrinsicByteRange(right);
  if (left.byteLength !== right.byteLength) return false;
  let different = 0;
  for (let index = 0; index < left.byteLength; index += 1)
    different |= left[index]! ^ right[index]!;
  return different === 0;
}

const HEX_TABLE: readonly string[] = Array.from({ length: 256 }, (_, value) =>
  value.toString(16).padStart(2, "0"),
);

export function bytesToHex(bytes: Uint8Array): string {
  bytes = intrinsicByteRange(bytes);
  let result = "";
  for (let index = 0; index < bytes.byteLength; index += 1)
    result += HEX_TABLE[bytes[index]!]!;
  return result;
}

export function hexToBytes(value: string, expectedBytes?: number): Uint8Array {
  if (
    !/^(?:[0-9a-f]{2})+$/.test(value) ||
    (expectedBytes !== undefined && value.length !== expectedBytes * 2)
  )
    throw new TypeError("invalid lowercase hexadecimal byte string");
  const result = new Uint8Array(value.length / 2);
  for (let index = 0; index < result.length; index += 1)
    result[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return result;
}

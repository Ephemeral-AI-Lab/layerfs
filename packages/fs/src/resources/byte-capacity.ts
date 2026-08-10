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

/** Reads the typed-array internal slot without invoking a subclass getter. */
export function intrinsicByteLength(value: Uint8Array): number {
  try {
    return Reflect.apply(typedArrayByteLength, value, []) as number;
  } catch {
    throw new TypeError("expected a Uint8Array");
  }
}

/** Creates a plain borrowed view without invoking caller-overridable methods. */
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

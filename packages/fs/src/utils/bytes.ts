export const MAX_SAFE_INTEGER = Number.MAX_SAFE_INTEGER;

export function checkedInteger(value: number, name: string, maximum = MAX_SAFE_INTEGER): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > maximum) {
    throw new RangeError(`${name} must be a safe integer in [0, ${maximum}]`);
  }
  return value;
}

export function checkedAdd(left: number, right: number, name = "sum"): number {
  checkedInteger(left, `${name}.left`);
  checkedInteger(right, `${name}.right`);
  const result = left + right;
  return checkedInteger(result, name);
}

export function checkedMultiply(left: number, right: number, name = "product"): number {
  checkedInteger(left, `${name}.left`);
  checkedInteger(right, `${name}.right`);
  const result = left * right;
  return checkedInteger(result, name);
}

export function freezeBytes(value: Uint8Array): Uint8Array {
  return value.slice();
}

export function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  let different = 0;
  for (let index = 0; index < left.byteLength; index += 1) {
    different |= left[index]! ^ right[index]!;
  }
  return different === 0;
}

export function concatBytes(parts: readonly Uint8Array[]): Uint8Array {
  const length = parts.reduce((sum, part) => checkedAdd(sum, part.byteLength), 0);
  const result = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.byteLength;
  }
  return result;
}

export function bytesToHex(bytes: Uint8Array): string {
  let result = "";
  for (const byte of bytes) result += byte.toString(16).padStart(2, "0");
  return result;
}

export function hexToBytes(value: string, expectedBytes?: number): Uint8Array {
  if (!/^(?:[0-9a-f]{2})+$/.test(value) || (expectedBytes !== undefined && value.length !== expectedBytes * 2)) {
    throw new TypeError("invalid lowercase hexadecimal byte string");
  }
  const result = new Uint8Array(value.length / 2);
  for (let index = 0; index < result.length; index += 1) {
    result[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  }
  return result;
}

export function writeU64(view: DataView, offset: number, value: number): void {
  checkedInteger(value, "uint64");
  view.setBigUint64(offset, BigInt(value), true);
}

export function readU64(view: DataView, offset: number, name: string): number {
  const value = view.getBigUint64(offset, true);
  if (value > BigInt(MAX_SAFE_INTEGER)) throw new RangeError(`${name} exceeds Number.MAX_SAFE_INTEGER`);
  return Number(value);
}

export function utf8(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

export function decodeUtf8(value: Uint8Array): string {
  return new TextDecoder("utf-8", { fatal: true }).decode(value);
}


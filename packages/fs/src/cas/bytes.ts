export function freezeBytes(value: Uint8Array): Uint8Array { return value.slice(); }

export function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  let different = 0; for (let index = 0; index < left.byteLength; index += 1) different |= left[index]! ^ right[index]!;
  return different === 0;
}

export function bytesToHex(bytes: Uint8Array): string {
  let result = ""; for (const byte of bytes) result += byte.toString(16).padStart(2, "0"); return result;
}

export function hexToBytes(value: string, expectedBytes?: number): Uint8Array {
  if (!/^(?:[0-9a-f]{2})+$/.test(value) || (expectedBytes !== undefined && value.length !== expectedBytes * 2)) throw new TypeError("invalid lowercase hexadecimal byte string");
  const result = new Uint8Array(value.length / 2); for (let index = 0; index < result.length; index += 1) result[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16); return result;
}

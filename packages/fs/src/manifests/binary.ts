import { MAX_SAFE_INTEGER, checkedInteger } from "../resources/safe-integers.js";

export function writeU64(view: DataView, offset: number, value: number): void {
  checkedInteger(value, "uint64");
  view.setBigUint64(offset, BigInt(value), true);
}
export function readU64(view: DataView, offset: number, name: string): number {
  const value = view.getBigUint64(offset, true);
  if (value > BigInt(MAX_SAFE_INTEGER))
    throw new RangeError(`${name} exceeds Number.MAX_SAFE_INTEGER`);
  return Number(value);
}

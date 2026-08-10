import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";

export type CowPageBytes = 4096 | 8192 | 16384;
export interface DirtyRange { readonly start: number; readonly end: number }
export interface CowPage { readonly index: number; readonly bytes: Uint8Array }

export function validateCowPageBytes(value: number): asserts value is CowPageBytes {
  if (value !== 4096 && value !== 8192 && value !== 16384) throw new RangeError("COW page size must be 4096, 8192, or 16384");
}

export function pageIndex(offset: number, pageBytes: CowPageBytes): number {
  checkedInteger(offset, "offset");
  return Math.floor(offset / pageBytes);
}

export function pageRange(offset: number, length: number, pageBytes: CowPageBytes): readonly number[] {
  checkedInteger(offset, "offset");
  checkedInteger(length, "length");
  if (length === 0) return [];
  const end = checkedAdd(offset, length);
  const first = pageIndex(offset, pageBytes);
  const last = pageIndex(end - 1, pageBytes);
  return Array.from({ length: last - first + 1 }, (_, index) => first + index);
}

export function mergeDirtyRanges(ranges: readonly DirtyRange[]): DirtyRange[] {
  const sorted = ranges.map(({ start, end }) => {
    checkedInteger(start, "range.start"); checkedInteger(end, "range.end");
    if (end < start) throw new RangeError("dirty range end precedes start");
    return { start, end };
  }).filter(({ start, end }) => end > start).sort((a, b) => a.start - b.start || a.end - b.end);
  const result: DirtyRange[] = [];
  for (const range of sorted) {
    const previous = result.at(-1);
    if (previous && range.start <= previous.end) result[result.length - 1] = { start: previous.start, end: Math.max(previous.end, range.end) };
    else result.push(range);
  }
  return result;
}

export function writeCowPages(base: Uint8Array, offset: number, content: Uint8Array, pageBytes: CowPageBytes): CowPage[] {
  validateCowPageBytes(pageBytes);
  const finalSize = Math.max(base.byteLength, checkedAdd(offset, content.byteLength));
  const pages: CowPage[] = [];
  for (const index of pageRange(offset, content.byteLength, pageBytes)) {
    const start = index * pageBytes;
    const end = Math.min(start + pageBytes, finalSize);
    const bytes = new Uint8Array(end - start);
    if (start < base.byteLength) bytes.set(base.subarray(start, Math.min(end, base.byteLength)));
    const writeStart = Math.max(start, offset);
    const writeEnd = Math.min(end, offset + content.byteLength);
    bytes.set(content.subarray(writeStart - offset, writeEnd - offset), writeStart - start);
    pages.push(Object.freeze({ index, bytes }));
  }
  return pages;
}

export function overlayCowPages(base: Uint8Array, pages: readonly CowPage[], pageBytes: CowPageBytes, logicalSize = base.byteLength): Uint8Array {
  checkedInteger(logicalSize, "logicalSize");
  const result = new Uint8Array(logicalSize);
  result.set(base.subarray(0, logicalSize));
  const seen = new Set<number>();
  for (const page of pages) {
    checkedInteger(page.index, "page.index");
    if (seen.has(page.index)) throw new Error("duplicate COW page index");
    seen.add(page.index);
    if (page.bytes.byteLength > pageBytes) throw new RangeError("COW page exceeds persisted page size");
    const start = page.index * pageBytes;
    if (start >= logicalSize && page.bytes.byteLength) throw new RangeError("COW page begins beyond logical EOF");
    result.set(page.bytes.subarray(0, Math.min(page.bytes.byteLength, logicalSize - start)), start);
  }
  return result;
}

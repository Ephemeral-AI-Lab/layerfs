import {
  checkedAdd,
  checkedInteger,
  checkedMultiply,
} from "../resources/safe-integers.js";
import { intrinsicByteLength, intrinsicByteRange } from "../resources/byte-capacity.js";

export type CowPageBytes = 4096 | 8192 | 16384;
/** 64 MiB at 4 KiB plus both partial endpoints. */
export const MAX_COW_PAGES_PER_WRITE = 16_385;
export const MAX_DIRTY_RANGES = 16_384;
const MAX_COW_PAGE_INDEX = Math.floor(Number.MAX_SAFE_INTEGER / 4096);
export interface DirtyRange {
  readonly start: number;
  readonly end: number;
}
export interface CowPage {
  readonly index: number;
  readonly bytes: Uint8Array;
}
export type CowPageIndex = number & { readonly __cowPageIndex: unique symbol };
export interface CowPageKey {
  readonly branchId: string;
  readonly inodeId: string;
  readonly pageIndex: CowPageIndex;
}

export function validateCowPageBytes(value: number): asserts value is CowPageBytes {
  if (value !== 4096 && value !== 8192 && value !== 16384)
    throw new RangeError("COW page size must be 4096, 8192, or 16384");
}

export function cowPageIndex(value: number): CowPageIndex {
  return checkedInteger(value, "page index", MAX_COW_PAGE_INDEX) as CowPageIndex;
}

export function createCowPageKey(
  branchId: string,
  inodeId: string,
  index: number,
): CowPageKey {
  if (!branchId) throw new RangeError("COW page key branchId must be nonempty");
  if (!inodeId) throw new RangeError("COW page key inodeId must be nonempty");
  return Object.freeze({ branchId, inodeId, pageIndex: cowPageIndex(index) });
}

export function pageIndex(offset: number, pageBytes: CowPageBytes): CowPageIndex {
  validateCowPageBytes(pageBytes);
  checkedInteger(offset, "offset");
  return cowPageIndex(Math.floor(offset / pageBytes));
}

export function pageRange(
  offset: number,
  length: number,
  pageBytes: CowPageBytes,
  maxPages = MAX_COW_PAGES_PER_WRITE,
): readonly number[] {
  validateCowPageBytes(pageBytes);
  checkedInteger(offset, "offset");
  checkedInteger(length, "length");
  checkedInteger(maxPages, "maxPages", MAX_COW_PAGES_PER_WRITE);
  if (maxPages === 0) throw new RangeError("maxPages must be positive");
  if (length === 0) return [];
  const end = checkedAdd(offset, length);
  const first = pageIndex(offset, pageBytes);
  const last = pageIndex(end - 1, pageBytes);
  const count = checkedInteger(last - first + 1, "page range count", maxPages);
  return Array.from({ length: count }, (_, index) => first + index);
}

export function mergeDirtyRanges(
  ranges: readonly DirtyRange[],
  maxRanges = MAX_DIRTY_RANGES,
): DirtyRange[] {
  checkedInteger(maxRanges, "maxRanges", MAX_DIRTY_RANGES);
  if (maxRanges === 0) throw new RangeError("maxRanges must be positive");
  if (ranges.length > maxRanges)
    throw new RangeError("dirty range count exceeds maxRanges");
  const sorted = ranges
    .map(({ start, end }) => {
      checkedInteger(start, "range.start");
      checkedInteger(end, "range.end");
      if (end < start) throw new RangeError("dirty range end precedes start");
      return { start, end };
    })
    .filter(({ start, end }) => end > start)
    .sort((a, b) => a.start - b.start || a.end - b.end);
  const result: DirtyRange[] = [];
  for (const range of sorted) {
    const previous = result.at(-1);
    if (previous && range.start <= previous.end)
      result[result.length - 1] = {
        start: previous.start,
        end: Math.max(previous.end, range.end),
      };
    else result.push(range);
  }
  return result;
}

export function writeCowPages(
  base: Uint8Array,
  offset: number,
  content: Uint8Array,
  pageBytes: CowPageBytes,
): CowPage[] {
  base = intrinsicByteRange(base);
  content = intrinsicByteRange(content);
  validateCowPageBytes(pageBytes);
  checkedInteger(offset, "offset");
  if (content.byteLength === 0)
    throw new RangeError("COW page overlays require a nonempty overwrite");
  const contentEnd = checkedAdd(offset, content.byteLength, "write end");
  if (contentEnd > base.byteLength)
    throw new RangeError("COW page overlays cannot extend the logical file");
  const finalSize = base.byteLength;
  const pages: CowPage[] = [];
  for (const index of pageRange(offset, content.byteLength, pageBytes)) {
    const start = checkedMultiply(index, pageBytes, "COW page offset");
    const end = Math.min(checkedAdd(start, pageBytes, "COW page end"), finalSize);
    const bytes = new Uint8Array(end - start);
    if (start < base.byteLength)
      bytes.set(base.subarray(start, Math.min(end, base.byteLength)));
    const writeStart = Math.max(start, offset);
    const writeEnd = Math.min(end, contentEnd);
    bytes.set(
      content.subarray(writeStart - offset, writeEnd - offset),
      writeStart - start,
    );
    pages.push(Object.freeze({ index, bytes }));
  }
  return pages;
}

export function overlayCowPages(
  base: Uint8Array,
  pages: readonly CowPage[],
  pageBytes: CowPageBytes,
  logicalSize?: number,
  maxPages = MAX_COW_PAGES_PER_WRITE,
): Uint8Array {
  base = intrinsicByteRange(base);
  logicalSize ??= base.byteLength;
  validateCowPageBytes(pageBytes);
  checkedInteger(logicalSize, "logicalSize");
  checkedInteger(maxPages, "maxPages", MAX_COW_PAGES_PER_WRITE);
  if (maxPages === 0) throw new RangeError("maxPages must be positive");
  if (pages.length > maxPages)
    throw new RangeError("COW overlay page count exceeds maxPages");
  if (logicalSize !== base.byteLength)
    throw new RangeError("COW page overlays cannot resize the logical file");
  const seen = new Set<number>();
  const validated: Array<{ bytes: Uint8Array; start: number }> = [];
  for (const page of pages) {
    let bytes: Uint8Array;
    try {
      bytes = intrinsicByteRange(page.bytes);
    } catch {
      throw new TypeError("COW page bytes must be a Uint8Array");
    }
    checkedInteger(page.index, "page.index");
    if (seen.has(page.index)) throw new Error("duplicate COW page index");
    seen.add(page.index);
    const start = checkedMultiply(page.index, pageBytes, "COW page offset");
    if (start >= logicalSize)
      throw new RangeError("COW page begins beyond logical EOF");
    const expectedLength = Math.min(pageBytes, logicalSize - start);
    if (intrinsicByteLength(bytes) !== expectedLength)
      throw new RangeError("COW page length does not match its complete logical page");
    validated.push({ bytes, start });
  }
  const result = new Uint8Array(logicalSize);
  result.set(base.subarray(0, logicalSize));
  for (const { bytes, start } of validated) result.set(bytes, start);
  return result;
}

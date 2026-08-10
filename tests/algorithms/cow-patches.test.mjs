import assert from "node:assert/strict";
import { test } from "node:test";
import { mergeDirtyRanges, overlayCowPages, pageRange, writeCowPages } from "../../packages/fs/dist/cow/pages.js";
import { applyStructuralPatches, replaceRange, truncateBytes } from "../../packages/fs/dist/patches/patches.js";

test("COW page overlays are exact at every persisted page size", () => {
  for (const pageBytes of [4096, 8192, 16384]) {
    const base = Uint8Array.from({ length: pageBytes * 2 + 13 }, (_, index) => index & 0xff);
    const insert = Uint8Array.of(9, 8, 7, 6);
    const offset = pageBytes - 2;
    const pages = writeCowPages(base, offset, insert, pageBytes);
    assert.deepEqual(pages.map(({ index }) => index), [0, 1]);
    const expected = base.slice(); expected.set(insert, offset);
    assert.deepEqual(overlayCowPages(base, pages, pageBytes), expected);
    assert.deepEqual(pageRange(offset, insert.length, pageBytes), [0, 1]);
  }
  assert.deepEqual(mergeDirtyRanges([{ start: 10, end: 20 }, { start: 0, end: 4 }, { start: 4, end: 12 }]), [{ start: 0, end: 20 }]);
});

test("ordered insertion, deletion, replacement, and truncation are deterministic", () => {
  const base = new TextEncoder().encode("abcdefghij");
  const patched = applyStructuralPatches(base, [
    { sequence: 0, offset: 2, deleteLength: 0, insertBytes: new TextEncoder().encode("XY") },
    { sequence: 1, offset: 6, deleteLength: 2, insertBytes: new TextEncoder().encode("!") },
    { sequence: 2, offset: 0, deleteLength: 1, insertBytes: new Uint8Array() },
  ]);
  assert.equal(new TextDecoder().decode(patched), "bXYcd!ghij");
  assert.equal(new TextDecoder().decode(replaceRange(base, 3, 4, new TextEncoder().encode("Q"))), "abcQhij");
  assert.deepEqual([...truncateBytes(Uint8Array.of(1, 2), 4)], [1, 2, 0, 0]);
  assert.throws(() => applyStructuralPatches(base, [{ sequence: 1, offset: 0, deleteLength: 0, insertBytes: new Uint8Array() }]), /contiguous/);
});


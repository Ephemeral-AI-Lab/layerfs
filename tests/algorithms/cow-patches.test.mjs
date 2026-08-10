import assert from "node:assert/strict";
import { test } from "node:test";
import {
  cowPageIndex,
  createCowPageKey,
  mergeDirtyRanges,
  MAX_DIRTY_RANGES,
  overlayCowPages,
  pageIndex,
  pageRange,
  writeCowPages,
} from "../../packages/fs/dist/cow/pages.js";
import {
  applyStructuralPatches,
  applyStructuralPatchesWithMetrics,
  replaceRange,
  truncateBytes,
} from "../../packages/fs/dist/patches/patches.js";

test("COW page overlays are exact at every persisted page size", () => {
  for (const pageBytes of [4096, 8192, 16384]) {
    const base = Uint8Array.from(
      { length: pageBytes * 2 + 13 },
      (_, index) => index & 0xff,
    );
    const insert = Uint8Array.of(9, 8, 7, 6);
    const offset = pageBytes - 2;
    const pages = writeCowPages(base, offset, insert, pageBytes);
    assert.deepEqual(
      pages.map(({ index }) => index),
      [0, 1],
    );
    const expected = base.slice();
    expected.set(insert, offset);
    assert.deepEqual(overlayCowPages(base, pages, pageBytes), expected);
    assert.deepEqual(pageRange(offset, insert.length, pageBytes), [0, 1]);
  }
  assert.deepEqual(
    mergeDirtyRanges([
      { start: 10, end: 20 },
      { start: 0, end: 4 },
      { start: 4, end: 12 },
    ]),
    [{ start: 0, end: 20 }],
  );
  const admittedRanges = Array.from({ length: MAX_DIRTY_RANGES }, (_, index) => ({
    start: index * 2,
    end: index * 2 + 1,
  }));
  assert.equal(mergeDirtyRanges(admittedRanges).length, MAX_DIRTY_RANGES);
  assert.throws(
    () =>
      mergeDirtyRanges([
        ...admittedRanges,
        { start: MAX_DIRTY_RANGES * 2, end: MAX_DIRTY_RANGES * 2 + 1 },
      ]),
    /dirty range count/,
  );
  assert.equal(mergeDirtyRanges(admittedRanges.slice(0, 2), 2).length, 2);
  assert.throws(() => mergeDirtyRanges(admittedRanges.slice(0, 3), 2), /dirty range/);
});

test("COW page geometry rejects malformed or resizing overlays before allocation", () => {
  for (const pageBytes of [0, 4095, 4097, Number.NaN]) {
    assert.throws(() => pageIndex(0, pageBytes), /page size/);
    assert.throws(() => pageRange(0, 1, pageBytes), /page size/);
    assert.throws(() => overlayCowPages(new Uint8Array(1), [], pageBytes), /page size/);
  }
  assert.throws(() => cowPageIndex(Number.MAX_SAFE_INTEGER), /page index/);
  assert.deepEqual(createCowPageKey("branch", "inode", 7), {
    branchId: "branch",
    inodeId: "inode",
    pageIndex: 7,
  });
  assert.throws(() => createCowPageKey("", "inode", 0), /branchId/);

  const pageBytes = 4096;
  const base = new Uint8Array(pageBytes * 2 + 13);
  assert.throws(() => writeCowPages(base, 0, new Uint8Array(), pageBytes), /nonempty/);
  assert.throws(
    () => writeCowPages(base, base.length - 1, Uint8Array.of(1, 2), pageBytes),
    /extend/,
  );
  assert.throws(
    () => overlayCowPages(base, [], pageBytes, base.length + 1),
    /cannot resize/,
  );
  assert.throws(
    () =>
      overlayCowPages(
        base,
        [
          { index: 0, bytes: new Uint8Array(pageBytes) },
          { index: 0, bytes: new Uint8Array(pageBytes) },
        ],
        pageBytes,
      ),
    /duplicate/,
  );
  assert.throws(
    () => overlayCowPages(base, [{ index: 3, bytes: Uint8Array.of(1) }], pageBytes),
    /beyond logical EOF/,
  );
  assert.throws(
    () =>
      overlayCowPages(
        base,
        [{ index: 0, bytes: new Uint8Array(pageBytes - 1) }],
        pageBytes,
      ),
    /complete logical page/,
  );
  assert.throws(
    () => overlayCowPages(base, [{ index: 2, bytes: new Uint8Array(12) }], pageBytes),
    /complete logical page/,
  );
  assert.throws(
    () =>
      overlayCowPages(
        base,
        [{ index: 0, bytes: { byteLength: pageBytes } }],
        pageBytes,
      ),
    /must be a Uint8Array/,
  );
  assert.throws(
    () =>
      overlayCowPages(
        base,
        [
          { index: 0, bytes: new Uint8Array(pageBytes) },
          { index: 1, bytes: new Uint8Array(pageBytes) },
        ],
        pageBytes,
        base.length,
        1,
      ),
    /page count/,
  );
});

test("COW page range is admitted and covers aligned and partial 64 MiB writes", () => {
  const maxWriteBytes = 64 * 1024 * 1024;
  for (const pageBytes of [4096, 8192, 16384]) {
    assert.equal(
      pageRange(0, maxWriteBytes, pageBytes).length,
      maxWriteBytes / pageBytes,
    );
    assert.equal(
      pageRange(1, maxWriteBytes, pageBytes).length,
      maxWriteBytes / pageBytes + 1,
    );
  }
  assert.deepEqual(pageRange(0, 4096, 4096, 1), [0]);
  assert.throws(() => pageRange(1, 4096, 4096, 1), /page range count/);
});

test("ordered insertion, deletion, replacement, and truncation are deterministic", () => {
  class SubstitutingBytes extends Uint8Array {
    subarray() {
      return new Uint8Array(this.byteLength).fill(0xff);
    }
  }
  const base = new TextEncoder().encode("abcdefghij");
  const patched = applyStructuralPatches(base, [
    {
      sequence: 0,
      offset: 2,
      deleteLength: 0,
      insertBytes: new TextEncoder().encode("XY"),
    },
    {
      sequence: 1,
      offset: 6,
      deleteLength: 2,
      insertBytes: new TextEncoder().encode("!"),
    },
    { sequence: 2, offset: 0, deleteLength: 1, insertBytes: new Uint8Array() },
  ]);
  assert.equal(new TextDecoder().decode(patched), "bXYcd!ghij");
  assert.equal(
    new TextDecoder().decode(replaceRange(base, 3, 4, new TextEncoder().encode("Q"))),
    "abcQhij",
  );
  assert.deepEqual([...truncateBytes(Uint8Array.of(1, 2), 4)], [1, 2, 0, 0]);
  const buffer = Buffer.from([1, 2, 3]);
  const truncated = truncateBytes(buffer, 2);
  buffer.fill(9);
  assert.deepEqual([...truncated], [1, 2]);
  assert.equal(Buffer.isBuffer(truncated), false);
  const subclassBase = new SubstitutingBytes(3);
  subclassBase.set([1, 2, 3]);
  assert.deepEqual([...truncateBytes(subclassBase, 2)], [1, 2]);
  const subclassInsertion = new SubstitutingBytes(1);
  subclassInsertion[0] = 9;
  assert.deepEqual(
    [
      ...applyStructuralPatches(subclassBase, [
        {
          sequence: 0,
          offset: 1,
          deleteLength: 1,
          insertBytes: subclassInsertion,
        },
      ]),
    ],
    [1, 9, 3],
  );
  assert.throws(
    () =>
      applyStructuralPatches(base, [
        { sequence: 1, offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
      ]),
    /contiguous/,
  );
});

test("structural patches use bounded piece metadata and one final payload copy", () => {
  for (const count of [32, 256]) {
    const base = Uint8Array.from({ length: 1024 * 1024 }, (_, index) => index);
    const patches = Array.from({ length: count }, (_, sequence) => ({
      sequence,
      offset: (sequence * 4051) % base.length,
      deleteLength: 1,
      insertBytes: Uint8Array.of(sequence & 0xff),
    }));
    const result = applyStructuralPatchesWithMetrics(base, patches);
    assert.equal(result.bytes.byteLength, base.byteLength);
    assert.equal(result.metrics.copiedBytes, base.byteLength);
    assert.ok(result.metrics.peakSegments <= count * 2 + 1);
    assert.ok(result.metrics.metadataSegmentsCreated <= (count + 1) ** 2);
    for (const patch of patches)
      assert.equal(result.bytes[patch.offset], patch.insertBytes[0]);
  }
  const patch = {
    sequence: 0,
    offset: 0,
    deleteLength: 0,
    insertBytes: new Uint8Array(),
  };
  assert.doesNotThrow(() =>
    applyStructuralPatchesWithMetrics(new Uint8Array(), [patch], 1),
  );
  assert.throws(
    () =>
      applyStructuralPatchesWithMetrics(
        new Uint8Array(),
        [patch, { ...patch, sequence: 1 }],
        1,
      ),
    /patch count/,
  );
});

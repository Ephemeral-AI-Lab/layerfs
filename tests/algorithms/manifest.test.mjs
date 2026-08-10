import assert from "node:assert/strict";
import { test } from "node:test";
import { bytesToHex } from "../../packages/fs/dist/cas/bytes.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import { decodeManifestNode, decodeManifestRoot } from "../../packages/fs/dist/manifests/codec.js";
import { lookupManifest, ManifestSequentialCursor, validateManifestTree } from "../../packages/fs/dist/manifests/cursor.js";
import { applyEntrySplice, rebuildManifestLocally } from "../../packages/fs/dist/operations/local-rebuild.js";

function fixture(length, seed = 0x12345678) {
  const bytes = new Uint8Array(length); let state = seed >>> 0;
  for (let index = 0; index < length; index += 1) { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; bytes[index] = state & 0xff; }
  return bytes;
}

const defaults = { minimum: 32768, average: 131072, maximum: 524288 };

test("segmented manifest root and leaf match checked golden hashes", () => {
  const manifest = buildManifest(fixture(2 * 1024 * 1024), defaults);
  assert.equal(manifest.id, "6c08078b39f26d3dd98b10a20e14371e4b2f96fd9164fd629214b8c74981e7f1");
  assert.deepEqual([...manifest.nodes.keys()], ["b69876e73a78d0cb95f34e5206711a90aba19ff31d02eb63594fb7220ea4c91c"]);
  const root = decodeManifestRoot(manifest.root, manifest.rootHash);
  assert.equal(root.fileSize, 2 * 1024 * 1024); assert.equal(root.entryCount, 12);
  const node = decodeManifestNode(manifest.nodes.values().next().value.encoded);
  assert.equal(node.kind, "leaf"); assert.equal(node.span, root.fileSize);
});

test("manifest trees are canonical, bounded, corruption-detecting, and lookup exact", () => {
  const bytes = fixture(1024 * 1024 + 333, 0xcafebabe);
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const first = buildManifest(bytes, parameters); const second = buildManifest(bytes, parameters);
  assert.equal(first.id, second.id); assert.ok(first.nodes.size > 10);
  const reader = { get(hash) { return first.nodes.get(bytesToHex(hash))?.encoded; } };
  validateManifestTree(first.root, reader, first.rootHash, 8);
  for (const offset of [0, 1, Math.floor(bytes.length / 2), bytes.length - 1]) {
    const located = lookupManifest(first.root, offset, reader, first.rootHash, 8);
    assert.ok(located.entry); assert.ok(offset >= located.entryOffset && offset < located.entryOffset + located.entry.length);
    assert.ok(located.nodesRead <= 8);
  }
  assert.equal(lookupManifest(first.root, bytes.length, reader).entry, null);
  const cursor = new ManifestSequentialCursor(first.root, 0, reader, first.rootHash, 8);
  const sequential = [];
  while (cursor.peek()) {
    assert.ok(cursor.retainedNodeCount <= 8);
    const current = cursor.next(); sequential.push([bytesToHex(current.entry.hash), current.entry.length, current.offset]);
  }
  let expectedOffset = 0;
  assert.deepEqual(sequential, first.entries.map((entry) => { const value = [bytesToHex(entry.hash), entry.length, expectedOffset]; expectedOffset += entry.length; return value; }));
  const corruptRoot = first.root.slice(); corruptRoot[20] ^= 1;
  assert.throws(() => decodeManifestRoot(corruptRoot, first.rootHash), /digest mismatch/);
  const [nodeHash, encodedNode] = first.nodes.entries().next().value;
  const corruptNode = encodedNode.encoded.slice(); corruptNode[16] ^= 1;
  const corruptReader = { get(hash) { const key = bytesToHex(hash); return key === nodeHash ? corruptNode : first.nodes.get(key)?.encoded; } };
  assert.throws(() => validateManifestTree(first.root, corruptReader, first.rootHash), /digest mismatch/);
});

test("local CDC reconnection and manifest-spine rebuilding equal a canonical full scan", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(2 * 1024 * 1024 + 29);
  const before = buildManifest(original, parameters);
  assert.ok(before.nodes.size > 10, "fixture must exercise a multi-level manifest");
  const edits = [
    { name: "overwrite", offset: 700_000, deleteLength: 5, insertBytes: Uint8Array.of(9, 8, 7, 6, 5) },
    { name: "insertion", offset: 900_000, deleteLength: 0, insertBytes: Uint8Array.of(1, 2, 3, 4, 5, 6, 7) },
    { name: "deletion", offset: 1_100_000, deleteLength: 11, insertBytes: new Uint8Array() },
    { name: "truncation", offset: 1_700_000, deleteLength: original.length - 1_700_000, insertBytes: new Uint8Array() },
  ];
  for (const edit of edits) {
    let sourceBytesRead = 0; let largestRead = 0;
    const local = rebuildManifestLocally({
      size: original.length,
      read(offset, length) { sourceBytesRead += length; largestRead = Math.max(largestRead, length); return original.slice(offset, offset + length); },
    }, before, edit);
    const edited = new Uint8Array(original.length - edit.deleteLength + edit.insertBytes.length);
    edited.set(original.subarray(0, edit.offset));
    edited.set(edit.insertBytes, edit.offset);
    edited.set(original.subarray(edit.offset + edit.deleteLength), edit.offset + edit.insertBytes.length);
    const canonical = buildManifest(edited, parameters);
    assert.equal(bytesToHex(local.rootHash), canonical.id, `${edit.name} root`);
    assert.deepEqual(applyEntrySplice(before.entries, local.entrySplice).map((entry) => [bytesToHex(entry.hash), entry.length]), canonical.entries.map((entry) => [bytesToHex(entry.hash), entry.length]), `${edit.name} entries`);
    assert.equal(local.fileSize, edited.length);
    assert.equal(local.metrics.sourceBytesRead, sourceBytesRead);
    assert.ok(largestRead <= parameters.maximum, `${edit.name} reads one bounded window at a time`);
    assert.ok(sourceBytesRead < 64 * 1024, `${edit.name} reconnects without scanning the complete source`);
    assert.ok(local.metrics.bytesHashed < 64 * 1024, `${edit.name} hashes only the reconnection window`);
    assert.ok(local.newNodes.size < canonical.nodes.size / 4, `${edit.name} rebuilds only affected manifest paths`);
    const nodes = new Map([...before.nodes, ...local.newNodes]);
    validateManifestTree(local.root, { get(hash) { return nodes.get(bytesToHex(hash))?.encoded; } }, local.rootHash, 8);
  }
});

test("seeded local rebuild property cases match full rebuilds at boundaries and EOF", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(256 * 1024 + 17, 0x5eedc0de);
  const before = buildManifest(original, parameters);
  let state = 0x91e10da5;
  const random = () => { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0; };
  for (let iteration = 0; iteration < 24; iteration += 1) {
    const offset = iteration < 3 ? [0, original.length, Math.floor(original.length / 2)][iteration] : random() % (original.length + 1);
    const deleteLength = Math.min(random() % 33, original.length - offset);
    const insert = fixture(random() % 33, random());
    const local = rebuildManifestLocally({ size: original.length, read(start, length) { return original.slice(start, start + length); } }, before, { offset, deleteLength, insertBytes: insert });
    const edited = new Uint8Array(original.length - deleteLength + insert.length);
    edited.set(original.subarray(0, offset)); edited.set(insert, offset); edited.set(original.subarray(offset + deleteLength), offset + insert.length);
    const canonical = buildManifest(edited, parameters);
    assert.equal(bytesToHex(local.rootHash), canonical.id, `seed=0x91e10da5 iteration=${iteration}`);
    assert.ok(local.metrics.scanWindowBytes === parameters.maximum);
    assert.ok(local.metrics.sourceBytesRead < original.length / 2, `iteration ${iteration} remained local`);
  }
});

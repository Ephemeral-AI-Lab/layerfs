import assert from "node:assert/strict";
import { test } from "node:test";
import { bytesToHex } from "../../packages/fs/dist/utils/bytes.js";
import { buildManifest } from "../../packages/fs/dist/manifests/builder.js";
import { decodeManifestNode, decodeManifestRoot } from "../../packages/fs/dist/manifests/codec.js";
import { lookupManifest, validateManifestTree } from "../../packages/fs/dist/manifests/cursor.js";

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
  const corruptRoot = first.root.slice(); corruptRoot[20] ^= 1;
  assert.throws(() => decodeManifestRoot(corruptRoot, first.rootHash), /digest mismatch/);
  const [nodeHash, encodedNode] = first.nodes.entries().next().value;
  const corruptNode = encodedNode.encoded.slice(); corruptNode[16] ^= 1;
  const corruptReader = { get(hash) { const key = bytesToHex(hash); return key === nodeHash ? corruptNode : first.nodes.get(key)?.encoded; } };
  assert.throws(() => validateManifestTree(first.root, corruptReader, first.rootHash), /digest mismatch/);
});

test("small edits fully rebuild to the canonical root and reuse unchanged CAS", () => {
  const original = fixture(4 * 1024 * 1024);
  const before = buildManifest(original, defaults);
  const edited = new Uint8Array(original.length + 3); edited.set(original.subarray(0, 700_000)); edited.set([1, 2, 3], 700_000); edited.set(original.subarray(700_000), 700_003);
  const localCandidate = buildManifest(edited, defaults); const fullRebuild = buildManifest(edited.slice(), defaults);
  assert.equal(localCandidate.id, fullRebuild.id);
  const reused = [...localCandidate.objects.keys()].filter((key) => before.objects.has(key));
  assert.ok(reused.length > 0, "content-defined chunking should reconnect to unchanged content");
});


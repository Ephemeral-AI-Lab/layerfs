import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { bytesToHex } from "../../packages/fs/dist/cas/bytes.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
  MAX_MANIFEST_ENTRY_COUNT,
} from "../../packages/fs/dist/manifests/codec.js";
import {
  lookupManifest,
  ManifestSequentialCursor,
  validateManifestTree,
} from "../../packages/fs/dist/manifests/cursor.js";
import {
  advanceManifestGroupingState,
  isManifestGroupBoundary,
} from "../../packages/fs/dist/manifests/grouping.js";
import {
  applyEntrySplice,
  rebuildManifestLocally,
} from "../../packages/fs/dist/operations/local-rebuild.js";
import { rebuildManifestLocallyOrStream } from "../../packages/fs/dist/operations/streamed-rebuild.js";

function fixture(length, seed = 0x12345678) {
  const bytes = new Uint8Array(length);
  let state = seed >>> 0;
  for (let index = 0; index < length; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[index] = state & 0xff;
  }
  return bytes;
}

const defaults = { minimum: 32768, average: 131072, maximum: 524288 };

class DurableManifestWorkspace {
  constructor(filename) {
    this.database = new DatabaseSync(filename);
    this.database.exec(
      "PRAGMA journal_mode=WAL; CREATE TABLE records(level INTEGER NOT NULL, record_index INTEGER NOT NULL, hash BLOB NOT NULL, span INTEGER NOT NULL, entry_count INTEGER NOT NULL, encoded BLOB NOT NULL, PRIMARY KEY(level,record_index)); CREATE INDEX records_hash ON records(hash); CREATE TABLE objects(hash BLOB PRIMARY KEY, bytes BLOB NOT NULL)",
    );
    this.insert = this.database.prepare(
      "INSERT INTO records(level,record_index,hash,span,entry_count,encoded) VALUES(?,?,?,?,?,?)",
    );
    this.page = this.database.prepare(
      "SELECT record_index,hash,span,entry_count FROM records WHERE level=? AND record_index>? ORDER BY record_index LIMIT ?",
    );
    this.node = this.database.prepare(
      "SELECT encoded FROM records WHERE hash=? LIMIT 1",
    );
    this.object = this.database.prepare(
      "INSERT OR IGNORE INTO objects(hash,bytes) VALUES(?,?)",
    );
    this.largestPage = 0;
  }
  writeNode(record) {
    this.insert.run(
      record.level,
      record.index,
      record.child.hash,
      record.child.span,
      record.child.entryCount,
      record.value.encoded,
    );
  }
  readLevel(level, afterIndex, limit) {
    const rows = this.page.all(level, afterIndex, limit);
    this.largestPage = Math.max(this.largestPage, rows.length);
    return rows.map((row) => ({
      index: row.record_index,
      child: { hash: row.hash, span: row.span, entryCount: row.entry_count },
    }));
  }
  putObject(hash, bytes) {
    this.object.run(hash, bytes);
  }
  get(hash) {
    return this.node.get(hash)?.encoded;
  }
  close() {
    this.database.close();
  }
}

class MemoryManifestWorkspace {
  constructor() {
    this.levels = new Map();
  }
  writeNode(record) {
    const level = this.levels.get(record.level) ?? [];
    level.push(record);
    this.levels.set(record.level, level);
  }
  readLevel(level, afterIndex, limit) {
    return (this.levels.get(level) ?? [])
      .filter((record) => record.index > afterIndex)
      .slice(0, limit);
  }
}

function storedNode(nodes, node) {
  const encoded = encodeManifestNode(node);
  const hash = sha256(encoded);
  nodes.set(bytesToHex(hash), encoded);
  return Object.freeze({ hash, span: node.span, entryCount: node.entryCount });
}

function storedRoot(parameters, child) {
  const root = encodeManifestRoot({
    parameters,
    fileSize: child.span,
    entryCount: child.entryCount,
    rootNodeHash: child.hash,
  });
  return Object.freeze({ root, rootHash: sha256(root) });
}

test("root, leaf, internal, grouping, and complete manifest golden vectors are exact", () => {
  const entryA = { hash: new Uint8Array(32).fill(0x11), length: 1 };
  const entryB = { hash: new Uint8Array(32).fill(0x22), length: 2 };
  const leafBytes = encodeManifestNode({
    kind: "leaf",
    span: 3,
    entryCount: 2,
    entries: [entryA, entryB],
  });
  assert.equal(
    bytesToHex(leafBytes),
    "4541464e01000001020000000000000003000000000000000200000000000000111111111111111111111111111111111111111111111111111111111111111101000000222222222222222222222222222222222222222222222222222222222222222202000000",
  );
  assert.equal(
    bytesToHex(sha256(leafBytes)),
    "e7b7034cb872766a9d02249f745276b89a32f2a53a7680641987ef93dc2f6c70",
  );
  const childA = { hash: sha256(leafBytes), span: 3, entryCount: 2 };
  const childB = { hash: new Uint8Array(32).fill(0x33), span: 5, entryCount: 1 };
  const internalBytes = encodeManifestNode({
    kind: "internal",
    span: 8,
    entryCount: 3,
    children: [childA, childB],
  });
  assert.equal(
    bytesToHex(internalBytes),
    "4541464e01000101020000000000000008000000000000000300000000000000e7b7034cb872766a9d02249f745276b89a32f2a53a7680641987ef93dc2f6c7003000000000000000200000000000000333333333333333333333333333333333333333333333333333333333333333305000000000000000100000000000000",
  );
  assert.equal(
    bytesToHex(sha256(internalBytes)),
    "45a4b9207f8f4b5dc90aee18f6b099f802018110740300cdf8ca165c6cba9065",
  );
  const rootBytes = encodeManifestRoot({
    parameters: { minimum: 1, average: 2, maximum: 4 },
    fileSize: 8,
    entryCount: 3,
    rootNodeHash: sha256(internalBytes),
  });
  assert.equal(
    bytesToHex(rootBytes),
    "45414652010001010100000002000000040000000800000000000000030000000000000045a4b9207f8f4b5dc90aee18f6b099f802018110740300cdf8ca165c6cba9065",
  );
  assert.equal(
    bytesToHex(sha256(rootBytes)),
    "dca081afd9e6ad4650d7e327557b22dcb3747b98a9ce11f01118b4c652fef6ce",
  );
  let groupingState = 0n;
  groupingState = advanceManifestGroupingState(groupingState, entryA);
  assert.equal(groupingState, 0x61dc0de1d6ec86bfn);
  groupingState = advanceManifestGroupingState(groupingState, entryB);
  assert.equal(groupingState, 0x42edc85640fd080fn);
  groupingState = 0n;
  const boundaries = [];
  for (let index = 0; index < 600; index += 1) {
    groupingState = advanceManifestGroupingState(groupingState, entryA);
    const count = (index % 256) + 1;
    if (isManifestGroupBoundary(count, groupingState, 64, 128, 256)) {
      boundaries.push([index + 1, count, groupingState]);
      groupingState = 0n;
    }
  }
  assert.deepEqual(boundaries, [
    [256, 256, 0xd0a479d1d6ec86bfn],
    [512, 256, 0xd0a479d1d6ec86bfn],
  ]);

  const manifest = buildManifest(fixture(2 * 1024 * 1024), defaults);
  assert.equal(
    manifest.id,
    "6c08078b39f26d3dd98b10a20e14371e4b2f96fd9164fd629214b8c74981e7f1",
  );
  assert.deepEqual(
    [...manifest.nodes.keys()],
    ["b69876e73a78d0cb95f34e5206711a90aba19ff31d02eb63594fb7220ea4c91c"],
  );
  assert.equal(
    bytesToHex(manifest.root),
    "454146520100010100800000000002000000080000002000000000000c00000000000000b69876e73a78d0cb95f34e5206711a90aba19ff31d02eb63594fb7220ea4c91c",
  );
  const root = decodeManifestRoot(manifest.root, manifest.rootHash);
  assert.equal(root.fileSize, 2 * 1024 * 1024);
  assert.equal(root.entryCount, 12);
  const node = decodeManifestNode(manifest.nodes.values().next().value.encoded);
  assert.equal(node.kind, "leaf");
  assert.equal(node.span, root.fileSize);
});

test("builder, validation, and lookup reject noncanonical manifest structures", () => {
  const parameters = { minimum: 1, average: 2, maximum: 4 };
  const entry = Object.freeze({ hash: sha256(Uint8Array.of(7)), length: 1 });
  const entries = (count) => Array.from({ length: count }, () => entry);
  const assertReadersReject = (stored, nodes, pattern) => {
    const reader = {
      get(hash) {
        return nodes.get(bytesToHex(hash));
      },
    };
    assert.throws(
      () => validateManifestTree(stored.root, reader, stored.rootHash),
      pattern,
    );
    assert.throws(
      () => lookupManifest(stored.root, 0, reader, stored.rootHash),
      pattern,
    );
  };

  assert.throws(
    () =>
      buildManifestFromEntries(
        [entry],
        { minimum: 1, average: 3, maximum: 4 },
        new MemoryManifestWorkspace(),
      ),
    /power of two/,
  );
  assert.throws(
    () =>
      encodeManifestRoot({
        parameters: { minimum: 1, average: 3, maximum: 4 },
        fileSize: 0,
        entryCount: 0,
        rootNodeHash: new Uint8Array(32),
      }),
    /power of two/,
  );
  assert.throws(
    () =>
      buildManifestFromEntries(
        [{ hash: entry.hash, length: 8 }],
        parameters,
        new MemoryManifestWorkspace(),
      ),
    /FastCDC maximum/,
  );
  assert.throws(
    () =>
      buildManifestFromEntries(
        [entry, entry],
        { minimum: 2, average: 2, maximum: 4 },
        new MemoryManifestWorkspace(),
      ),
    /below the FastCDC minimum/,
  );
  assert.throws(
    () =>
      buildManifestFromEntries([entry], parameters, new MemoryManifestWorkspace(), {
        maxDepth: 65,
      }),
    /maxDepth/,
  );

  {
    const nodes = new Map();
    const leaf = storedNode(nodes, {
      kind: "leaf",
      span: 1,
      entryCount: 1,
      entries: [entry],
    });
    const stored = storedRoot(parameters, leaf);
    const reader = {
      get(hash) {
        return nodes.get(bytesToHex(hash));
      },
    };
    validateManifestTree(stored.root, reader, stored.rootHash, 64);
    assert.throws(
      () => validateManifestTree(stored.root, reader, stored.rootHash, 65),
      /maxDepth/,
    );
    assert.throws(
      () => lookupManifest(stored.root, 0, reader, stored.rootHash, 65),
      /maxDepth/,
    );
  }

  {
    const nodes = new Map();
    const leaf = storedNode(nodes, {
      kind: "leaf",
      span: 1,
      entryCount: 1,
      entries: [entry],
    });
    const wrapper = storedNode(nodes, {
      kind: "internal",
      span: 1,
      entryCount: 1,
      children: [leaf],
    });
    assertReadersReject(storedRoot(parameters, wrapper), nodes, /unary internal root/);
  }

  {
    const nodes = new Map();
    const premature = storedNode(nodes, {
      kind: "leaf",
      span: 1,
      entryCount: 1,
      entries: [entry],
    });
    const final = storedNode(nodes, {
      kind: "leaf",
      span: 64,
      entryCount: 64,
      entries: entries(64),
    });
    const parent = storedNode(nodes, {
      kind: "internal",
      span: 65,
      entryCount: 65,
      children: [premature, final],
    });
    assertReadersReject(storedRoot(parameters, parent), nodes, /canonical boundary/);
  }

  {
    const nodes = new Map();
    const oversized = storedNode(nodes, {
      kind: "leaf",
      span: 8,
      entryCount: 1,
      entries: [{ hash: entry.hash, length: 8 }],
    });
    assertReadersReject(storedRoot(parameters, oversized), nodes, /FastCDC maximum/);
  }

  {
    const nodes = new Map();
    const shallow = storedNode(nodes, {
      kind: "leaf",
      span: 256,
      entryCount: 256,
      entries: entries(256),
    });
    const deepFirst = storedNode(nodes, {
      kind: "leaf",
      span: 256,
      entryCount: 256,
      entries: entries(256),
    });
    const deepLast = storedNode(nodes, {
      kind: "leaf",
      span: 1,
      entryCount: 1,
      entries: [entry],
    });
    const nested = storedNode(nodes, {
      kind: "internal",
      span: 257,
      entryCount: 257,
      children: [deepFirst, deepLast],
    });
    const parent = storedNode(nodes, {
      kind: "internal",
      span: 513,
      entryCount: 513,
      children: [shallow, nested],
    });
    const stored = storedRoot(parameters, parent);
    const reader = {
      get(hash) {
        return nodes.get(bytesToHex(hash));
      },
    };
    assert.throws(
      () => validateManifestTree(stored.root, reader, stored.rootHash),
      /unbalanced manifest tree/,
    );
  }
});

test("manifest codecs reject overflow and malformed encodings without digest checks", () => {
  const hash = new Uint8Array(32).fill(0x44);
  const maximumRoot = encodeManifestRoot({
    parameters: { minimum: 1, average: 2, maximum: 4 },
    fileSize: MAX_MANIFEST_ENTRY_COUNT,
    entryCount: MAX_MANIFEST_ENTRY_COUNT,
    rootNodeHash: hash,
  });
  assert.equal(decodeManifestRoot(maximumRoot).entryCount, MAX_MANIFEST_ENTRY_COUNT);
  assert.throws(
    () =>
      encodeManifestRoot({
        parameters: { minimum: 1, average: 2, maximum: 4 },
        fileSize: MAX_MANIFEST_ENTRY_COUNT + 1,
        entryCount: MAX_MANIFEST_ENTRY_COUNT + 1,
        rootNodeHash: hash,
      }),
    /manifest root entry count/,
  );
  const maximumInternal = encodeManifestNode({
    kind: "internal",
    span: 1,
    entryCount: MAX_MANIFEST_ENTRY_COUNT,
    children: [{ hash, span: 1, entryCount: MAX_MANIFEST_ENTRY_COUNT }],
  });
  assert.equal(
    decodeManifestNode(maximumInternal).entryCount,
    MAX_MANIFEST_ENTRY_COUNT,
  );
  assert.throws(
    () =>
      encodeManifestNode({
        kind: "internal",
        span: 1,
        entryCount: MAX_MANIFEST_ENTRY_COUNT + 1,
        children: [{ hash, span: 1, entryCount: MAX_MANIFEST_ENTRY_COUNT }],
      }),
    /manifest node entry count/,
  );
  assert.throws(
    () =>
      encodeManifestNode({
        kind: "internal",
        span: 1,
        entryCount: MAX_MANIFEST_ENTRY_COUNT,
        children: [{ hash, span: 1, entryCount: MAX_MANIFEST_ENTRY_COUNT + 1 }],
      }),
    /manifest child entry count/,
  );
  const oversizedRootCount = maximumRoot.slice();
  new DataView(oversizedRootCount.buffer).setBigUint64(
    28,
    BigInt(MAX_MANIFEST_ENTRY_COUNT) + 1n,
    true,
  );
  assert.throws(
    () => decodeManifestRoot(oversizedRootCount),
    /manifest root entry count/,
  );
  const oversizedNodeCount = maximumInternal.slice();
  new DataView(oversizedNodeCount.buffer).setBigUint64(
    24,
    BigInt(MAX_MANIFEST_ENTRY_COUNT) + 1n,
    true,
  );
  assert.throws(
    () => decodeManifestNode(oversizedNodeCount),
    /manifest node entry count/,
  );
  const oversizedChildCount = maximumInternal.slice();
  new DataView(oversizedChildCount.buffer).setBigUint64(
    72,
    BigInt(MAX_MANIFEST_ENTRY_COUNT) + 1n,
    true,
  );
  assert.throws(
    () => decodeManifestNode(oversizedChildCount),
    /manifest child entry count/,
  );
  const leaf = encodeManifestNode({
    kind: "leaf",
    span: 1,
    entryCount: 1,
    entries: [{ hash, length: 1 }],
  });
  for (let length = 0; length < 32; length += 1)
    assert.throws(() => decodeManifestNode(leaf.slice(0, length)), /truncated/);
  for (let extra = 1; extra <= 16; extra += 1) {
    const extended = new Uint8Array(leaf.length + extra);
    extended.set(leaf);
    assert.throws(
      () => decodeManifestNode(extended),
      /noncanonical manifest node size/,
    );
  }
  const reserved = leaf.slice();
  reserved[12] = 1;
  assert.throws(() => decodeManifestNode(reserved), /malformed manifest node header/);
  const impossibleCount = leaf.slice();
  new DataView(impossibleCount.buffer).setUint32(8, 2, true);
  assert.throws(
    () => decodeManifestNode(impossibleCount),
    /noncanonical manifest node size/,
  );
  const zeroLength = leaf.slice();
  new DataView(zeroLength.buffer).setUint32(64, 0, true);
  assert.throws(() => decodeManifestNode(zeroLength), /zero-length manifest entry/);

  const emptyInternal = new Uint8Array(32);
  emptyInternal.set([0x45, 0x41, 0x46, 0x4e]);
  const emptyView = new DataView(emptyInternal.buffer);
  emptyView.setUint16(4, 1, true);
  emptyInternal[6] = 1;
  emptyInternal[7] = 1;
  assert.throws(() => decodeManifestNode(emptyInternal), /empty internal/);
  assert.throws(
    () =>
      encodeManifestNode({
        kind: "internal",
        span: 0,
        entryCount: 0,
        children: [],
      }),
    /empty internal/,
  );
  assert.throws(
    () =>
      encodeManifestNode({
        kind: "leaf",
        span: 257,
        entryCount: 257,
        entries: Array.from({ length: 257 }, () => ({ hash, length: 1 })),
      }),
    /capacity/,
  );
  assert.throws(
    () =>
      encodeManifestNode({
        kind: "internal",
        span: 129,
        entryCount: 129,
        children: Array.from({ length: 129 }, () => ({
          hash,
          span: 1,
          entryCount: 1,
        })),
      }),
    /capacity/,
  );

  assert.throws(
    () =>
      encodeManifestNode({
        kind: "internal",
        span: Number.MAX_SAFE_INTEGER,
        entryCount: 2,
        children: [
          { hash, span: Number.MAX_SAFE_INTEGER, entryCount: 1 },
          { hash, span: 1, entryCount: 1 },
        ],
      }),
    /safe integer/,
  );
  const root = encodeManifestRoot({
    parameters: { minimum: 1, average: 2, maximum: 4 },
    fileSize: 1,
    entryCount: 1,
    rootNodeHash: hash,
  });
  new DataView(root.buffer).setBigUint64(
    20,
    BigInt(Number.MAX_SAFE_INTEGER) + 1n,
    true,
  );
  assert.throws(() => decodeManifestRoot(root), /Number.MAX_SAFE_INTEGER/);
});

test("manifest trees are canonical, bounded, corruption-detecting, and lookup exact", () => {
  const bytes = fixture(1024 * 1024 + 333, 0xcafebabe);
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const first = buildManifest(bytes, parameters);
  const second = buildManifest(bytes, parameters);
  assert.equal(first.id, second.id);
  assert.ok(first.nodes.size > 10);
  const reader = {
    get(hash) {
      return first.nodes.get(bytesToHex(hash))?.encoded;
    },
  };
  validateManifestTree(first.root, reader, first.rootHash, 8);
  for (const offset of [0, 1, Math.floor(bytes.length / 2), bytes.length - 1]) {
    const located = lookupManifest(first.root, offset, reader, first.rootHash, 8);
    assert.ok(located.entry);
    assert.ok(
      offset >= located.entryOffset &&
        offset < located.entryOffset + located.entry.length,
    );
    assert.ok(located.nodesRead <= 8);
  }
  assert.equal(lookupManifest(first.root, bytes.length, reader).entry, null);
  const cursor = new ManifestSequentialCursor(first.root, 0, reader, first.rootHash, 8);
  const sequential = [];
  while (cursor.peek()) {
    assert.ok(cursor.retainedNodeCount <= 8);
    const current = cursor.next();
    sequential.push([
      bytesToHex(current.entry.hash),
      current.entry.length,
      current.offset,
    ]);
  }
  let expectedOffset = 0;
  assert.deepEqual(
    sequential,
    first.entries.map((entry) => {
      const value = [bytesToHex(entry.hash), entry.length, expectedOffset];
      expectedOffset += entry.length;
      return value;
    }),
  );
  const corruptRoot = first.root.slice();
  corruptRoot[20] ^= 1;
  assert.throws(
    () => decodeManifestRoot(corruptRoot, first.rootHash),
    /digest mismatch/,
  );
  const [nodeHash, encodedNode] = first.nodes.entries().next().value;
  const corruptNode = encodedNode.encoded.slice();
  corruptNode[16] ^= 1;
  const corruptReader = {
    get(hash) {
      const key = bytesToHex(hash);
      return key === nodeHash ? corruptNode : first.nodes.get(key)?.encoded;
    },
  };
  assert.throws(
    () => validateManifestTree(first.root, corruptReader, first.rootHash),
    /digest mismatch/,
  );
});

test("100001-entry canonical construction retains only a group and keyset page", () => {
  const directory = mkdtempSync(path.join(tmpdir(), "efs-m1-builder-"));
  const workspace = new DurableManifestWorkspace(path.join(directory, "manifest.db"));
  try {
    const entryHash = sha256(Uint8Array.of(7));
    function* entries() {
      for (let index = 0; index < 100_001; index += 1)
        yield { hash: entryHash, length: 1 };
    }
    const built = buildManifestFromEntries(
      entries(),
      { minimum: 1, average: 2, maximum: 4 },
      workspace,
      { readBatchRecords: 17, maxDepth: 8 },
    );
    assert.equal(built.entryCount, 100_001);
    assert.equal(built.fileSize, 100_001);
    assert.ok(built.nodeCount > 390);
    assert.ok(built.peakRetainedRecords <= 256 + 17);
    assert.ok(workspace.largestPage <= 17);
    validateManifestTree(built.root, workspace, built.rootHash, 8);
  } finally {
    workspace.close();
    rmSync(directory, { recursive: true, force: true });
  }
});

test("local rebuild crosses a fixed cap into a durable streamed fallback", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(64 * 1024 + 19, 0xa11ce);
  const before = buildManifest(original, parameters);
  const edit = { offset: 25_000, deleteLength: 1, insertBytes: Uint8Array.of(42) };
  const directory = mkdtempSync(path.join(tmpdir(), "efs-m1-fallback-"));
  const workspace = new DurableManifestWorkspace(path.join(directory, "fallback.db"));
  try {
    const result = rebuildManifestLocallyOrStream(
      {
        size: original.length,
        read(offset, length) {
          return original.slice(offset, offset + length);
        },
      },
      before,
      edit,
      parameters,
      workspace,
      workspace,
      {
        maxRetainedEntries: 1,
        maxRetainedNodes: 1,
        maxAffectedEntries: 1,
        maxAffectedBytes: 1,
      },
      { readWindowBytes: 257, manifestReadBatchRecords: 11, maxManifestDepth: 8 },
    );
    assert.equal(result.mode, "streamed-fallback");
    assert.match(result.localLimitReason, /streamed workspace fallback/);
    assert.ok(result.metrics.largestSourceRead <= 257);
    assert.ok(result.metrics.peakRetainedRecords <= 267);
    assert.equal(result.metrics.sourceBytesRead, original.length - 1);
    const edited = original.slice();
    edited[edit.offset] = 42;
    const canonical = buildManifest(edited, parameters);
    assert.equal(bytesToHex(result.manifest.rootHash), canonical.id);
    validateManifestTree(result.manifest.root, workspace, result.manifest.rootHash, 8);
  } finally {
    workspace.close();
    rmSync(directory, { recursive: true, force: true });
  }
});

test("local CDC reconnection and manifest-spine rebuilding equal a canonical full scan", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(2 * 1024 * 1024 + 29);
  const before = buildManifest(original, parameters);
  assert.ok(before.nodes.size > 10, "fixture must exercise a multi-level manifest");
  const edits = [
    {
      name: "overwrite",
      offset: 700_000,
      deleteLength: 5,
      insertBytes: Uint8Array.of(9, 8, 7, 6, 5),
    },
    {
      name: "insertion",
      offset: 900_000,
      deleteLength: 0,
      insertBytes: Uint8Array.of(1, 2, 3, 4, 5, 6, 7),
    },
    {
      name: "deletion",
      offset: 1_100_000,
      deleteLength: 11,
      insertBytes: new Uint8Array(),
    },
    {
      name: "truncation",
      offset: 1_700_000,
      deleteLength: original.length - 1_700_000,
      insertBytes: new Uint8Array(),
    },
  ];
  for (const edit of edits) {
    let sourceBytesRead = 0;
    let largestRead = 0;
    const local = rebuildManifestLocally(
      {
        size: original.length,
        read(offset, length) {
          sourceBytesRead += length;
          largestRead = Math.max(largestRead, length);
          return original.slice(offset, offset + length);
        },
      },
      before,
      edit,
    );
    const edited = new Uint8Array(
      original.length - edit.deleteLength + edit.insertBytes.length,
    );
    edited.set(original.subarray(0, edit.offset));
    edited.set(edit.insertBytes, edit.offset);
    edited.set(
      original.subarray(edit.offset + edit.deleteLength),
      edit.offset + edit.insertBytes.length,
    );
    const canonical = buildManifest(edited, parameters);
    assert.equal(bytesToHex(local.rootHash), canonical.id, `${edit.name} root`);
    assert.deepEqual(
      applyEntrySplice(before.entries, local.entrySplice).map((entry) => [
        bytesToHex(entry.hash),
        entry.length,
      ]),
      canonical.entries.map((entry) => [bytesToHex(entry.hash), entry.length]),
      `${edit.name} entries`,
    );
    assert.equal(local.fileSize, edited.length);
    assert.equal(local.metrics.sourceBytesRead, sourceBytesRead);
    assert.ok(
      largestRead <= parameters.maximum,
      `${edit.name} reads one bounded window at a time`,
    );
    assert.ok(
      sourceBytesRead < 64 * 1024,
      `${edit.name} reconnects without scanning the complete source`,
    );
    assert.ok(
      local.metrics.bytesHashed < 64 * 1024,
      `${edit.name} hashes only the reconnection window`,
    );
    assert.ok(
      local.newNodes.size < canonical.nodes.size / 4,
      `${edit.name} rebuilds only affected manifest paths`,
    );
    const nodes = new Map([...before.nodes, ...local.newNodes]);
    validateManifestTree(
      local.root,
      {
        get(hash) {
          return nodes.get(bytesToHex(hash))?.encoded;
        },
      },
      local.rootHash,
      8,
    );
  }
});

test("seeded local rebuild property cases match full rebuilds at boundaries and EOF", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(256 * 1024 + 17, 0x5eedc0de);
  const before = buildManifest(original, parameters);
  let state = 0x91e10da5;
  const random = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return state >>> 0;
  };
  for (let iteration = 0; iteration < 24; iteration += 1) {
    const offset =
      iteration < 3
        ? [0, original.length, Math.floor(original.length / 2)][iteration]
        : random() % (original.length + 1);
    const deleteLength = Math.min(random() % 33, original.length - offset);
    const insert = fixture(random() % 33, random());
    const local = rebuildManifestLocally(
      {
        size: original.length,
        read(start, length) {
          return original.slice(start, start + length);
        },
      },
      before,
      { offset, deleteLength, insertBytes: insert },
    );
    const edited = new Uint8Array(original.length - deleteLength + insert.length);
    edited.set(original.subarray(0, offset));
    edited.set(insert, offset);
    edited.set(original.subarray(offset + deleteLength), offset + insert.length);
    const canonical = buildManifest(edited, parameters);
    assert.equal(
      bytesToHex(local.rootHash),
      canonical.id,
      `seed=0x91e10da5 iteration=${iteration}`,
    );
    assert.ok(local.metrics.scanWindowBytes === parameters.maximum);
    assert.ok(
      local.metrics.sourceBytesRead < original.length / 2,
      `iteration ${iteration} remained local`,
    );
  }
});

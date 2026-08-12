import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { bytesToHex } from "../../packages/fs/dist/cas/bytes.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import {
  findFastCdcBoundary,
  StreamingFastCdc,
} from "../../packages/fs/dist/cdc/fastcdc.js";
import {
  buildManifest,
  MAX_DIAGNOSTIC_CONTENT_BYTES,
} from "../../packages/fs/dist/operations/full-rebuild.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
  MAX_MANIFEST_ENTRY_COUNT,
  MAX_MANIFEST_NODE_BYTES,
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
  DEFAULT_LOCAL_REBUILD_LIMITS,
  LocalRebuildLimitError,
  rebuildDiagnosticManifestLocally,
} from "../../packages/fs/dist/operations/local-rebuild.js";
import {
  BoundedRebuildFallbackError,
  boundedPathAtOffset,
  buildBoundedManifestState,
  rebuildManifestBoundedOwned,
} from "../../packages/fs/dist/operations/bounded-local-rebuild.js";
import {
  rebuildEditedContentStreaming,
  rebuildManifestLocallyOrStream,
} from "../../packages/fs/dist/operations/streamed-rebuild.js";
import { MAX_CONTENT_OBJECT_BYTES } from "../../packages/fs/dist/resources/limits.js";

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

function countUint8ArraySetCalls(callback) {
  const original = Uint8Array.prototype.set;
  let calls = 0;
  Uint8Array.prototype.set = function countedSet(...args) {
    calls += 1;
    return Reflect.apply(original, this, args);
  };
  try {
    callback();
    return calls;
  } finally {
    Uint8Array.prototype.set = original;
  }
}

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

function workspaceNodes(workspace) {
  const nodes = new Map();
  for (const records of workspace.levels.values())
    for (const record of records)
      nodes.set(bytesToHex(record.value.hash), record.value);
  return nodes;
}

function mutableDiagnosticCopy(manifest) {
  return {
    id: manifest.id,
    rootHash: manifest.rootHash.slice(),
    root: manifest.root.slice(),
    entries: manifest.entries.map((entry) => ({
      hash: entry.hash.slice(),
      length: entry.length,
    })),
    nodes: new Map(
      [...manifest.nodes].map(([key, value]) => [
        key,
        {
          hash: value.hash.slice(),
          encoded: value.encoded.slice(),
          node: decodeManifestNode(value.encoded),
        },
      ]),
    ),
  };
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

function variedEntry(index) {
  const identity = new Uint8Array(8);
  new DataView(identity.buffer).setBigUint64(0, BigInt(index), true);
  return Object.freeze({ hash: sha256(identity), length: 100 + (index % 17) });
}

function diverseGoldenEntry(index) {
  const identity = new Uint8Array(4);
  new DataView(identity.buffer).setUint32(0, index, true);
  return Object.freeze({ hash: sha256(identity), length: (index % 4) + 1 });
}

test("root, leaf, internal, grouping, and complete manifest golden vectors are exact", () => {
  const emptyLeaf = encodeManifestNode({
    kind: "leaf",
    span: 0,
    entryCount: 0,
    entries: [],
  });
  assert.equal(
    bytesToHex(emptyLeaf),
    "4541464e01000001000000000000000000000000000000000000000000000000",
  );
  assert.equal(
    bytesToHex(sha256(emptyLeaf)),
    "166659473d5d3838ca47c6a541fc969e6377d165e2b6f36e40b7be1db7b92527",
  );
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
  const fullEntries = Array.from({ length: 256 }, (_, index) => {
    const identity = new Uint8Array(4);
    new DataView(identity.buffer).setUint32(0, index, true);
    return { hash: sha256(identity), length: index + 1 };
  });
  const fullLeaf = encodeManifestNode({
    kind: "leaf",
    span: 32_896,
    entryCount: 256,
    entries: fullEntries,
  });
  const expectedFullLeaf = new Uint8Array(32 + 256 * 36);
  expectedFullLeaf.set([0x45, 0x41, 0x46, 0x4e]);
  const fullView = new DataView(expectedFullLeaf.buffer);
  fullView.setUint16(4, 1, true);
  expectedFullLeaf[6] = 0;
  expectedFullLeaf[7] = 1;
  fullView.setUint32(8, 256, true);
  fullView.setBigUint64(16, 32_896n, true);
  fullView.setBigUint64(24, 256n, true);
  for (let index = 0; index < fullEntries.length; index += 1) {
    const offset = 32 + index * 36;
    expectedFullLeaf.set(fullEntries[index].hash, offset);
    fullView.setUint32(offset + 32, index + 1, true);
  }
  assert.deepEqual(fullLeaf, expectedFullLeaf);
  assert.equal(
    bytesToHex(sha256(fullLeaf)),
    "39a12e626c1e1dde1ff0b47d26e0190e288e8e3652325f55763461917027ba87",
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

test("diagnostic full rebuild detaches Node Buffer object ranges", () => {
  const source = Buffer.from(fixture(4096, 0xb0ffee));
  const reference = buildManifest(new Uint8Array(source), {
    minimum: 64,
    average: 128,
    maximum: 512,
  });
  const built = buildManifest(source, { minimum: 64, average: 128, maximum: 512 });
  const firstObject = built.objects.values().next().value;
  const expectedObject = firstObject.slice();
  source.fill(0);
  assert.equal(bytesToHex(built.rootHash), bytesToHex(reference.rootHash));
  assert.deepEqual(firstObject, expectedObject);
  assert.equal(Buffer.isBuffer(firstObject), false);
});

test("varied manifest grouping has natural boundaries and reconnecting subtree goldens", () => {
  const goldenWorkspace = new MemoryManifestWorkspace();
  const golden = buildManifestFromEntries(
    Array.from({ length: 600 }, (_, index) => diverseGoldenEntry(index)),
    { minimum: 1, average: 2, maximum: 4 },
    goldenWorkspace,
    { readBatchRecords: 17 },
  );
  let goldenEnd = 0;
  assert.deepEqual(
    goldenWorkspace.levels.get(0).map((record) => {
      goldenEnd += record.value.node.entries.length;
      return goldenEnd;
    }),
    [105, 204, 272, 528, 600],
  );
  assert.equal(
    bytesToHex(golden.rootHash),
    "bd7ed42c2a32cea19d79921bf94b19ea7c7ff42e04dd8da1de4acd826cd46d42",
  );
  assert.equal(golden.fileSize, 1500);
  assert.equal(golden.nodeCount, 6);
  assert.equal(golden.depth, 2);
  assert.equal(golden.groupingRecordCount, 605);
  assert.equal(golden.groupingRecordBytesProcessed, 21_840);

  const deepWorkspace = new MemoryManifestWorkspace();
  const deep = buildManifestFromEntries(
    Array.from({ length: 22_000 }, (_, index) => diverseGoldenEntry(index)),
    { minimum: 1, average: 2, maximum: 4 },
    deepWorkspace,
    { readBatchRecords: 17 },
  );
  assert.equal(deep.depth, 3);
  assert.equal(deep.nodeCount, 146);
  assert.equal(
    bytesToHex(deep.root),
    "4541465201000101010000000200000004000000d8d6000000000000f0550000000000005bf66b17b8e92ae5965acdec219647377bfeb27349088cbeeb45dada9513bc9e",
  );
  assert.equal(
    bytesToHex(deep.rootHash),
    "2501ef8b9619af95229002f5062d8a33275b948f74867908a069b32861bdb72d",
  );

  let state = 0n;
  let count = 0;
  const groups = [];
  for (let index = 0; index < 10_000; index += 1) {
    state = advanceManifestGroupingState(state, variedEntry(index));
    count += 1;
    if (isManifestGroupBoundary(count, state, 64, 128, 256)) {
      groups.push(count);
      count = 0;
      state = 0n;
    }
  }
  if (count) groups.push(count);
  assert.deepEqual(
    groups.slice(0, 20),
    [
      196, 256, 85, 109, 78, 163, 256, 153, 214, 228, 256, 166, 139, 231, 123, 117, 106,
      110, 70, 253,
    ],
  );
  assert.equal(groups.length, 56);
  assert.ok(Math.min(...groups.slice(0, -1)) > 64);
  assert.ok(groups.some((value) => value < 256));

  const entries = Array.from({ length: 5000 }, (_, index) => variedEntry(index));
  const originalWorkspace = new MemoryManifestWorkspace();
  const original = buildManifestFromEntries(
    entries,
    { minimum: 64, average: 128, maximum: 512 },
    originalWorkspace,
    { readBatchRecords: 17 },
  );
  assert.equal(
    bytesToHex(original.rootHash),
    "97df2a12f31787e1e91bbccb5298654f28fbe90ad462b14ae700321ae5a3e04f",
  );
  assert.deepEqual(
    originalWorkspace.levels
      .get(0)
      .slice(0, 20)
      .map((record) => record.value.node.entries.length),
    [
      196, 256, 85, 109, 78, 163, 256, 153, 214, 228, 256, 166, 139, 231, 123, 117, 106,
      110, 70, 253,
    ],
  );
  const prependedWorkspace = new MemoryManifestWorkspace();
  const prepended = buildManifestFromEntries(
    [variedEntry(999_999), ...entries],
    { minimum: 64, average: 128, maximum: 512 },
    prependedWorkspace,
    { readBatchRecords: 17 },
  );
  assert.equal(
    bytesToHex(prepended.rootHash),
    "61c96b19854b88623c0fa773346c4ce9698b8a952adc1ba960e642f5465dd6bb",
  );
  const oldHashes = new Set(
    [...originalWorkspace.levels.values()].flatMap((records) =>
      records.map((record) => bytesToHex(record.value.hash)),
    ),
  );
  const reused = [...prependedWorkspace.levels.values()]
    .flatMap((records) => records)
    .filter((record) => oldHashes.has(bytesToHex(record.value.hash))).length;
  assert.ok(reused >= 28, `expected substantial suffix reuse, observed ${reused}`);
});

test("recomputed-digest corruption matrix rejects before affected content is exposed", () => {
  const parameters = { minimum: 1, average: 2, maximum: 4 };
  const workspace = new MemoryManifestWorkspace();
  const manifest = buildManifestFromEntries(
    Array.from({ length: 600 }, (_, index) => diverseGoldenEntry(index)),
    parameters,
    workspace,
    { readBatchRecords: 17 },
  );
  const nodeBytes = new Map(
    [...workspace.levels.values()]
      .flatMap((records) => records)
      .map((record) => [bytesToHex(record.value.hash), record.value.encoded.slice()]),
  );
  const readerFor = (nodes) => ({
    get(hash) {
      return nodes.get(bytesToHex(hash));
    },
  });
  const rootWithNode = (encoded, root = manifest.root) => {
    const rootBytes = root.slice();
    const hash = sha256(encoded);
    rootBytes.set(hash, 36);
    const nodes = new Map(nodeBytes);
    nodes.set(bytesToHex(hash), encoded);
    return { root: rootBytes, rootHash: sha256(rootBytes), nodes, hash };
  };
  const assertStructuralRejection = (root, rootHash, nodes, name) =>
    assert.throws(
      () => lookupManifest(root, 0, readerFor(nodes), rootHash),
      undefined,
      name,
    );

  const rootMutations = [
    ["root magic", (bytes) => (bytes[0] ^= 1)],
    ["root version", (bytes) => (bytes[4] = 2)],
    ["root flags", (bytes) => (bytes[6] = 2)],
    ["root chunker", (bytes) => (bytes[7] = 2)],
    ["root minimum", (bytes) => new DataView(bytes.buffer).setUint32(8, 0, true)],
    ["root average", (bytes) => new DataView(bytes.buffer).setUint32(12, 3, true)],
    ["root maximum", (bytes) => new DataView(bytes.buffer).setUint32(16, 0, true)],
    [
      "root file span",
      (bytes) => new DataView(bytes.buffer).setBigUint64(20, 1501n, true),
    ],
    [
      "root entry count",
      (bytes) => new DataView(bytes.buffer).setBigUint64(28, 601n, true),
    ],
    ["root node hash", (bytes) => bytes.fill(0, 36, 68)],
  ];
  for (const [name, mutate] of rootMutations) {
    const root = manifest.root.slice();
    mutate(root);
    assertStructuralRejection(root, sha256(root), nodeBytes, name);
  }

  const rootNodeKey = bytesToHex(manifest.root.slice(36));
  const rootNode = nodeBytes.get(rootNodeKey);
  const nodeHeaderMutations = [
    ["node magic", (bytes) => (bytes[0] ^= 1)],
    ["node version", (bytes) => (bytes[4] = 2)],
    ["node kind", (bytes) => (bytes[6] = 0)],
    ["node algorithm", (bytes) => (bytes[7] = 2)],
    ["node record count", (bytes) => new DataView(bytes.buffer).setUint32(8, 6, true)],
    ["node reserved", (bytes) => new DataView(bytes.buffer).setUint32(12, 1, true)],
    ["node span", (bytes) => new DataView(bytes.buffer).setBigUint64(16, 1501n, true)],
    [
      "node entry count",
      (bytes) => new DataView(bytes.buffer).setBigUint64(24, 601n, true),
    ],
  ];
  for (const [name, mutate] of nodeHeaderMutations) {
    const encoded = rootNode.slice();
    mutate(encoded);
    const variant = rootWithNode(encoded);
    assertStructuralRejection(variant.root, variant.rootHash, variant.nodes, name);
  }

  const decodedRootNode = decodeManifestNode(rootNode);
  assert.equal(decodedRootNode.kind, "internal");
  const firstChild = decodedRootNode.children[0];
  const leafKey = bytesToHex(firstChild.hash);
  const leaf = nodeBytes.get(leafKey);
  const replaceFirstLeaf = (encoded) => {
    const leafHash = sha256(encoded);
    const parent = rootNode.slice();
    parent.set(leafHash, 32);
    const variant = rootWithNode(parent);
    variant.nodes.set(bytesToHex(leafHash), encoded);
    return variant;
  };

  const zeroLengthLeaf = leaf.slice();
  new DataView(zeroLengthLeaf.buffer).setUint32(64, 0, true);
  let variant = replaceFirstLeaf(zeroLengthLeaf);
  assertStructuralRejection(
    variant.root,
    variant.rootHash,
    variant.nodes,
    "leaf record length",
  );

  const missingObjectLeaf = leaf.slice();
  missingObjectLeaf[32] ^= 1;
  variant = replaceFirstLeaf(missingObjectLeaf);
  let exposedBytes = 0;
  assert.throws(() => {
    const selected = lookupManifest(
      variant.root,
      0,
      readerFor(variant.nodes),
      variant.rootHash,
    );
    const object = new Map().get(bytesToHex(selected.entry.hash));
    if (!object) throw new Error("missing CAS object");
    exposedBytes += object.byteLength;
  }, /missing CAS object/);
  assert.equal(exposedBytes, 0, "leaf object-hash corruption exposed content");

  const missingChildParent = rootNode.slice();
  missingChildParent.fill(0, 32, 64);
  variant = rootWithNode(missingChildParent);
  assertStructuralRejection(
    variant.root,
    variant.rootHash,
    variant.nodes,
    "internal child hash",
  );

  for (const field of ["span", "count"]) {
    const parent = rootNode.slice();
    const parentView = new DataView(parent.buffer);
    const root = manifest.root.slice();
    const rootView = new DataView(root.buffer);
    if (field === "span") {
      parentView.setBigUint64(64, BigInt(firstChild.span + 1), true);
      parentView.setBigUint64(16, 1501n, true);
      rootView.setBigUint64(20, 1501n, true);
    } else {
      parentView.setBigUint64(72, BigInt(firstChild.entryCount + 1), true);
      parentView.setBigUint64(24, 601n, true);
      rootView.setBigUint64(28, 601n, true);
    }
    variant = rootWithNode(parent, root);
    assertStructuralRejection(
      variant.root,
      variant.rootHash,
      variant.nodes,
      `internal child ${field}`,
    );
  }

  const missingRootNodes = new Map(nodeBytes);
  missingRootNodes.delete(rootNodeKey);
  assertStructuralRejection(
    manifest.root,
    manifest.rootHash,
    missingRootNodes,
    "missing root node",
  );
  const missingLeafNodes = new Map(nodeBytes);
  missingLeafNodes.delete(leafKey);
  assertStructuralRejection(
    manifest.root,
    manifest.rootHash,
    missingLeafNodes,
    "missing child node",
  );

  const children = decodedRootNode.children;
  for (const [name, changedChildren] of [
    ["deleted child", children.slice(1)],
    ["duplicate child", [children[0], ...children]],
  ]) {
    const encoded = encodeManifestNode({
      kind: "internal",
      span: changedChildren.reduce((sum, child) => sum + child.span, 0),
      entryCount: changedChildren.reduce((sum, child) => sum + child.entryCount, 0),
      children: changedChildren,
    });
    variant = rootWithNode(encoded);
    assertStructuralRejection(variant.root, variant.rootHash, variant.nodes, name);
  }

  // Reordering can describe a different valid file under a different root identity.
  // It is corruption only relative to the selected authenticated root, which must not
  // be replaced merely because every altered descendant was rehashed.
  const reordered = encodeManifestNode({
    kind: "internal",
    span: decodedRootNode.span,
    entryCount: decodedRootNode.entryCount,
    children: [children[1], children[0], ...children.slice(2)],
  });
  variant = rootWithNode(reordered);
  assertStructuralRejection(
    variant.root,
    manifest.rootHash,
    variant.nodes,
    "reordered child under the authoritative root identity",
  );
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

test("manifest builder snapshots caller records, parameters, and borrowed workspace pages", () => {
  const parameters = { minimum: 1, average: 2, maximum: 4 };
  const firstHash = sha256(Uint8Array.of(1));
  const secondHash = sha256(Uint8Array.of(2));
  const reused = { hash: firstHash.slice(), length: 1 };
  function* aliasedEntries() {
    yield reused;
    reused.hash.set(secondHash);
    reused.length = 2;
    yield reused;
  }
  const aliasedWorkspace = new MemoryManifestWorkspace();
  const aliased = buildManifestFromEntries(
    aliasedEntries(),
    parameters,
    aliasedWorkspace,
  );
  const ownedWorkspace = new MemoryManifestWorkspace();
  const owned = buildManifestFromEntries(
    [
      { hash: firstHash, length: 1 },
      { hash: secondHash, length: 2 },
    ],
    parameters,
    ownedWorkspace,
  );
  assert.equal(bytesToHex(aliased.rootHash), bytesToHex(owned.rootHash));

  const borrowedBufferHash = Buffer.from(firstHash);
  function* bufferEntry() {
    yield { hash: borrowedBufferHash, length: 1 };
    borrowedBufferHash.fill(0);
  }
  const bufferBuilt = buildManifestFromEntries(
    bufferEntry(),
    parameters,
    new MemoryManifestWorkspace(),
  );
  const expectedBufferBuilt = buildManifestFromEntries(
    [{ hash: firstHash, length: 1 }],
    parameters,
    new MemoryManifestWorkspace(),
  );
  assert.equal(
    bytesToHex(bufferBuilt.rootHash),
    bytesToHex(expectedBufferBuilt.rootHash),
  );

  const mutableParameters = { minimum: 1, average: 2, maximum: 4 };
  function* mutatingParameters() {
    yield { hash: firstHash, length: 1 };
    mutableParameters.minimum = 4;
    mutableParameters.average = 4;
    mutableParameters.maximum = 4;
    yield { hash: secondHash, length: 2 };
  }
  const parameterResult = buildManifestFromEntries(
    mutatingParameters(),
    mutableParameters,
    new MemoryManifestWorkspace(),
  );
  assert.deepEqual(decodeManifestRoot(parameterResult.root).parameters, parameters);

  const parameterGetterReads = { minimum: 0, average: 0, maximum: 0 };
  const getterParameterResult = buildManifestFromEntries(
    [{ hash: firstHash, length: 1 }],
    {
      get minimum() {
        parameterGetterReads.minimum += 1;
        return 1;
      },
      get average() {
        parameterGetterReads.average += 1;
        return 2;
      },
      get maximum() {
        parameterGetterReads.maximum += 1;
        return 4;
      },
    },
    new MemoryManifestWorkspace(),
  );
  assert.deepEqual(
    decodeManifestRoot(getterParameterResult.root).parameters,
    parameters,
  );
  assert.deepEqual(parameterGetterReads, { minimum: 1, average: 1, maximum: 1 });

  class InvalidatingWorkspace extends MemoryManifestWorkspace {
    activeRows = [];
    invalidate() {
      for (const row of this.activeRows) row.child.hash.fill(0);
      this.activeRows = [];
    }
    writeNode(record) {
      this.invalidate();
      const stored = {
        ...record,
        child: { ...record.child, hash: record.child.hash.slice() },
        value: {
          ...record.value,
          hash: record.value.hash.slice(),
          encoded: record.value.encoded.slice(),
        },
      };
      super.writeNode(stored);
      record.child.hash.fill(0);
      record.value.hash.fill(0);
      record.value.encoded.fill(0);
    }
    readLevel(level, afterIndex, limit) {
      this.invalidate();
      this.activeRows = super.readLevel(level, afterIndex, limit).map((row) => ({
        index: row.index,
        child: { ...row.child, hash: Buffer.from(row.child.hash) },
      }));
      return this.activeRows;
    }
  }
  const many = Array.from({ length: 20_000 }, (_, index) => variedEntry(index));
  const invalidating = buildManifestFromEntries(
    many,
    { minimum: 64, average: 128, maximum: 512 },
    new InvalidatingWorkspace(),
    { readBatchRecords: 64 },
  );
  const reference = buildManifestFromEntries(
    many,
    { minimum: 64, average: 128, maximum: 512 },
    new MemoryManifestWorkspace(),
    { readBatchRecords: 64 },
  );
  assert.equal(bytesToHex(invalidating.rootHash), bytesToHex(reference.rootHash));

  class CorruptTotalsWorkspace extends MemoryManifestWorkspace {
    readLevel(level, afterIndex, limit) {
      const rows = super.readLevel(level, afterIndex, limit);
      return rows.map((row, index) =>
        afterIndex < 0 && index === 0
          ? { ...row, child: { ...row.child, span: row.child.span + 1 } }
          : row,
      );
    }
  }
  assert.throws(
    () =>
      buildManifestFromEntries(
        many,
        { minimum: 64, average: 128, maximum: 512 },
        new CorruptTotalsWorkspace(),
        { readBatchRecords: 17 },
      ),
    /root totals differ/,
  );
});

test("manifest builder enforces maxEntries before copying or over-pulling", () => {
  const entry = { hash: sha256(Uint8Array.of(1)), length: 1 };
  let pulls = 0;
  function* entries(count) {
    for (let index = 0; index < count; index += 1) {
      pulls += 1;
      yield entry;
    }
  }
  const acceptedWorkspace = new MemoryManifestWorkspace();
  assert.doesNotThrow(() =>
    buildManifestFromEntries(
      entries(1),
      { minimum: 1, average: 2, maximum: 4 },
      acceptedWorkspace,
      { maxEntries: 1 },
    ),
  );
  assert.equal(pulls, 1);
  assert.equal(acceptedWorkspace.levels.get(0).length, 1);
  pulls = 0;
  const rejectedWorkspace = new MemoryManifestWorkspace();
  assert.throws(
    () =>
      buildManifestFromEntries(
        entries(3),
        { minimum: 1, average: 2, maximum: 4 },
        rejectedWorkspace,
        { maxEntries: 1 },
      ),
    /observed manifest entry count/,
  );
  assert.equal(pulls, 2);
  assert.equal(rejectedWorkspace.levels.size, 0);
});

test("manifest codecs reject overflow and malformed encodings without digest checks", () => {
  const hash = new Uint8Array(32).fill(0x44);
  class SubstitutingBytes extends Uint8Array {
    subarray() {
      return new Uint8Array(this.byteLength).fill(0xff);
    }
  }
  const rootHashView = new SubstitutingBytes(32);
  rootHashView.set(hash);
  const expectedRootHashView = new Uint8Array(rootHashView);
  const rootParameterReads = { minimum: 0, average: 0, maximum: 0 };
  let rootEntryCountReads = 0;
  const getterRoot = encodeManifestRoot({
    parameters: {
      get minimum() {
        rootParameterReads.minimum += 1;
        rootHashView.fill(0);
        return 1;
      },
      get average() {
        rootParameterReads.average += 1;
        return 2;
      },
      get maximum() {
        rootParameterReads.maximum += 1;
        return 4;
      },
    },
    fileSize: 1,
    get entryCount() {
      rootEntryCountReads += 1;
      return rootEntryCountReads === 1 ? 1 : 2;
    },
    rootNodeHash: rootHashView,
  });
  assert.deepEqual(rootParameterReads, { minimum: 1, average: 1, maximum: 1 });
  assert.equal(rootEntryCountReads, 1);
  assert.equal(decodeManifestRoot(getterRoot).entryCount, 1);
  assert.deepEqual(decodeManifestRoot(getterRoot).rootNodeHash, expectedRootHashView);

  const leafHashView = new SubstitutingBytes(32);
  leafHashView.fill(0x55);
  const expectedLeafHash = new Uint8Array(leafHashView);
  let leafSpanReads = 0;
  let leafCountReads = 0;
  let leafLengthReads = 0;
  const getterLeaf = encodeManifestNode({
    kind: "leaf",
    get span() {
      leafSpanReads += 1;
      return leafSpanReads === 1 ? 1 : 2;
    },
    get entryCount() {
      leafCountReads += 1;
      return leafCountReads === 1 ? 1 : 2;
    },
    entries: [
      {
        hash: leafHashView,
        get length() {
          leafLengthReads += 1;
          leafHashView.fill(0);
          return leafLengthReads === 1 ? 1 : 2;
        },
      },
    ],
  });
  const decodedGetterLeaf = decodeManifestNode(getterLeaf);
  assert.equal(leafSpanReads, 1);
  assert.equal(leafCountReads, 1);
  assert.equal(leafLengthReads, 1);
  assert.equal(decodedGetterLeaf.span, 1);
  assert.deepEqual(decodedGetterLeaf.entries[0].hash, expectedLeafHash);

  const childHashView = new SubstitutingBytes(32);
  childHashView.fill(0x66);
  const expectedChildHash = new Uint8Array(childHashView);
  let childSpanReads = 0;
  let childCountReads = 0;
  const getterInternal = encodeManifestNode({
    kind: "internal",
    span: 1,
    entryCount: 1,
    children: [
      {
        hash: childHashView,
        get span() {
          childSpanReads += 1;
          return childSpanReads === 1 ? 1 : 2;
        },
        get entryCount() {
          childCountReads += 1;
          childHashView.fill(0);
          return childCountReads === 1 ? 1 : 2;
        },
      },
    ],
  });
  const decodedGetterInternal = decodeManifestNode(getterInternal);
  assert.equal(childSpanReads, 1);
  assert.equal(childCountReads, 1);
  assert.deepEqual(decodedGetterInternal.children[0].hash, expectedChildHash);
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
  assert.throws(
    () =>
      decodeManifestNode(
        new Uint8Array(MAX_MANIFEST_NODE_BYTES + 1),
        new Uint8Array(32),
      ),
    /absolute v1 byte maximum/,
  );
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

test("format inspection accepts uint32 parameters while materializing paths reject unsupported maxima", () => {
  const unsupported = {
    minimum: 1,
    average: 2,
    maximum: MAX_CONTENT_OBJECT_BYTES + 1,
  };
  const emptyNode = encodeManifestNode({
    kind: "leaf",
    span: 0,
    entryCount: 0,
    entries: [],
  });
  const emptyHash = sha256(emptyNode);
  const root = encodeManifestRoot({
    parameters: unsupported,
    fileSize: 0,
    entryCount: 0,
    rootNodeHash: emptyHash,
  });
  assert.deepEqual(decodeManifestRoot(root).parameters, unsupported);
  let nodeLookups = 0;
  class CountingNodes extends Map {
    get(key) {
      nodeLookups += 1;
      return super.get(key);
    }
  }
  const old = Object.freeze({
    id: bytesToHex(sha256(root)),
    root,
    rootHash: sha256(root),
    entries: Object.freeze([]),
    nodes: new CountingNodes([
      [
        bytesToHex(emptyHash),
        Object.freeze({
          hash: emptyHash,
          encoded: emptyNode,
          node: decodeManifestNode(emptyNode),
        }),
      ],
    ]),
  });
  let reads = 0;
  const source = {
    size: 0,
    read() {
      reads += 1;
      return new Uint8Array();
    },
  };
  class TrackedInsertion extends Uint8Array {
    copyAttempts = 0;
    subarray(start, end) {
      this.copyAttempts += 1;
      return super.subarray(start, end);
    }
  }
  const insertion = new TrackedInsertion(1);
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(source, old, {
        offset: 0,
        deleteLength: 0,
        insertBytes: insertion,
      }),
    /effective content-object limit/,
  );
  assert.equal(nodeLookups, 0);
  assert.equal(insertion.copyAttempts, 0);
  assert.throws(
    () => buildManifest(new Uint8Array(), unsupported),
    /effective content-object limit/,
  );
  assert.throws(
    () =>
      rebuildEditedContentStreaming(
        source,
        { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
        unsupported,
        new MemoryManifestWorkspace(),
        { putObject() {} },
      ),
    /effective content-object limit/,
  );
  assert.equal(reads, 0);
  assert.equal(MAX_DIAGNOSTIC_CONTENT_BYTES, MAX_CONTENT_OBJECT_BYTES);
  assert.throws(
    () => buildManifest(new Uint8Array(MAX_DIAGNOSTIC_CONTENT_BYTES + 1), defaults),
    /diagnostic manifest input/,
  );
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

test("manifest readers isolate authoritative hashes from malicious reader mutation", () => {
  const entryA = { hash: sha256(Uint8Array.of(1)), length: 1 };
  const entryB = { hash: sha256(Uint8Array.of(2)), length: 1 };
  const encodedA = encodeManifestNode({
    kind: "leaf",
    span: 1,
    entryCount: 1,
    entries: [entryA],
  });
  const encodedB = encodeManifestNode({
    kind: "leaf",
    span: 1,
    entryCount: 1,
    entries: [entryB],
  });
  const hashA = sha256(encodedA);
  const hashB = sha256(encodedB);
  const root = encodeManifestRoot({
    parameters: { minimum: 1, average: 2, maximum: 4 },
    fileSize: 1,
    entryCount: 1,
    rootNodeHash: hashA,
  });
  const reader = {
    get(lookupHash) {
      assert.equal(Buffer.isBuffer(lookupHash), false);
      lookupHash.set(hashB);
      return encodedB;
    },
  };
  const bufferRoot = Buffer.from(root);
  const bufferRootHash = Buffer.from(sha256(root));
  assert.throws(
    () => lookupManifest(bufferRoot, 0, reader, bufferRootHash),
    /digest mismatch/,
  );
  assert.throws(
    () => validateManifestTree(bufferRoot, reader, bufferRootHash),
    /digest mismatch/,
  );

  const stableReader = {
    get(lookupHash) {
      assert.equal(Buffer.isBuffer(lookupHash), false);
      return Buffer.from(encodedA);
    },
  };
  const cursor = new ManifestSequentialCursor(
    Buffer.from(root),
    0,
    stableReader,
    Buffer.from(sha256(root)),
  );
  const peeked = cursor.peek();
  const expectedEntryHash = bytesToHex(peeked.entry.hash);
  peeked.entry.hash.fill(0);
  assert.equal(bytesToHex(cursor.peek().entry.hash), expectedEntryHash);
  const returned = cursor.next();
  assert.equal(bytesToHex(returned.entry.hash), expectedEntryHash);
  returned.entry.hash.fill(0);
  assert.equal(cursor.peek(), null);

  const encodedBuffer = Buffer.from(encodedA);
  const decoded = decodeManifestNode(encodedBuffer, Buffer.from(hashA));
  const decodedHash = decoded.entries[0].hash.slice();
  encodedBuffer.fill(0);
  assert.deepEqual(decoded.entries[0].hash, decodedHash);
  assert.equal(Buffer.isBuffer(decoded.entries[0].hash), false);
  const rootBuffer = Buffer.from(root);
  const decodedRoot = decodeManifestRoot(rootBuffer, Buffer.from(sha256(root)));
  const decodedRootNodeHash = decodedRoot.rootNodeHash.slice();
  rootBuffer.fill(0);
  assert.deepEqual(decodedRoot.rootNodeHash, decodedRootNodeHash);
  assert.equal(Buffer.isBuffer(decodedRoot.rootNodeHash), false);
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
    assert.equal(built.nodeCount, 396);
    assert.equal(built.depth, 3);
    assert.equal(built.peakRetainedRecords, 259);
    assert.equal(built.peakRetainedSerializedRecordBytes, 9336);
    assert.equal(built.groupingRecordCount, 100_396);
    assert.equal(built.groupingRecordBytesProcessed, 3_618_996);
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
  const insertion = Buffer.from([42]);
  const edit = { offset: 25_000, deleteLength: 1, insertBytes: insertion };
  const directory = mkdtempSync(path.join(tmpdir(), "efs-m1-fallback-"));
  const workspace = new DurableManifestWorkspace(path.join(directory, "fallback.db"));
  try {
    const result = rebuildManifestLocallyOrStream(
      {
        size: original.length,
        read(offset, length) {
          insertion.fill(0);
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
    assert.ok(result.metrics.peakPendingEntries <= 256);
    assert.equal(result.metrics.insertionCopyCount, 1);
    assert.equal(result.metrics.insertionBytesCopied, 1);
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

test("diagnostic local rebuild authenticates cached entries and the complete capped closure", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(64 * 1024 + 19, 0xcac4ed);
  const built = buildManifest(original, parameters);
  const edit = { offset: 20_000, deleteLength: 1, insertBytes: Uint8Array.of(42) };
  const cases = [
    ["entry hash", (copy) => (copy.entries[0].hash[0] ^= 1)],
    ["encoded node", (copy) => (copy.nodes.values().next().value.encoded[16] ^= 1)],
    ["cached hash", (copy) => (copy.nodes.values().next().value.hash[0] ^= 1)],
    [
      "cached decoded node",
      (copy) => {
        const node = [...copy.nodes.values()].find(
          (value) => value.node.kind === "leaf" && value.node.entries.length > 0,
        ).node;
        node.entries[0].hash[0] ^= 1;
      },
    ],
    ["missing node", (copy) => copy.nodes.delete(bytesToHex(copy.root.slice(36)))],
    [
      "extra node",
      (copy) => {
        const value = copy.nodes.values().next().value;
        copy.nodes.set("00".repeat(32), value);
      },
    ],
  ];
  for (const [name, mutate] of cases) {
    const copy = mutableDiagnosticCopy(built);
    mutate(copy);
    let sourceReads = 0;
    assert.throws(
      () =>
        rebuildDiagnosticManifestLocally(
          {
            size: original.length,
            read(offset, length) {
              sourceReads += 1;
              return original.slice(offset, offset + length);
            },
          },
          copy,
          edit,
        ),
      undefined,
      name,
    );
    assert.equal(sourceReads, 0, `${name} was detected after source I/O`);
  }

  class CountingMap extends Map {
    reads = 0;
    get(key) {
      this.reads += 1;
      return super.get(key);
    }
  }
  const overCap = mutableDiagnosticCopy(built);
  const decoded = decodeManifestRoot(overCap.root, overCap.rootHash);
  overCap.root = encodeManifestRoot({
    parameters: decoded.parameters,
    fileSize: decoded.fileSize,
    entryCount: 16_385,
    rootNodeHash: decoded.rootNodeHash,
  });
  overCap.rootHash = sha256(overCap.root);
  overCap.id = bytesToHex(overCap.rootHash);
  overCap.nodes = new CountingMap(overCap.nodes);
  let overCapSourceReads = 0;
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(
        {
          size: original.length,
          read(offset, length) {
            overCapSourceReads += 1;
            return original.slice(offset, offset + length);
          },
        },
        overCap,
        edit,
      ),
    LocalRebuildLimitError,
  );
  assert.equal(overCap.nodes.reads, 0);
  assert.equal(overCapSourceReads, 0);

  const oversizedBytes = new Uint8Array(MAX_CONTENT_OBJECT_BYTES);
  const malformedByteCases = [
    [
      "oversized root",
      (copy) => {
        copy.root = oversizedBytes;
      },
      0,
      /exactly 68 bytes/,
    ],
    [
      "oversized root hash",
      (copy) => {
        copy.rootHash = oversizedBytes;
      },
      0,
      /32 bytes/,
    ],
    [
      "oversized cached hash",
      (copy) => {
        const rootNode = bytesToHex(decodeManifestRoot(copy.root).rootNodeHash);
        copy.nodes.get(rootNode).hash = oversizedBytes;
      },
      1,
      /cached node hash must contain 32 bytes/,
    ],
    [
      "oversized cached encoding",
      (copy) => {
        const rootNode = bytesToHex(decodeManifestRoot(copy.root).rootNodeHash);
        copy.nodes.get(rootNode).encoded = new Uint8Array(MAX_MANIFEST_NODE_BYTES + 1);
      },
      1,
      /cached node exceeds the v1 byte maximum/,
    ],
  ];
  for (const [name, mutate, expectedMapReads, message] of malformedByteCases) {
    const copy = mutableDiagnosticCopy(built);
    mutate(copy);
    copy.nodes = new CountingMap(copy.nodes);
    let sourceReads = 0;
    assert.throws(
      () =>
        rebuildDiagnosticManifestLocally(
          {
            size: original.length,
            read() {
              sourceReads += 1;
              return new Uint8Array();
            },
          },
          copy,
          edit,
        ),
      message,
      name,
    );
    assert.equal(copy.nodes.reads, expectedMapReads, `${name} node reads`);
    assert.equal(sourceReads, 0, `${name} reached source I/O`);
  }

  const repeatedEntry = { hash: sha256(Uint8Array.of(99)), length: 1 };
  const repeatedLeafNode = {
    kind: "leaf",
    span: 256,
    entryCount: 256,
    entries: Array.from({ length: 256 }, () => repeatedEntry),
  };
  const repeatedLeafEncoded = encodeManifestNode(repeatedLeafNode);
  const repeatedLeafHash = sha256(repeatedLeafEncoded);
  const repeatedChild = { hash: repeatedLeafHash, span: 256, entryCount: 256 };
  const repeatedRootNode = {
    kind: "internal",
    span: 768,
    entryCount: 768,
    children: [repeatedChild, repeatedChild, repeatedChild],
  };
  const repeatedRootEncoded = encodeManifestNode(repeatedRootNode);
  const repeatedRootNodeHash = sha256(repeatedRootEncoded);
  const repeatedRoot = encodeManifestRoot({
    parameters: { minimum: 1, average: 2, maximum: 4 },
    fileSize: 768,
    entryCount: 768,
    rootNodeHash: repeatedRootNodeHash,
  });
  const repeatedRootHash = sha256(repeatedRoot);
  let repeatedSourceReads = 0;
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(
        {
          size: 768,
          read() {
            repeatedSourceReads += 1;
            return new Uint8Array();
          },
        },
        {
          id: bytesToHex(repeatedRootHash),
          root: repeatedRoot,
          rootHash: repeatedRootHash,
          entries: Array.from({ length: 768 }, () => repeatedEntry),
          nodes: new Map([
            [
              bytesToHex(repeatedRootNodeHash),
              {
                hash: repeatedRootNodeHash,
                encoded: repeatedRootEncoded,
                node: repeatedRootNode,
              },
            ],
            [
              bytesToHex(repeatedLeafHash),
              {
                hash: repeatedLeafHash,
                encoded: repeatedLeafEncoded,
                node: repeatedLeafNode,
              },
            ],
          ]),
        },
        { offset: 1, deleteLength: 1, insertBytes: Uint8Array.of(1) },
        {
          maxRetainedEntries: 16_384,
          maxRetainedNodes: 2,
          maxAffectedEntries: 4096,
          maxAffectedBytes: 16 * 1024 * 1024,
        },
      ),
    /node-visit limit/,
  );
  assert.equal(repeatedSourceReads, 0);

  const insertion = Buffer.from([7, 8, 9]);
  const insertionOriginal = Buffer.from(insertion);
  const inserted = rebuildDiagnosticManifestLocally(
    {
      size: original.length,
      read(offset, length) {
        insertion.fill(0);
        return original.slice(offset, offset + length);
      },
    },
    built,
    { offset: 20_000, deleteLength: 0, insertBytes: insertion },
  );
  const expectedInserted = new Uint8Array(original.length + insertionOriginal.length);
  expectedInserted.set(original.subarray(0, 20_000));
  expectedInserted.set(insertionOriginal, 20_000);
  expectedInserted.set(original.subarray(20_000), 20_000 + insertionOriginal.length);
  assert.equal(
    bytesToHex(inserted.rootHash),
    buildManifest(expectedInserted, parameters).id,
  );
  assert.equal(inserted.metrics.insertionCopyCount, 1);
  assert.equal(inserted.metrics.insertionBytesCopied, insertionOriginal.length);

  const wrapperInsertion = Buffer.from([11, 12, 13]);
  const wrapperOriginal = Buffer.from(wrapperInsertion);
  const wrapperResult = rebuildManifestLocallyOrStream(
    {
      size: original.length,
      read(offset, length) {
        wrapperInsertion.fill(0);
        return original.slice(offset, offset + length);
      },
    },
    built,
    { offset: 20_000, deleteLength: 0, insertBytes: wrapperInsertion },
    parameters,
    new MemoryManifestWorkspace(),
    { putObject() {} },
  );
  assert.equal(wrapperResult.mode, "local");
  assert.equal(wrapperResult.manifest.metrics.insertionCopyCount, 1);
  assert.equal(
    wrapperResult.manifest.metrics.insertionBytesCopied,
    wrapperOriginal.length,
  );
  const wrapperExpected = new Uint8Array(original.length + wrapperOriginal.length);
  wrapperExpected.set(original.subarray(0, 20_000));
  wrapperExpected.set(wrapperOriginal, 20_000);
  wrapperExpected.set(original.subarray(20_000), 20_000 + wrapperOriginal.length);
  assert.equal(
    bytesToHex(wrapperResult.manifest.rootHash),
    buildManifest(wrapperExpected, parameters).id,
  );
});

test("diagnostic local rebuild enforces its retained limits before source work", () => {
  // M3.2 contract change: the fixed 16 MiB content cap is lifted; the
  // retained-entry ceiling is now the pre-source-work guard. With maximum=1
  // every byte is one entry, so lowering maxRetainedEntries below the source
  // entry count provably rejects before any source read.
  const parameters = { minimum: 1, average: 1, maximum: 1 };
  const original = new Uint8Array(200);
  const before = buildManifest(original, parameters);
  const limits = {
    ...DEFAULT_LOCAL_REBUILD_LIMITS,
    maxRetainedEntries: 100,
    maxAffectedEntries: 100,
  };
  const source = {
    size: original.length,
    reads: 0,
    read(offset, length) {
      this.reads += 1;
      return original.slice(offset, offset + length);
    },
  };
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(
        source,
        before,
        {
          offset: original.length,
          deleteLength: 0,
          insertBytes: Uint8Array.of(1),
        },
        limits,
      ),
    (error) => {
      assert.ok(error instanceof LocalRebuildLimitError);
      assert.deepEqual(error.attemptMetrics, {
        sourceBytesRead: 0,
        bytesHashed: 0,
        largestSourceRead: 0,
        chunkerInputBytesCopied: 0,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 0,
      });
      return true;
    },
  );
  assert.equal(source.reads, 0);

  const workspace = new MemoryManifestWorkspace();
  const streamed = rebuildManifestLocallyOrStream(
    source,
    before,
    {
      offset: original.length,
      deleteLength: 0,
      insertBytes: Uint8Array.of(1),
    },
    parameters,
    workspace,
    { putObject() {} },
    limits,
    { readWindowBytes: parameters.maximum, manifestReadBatchRecords: 17 },
  );
  assert.equal(streamed.mode, "streamed-fallback");
  assert.equal(
    decodeManifestRoot(streamed.manifest.root, streamed.manifest.rootHash).fileSize,
    original.length + 1,
  );
  assert.ok(streamed.metrics.peakPendingEntries <= 256);
});

test("diagnostic local rebuild handles appends beyond the lifted 16 MiB diagnostic cap", () => {
  // M3.2 contract change: the fixed 16 MiB per-file cap is lifted. An append
  // past the old boundary now reconnects locally; the streamed rebuild is the
  // byte-identical reference.
  const original = new Uint8Array(MAX_DIAGNOSTIC_CONTENT_BYTES).fill(0x5a);
  const before = buildManifest(original, defaults);
  const source = {
    size: original.length,
    reads: 0,
    read(offset, length) {
      this.reads += 1;
      return original.slice(offset, offset + length);
    },
  };
  const edit = {
    offset: original.length,
    deleteLength: 0,
    insertBytes: Uint8Array.of(1),
  };
  const local = rebuildDiagnosticManifestLocally(source, before, edit);
  assert.equal(local.fileSize, MAX_DIAGNOSTIC_CONTENT_BYTES + 1);
  assert.equal(local.metrics.fellBackToEnd, false);
  const workspace = new MemoryManifestWorkspace();
  const streamed = rebuildManifestLocallyOrStream(
    source,
    before,
    edit,
    defaults,
    workspace,
    { putObject() {} },
    undefined,
    { readWindowBytes: defaults.maximum, manifestReadBatchRecords: 17 },
  );
  assert.equal(bytesToHex(local.rootHash), bytesToHex(streamed.manifest.rootHash));
});

test("diagnostic local limits are fixed lowering-only caps", () => {
  const parameters = { minimum: 1, average: 1, maximum: 1 };
  const original = Uint8Array.of(1, 2);
  const before = buildManifest(original, parameters);
  let sourceReads = 0;
  const source = {
    size: original.length,
    read(offset, length) {
      sourceReads += 1;
      return original.slice(offset, offset + length);
    },
  };
  const insertBytes = Uint8Array.of(7, 8);
  const edit = { offset: 1, deleteLength: 1, insertBytes };
  for (const name of Object.keys(DEFAULT_LOCAL_REBUILD_LIMITS)) {
    let insertionReads = 0;
    const unreadEdit = {
      offset: 1,
      deleteLength: 1,
      get insertBytes() {
        insertionReads += 1;
        return insertBytes;
      },
    };
    const limits = {
      ...DEFAULT_LOCAL_REBUILD_LIMITS,
      [name]: DEFAULT_LOCAL_REBUILD_LIMITS[name] + 1,
    };
    assert.throws(
      () => rebuildDiagnosticManifestLocally(source, before, unreadEdit, limits),
      /fixed diagnostic cap/,
      `${name} accepted an expanded diagnostic cap`,
    );
    assert.equal(insertionReads, 0, `${name} read insertion before admission`);
  }
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(source, before, edit, {
        ...DEFAULT_LOCAL_REBUILD_LIMITS,
        maxRetainedEntries: 1,
        maxAffectedEntries: 2,
      }),
    /maxAffectedEntries exceeds maxRetainedEntries/,
  );
  assert.equal(sourceReads, 0);

  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(source, before, edit, {
        ...DEFAULT_LOCAL_REBUILD_LIMITS,
        maxRetainedEntries: 2,
        maxAffectedEntries: 2,
      }),
    /retained-entry/,
  );

  class SpoofedOversizedInsertion extends Uint8Array {
    get byteLength() {
      return 1;
    }
    subarray() {
      return Uint8Array.of(99);
    }
  }
  class CountingMap extends Map {
    reads = 0;
    get(key) {
      this.reads += 1;
      return super.get(key);
    }
  }
  const oversizedInsertion = new SpoofedOversizedInsertion(
    MAX_CONTENT_OBJECT_BYTES + 1,
  );
  const countedBefore = mutableDiagnosticCopy(before);
  countedBefore.nodes = new CountingMap(countedBefore.nodes);
  sourceReads = 0;
  const copyCalls = countUint8ArraySetCalls(() =>
    assert.throws(
      () =>
        rebuildDiagnosticManifestLocally(source, countedBefore, {
          offset: 0,
          deleteLength: 0,
          insertBytes: oversizedInsertion,
        }),
      /supported object limit/,
    ),
  );
  assert.equal(copyCalls, 0);
  assert.equal(sourceReads, 0);
  assert.equal(countedBefore.nodes.reads, 0);

  const largeInsertion = new Uint8Array(
    DEFAULT_LOCAL_REBUILD_LIMITS.maxAffectedEntries + 1,
  );
  const empty = buildManifest(new Uint8Array(), parameters);
  const streamed = rebuildManifestLocallyOrStream(
    { size: 0, read: () => new Uint8Array() },
    empty,
    { offset: 0, deleteLength: 0, insertBytes: largeInsertion },
    parameters,
    new MemoryManifestWorkspace(),
    { putObject() {} },
    undefined,
    { readWindowBytes: 64, manifestReadBatchRecords: 17 },
  );
  assert.equal(streamed.mode, "streamed-fallback");
  assert.equal(streamed.metrics.insertionCopyCount, 1);
  assert.equal(streamed.metrics.insertionBytesCopied, largeInsertion.length);
  assert.equal(
    decodeManifestRoot(streamed.manifest.root, streamed.manifest.rootHash).entryCount,
    largeInsertion.length,
  );
});

test("streamed rebuild owns callback inputs and isolates mutating object sinks", () => {
  const original = fixture(64 * 1024 + 19, 0xbadc0de);
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const edit = {
    offset: 25_000,
    deleteLength: 1,
    insertBytes: Buffer.from([42]),
  };
  const options = { readWindowBytes: 257, manifestReadBatchRecords: 11 };
  const source = {
    size: original.length,
    read(offset, length) {
      source.size = 0;
      edit.offset = 0;
      edit.deleteLength = 0;
      edit.insertBytes.fill(0);
      parameters.minimum = 1;
      parameters.average = 2;
      parameters.maximum = 4;
      options.readWindowBytes = 1;
      return original.slice(offset, offset + length);
    },
  };
  const workspace = new MemoryManifestWorkspace();
  const result = rebuildEditedContentStreaming(
    source,
    edit,
    parameters,
    workspace,
    {
      putObject(hash, bytes) {
        assert.equal(Buffer.isBuffer(hash), false);
        assert.equal(Buffer.isBuffer(bytes), false);
        hash.fill(0);
        bytes.fill(0);
      },
    },
    "ownership regression",
    options,
  );
  const edited = original.slice();
  edited[25_000] = 42;
  assert.equal(
    bytesToHex(result.manifest.rootHash),
    buildManifest(edited, { minimum: 64, average: 128, maximum: 512 }).id,
  );
  assert.ok(result.metrics.peakPendingEntries <= 256);
  assert.equal(result.metrics.insertionCopyCount, 1);
  assert.equal(result.metrics.insertionBytesCopied, 1);
});

test("streamed rebuild normalizes subclass source ranges before consumption", () => {
  class SubstitutingSourceRange extends Uint8Array {
    get byteLength() {
      return 1;
    }
    subarray() {
      return Uint8Array.of(0xff);
    }
  }
  const original = fixture(4096 + 37, 0x51ced);
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const result = rebuildEditedContentStreaming(
    {
      size: original.length,
      read(offset, length) {
        const range = new SubstitutingSourceRange(length);
        range.set(original.slice(offset, offset + length));
        return range;
      },
    },
    { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
    parameters,
    new MemoryManifestWorkspace(),
    { putObject() {} },
    "subclass source normalization",
    { readWindowBytes: 100, manifestReadBatchRecords: 17 },
  );
  const decoded = decodeManifestRoot(result.manifest.root, result.manifest.rootHash);
  assert.equal(
    bytesToHex(result.manifest.rootHash),
    buildManifest(original, parameters).id,
  );
  assert.equal(decoded.fileSize, original.length);
  assert.equal(result.metrics.sourceBytesRead, original.length);
  assert.equal(result.metrics.bytesHashed, original.length);
});

test("streamed rebuild validates size and attempted-local metrics before callbacks", () => {
  let sourceReads = 0;
  let workspaceReads = 0;
  let workspaceWrites = 0;
  let objectPuts = 0;
  const source = {
    size: Number.MAX_SAFE_INTEGER,
    read() {
      sourceReads += 1;
      return new Uint8Array();
    },
  };
  const workspace = {
    readLevel() {
      workspaceReads += 1;
      return [];
    },
    writeNode() {
      workspaceWrites += 1;
    },
  };
  const sink = {
    putObject() {
      objectPuts += 1;
    },
  };
  assert.throws(
    () =>
      rebuildEditedContentStreaming(
        source,
        {
          offset: Number.MAX_SAFE_INTEGER,
          deleteLength: 0,
          insertBytes: Uint8Array.of(1),
        },
        { minimum: 1, average: 2, maximum: 4 },
        workspace,
        sink,
      ),
    /rebuilt content size/,
  );
  const invalidAttemptedMetrics = [
    ["negative", { sourceBytesRead: -1, bytesHashed: 0, largestSourceRead: 0 }],
    [
      "largest exceeds source",
      { sourceBytesRead: 1, bytesHashed: 0, largestSourceRead: 2 },
    ],
    [
      "output exceeds input",
      {
        sourceBytesRead: 0,
        bytesHashed: 0,
        largestSourceRead: 0,
        chunkerInputBytesCopied: 1,
        chunkerOutputBytesCopied: 2,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 2,
      },
    ],
    [
      "scan exceeds input",
      {
        sourceBytesRead: 0,
        bytesHashed: 0,
        largestSourceRead: 0,
        chunkerInputBytesCopied: 1,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 2,
        editedInputBytesPrepared: 1,
      },
    ],
    [
      "hashed exceeds output",
      {
        sourceBytesRead: 0,
        bytesHashed: 1,
        largestSourceRead: 0,
        chunkerInputBytesCopied: 1,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 1,
      },
    ],
    [
      "input exceeds prepared",
      {
        sourceBytesRead: 0,
        bytesHashed: 0,
        largestSourceRead: 0,
        chunkerInputBytesCopied: 2,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 1,
      },
    ],
    [
      "source exceeds prepared",
      {
        sourceBytesRead: 2,
        bytesHashed: 0,
        largestSourceRead: 1,
        chunkerInputBytesCopied: 0,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 1,
      },
    ],
    [
      "largest read exceeds configured maximum",
      {
        sourceBytesRead: 5,
        bytesHashed: 0,
        largestSourceRead: 5,
        chunkerInputBytesCopied: 5,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 5,
      },
    ],
    [
      "prepared input exceeds one-window read-ahead",
      {
        sourceBytesRead: 0,
        bytesHashed: 0,
        largestSourceRead: 0,
        chunkerInputBytesCopied: 1,
        chunkerOutputBytesCopied: 0,
        chunkerBoundaryBytesScanned: 0,
        editedInputBytesPrepared: 6,
      },
    ],
  ];
  for (const [name, attempted] of invalidAttemptedMetrics)
    assert.throws(
      () =>
        rebuildEditedContentStreaming(
          { ...source, size: 1 },
          { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
          { minimum: 1, average: 2, maximum: 4 },
          workspace,
          sink,
          "invalid attempted metrics",
          {},
          attempted,
        ),
      undefined,
      name,
    );
  assert.equal(sourceReads, 0);
  assert.equal(workspaceReads, 0);
  assert.equal(workspaceWrites, 0);
  assert.equal(objectPuts, 0);

  const exactBoundary = rebuildEditedContentStreaming(
    {
      size: 4,
      read(_offset, length) {
        return new Uint8Array(length);
      },
    },
    { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
    { minimum: 1, average: 2, maximum: 4 },
    new MemoryManifestWorkspace(),
    { putObject() {} },
    "exact attempted metric boundary",
    {},
    {
      sourceBytesRead: 4,
      bytesHashed: 4,
      largestSourceRead: 4,
      chunkerInputBytesCopied: 4,
      chunkerOutputBytesCopied: 4,
      chunkerBoundaryBytesScanned: 4,
      editedInputBytesPrepared: 8,
    },
  );
  assert.equal(exactBoundary.metrics.attemptedLocalLargestSourceRead, 4);
  assert.equal(exactBoundary.metrics.attemptedLocalEditedInputBytesPrepared, 8);
});

test("invalid rebuild controls reject before copying insertion bytes", () => {
  class TrackedInsertion extends Uint8Array {
    subarray(start, end) {
      this.copyAttempts += 1;
      return super.subarray(start, end);
    }
    copyAttempts = 0;
  }
  const insertion = new TrackedInsertion(1);
  insertion[0] = 9;
  const bytes = fixture(4096, 0xabad1dea);
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const before = buildManifest(bytes, parameters);
  const source = {
    size: bytes.length,
    read(offset, length) {
      return bytes.slice(offset, offset + length);
    },
  };
  const edit = { offset: 1, deleteLength: 1, insertBytes: insertion };
  const workspace = new MemoryManifestWorkspace();
  const sink = { putObject() {} };
  assert.throws(() =>
    rebuildEditedContentStreaming(
      source,
      edit,
      { minimum: 1, average: 3, maximum: 4 },
      workspace,
      sink,
    ),
  );
  assert.throws(() =>
    rebuildEditedContentStreaming(source, edit, parameters, workspace, sink, "bad", {
      readWindowBytes: 0,
    }),
  );
  assert.throws(() =>
    rebuildEditedContentStreaming(
      source,
      edit,
      parameters,
      workspace,
      sink,
      "bad",
      {},
      { sourceBytesRead: -1, bytesHashed: 0, largestSourceRead: 0 },
    ),
  );
  assert.throws(() =>
    rebuildDiagnosticManifestLocally(source, before, edit, {
      maxRetainedEntries: 0,
      maxRetainedNodes: 1,
      maxAffectedEntries: 1,
      maxAffectedBytes: 1,
    }),
  );
  assert.throws(() =>
    rebuildManifestLocallyOrStream(source, before, edit, parameters, workspace, sink, {
      maxRetainedEntries: 1,
      maxRetainedNodes: 1,
      maxAffectedEntries: 1,
      maxAffectedBytes: 0,
    }),
  );
  assert.throws(() =>
    rebuildEditedContentStreaming(
      source,
      { ...edit, offset: source.size + 1 },
      parameters,
      workspace,
      sink,
    ),
  );
  assert.equal(insertion.copyAttempts, 0);
});

test("local fallback preflights work and reports both attempted and fallback phases", () => {
  const parameters = { minimum: 64, average: 128, maximum: 512 };
  const original = fixture(64 * 1024 + 19, 0xa11ce);
  const before = buildManifest(original, parameters);
  const edit = { offset: 25_000, deleteLength: 1, insertBytes: Uint8Array.of(42) };
  let reads = 0;
  let readBytes = 0;
  let largestRead = 0;
  const source = {
    size: original.length,
    read(offset, length) {
      reads += 1;
      readBytes += length;
      largestRead = Math.max(largestRead, length);
      return original.slice(offset, offset + length);
    },
  };
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(source, before, edit, {
        maxRetainedEntries: 16_384,
        maxRetainedNodes: 32_768,
        maxAffectedEntries: 4096,
        maxAffectedBytes: 1,
      }),
    (error) => {
      assert.ok(error instanceof LocalRebuildLimitError);
      assert.equal(error.attemptMetrics.bytesHashed, 0);
      assert.ok(error.attemptMetrics.sourceBytesRead <= 2);
      return true;
    },
  );
  assert.ok(reads <= 1);

  reads = 0;
  readBytes = 0;
  largestRead = 0;
  const workspace = new MemoryManifestWorkspace();
  const result = rebuildManifestLocallyOrStream(
    source,
    before,
    edit,
    parameters,
    workspace,
    { putObject() {} },
    {
      maxRetainedEntries: 16_384,
      maxRetainedNodes: 32_768,
      maxAffectedEntries: 1,
      maxAffectedBytes: MAX_CONTENT_OBJECT_BYTES,
    },
    { readWindowBytes: 257, manifestReadBatchRecords: 11 },
  );
  assert.equal(result.mode, "streamed-fallback");
  assert.ok(result.metrics.attemptedLocalSourceBytesRead > 0);
  assert.ok(result.metrics.fallbackSourceBytesRead > 0);
  assert.equal(result.metrics.sourceBytesRead, readBytes);
  assert.equal(result.metrics.largestSourceRead, largestRead);
  assert.equal(
    result.metrics.sourceBytesRead,
    result.metrics.attemptedLocalSourceBytesRead +
      result.metrics.fallbackSourceBytesRead,
  );
  assert.equal(
    result.metrics.bytesHashed,
    result.metrics.attemptedLocalBytesHashed + result.metrics.fallbackBytesHashed,
  );
  assert.equal(
    result.metrics.chunkerInputBytesCopied,
    result.metrics.attemptedLocalChunkerInputBytesCopied +
      result.metrics.fallbackChunkerInputBytesCopied,
  );
  assert.equal(
    result.metrics.chunkerOutputBytesCopied,
    result.metrics.attemptedLocalChunkerOutputBytesCopied +
      result.metrics.fallbackChunkerOutputBytesCopied,
  );
});

test("diagnostic local FastCDC work stays linear under hostile valid ratios", () => {
  const parameters = {
    minimum: 1,
    average: 2,
    maximum: 8 * 1024 * 1024,
  };
  const before = buildManifest(new Uint8Array(), parameters);
  const insertion = new Uint8Array(MAX_DIAGNOSTIC_CONTENT_BYTES);
  const started = performance.now();
  let attempt;
  assert.throws(
    () =>
      rebuildDiagnosticManifestLocally(
        {
          size: 0,
          read() {
            throw new Error("empty source must not be read");
          },
        },
        before,
        { offset: 0, deleteLength: 0, insertBytes: insertion },
        {
          maxRetainedEntries: 16_384,
          maxRetainedNodes: 32_768,
          maxAffectedEntries: 4096,
          maxAffectedBytes: 13_913,
        },
      ),
    (error) => {
      assert.ok(error instanceof LocalRebuildLimitError);
      attempt = error.attemptMetrics;
      return true;
    },
  );
  const elapsedMs = performance.now() - started;
  assert.equal(attempt.sourceBytesRead, 0);
  assert.ok(attempt.chunkerInputBytesCopied <= 13_914);
  assert.ok(attempt.chunkerOutputBytesCopied <= attempt.chunkerInputBytesCopied);
  assert.ok(attempt.chunkerBoundaryBytesScanned <= attempt.chunkerInputBytesCopied);
  assert.ok(
    attempt.editedInputBytesPrepared <=
      attempt.chunkerInputBytesCopied + parameters.maximum,
  );
  assert.ok(elapsedMs < 3_000, `linear local fallback took ${elapsedMs}ms`);
});

test("local and forced-fallback modes reject manifest parameter changes identically", () => {
  const originalParameters = { minimum: 64, average: 128, maximum: 512 };
  const mismatched = { minimum: 64, average: 128, maximum: 256 };
  const original = fixture(4096, 123);
  const before = buildManifest(original, originalParameters);
  const source = {
    size: original.length,
    read(offset, length) {
      return original.slice(offset, offset + length);
    },
  };
  const edit = { offset: 1, deleteLength: 1, insertBytes: Uint8Array.of(9) };
  for (const limits of [
    undefined,
    {
      maxRetainedEntries: 1,
      maxRetainedNodes: 1,
      maxAffectedEntries: 1,
      maxAffectedBytes: 1,
    },
  ])
    assert.throws(
      () =>
        rebuildManifestLocallyOrStream(
          source,
          before,
          edit,
          mismatched,
          new MemoryManifestWorkspace(),
          { putObject() {} },
          limits,
        ),
      /parameters must match/,
    );
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
    const local = rebuildDiagnosticManifestLocally(
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
    const local = rebuildDiagnosticManifestLocally(
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

test("bounded Merkle rebuild golden vectors match the full path across file sizes and edit shapes", () => {
  const parameters = defaults;
  const sizes = [1, 20, 100].map((mib) => mib * 1024 * 1024);
  for (const size of sizes) {
    const original = fixture(size, 0x5eed0000 ^ size);
    const workspace = new MemoryManifestWorkspace();
    const chunked = [];
    new StreamingFastCdc(parameters).drain(
      original,
      (chunk) => {
        chunked.push({ hash: sha256(chunk), length: chunk.length });
      },
      true,
    );
    const built = buildManifestFromEntries(chunked, parameters, workspace, {
      maxDepth: 8,
    });
    const before = {
      id: bytesToHex(built.rootHash),
      rootHash: built.rootHash,
      root: built.root,
      nodes: workspaceNodes(workspace),
      entries: chunked,
    };
    const entryOffsets = [0];
    for (const entry of before.entries)
      entryOffsets.push(entryOffsets.at(-1) + entry.length);
    const atEntry = (index, inside = 0) =>
      entryOffsets[index] + Math.min(inside, before.entries[index].length - 1);
    const edits = [
      {
        name: "prepend",
        offset: 0,
        deleteLength: 0,
        insertBytes: Uint8Array.of(1, 2, 3),
      },
      {
        name: "append",
        offset: size,
        deleteLength: 0,
        insertBytes: Uint8Array.of(4, 5, 6),
      },
      {
        name: "truncate",
        offset: Math.floor(size * 0.75),
        deleteLength: size - Math.floor(size * 0.75),
        insertBytes: new Uint8Array(),
      },
      {
        name: "mid-leaf-delete",
        offset: atEntry(Math.floor(before.entries.length / 2), 1),
        deleteLength: 1,
        insertBytes: Uint8Array.of(7),
      },
      {
        name: "eof-replace",
        offset: size - 1,
        deleteLength: 1,
        insertBytes: Uint8Array.of(8),
      },
    ];
    if (before.entries.length > 256) {
      const crossStart = atEntry(255, 1);
      const crossEnd = entryOffsets[258];
      edits.push({
        name: "cross-leaf-delete",
        offset: crossStart,
        deleteLength: crossEnd - crossStart,
        insertBytes: Uint8Array.of(9, 10),
      });
    }
    for (const edit of edits) {
      const source = {
        size,
        read(offset, length) {
          return original.slice(offset, offset + length);
        },
      };
      const full = rebuildDiagnosticManifestLocally(source, before, edit);
      const state = buildBoundedManifestState(
        before,
        edit.offset,
        edit.deleteLength,
        DEFAULT_LOCAL_REBUILD_LIMITS,
        edit.insertBytes.length === edit.deleteLength,
      );
      const dirtyPath = boundedPathAtOffset(before, edit.offset + edit.deleteLength);
      assert.deepEqual(
        state.dirtyEndLeaf.path,
        dirtyPath.frames.at(-1).path,
        `${size} MiB ${edit.name} dirty-end path`,
      );
      assert.equal(
        state.boundary.get(state.dirtyEndLeaf.leafOffset),
        state.dirtyEndLeaf.startEntryIndex,
        `${size} MiB ${edit.name} dirty-end boundary`,
      );
      const bounded = rebuildManifestBoundedOwned(
        state,
        source,
        edit,
        DEFAULT_LOCAL_REBUILD_LIMITS,
        sha256,
      );
      assert.equal(
        bytesToHex(bounded.rootHash),
        bytesToHex(full.rootHash),
        `${size} MiB ${edit.name} root`,
      );
      assert.deepEqual(
        bounded.root,
        full.root,
        `${size} MiB ${edit.name} encoded root`,
      );
      assert.deepEqual(
        bounded.entrySplice,
        full.entrySplice,
        `${size} MiB ${edit.name} splice`,
      );
      assert.deepEqual(
        applyEntrySplice(before.entries, bounded.entrySplice),
        applyEntrySplice(before.entries, full.entrySplice),
        `${size} MiB ${edit.name} entry stream`,
      );
      assert.equal(
        bounded.metrics.reconnectOldOffset,
        full.metrics.reconnectOldOffset,
        `${size} MiB ${edit.name} reconnect offset`,
      );
      assert.ok(
        state.boundary.has(full.metrics.reconnectOldOffset),
        `${size} MiB ${edit.name} reconnect is in the loaded boundary map`,
      );
      if (before.nodes.size > 1)
        assert.ok(
          state.levelWindows.slice(1).some((window) => window?.fringe.length >= 0),
          `${size} MiB ${edit.name} has level windows`,
        );
    }
  }
});

test("bounded local rebuild falls back when its retained window is too small", () => {
  const parameters = { minimum: 1, average: 1, maximum: 1 };
  const original = fixture(4096, 0xfeed1234);
  const before = buildManifest(original, parameters);
  assert.throws(
    () =>
      buildBoundedManifestState(before, 0, 0, {
        ...DEFAULT_LOCAL_REBUILD_LIMITS,
        maxAffectedEntries: 1,
      }),
    (error) => error instanceof BoundedRebuildFallbackError,
  );
});

test("bounded local rebuild is byte-identical to the full-state rebuild across the edit-shape corpus", () => {
  const parameters = { minimum: 32768, average: 131072, maximum: 524288 };
  const MIB = 1024 * 1024;
  const shapes = (size, leafCount) => [
    {
      name: "append",
      edit: { offset: size, deleteLength: 0, insertBytes: Uint8Array.of(1, 2, 3) },
    },
    {
      name: "prepend",
      edit: { offset: 0, deleteLength: 0, insertBytes: Uint8Array.of(9, 8, 7, 6, 5) },
    },
    {
      name: "truncate",
      edit: {
        offset: Math.floor(size * 0.75),
        deleteLength: Math.floor(size * 0.25),
        insertBytes: new Uint8Array(),
      },
    },
    {
      name: "replace-mid",
      edit: {
        offset: Math.floor(size / 2),
        deleteLength: 1,
        insertBytes: Uint8Array.of(7, 7),
      },
    },
    {
      name: "delete-mid",
      edit: {
        offset: Math.floor(size / 2),
        deleteLength: 17,
        insertBytes: new Uint8Array(),
      },
    },
    {
      name: "replace-eof",
      edit: { offset: size - 1, deleteLength: 1, insertBytes: Uint8Array.of(3) },
    },
    ...(leafCount > 1
      ? [
          {
            name: "cross-leaf-delete",
            edit: {
              offset: Math.floor(size * 0.4),
              deleteLength: Math.floor((size / leafCount) * 2.2),
              insertBytes: new Uint8Array(),
            },
          },
        ]
      : []),
  ];
  for (const size of [1 * MIB, 20 * MIB, 100 * MIB]) {
    const original = fixture(size, 0x5eed ^ size);
    const workspace = new MemoryManifestWorkspace();
    const chunked = [];
    new StreamingFastCdc(parameters).drain(
      original,
      (chunk) => {
        chunked.push({ hash: sha256(chunk), length: chunk.length });
      },
      true,
    );
    const built = buildManifestFromEntries(chunked, parameters, workspace, {
      maxDepth: 8,
    });
    const before = {
      id: bytesToHex(built.rootHash),
      rootHash: built.rootHash,
      root: built.root,
      nodes: workspaceNodes(workspace),
      entries: chunked,
    };
    const leafCount = [...workspaceNodes(workspace).values()].filter(
      (n) => n.node.kind === "leaf",
    ).length;
    const source = {
      size: original.length,
      read: (offset, length) => original.slice(offset, offset + length),
    };
    for (const shape of shapes(size, leafCount)) {
      const edit = shape.edit;
      const full = rebuildDiagnosticManifestLocally(source, before, {
        offset: edit.offset,
        deleteLength: edit.deleteLength,
        insertBytes: edit.insertBytes,
      });
      const state = buildBoundedManifestState(
        before,
        edit.offset,
        edit.deleteLength,
        DEFAULT_LOCAL_REBUILD_LIMITS,
      );
      let bounded;
      try {
        bounded = rebuildManifestBoundedOwned(
          state,
          source,
          {
            offset: edit.offset,
            deleteLength: edit.deleteLength,
            insertBytes: edit.insertBytes,
          },
          DEFAULT_LOCAL_REBUILD_LIMITS,
          sha256,
        );
      } catch (error) {
        assert.ok(
          error instanceof BoundedRebuildFallbackError,
          `${size / MIB}MiB ${shape.name} fell back for an unexpected reason`,
        );
        continue;
      }
      assert.equal(
        bytesToHex(bounded.rootHash),
        bytesToHex(full.rootHash),
        `${size / MIB}MiB ${shape.name} root hash`,
      );
      assert.equal(
        bytesToHex(bounded.root),
        bytesToHex(full.root),
        `${size / MIB}MiB ${shape.name} root bytes`,
      );
      assert.equal(
        bounded.entrySplice.start,
        full.entrySplice.start,
        `${size / MIB}MiB ${shape.name} splice start`,
      );
      assert.equal(
        bounded.entrySplice.deleteCount,
        full.entrySplice.deleteCount,
        `${size / MIB}MiB ${shape.name} splice delete count`,
      );
      assert.deepEqual(
        bounded.entrySplice.entries.map((entry) => [
          bytesToHex(entry.hash),
          entry.length,
        ]),
        full.entrySplice.entries.map((entry) => [bytesToHex(entry.hash), entry.length]),
        `${size / MIB}MiB ${shape.name} splice entries`,
      );
      assert.equal(
        bounded.fileSize,
        full.fileSize,
        `${size / MIB}MiB ${shape.name} file size`,
      );
      assert.equal(
        bounded.entryCount,
        full.entryCount,
        `${size / MIB}MiB ${shape.name} entry count`,
      );
      // The dirty-end-leaf map binds the chunker reconnect: the reconnect
      // offset is an entry boundary inside the loaded leaves, and the
      // reconnect entry equals the splice end.
      const metrics = bounded.metrics;
      assert.ok(
        state.boundary.has(metrics.reconnectOldOffset),
        `${size / MIB}MiB ${shape.name} reconnect offset in the loaded boundary map`,
      );
      assert.equal(
        state.boundary.get(metrics.reconnectOldOffset),
        bounded.entrySplice.start + bounded.entrySplice.deleteCount,
        `${size / MIB}MiB ${shape.name} reconnect entry equals the splice end`,
      );
      // The reconnect window stays bounded: the loaded fringe never exceeds
      // the retained-entry budget.
      assert.ok(
        state.fringeLeaves.length * 256 <=
          DEFAULT_LOCAL_REBUILD_LIMITS.maxRetainedEntries,
      );
    }
  }
});

test("bounded local rebuild matches the full-state rebuild on a seeded random corpus", () => {
  const parameters = { minimum: 32768, average: 131072, maximum: 524288 };
  const MIB = 1024 * 1024;
  const original = fixture(40 * MIB, 0xabcd);
  const workspace = new MemoryManifestWorkspace();
  const chunked = [];
  new StreamingFastCdc(parameters).drain(
    original,
    (chunk) => {
      chunked.push({ hash: sha256(chunk), length: chunk.length });
    },
    true,
  );
  const built = buildManifestFromEntries(chunked, parameters, workspace, {
    maxDepth: 8,
  });
  const before = {
    id: bytesToHex(built.rootHash),
    rootHash: built.rootHash,
    root: built.root,
    nodes: workspaceNodes(workspace),
    entries: chunked,
  };
  const source = {
    size: original.length,
    read: (offset, length) => original.slice(offset, offset + length),
  };
  let state = 0x91e10da5;
  const random = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return state >>> 0;
  };
  let matched = 0;
  let fellBack = 0;
  for (let iteration = 0; iteration < 16; iteration += 1) {
    const offset = random() % (original.length + 1);
    const deleteLength = Math.min(random() % 33, original.length - offset);
    const insert = fixture(random() % 33, random());
    const edit = { offset, deleteLength, insertBytes: insert };
    const full = rebuildDiagnosticManifestLocally(source, before, edit);
    const stateBounded = buildBoundedManifestState(
      before,
      offset,
      deleteLength,
      DEFAULT_LOCAL_REBUILD_LIMITS,
    );
    let bounded;
    try {
      bounded = rebuildManifestBoundedOwned(
        stateBounded,
        source,
        edit,
        DEFAULT_LOCAL_REBUILD_LIMITS,
        sha256,
      );
    } catch (error) {
      assert.ok(
        error instanceof BoundedRebuildFallbackError,
        `iteration ${iteration} fell back unexpectedly`,
      );
      fellBack += 1;
      continue;
    }
    matched += 1;
    assert.equal(
      bytesToHex(bounded.rootHash),
      bytesToHex(full.rootHash),
      `iteration ${iteration} root`,
    );
    assert.equal(
      bounded.entrySplice.start,
      full.entrySplice.start,
      `iteration ${iteration} splice`,
    );
    assert.equal(
      bounded.entrySplice.deleteCount,
      full.entrySplice.deleteCount,
      `iteration ${iteration} splice`,
    );
  }
  assert.ok(matched >= 8, `bounded matched ${matched} of 16 random edits`);
});

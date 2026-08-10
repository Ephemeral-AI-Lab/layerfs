import {
  bytesToHex,
  intrinsicByteLength as casIntrinsicByteLength,
} from "./cas/bytes.js";
import { IncrementalSha256, createCasObject, sha256, sha256Hex } from "./cas/sha256.js";
import {
  DEFAULT_FASTCDC,
  StreamingFastCdc,
  fastCdcChunks,
  fastCdcGearTableV1,
} from "./cdc/fastcdc.js";
import { overlayCowPages, writeCowPages } from "./cow/pages.js";
import { buildManifestFromEntries } from "./manifests/builder.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
} from "./manifests/codec.js";
import {
  lookupManifest,
  ManifestSequentialCursor,
  validateManifestTree,
} from "./manifests/cursor.js";
import { buildManifest } from "./operations/full-rebuild.js";
import { rebuildDiagnosticManifestLocally } from "./operations/local-rebuild.js";
import { rebuildEditedContentStreaming } from "./operations/streamed-rebuild.js";
import { applyStructuralPatchesWithMetrics } from "./patches/patches.js";
import {
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  DEFAULT_STORAGE_LIMITS,
  requiredRuntimeProgressBytes,
  validateRuntimeLimits,
} from "./resources/limits.js";
import { intrinsicByteLength as resourceIntrinsicByteLength } from "./resources/byte-capacity.js";

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

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

class SubstitutingBytes extends Uint8Array {
  get byteLength() {
    return 1;
  }
  subarray() {
    return new Uint8Array(this.byteLength).fill(0xff);
  }
}

function rejects(callback, pattern, message) {
  try {
    callback();
  } catch (error) {
    assert(pattern.test(String(error?.message)), `${message}: ${error}`);
    return;
  }
  throw new Error(`${message}: expected rejection`);
}

class MemoryWorkspace {
  constructor() {
    this.levels = new Map();
  }
  writeNode(record) {
    const level = this.levels.get(record.level) ?? [];
    level.push(record);
    this.levels.set(record.level, level);
  }
  readLevel(level, afterIndex, limit) {
    return (this.levels.get(level) ?? []).slice(afterIndex + 1, afterIndex + 1 + limit);
  }
}

function diverseEntry(index) {
  const identity = new Uint8Array(4);
  new DataView(identity.buffer).setUint32(0, index, true);
  return { hash: sha256(identity), length: (index % 4) + 1 };
}

export default {
  fetch() {
    const checks = [];
    const check = (name, callback) => {
      const metrics = callback() ?? {};
      checks.push({ name, ok: true, metrics });
    };

    check("sha256-golden", () => {
      assert(
        sha256Hex(new Uint8Array()) ===
          "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "empty SHA-256 golden mismatch",
      );
      assert(
        sha256Hex(new TextEncoder().encode("abc")) ===
          "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA-256 golden mismatch",
      );
      assert(
        bytesToHex(
          new IncrementalSha256()
            .update(new TextEncoder().encode("a"))
            .update(new TextEncoder().encode("bc"))
            .digest(),
        ) === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "segmented SHA-256 golden mismatch",
      );
      const source = new SubstitutingBytes(3);
      source.set([4, 5, 6]);
      assert(
        Array.from(createCasObject(source).bytes).join(",") === "4,5,6",
        "CAS ownership used caller subarray override",
      );
      assert(
        casIntrinsicByteLength(source) === 3 &&
          resourceIntrinsicByteLength(source) === 3,
        "intrinsic byte-capacity implementations diverged",
      );
    });

    check("fastcdc-boundary-goldens", () => {
      const gear = fastCdcGearTableV1();
      assert(
        Array.from(gear.slice(0, 8), (value) => value.toString(16)).join(",") ===
          "510c4619,e02e553e,7bb98f3a,183a8b5,e6336d1f,f989d237,ba2529d0,fcfbedbf",
        "Gear table mismatch",
      );
      gear.fill(0);
      const vectors = [
        [0, ""],
        [DEFAULT_FASTCDC.minimum - 1, "32767"],
        [DEFAULT_FASTCDC.minimum, "32768"],
        [DEFAULT_FASTCDC.average, "118265,12807"],
        [DEFAULT_FASTCDC.maximum, "118265,231191,174832"],
        [
          2 * 1024 * 1024,
          "118265,231191,325530,155909,187710,141143,175869,138460,346490,147103,109121,20361",
        ],
      ];
      for (const [length, expected] of vectors)
        assert(
          fastCdcChunks(fixture(length))
            .map((chunk) => chunk.length)
            .join(",") === expected,
          `FastCDC boundary golden mismatch at ${length}`,
        );
    });

    check("streaming-fastcdc", () => {
      const bytes = fixture(3 * 1024 * 1024 + 17);
      const fingerprint = (chunks) =>
        chunks.map((chunk) => `${chunk.byteLength}:${sha256Hex(chunk)}`).join(",");
      const expected = fastCdcChunks(bytes).map(({ offset, length }) =>
        bytes.slice(offset, offset + length),
      );
      const partitioned = (input, partitions) => {
        const stream = new StreamingFastCdc();
        const chunks = [];
        let offset = 0;
        let part = 0;
        while (offset < input.byteLength) {
          const size = partitions[part++ % partitions.length];
          chunks.push(...stream.push(input.subarray(offset, offset + size)));
          offset += size;
        }
        chunks.push(...stream.finish());
        return chunks;
      };
      const tiny = fixture(64 * 1024 + 17, 0x13579bdf);
      const tinyExpected = fastCdcChunks(tiny).map(({ offset, length }) =>
        tiny.slice(offset, offset + length),
      );
      assert(
        fingerprint(partitioned(tiny, [1])) === fingerprint(tinyExpected),
        "single-byte streaming partition mismatch",
      );
      assert(
        fingerprint(partitioned(bytes, [7, 4096, 65_537, 524_288])) ===
          fingerprint(expected),
        "irregular streaming partitions mismatch",
      );
      const actual = [];
      const streaming = new StreamingFastCdc();
      streaming.drain(bytes, (chunk) => actual.push(chunk.byteLength), true);
      assert(
        actual.join(",") === expected.map((chunk) => chunk.byteLength).join(","),
        "streaming boundary mismatch",
      );
      assert(
        streaming.metrics.inputBytesCopied === bytes.length &&
          streaming.metrics.outputBytesCopied === bytes.length &&
          streaming.metrics.boundaryBytesScanned <= bytes.length &&
          streaming.metrics.peakPushOutputBytes === 0 &&
          streaming.metrics.peakPushOutputCount === 0,
        "streaming metrics are not bounded",
      );
      const prebuffered = new StreamingFastCdc({
        minimum: 1024,
        average: 1024,
        maximum: 1024,
      });
      prebuffered.drain(new Uint8Array(1023), () => {});
      const pushed = prebuffered.push(Uint8Array.of(1, 2), true);
      assert(
        pushed.length === 2 &&
          pushed[0].byteLength === 1024 &&
          pushed[1].byteLength === 1 &&
          prebuffered.metrics.peakPushOutputBytes === 1025 &&
          prebuffered.metrics.peakPushOutputCount === 2,
        "bounded push metrics mismatch",
      );
      const getterReads = { minimum: 0, average: 0, maximum: 0 };
      const getterChunker = new StreamingFastCdc({
        get minimum() {
          getterReads.minimum += 1;
          return 1;
        },
        get average() {
          getterReads.average += 1;
          return 2;
        },
        get maximum() {
          getterReads.maximum += 1;
          return getterReads.maximum === 1 ? 4 : 16 * 1024 * 1024 + 1;
        },
      });
      assert(getterChunker.capacityBytes === 4, "getter capacity snapshot mismatch");
      assert(
        getterReads.minimum === 1 &&
          getterReads.average === 1 &&
          getterReads.maximum === 1,
        "FastCDC configuration getters were reread",
      );
      rejects(() => streaming.finish(), /finalized/, "repeated finish");
      return {
        ...streaming.metrics,
        boundedPushOutputBytes: prebuffered.metrics.peakPushOutputBytes,
        boundedPushOutputCount: prebuffered.metrics.peakPushOutputCount,
      };
    });

    check("manifest-diverse-grouping-root", () => {
      const workspace = new MemoryWorkspace();
      const manifest = buildManifestFromEntries(
        Array.from({ length: 600 }, (_, index) => diverseEntry(index)),
        { minimum: 1, average: 2, maximum: 4 },
        workspace,
        { readBatchRecords: 17 },
      );
      let end = 0;
      const boundaries = workspace.levels.get(0).map((record) => {
        end += record.value.node.entries.length;
        return end;
      });
      assert(boundaries.join(",") === "105,204,272,528,600", "group boundaries");
      assert(
        bytesToHex(manifest.rootHash) ===
          "bd7ed42c2a32cea19d79921bf94b19ea7c7ff42e04dd8da1de4acd826cd46d42",
        "diverse manifest root mismatch",
      );
      assert(
        manifest.fileSize === 1500 &&
          manifest.nodeCount === 6 &&
          manifest.depth === 2 &&
          manifest.groupingRecordCount === 605 &&
          manifest.groupingRecordBytesProcessed === 21840,
        "diverse manifest metrics mismatch",
      );
      return {
        nodeCount: manifest.nodeCount,
        groupingRecordCount: manifest.groupingRecordCount,
      };
    });

    check("manifest-binary-goldens", () => {
      const emptyLeaf = encodeManifestNode({
        kind: "leaf",
        span: 0,
        entryCount: 0,
        entries: [],
      });
      assert(
        bytesToHex(emptyLeaf) ===
          "4541464e01000001000000000000000000000000000000000000000000000000" &&
          bytesToHex(sha256(emptyLeaf)) ===
            "166659473d5d3838ca47c6a541fc969e6377d165e2b6f36e40b7be1db7b92527",
        "empty-leaf golden mismatch",
      );
      const entryA = { hash: new Uint8Array(32).fill(0x11), length: 1 };
      const entryB = { hash: new Uint8Array(32).fill(0x22), length: 2 };
      const leaf = encodeManifestNode({
        kind: "leaf",
        span: 3,
        entryCount: 2,
        entries: [entryA, entryB],
      });
      assert(
        bytesToHex(leaf) ===
          "4541464e01000001020000000000000003000000000000000200000000000000111111111111111111111111111111111111111111111111111111111111111101000000222222222222222222222222222222222222222222222222222222222222222202000000",
        "leaf envelope golden mismatch",
      );
      assert(
        bytesToHex(sha256(leaf)) ===
          "e7b7034cb872766a9d02249f745276b89a32f2a53a7680641987ef93dc2f6c70",
        "leaf digest golden mismatch",
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
      assert(
        bytesToHex(fullLeaf) === bytesToHex(expectedFullLeaf) &&
          bytesToHex(sha256(fullLeaf)) ===
            "39a12e626c1e1dde1ff0b47d26e0190e288e8e3652325f55763461917027ba87",
        "full-leaf golden mismatch",
      );
      const internal = encodeManifestNode({
        kind: "internal",
        span: 8,
        entryCount: 3,
        children: [
          { hash: sha256(leaf), span: 3, entryCount: 2 },
          {
            hash: new Uint8Array(32).fill(0x33),
            span: 5,
            entryCount: 1,
          },
        ],
      });
      assert(
        bytesToHex(internal) ===
          "4541464e01000101020000000000000008000000000000000300000000000000e7b7034cb872766a9d02249f745276b89a32f2a53a7680641987ef93dc2f6c7003000000000000000200000000000000333333333333333333333333333333333333333333333333333333333333333305000000000000000100000000000000",
        "internal-node envelope golden mismatch",
      );
      assert(
        bytesToHex(sha256(internal)) ===
          "45a4b9207f8f4b5dc90aee18f6b099f802018110740300cdf8ca165c6cba9065",
        "internal-node digest golden mismatch",
      );
      const root = encodeManifestRoot({
        parameters: { minimum: 1, average: 2, maximum: 4 },
        fileSize: 8,
        entryCount: 3,
        rootNodeHash: sha256(internal),
      });
      assert(root.byteLength === 68, "root envelope size mismatch");
      assert(
        bytesToHex(root) ===
          "45414652010001010100000002000000040000000800000000000000030000000000000045a4b9207f8f4b5dc90aee18f6b099f802018110740300cdf8ca165c6cba9065",
        "root envelope golden mismatch",
      );
      assert(
        bytesToHex(sha256(root)) ===
          "dca081afd9e6ad4650d7e327557b22dcb3747b98a9ce11f01118b4c652fef6ce",
        "root digest golden mismatch",
      );
      const mutableHash = new SubstitutingBytes(32);
      mutableHash.fill(0x77);
      const expectedMutableHash = bytesToHex(new Uint8Array(mutableHash));
      let lengthReads = 0;
      const getterLeaf = encodeManifestNode({
        kind: "leaf",
        span: 1,
        entryCount: 1,
        entries: [
          {
            hash: mutableHash,
            get length() {
              lengthReads += 1;
              mutableHash.fill(0);
              return lengthReads === 1 ? 1 : 2;
            },
          },
        ],
      });
      assert(lengthReads === 1, "manifest entry getter was reread");
      assert(
        bytesToHex(decodeManifestNode(getterLeaf).entries[0].hash) ===
          expectedMutableHash,
        "manifest entry hash ownership mismatch",
      );
      let rootCountReads = 0;
      const getterRoot = encodeManifestRoot({
        parameters: { minimum: 1, average: 2, maximum: 4 },
        fileSize: 1,
        get entryCount() {
          rootCountReads += 1;
          return rootCountReads === 1 ? 1 : 2;
        },
        rootNodeHash: sha256(getterLeaf),
      });
      assert(
        rootCountReads === 1 && decodeManifestRoot(getterRoot).entryCount === 1,
        "manifest root scalar getter was reread",
      );
      const complete = buildManifest(fixture(2 * 1024 * 1024), DEFAULT_FASTCDC);
      assert(
        complete.id ===
          "6c08078b39f26d3dd98b10a20e14371e4b2f96fd9164fd629214b8c74981e7f1",
        "complete manifest digest mismatch",
      );
      assert(
        bytesToHex(complete.root) ===
          "454146520100010100800000000002000000080000002000000000000c00000000000000b69876e73a78d0cb95f34e5206711a90aba19ff31d02eb63594fb7220ea4c91c",
        "complete root envelope mismatch",
      );
      const deepWorkspace = new MemoryWorkspace();
      const deep = buildManifestFromEntries(
        Array.from({ length: 22_000 }, (_, index) => diverseEntry(index)),
        { minimum: 1, average: 2, maximum: 4 },
        deepWorkspace,
        { readBatchRecords: 17 },
      );
      assert(
        deep.depth === 3 &&
          deep.nodeCount === 146 &&
          bytesToHex(deep.root) ===
            "4541465201000101010000000200000004000000d8d6000000000000f0550000000000005bf66b17b8e92ae5965acdec219647377bfeb27349088cbeeb45dada9513bc9e" &&
          bytesToHex(deep.rootHash) ===
            "2501ef8b9619af95229002f5062d8a33275b948f74867908a069b32861bdb72d",
        "depth-three complete-manifest golden mismatch",
      );
      return {
        emptyLeafBytes: emptyLeaf.byteLength,
        leafBytes: leaf.byteLength,
        fullLeafBytes: fullLeaf.byteLength,
        internalBytes: internal.byteLength,
        rootBytes: root.byteLength,
        deepDepth: deep.depth,
        deepNodeCount: deep.nodeCount,
      };
    });

    check("manifest-codec-cursor-corruption", () => {
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
      const malicious = {
        get(hash) {
          hash.set(hashB);
          return encodedB;
        },
      };
      rejects(
        () => lookupManifest(root, 0, malicious, sha256(root)),
        /digest mismatch/,
        "cursor hash alias",
      );
      rejects(
        () => validateManifestTree(root, malicious, sha256(root)),
        /digest mismatch/,
        "tree hash alias",
      );
      const malformed = encodedA.slice();
      malformed[12] = 1;
      rejects(
        () => decodeManifestNode(malformed),
        /malformed manifest node header/,
        "reserved header field",
      );

      const workspace = new MemoryWorkspace();
      const manifest = buildManifestFromEntries(
        Array.from({ length: 600 }, (_, index) => diverseEntry(index)),
        { minimum: 1, average: 2, maximum: 4 },
        workspace,
        { readBatchRecords: 17 },
      );
      const nodeBytes = new Map(
        [...workspace.levels.values()]
          .flatMap((records) => records)
          .map((record) => [
            bytesToHex(record.value.hash),
            record.value.encoded.slice(),
          ]),
      );
      const readerFor = (nodes) => ({
        get(hash) {
          return nodes.get(bytesToHex(hash));
        },
      });
      const rootWithNode = (encoded, baseRoot = manifest.root) => {
        const rootBytes = baseRoot.slice();
        const nodeHash = sha256(encoded);
        rootBytes.set(nodeHash, 36);
        const nodes = new Map(nodeBytes);
        nodes.set(bytesToHex(nodeHash), encoded);
        return { root: rootBytes, rootHash: sha256(rootBytes), nodes };
      };
      const rejectLookup = (rootBytes, rootHash, nodes, name) =>
        rejects(
          () => lookupManifest(rootBytes, 0, readerFor(nodes), rootHash),
          /./,
          name,
        );
      const rootMutations = [
        (bytes) => (bytes[0] ^= 1),
        (bytes) => (bytes[4] = 2),
        (bytes) => (bytes[6] = 2),
        (bytes) => (bytes[7] = 2),
        (bytes) => new DataView(bytes.buffer).setUint32(8, 0, true),
        (bytes) => new DataView(bytes.buffer).setUint32(12, 3, true),
        (bytes) => new DataView(bytes.buffer).setUint32(16, 0, true),
        (bytes) => new DataView(bytes.buffer).setBigUint64(20, 1501n, true),
        (bytes) => new DataView(bytes.buffer).setBigUint64(28, 601n, true),
        (bytes) => bytes.fill(0, 36, 68),
      ];
      for (const mutate of rootMutations) {
        const changed = manifest.root.slice();
        mutate(changed);
        rejectLookup(changed, sha256(changed), nodeBytes, "root field corruption");
      }

      const rootNodeKey = bytesToHex(manifest.root.slice(36));
      const rootNode = nodeBytes.get(rootNodeKey);
      const nodeMutations = [
        (bytes) => (bytes[0] ^= 1),
        (bytes) => (bytes[4] = 2),
        (bytes) => (bytes[6] = 0),
        (bytes) => (bytes[7] = 2),
        (bytes) => new DataView(bytes.buffer).setUint32(8, 6, true),
        (bytes) => new DataView(bytes.buffer).setUint32(12, 1, true),
        (bytes) => new DataView(bytes.buffer).setBigUint64(16, 1501n, true),
        (bytes) => new DataView(bytes.buffer).setBigUint64(24, 601n, true),
      ];
      for (const mutate of nodeMutations) {
        const changed = rootNode.slice();
        mutate(changed);
        const variant = rootWithNode(changed);
        rejectLookup(
          variant.root,
          variant.rootHash,
          variant.nodes,
          "node header corruption",
        );
      }

      const decodedParent = decodeManifestNode(rootNode);
      const firstChild = decodedParent.children[0];
      const firstLeafKey = bytesToHex(firstChild.hash);
      const firstLeaf = nodeBytes.get(firstLeafKey);
      const detachedCursor = new ManifestSequentialCursor(
        manifest.root,
        0,
        readerFor(nodeBytes),
        manifest.rootHash,
      );
      const detachedPeek = detachedCursor.peek();
      const detachedHash = bytesToHex(detachedPeek.entry.hash);
      detachedPeek.entry.hash.fill(0);
      assert(
        bytesToHex(detachedCursor.peek().entry.hash) === detachedHash &&
          bytesToHex(detachedCursor.next().entry.hash) === detachedHash,
        "cursor exposed mutable private entry state",
      );
      const replaceFirstLeaf = (encoded) => {
        const parent = rootNode.slice();
        const leafHash = sha256(encoded);
        parent.set(leafHash, 32);
        const variant = rootWithNode(parent);
        variant.nodes.set(bytesToHex(leafHash), encoded);
        return variant;
      };
      const zeroLength = firstLeaf.slice();
      new DataView(zeroLength.buffer).setUint32(64, 0, true);
      let variant = replaceFirstLeaf(zeroLength);
      rejectLookup(
        variant.root,
        variant.rootHash,
        variant.nodes,
        "leaf record length corruption",
      );

      const missingObject = firstLeaf.slice();
      missingObject[32] ^= 1;
      variant = replaceFirstLeaf(missingObject);
      let exposedBytes = 0;
      rejects(
        () => {
          const selected = lookupManifest(
            variant.root,
            0,
            readerFor(variant.nodes),
            variant.rootHash,
          );
          const object = new Map().get(bytesToHex(selected.entry.hash));
          if (!object) throw new Error("missing CAS object");
          exposedBytes += object.byteLength;
        },
        /missing CAS object/,
        "leaf object hash corruption",
      );
      assert(exposedBytes === 0, "corrupt leaf exposed object bytes");

      const missingChild = rootNode.slice();
      missingChild.fill(0, 32, 64);
      variant = rootWithNode(missingChild);
      rejectLookup(
        variant.root,
        variant.rootHash,
        variant.nodes,
        "internal child hash corruption",
      );
      for (const field of ["span", "count"]) {
        const parent = rootNode.slice();
        const parentView = new DataView(parent.buffer);
        const rootBytes = manifest.root.slice();
        const rootView = new DataView(rootBytes.buffer);
        if (field === "span") {
          parentView.setBigUint64(64, BigInt(firstChild.span + 1), true);
          parentView.setBigUint64(16, 1501n, true);
          rootView.setBigUint64(20, 1501n, true);
        } else {
          parentView.setBigUint64(72, BigInt(firstChild.entryCount + 1), true);
          parentView.setBigUint64(24, 601n, true);
          rootView.setBigUint64(28, 601n, true);
        }
        variant = rootWithNode(parent, rootBytes);
        rejectLookup(
          variant.root,
          variant.rootHash,
          variant.nodes,
          `internal child ${field} corruption`,
        );
      }

      const withoutRoot = new Map(nodeBytes);
      withoutRoot.delete(rootNodeKey);
      rejectLookup(manifest.root, manifest.rootHash, withoutRoot, "missing root node");
      const withoutLeaf = new Map(nodeBytes);
      withoutLeaf.delete(firstLeafKey);
      rejectLookup(manifest.root, manifest.rootHash, withoutLeaf, "missing leaf node");

      for (const changedChildren of [
        decodedParent.children.slice(1),
        [decodedParent.children[0], ...decodedParent.children],
      ]) {
        const changed = encodeManifestNode({
          kind: "internal",
          span: changedChildren.reduce((sum, child) => sum + child.span, 0),
          entryCount: changedChildren.reduce((sum, child) => sum + child.entryCount, 0),
          children: changedChildren,
        });
        variant = rootWithNode(changed);
        rejectLookup(
          variant.root,
          variant.rootHash,
          variant.nodes,
          "deleted or duplicate child",
        );
      }
      const reordered = encodeManifestNode({
        kind: "internal",
        span: decodedParent.span,
        entryCount: decodedParent.entryCount,
        children: [
          decodedParent.children[1],
          decodedParent.children[0],
          ...decodedParent.children.slice(2),
        ],
      });
      variant = rootWithNode(reordered);
      rejectLookup(
        variant.root,
        manifest.rootHash,
        variant.nodes,
        "reordered child under authoritative root",
      );
      return {
        rootMutations: rootMutations.length,
        nodeMutations: nodeMutations.length,
      };
    });

    check("cow-pages", () => {
      let totalPages = 0;
      for (const pageBytes of [4096, 8192, 16384]) {
        const base = fixture(pageBytes * 2 + 13);
        const offset = pageBytes - 2;
        const pages = writeCowPages(base, offset, Uint8Array.of(9, 8, 7, 6), pageBytes);
        const expected = base.slice();
        expected.set(Uint8Array.of(9, 8, 7, 6), offset);
        assert(
          bytesToHex(sha256(overlayCowPages(base, pages, pageBytes))) ===
            bytesToHex(sha256(expected)),
          `COW overlay mismatch at ${pageBytes}`,
        );
        rejects(
          () =>
            overlayCowPages(
              base,
              [{ index: 0, bytes: new Uint8Array(pageBytes - 1) }],
              pageBytes,
            ),
          /complete logical page/,
          `short COW page at ${pageBytes}`,
        );
        totalPages += pages.length;
      }
      return { pageSizesTested: 3, pages: totalPages };
    });

    check("structural-patches", () => {
      const base = fixture(64 * 1024);
      const patches = Array.from({ length: 32 }, (_, sequence) => ({
        sequence,
        offset: sequence * 101,
        deleteLength: 1,
        insertBytes: Uint8Array.of(sequence),
      }));
      const result = applyStructuralPatchesWithMetrics(base, patches);
      assert(result.metrics.copiedBytes === base.length, "patch copy amplification");
      assert(result.metrics.peakSegments <= 65, "patch segment bound");
      return result.metrics;
    });

    check("diagnostic-local-rebuild", () => {
      const bytes = fixture(2 * 1024 * 1024);
      const manifest = buildManifest(bytes, DEFAULT_FASTCDC);
      const insertion = new SubstitutingBytes(1);
      insertion[0] = 42;
      const local = rebuildDiagnosticManifestLocally(
        {
          size: bytes.length,
          read(offset, length) {
            insertion.fill(0);
            return bytes.slice(offset, offset + length);
          },
        },
        manifest,
        { offset: 700000, deleteLength: 1, insertBytes: insertion },
      );
      const edited = bytes.slice();
      edited[700000] = 42;
      assert(
        bytesToHex(local.rootHash) === buildManifest(edited, DEFAULT_FASTCDC).id,
        "local rebuild mismatch",
      );
      assert(
        local.metrics.sourceBytesRead <= DEFAULT_FASTCDC.maximum &&
          local.metrics.bytesHashed <= DEFAULT_FASTCDC.maximum &&
          local.metrics.editedInputBytesPrepared <=
            local.metrics.chunkerInputBytesCopied + DEFAULT_FASTCDC.maximum,
        "diagnostic local work exceeds one FastCDC window",
      );
      const corrupted = buildManifest(bytes, DEFAULT_FASTCDC);
      corrupted.entries[0].hash[0] ^= 1;
      let corruptReads = 0;
      rejects(
        () =>
          rebuildDiagnosticManifestLocally(
            {
              size: bytes.length,
              read(offset, length) {
                corruptReads += 1;
                return bytes.slice(offset, offset + length);
              },
            },
            corrupted,
            { offset: 700000, deleteLength: 1, insertBytes: Uint8Array.of(42) },
          ),
        /cached entry/,
        "diagnostic cached-entry authentication",
      );
      assert(corruptReads === 0, "corrupt diagnostic cache caused source I/O");
      return local.metrics;
    });

    check("streamed-rebuild-sink-ownership", () => {
      const bytes = fixture(64 * 1024 + 19, 0xbadc0de);
      const workspace = new MemoryWorkspace();
      const invalidAttemptedMetrics = [
        {
          sourceBytesRead: 0,
          bytesHashed: 0,
          largestSourceRead: 0,
          chunkerInputBytesCopied: 1,
          chunkerOutputBytesCopied: 2,
          chunkerBoundaryBytesScanned: 0,
          editedInputBytesPrepared: 2,
        },
        {
          sourceBytesRead: 0,
          bytesHashed: 0,
          largestSourceRead: 0,
          chunkerInputBytesCopied: 1,
          chunkerOutputBytesCopied: 0,
          chunkerBoundaryBytesScanned: 2,
          editedInputBytesPrepared: 1,
        },
        {
          sourceBytesRead: 0,
          bytesHashed: 1,
          largestSourceRead: 0,
          chunkerInputBytesCopied: 1,
          chunkerOutputBytesCopied: 0,
          chunkerBoundaryBytesScanned: 0,
          editedInputBytesPrepared: 1,
        },
        {
          sourceBytesRead: 0,
          bytesHashed: 0,
          largestSourceRead: 0,
          chunkerInputBytesCopied: 2,
          chunkerOutputBytesCopied: 0,
          chunkerBoundaryBytesScanned: 0,
          editedInputBytesPrepared: 1,
        },
        {
          sourceBytesRead: 2,
          bytesHashed: 0,
          largestSourceRead: 1,
          chunkerInputBytesCopied: 0,
          chunkerOutputBytesCopied: 0,
          chunkerBoundaryBytesScanned: 0,
          editedInputBytesPrepared: 1,
        },
        {
          sourceBytesRead: 5,
          bytesHashed: 0,
          largestSourceRead: 5,
          chunkerInputBytesCopied: 5,
          chunkerOutputBytesCopied: 0,
          chunkerBoundaryBytesScanned: 0,
          editedInputBytesPrepared: 5,
        },
        {
          sourceBytesRead: 0,
          bytesHashed: 0,
          largestSourceRead: 0,
          chunkerInputBytesCopied: 1,
          chunkerOutputBytesCopied: 0,
          chunkerBoundaryBytesScanned: 0,
          editedInputBytesPrepared: 6,
        },
      ];
      for (const [index, attempted] of invalidAttemptedMetrics.entries())
        rejects(
          () =>
            rebuildEditedContentStreaming(
              { size: 1, read: () => Uint8Array.of(1) },
              { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
              { minimum: 1, average: 2, maximum: 4 },
              new MemoryWorkspace(),
              { putObject() {} },
              "invalid attempted metrics",
              {},
              attempted,
            ),
          /./,
          `attempted-local relation ${index}`,
        );
      const exactAttemptedBoundary = rebuildEditedContentStreaming(
        { size: 4, read: (_offset, length) => new Uint8Array(length) },
        { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
        { minimum: 1, average: 2, maximum: 4 },
        new MemoryWorkspace(),
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
      assert(
        exactAttemptedBoundary.metrics.attemptedLocalLargestSourceRead === 4 &&
          exactAttemptedBoundary.metrics.attemptedLocalEditedInputBytesPrepared === 8,
        "exact attempted-local boundary rejected or changed",
      );
      const subclassSourceBytes = fixture(4096 + 37, 0x51ced);
      const subclassSourceParameters = { minimum: 64, average: 128, maximum: 512 };
      const subclassSourceResult = rebuildEditedContentStreaming(
        {
          size: subclassSourceBytes.length,
          read(offset, length) {
            const range = new SubstitutingBytes(length);
            range.set(subclassSourceBytes.slice(offset, offset + length));
            return range;
          },
        },
        { offset: 0, deleteLength: 0, insertBytes: new Uint8Array() },
        subclassSourceParameters,
        new MemoryWorkspace(),
        { putObject() {} },
        "subclass source normalization",
        { readWindowBytes: 100, manifestReadBatchRecords: 17 },
      );
      const subclassSourceRoot = decodeManifestRoot(
        subclassSourceResult.manifest.root,
        subclassSourceResult.manifest.rootHash,
      );
      assert(
        bytesToHex(subclassSourceResult.manifest.rootHash) ===
          buildManifest(subclassSourceBytes, subclassSourceParameters).id &&
          subclassSourceRoot.fileSize === subclassSourceBytes.length &&
          subclassSourceResult.metrics.sourceBytesRead === subclassSourceBytes.length &&
          subclassSourceResult.metrics.bytesHashed === subclassSourceBytes.length,
        "subclass source range changed streamed content",
      );
      const insertion = new SubstitutingBytes(1);
      insertion[0] = 42;
      const result = rebuildEditedContentStreaming(
        {
          size: bytes.length,
          read(offset, length) {
            insertion.fill(0);
            return bytes.slice(offset, offset + length);
          },
        },
        { offset: 25000, deleteLength: 1, insertBytes: insertion },
        { minimum: 64, average: 128, maximum: 512 },
        workspace,
        {
          putObject(hash, object) {
            hash.fill(0);
            object.fill(0);
          },
        },
        "workerd ownership",
        { readWindowBytes: 257, manifestReadBatchRecords: 11 },
      );
      const edited = bytes.slice();
      edited[25000] = 42;
      assert(
        bytesToHex(result.manifest.rootHash) ===
          buildManifest(edited, { minimum: 64, average: 128, maximum: 512 }).id,
        "streamed rebuild root mismatch",
      );
      assert(
        result.metrics.insertionCopyCount === 1 &&
          result.metrics.insertionBytesCopied === 1,
        "streamed insertion ownership metric mismatch",
      );
      return result.metrics;
    });

    check("runtime-progress-bound", () => {
      const required = requiredRuntimeProgressBytes(
        DEFAULT_FILESYSTEM_LIMITS,
        DEFAULT_STORAGE_LIMITS,
        4096,
      );
      validateRuntimeLimits(
        DEFAULT_FILESYSTEM_LIMITS,
        DEFAULT_STORAGE_LIMITS,
        { ...DEFAULT_RUNTIME_LIMITS, maxManagedResidentBytes: required },
        4096,
      );
      rejects(
        () =>
          validateRuntimeLimits(
            DEFAULT_FILESYSTEM_LIMITS,
            DEFAULT_STORAGE_LIMITS,
            { ...DEFAULT_RUNTIME_LIMITS, maxManagedResidentBytes: required - 1 },
            4096,
          ),
        /minimum progress working set/,
        "runtime below-bound admission",
      );
      assert(
        requiredRuntimeProgressBytes(
          DEFAULT_FILESYSTEM_LIMITS,
          DEFAULT_STORAGE_LIMITS,
          8192,
        ) === 102277120 &&
          requiredRuntimeProgressBytes(
            DEFAULT_FILESYSTEM_LIMITS,
            DEFAULT_STORAGE_LIMITS,
            16384,
          ) === 102285312,
        "COW page progress totals mismatch",
      );
      for (const pageBytes of [1, 4095, 4097, Number.NaN])
        rejects(
          () =>
            requiredRuntimeProgressBytes(
              DEFAULT_FILESYSTEM_LIMITS,
              DEFAULT_STORAGE_LIMITS,
              pageBytes,
            ),
          /cowPageBytes/,
          `invalid COW page ${pageBytes}`,
        );
      const getterReads = { preferred: 0, node: 0 };
      requiredRuntimeProgressBytes(
        {
          ...DEFAULT_FILESYSTEM_LIMITS,
          get preferredStreamChunkBytes() {
            getterReads.preferred += 1;
            return DEFAULT_FILESYSTEM_LIMITS.preferredStreamChunkBytes;
          },
        },
        {
          ...DEFAULT_STORAGE_LIMITS,
          get maxManifestNodeBytes() {
            getterReads.node += 1;
            return DEFAULT_STORAGE_LIMITS.maxManifestNodeBytes;
          },
        },
        4096,
      );
      assert(
        getterReads.preferred === 1 && getterReads.node === 1,
        "runtime progress getters were reread",
      );
      return { requiredBytes: required, pageSizesTested: 3 };
    });

    return Response.json({
      runtime: "workerd",
      passed: checks.length,
      checks,
    });
  },
};

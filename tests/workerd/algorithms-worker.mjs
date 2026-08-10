import { sha256, sha256Hex } from "./cas/sha256.js";
import {
  DEFAULT_FASTCDC,
  FASTCDC_GEAR_V1,
  StreamingFastCdc,
  fastCdcChunks,
} from "./cdc/fastcdc.js";
import { buildManifest } from "./operations/full-rebuild.js";
import { rebuildManifestLocally } from "./operations/local-rebuild.js";
import { bytesToHex } from "./cas/bytes.js";
import { encodeManifestNode, encodeManifestRoot } from "./manifests/codec.js";
import {
  advanceManifestGroupingState,
  isManifestGroupBoundary,
} from "./manifests/grouping.js";

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

export default {
  fetch() {
    const abc = new TextEncoder().encode("abc");
    assert(
      sha256Hex(abc) ===
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
      "workerd SHA-256 golden mismatch",
    );
    assert(
      Array.from(FASTCDC_GEAR_V1.slice(0, 4), (value) => value.toString(16)).join(
        ",",
      ) === "510c4619,e02e553e,7bb98f3a,183a8b5",
      "workerd Gear table mismatch",
    );
    const bytes = fixture(2 * 1024 * 1024);
    assert(
      fastCdcChunks(bytes)
        .map((chunk) => chunk.length)
        .join(",") ===
        "118265,231191,325530,155909,187710,141143,175869,138460,346490,147103,109121,20361",
      "workerd FastCDC golden mismatch",
    );
    const drainedLengths = [];
    const draining = new StreamingFastCdc();
    draining.drain(bytes, (chunk) => drainedLengths.push(chunk.byteLength), true);
    assert(
      drainedLengths.join(",") ===
        fastCdcChunks(bytes)
          .map((chunk) => chunk.length)
          .join(",") &&
        draining.bufferedBytes === 0 &&
        draining.maxPushBytes === DEFAULT_FASTCDC.maximum,
      "workerd draining FastCDC mismatch",
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
        "4541464e01000001020000000000000003000000000000000200000000000000111111111111111111111111111111111111111111111111111111111111111101000000222222222222222222222222222222222222222222222222222222222222222202000000" &&
        bytesToHex(sha256(leaf)) ===
          "e7b7034cb872766a9d02249f745276b89a32f2a53a7680641987ef93dc2f6c70",
      "workerd leaf golden mismatch",
    );
    const childA = { hash: sha256(leaf), span: 3, entryCount: 2 };
    const childB = { hash: new Uint8Array(32).fill(0x33), span: 5, entryCount: 1 };
    const internal = encodeManifestNode({
      kind: "internal",
      span: 8,
      entryCount: 3,
      children: [childA, childB],
    });
    assert(
      bytesToHex(internal) ===
        "4541464e01000101020000000000000008000000000000000300000000000000e7b7034cb872766a9d02249f745276b89a32f2a53a7680641987ef93dc2f6c7003000000000000000200000000000000333333333333333333333333333333333333333333333333333333333333333305000000000000000100000000000000" &&
        bytesToHex(sha256(internal)) ===
          "45a4b9207f8f4b5dc90aee18f6b099f802018110740300cdf8ca165c6cba9065",
      "workerd internal-node golden mismatch",
    );
    const rootVector = encodeManifestRoot({
      parameters: { minimum: 1, average: 2, maximum: 4 },
      fileSize: 8,
      entryCount: 3,
      rootNodeHash: sha256(internal),
    });
    assert(
      bytesToHex(rootVector) ===
        "45414652010001010100000002000000040000000800000000000000030000000000000045a4b9207f8f4b5dc90aee18f6b099f802018110740300cdf8ca165c6cba9065" &&
        bytesToHex(sha256(rootVector)) ===
          "dca081afd9e6ad4650d7e327557b22dcb3747b98a9ce11f01118b4c652fef6ce",
      "workerd root-envelope golden mismatch",
    );
    let groupingState = advanceManifestGroupingState(0n, entryA);
    assert(groupingState === 0x61dc0de1d6ec86bfn, "workerd grouping state 1 mismatch");
    groupingState = advanceManifestGroupingState(groupingState, entryB);
    assert(groupingState === 0x42edc85640fd080fn, "workerd grouping state 2 mismatch");
    groupingState = 0n;
    const groupingBoundaries = [];
    for (let index = 0; index < 600; index += 1) {
      groupingState = advanceManifestGroupingState(groupingState, entryA);
      const count = (index % 256) + 1;
      if (isManifestGroupBoundary(count, groupingState, 64, 128, 256)) {
        groupingBoundaries.push(index + 1);
        groupingState = 0n;
      }
    }
    assert(
      groupingBoundaries.join(",") === "256,512",
      "workerd grouping boundary mismatch",
    );
    const manifest = buildManifest(bytes, {
      minimum: 32768,
      average: 131072,
      maximum: 524288,
    });
    assert(
      manifest.id ===
        "6c08078b39f26d3dd98b10a20e14371e4b2f96fd9164fd629214b8c74981e7f1",
      "workerd manifest golden mismatch",
    );
    assert(
      bytesToHex(manifest.root) ===
        "454146520100010100800000000002000000080000002000000000000c00000000000000b69876e73a78d0cb95f34e5206711a90aba19ff31d02eb63594fb7220ea4c91c",
      "workerd complete-root envelope mismatch",
    );
    const local = rebuildManifestLocally(
      {
        size: bytes.length,
        read(offset, length) {
          return bytes.slice(offset, offset + length);
        },
      },
      manifest,
      { offset: 700000, deleteLength: 1, insertBytes: Uint8Array.of(42) },
    );
    const edited = bytes.slice();
    edited[700000] = 42;
    assert(
      bytesToHex(local.rootHash) ===
        buildManifest(edited, { minimum: 32768, average: 131072, maximum: 524288 }).id,
      "workerd local rebuild mismatch",
    );
    return Response.json({
      runtime: "workerd",
      passed: 10,
      sourceBytesRead: local.metrics.sourceBytesRead,
      bytesHashed: local.metrics.bytesHashed,
    });
  },
};

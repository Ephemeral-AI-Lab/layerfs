import { sha256Hex } from "./cas/sha256.js";
import { FASTCDC_GEAR_V1, fastCdcChunks } from "./cdc/fastcdc.js";
import { buildManifest } from "./operations/full-rebuild.js";
import { rebuildManifestLocally } from "./operations/local-rebuild.js";
import { bytesToHex } from "./cas/bytes.js";

function fixture(length, seed = 0x12345678) {
  const bytes = new Uint8Array(length); let state = seed >>> 0;
  for (let index = 0; index < length; index += 1) { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; bytes[index] = state & 0xff; }
  return bytes;
}

function assert(condition, message) { if (!condition) throw new Error(message); }

export default {
  fetch() {
    const abc = new TextEncoder().encode("abc");
    assert(sha256Hex(abc) === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "workerd SHA-256 golden mismatch");
    assert(Array.from(FASTCDC_GEAR_V1.slice(0, 4), (value) => value.toString(16)).join(",") === "510c4619,e02e553e,7bb98f3a,183a8b5", "workerd Gear table mismatch");
    const bytes = fixture(2 * 1024 * 1024);
    assert(fastCdcChunks(bytes).map((chunk) => chunk.length).join(",") === "118265,231191,325530,155909,187710,141143,175869,138460,346490,147103,109121,20361", "workerd FastCDC golden mismatch");
    const manifest = buildManifest(bytes, { minimum: 32768, average: 131072, maximum: 524288 });
    assert(manifest.id === "6c08078b39f26d3dd98b10a20e14371e4b2f96fd9164fd629214b8c74981e7f1", "workerd manifest golden mismatch");
    const local = rebuildManifestLocally({ size: bytes.length, read(offset, length) { return bytes.slice(offset, offset + length); } }, manifest, { offset: 700000, deleteLength: 1, insertBytes: Uint8Array.of(42) });
    const edited = bytes.slice(); edited[700000] = 42;
    assert(bytesToHex(local.rootHash) === buildManifest(edited, { minimum: 32768, average: 131072, maximum: 524288 }).id, "workerd local rebuild mismatch");
    return Response.json({ runtime: "workerd", passed: 5, sourceBytesRead: local.metrics.sourceBytesRead, bytesHashed: local.metrics.bytesHashed });
  },
};

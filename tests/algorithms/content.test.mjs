import assert from "node:assert/strict";
import { test } from "node:test";
import { IncrementalSha256, createCasObject, sha256Hex, verifyCasObject } from "../../packages/fs/dist/cas/sha256.js";
import { DEFAULT_FASTCDC, FASTCDC_GEAR_V1, StreamingFastCdc, fastCdcChunks } from "../../packages/fs/dist/cdc/fastcdc.js";

function fixture(length, seed = 0x12345678) {
  const bytes = new Uint8Array(length);
  let state = seed >>> 0;
  for (let index = 0; index < length; index += 1) {
    state ^= state << 13; state ^= state >>> 17; state ^= state << 5;
    bytes[index] = state & 0xff;
  }
  return bytes;
}

test("CAS SHA-256 matches golden vectors and freezes inputs", () => {
  assert.equal(sha256Hex(new Uint8Array()), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  assert.equal(sha256Hex(new TextEncoder().encode("abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  const hasher = new IncrementalSha256().update(new TextEncoder().encode("a")).update(new TextEncoder().encode("bc"));
  assert.equal(Buffer.from(hasher.digest()).toString("hex"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  const source = Uint8Array.of(1, 2, 3); const object = createCasObject(source); source[0] = 9;
  assert.deepEqual([...object.bytes], [1, 2, 3]);
  assert.doesNotThrow(() => verifyCasObject(object.id, object.bytes));
  assert.throws(() => verifyCasObject(object.id, Uint8Array.of(1, 2, 4)), /digest mismatch/);
});

test("fastcdc-v1 gear and two-megabyte boundary vector are exact", () => {
  assert.deepEqual(Array.from(FASTCDC_GEAR_V1.slice(0, 8), (value) => value.toString(16)), ["510c4619", "e02e553e", "7bb98f3a", "183a8b5", "e6336d1f", "f989d237", "ba2529d0", "fcfbedbf"]);
  const bytes = fixture(2 * 1024 * 1024);
  const chunks = fastCdcChunks(bytes);
  assert.deepEqual(chunks.map(({ length }) => length), [118265, 231191, 325530, 155909, 187710, 141143, 175869, 138460, 346490, 147103, 109121, 20361]);
  assert.equal(chunks.reduce((sum, chunk) => sum + chunk.length, 0), bytes.length);
  assert.ok(chunks.every((chunk) => chunk.length > 0 && chunk.length <= DEFAULT_FASTCDC.maximum));
});

test("streaming FastCDC is invariant to every tested input partition", () => {
  const bytes = fixture(3 * 1024 * 1024 + 17);
  const expected = fastCdcChunks(bytes).map(({ offset, length }) => bytes.slice(offset, offset + length));
  for (const partition of [1, 7, 4096, 65_537, 524_288, 900_001]) {
    const stream = new StreamingFastCdc(); const actual = [];
    for (let offset = 0; offset < bytes.length; offset += partition) actual.push(...stream.push(bytes.subarray(offset, offset + partition)));
    actual.push(...stream.finish());
    assert.deepEqual(actual.map((chunk) => Buffer.from(chunk).toString("hex")), expected.map((chunk) => Buffer.from(chunk).toString("hex")));
    assert.equal(stream.bufferedBytes, 0);
  }
});


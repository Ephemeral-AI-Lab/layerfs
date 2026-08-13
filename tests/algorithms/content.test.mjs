import assert from "node:assert/strict";
import { test } from "node:test";
import {
  bytesToHex,
  intrinsicByteLength as casIntrinsicByteLength,
  intrinsicByteRange as casIntrinsicByteRange,
} from "../../packages/fs/dist/cas/bytes.js";
import {
  IncrementalSha256,
  casObjectId,
  createCasObject,
  manifestId,
  manifestIdFromHash,
  sha256Hex,
  verifyCasObject,
} from "../../packages/fs/dist/cas/sha256.js";
import {
  DEFAULT_FASTCDC,
  MAX_DIAGNOSTIC_FASTCDC_CHUNKS,
  MAX_STREAMING_FASTCDC_BYTES,
  MAX_STREAMING_FASTCDC_PUSH_BYTES,
  StreamingFastCdc,
  fastCdcGearTableV1,
  fastCdcChunks,
  findFastCdcBoundary,
} from "../../packages/fs/dist/cdc/fastcdc.js";
import {
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  DEFAULT_STORAGE_LIMITS,
  CONTENT_COLLECTOR_REFERENCE_BYTES,
  MAX_CONTENT_COLLECTOR_PUSH_BYTES,
  MAX_CONTENT_COLLECTOR_REFERENCES,
  MAX_CONTENT_OBJECT_BYTES,
  requiredRuntimeProgressBytes,
  validateRuntimeLimits,
} from "../../packages/fs/dist/resources/limits.js";
import {
  intrinsicByteLength as resourceIntrinsicByteLength,
  intrinsicByteRange as resourceIntrinsicByteRange,
} from "../../packages/fs/dist/resources/byte-capacity.js";

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

test("bytesToHex is lowercase, zero-padded, and respects intrinsic byte ranges", () => {
  const expectedHex = (bytes) =>
    Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

  assert.equal(bytesToHex(new Uint8Array()), "");
  assert.equal(
    bytesToHex(Uint8Array.of(0x00, 0x01, 0x0f, 0x10, 0xab, 0xff)),
    "00010f10abff",
  );

  const allBytes = Uint8Array.from({ length: 256 }, (_, value) => value);
  assert.equal(bytesToHex(allBytes), expectedHex(allBytes));
  assert.equal(bytesToHex(Buffer.from([0x00, 0x0f, 0x80, 0xff])), "000f80ff");

  class AdversarialBytes extends Uint8Array {
    get byteLength() {
      return 1;
    }
    get byteOffset() {
      return 0;
    }
    get buffer() {
      return new ArrayBuffer(1);
    }
    subarray() {
      return Uint8Array.of(0xff);
    }
    [Symbol.iterator]() {
      return Uint8Array.of(0xee)[Symbol.iterator]();
    }
  }
  const adversarial = new AdversarialBytes(4);
  adversarial.set([0x00, 0x0f, 0x10, 0xff]);
  assert.equal(bytesToHex(adversarial), "000f10ff");

  let state = 0x9e3779b9;
  for (let iteration = 0; iteration < 128; iteration += 1) {
    state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0;
    const bytes = new Uint8Array(state % 1025);
    for (let index = 0; index < bytes.byteLength; index += 1) {
      state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0;
      bytes[index] = state >>> 24;
    }
    assert.equal(bytesToHex(bytes), expectedHex(bytes), `iteration ${iteration}`);
  }
});

test("CAS SHA-256 matches golden vectors and freezes inputs", () => {
  assert.equal(
    sha256Hex(new Uint8Array()),
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
  assert.equal(
    sha256Hex(new TextEncoder().encode("abc")),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
  const hasher = new IncrementalSha256()
    .update(new TextEncoder().encode("a"))
    .update(new TextEncoder().encode("bc"));
  assert.equal(
    Buffer.from(hasher.digest()).toString("hex"),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
  const source = Buffer.from([1, 2, 3]);
  const object = createCasObject(source);
  source[0] = 9;
  assert.deepEqual([...object.bytes], [1, 2, 3]);
  assert.equal(Buffer.isBuffer(object.bytes), false);
  class SubstitutingBytes extends Uint8Array {
    get byteLength() {
      return 1;
    }
    subarray() {
      return new Uint8Array(this.byteLength).fill(0xff);
    }
  }
  const substituting = new SubstitutingBytes(3);
  substituting.set([4, 5, 6]);
  const substitutedObject = createCasObject(substituting);
  assert.deepEqual([...substitutedObject.bytes], [4, 5, 6]);
  assert.equal(casIntrinsicByteLength(substituting), 3);
  assert.equal(resourceIntrinsicByteLength(substituting), 3);
  assert.deepEqual([...casIntrinsicByteRange(substituting, 1)], [5, 6]);
  assert.deepEqual([...resourceIntrinsicByteRange(substituting, 1)], [5, 6]);
  const returned = object.bytes;
  returned.fill(0);
  assert.deepEqual([...object.bytes], [1, 2, 3]);
  assert.equal(Buffer.isBuffer(object.bytes), false);
  assert.doesNotThrow(() => verifyCasObject(object.id, object.bytes));
  assert.throws(
    () => verifyCasObject(object.id, Uint8Array.of(1, 2, 4)),
    /digest mismatch/,
  );
  assert.equal(casObjectId(object.id), object.id);
  assert.equal(manifestId(object.id), object.id);
  assert.equal(manifestIdFromHash(new Uint8Array(32)), "0".repeat(64));
  assert.throws(() => casObjectId("A".repeat(64)), /lowercase hexadecimal/);
  assert.throws(
    () => verifyCasObject("bad", new Uint8Array(1024 * 1024)),
    /exactly 64/,
  );
});

test("fastcdc-v1 gear and boundary fixture vectors are exact", () => {
  assert.deepEqual(
    Array.from(fastCdcGearTableV1().slice(0, 8), (value) => value.toString(16)),
    [
      "510c4619",
      "e02e553e",
      "7bb98f3a",
      "183a8b5",
      "e6336d1f",
      "f989d237",
      "ba2529d0",
      "fcfbedbf",
    ],
  );
  const vectors = [
    ["empty", 0, []],
    ["sub-minimum", DEFAULT_FASTCDC.minimum - 1, [32767]],
    ["minimum", DEFAULT_FASTCDC.minimum, [32768]],
    ["average", DEFAULT_FASTCDC.average, [118265, 12807]],
    ["maximum", DEFAULT_FASTCDC.maximum, [118265, 231191, 174832]],
    [
      "large",
      2 * 1024 * 1024,
      [
        118265, 231191, 325530, 155909, 187710, 141143, 175869, 138460, 346490, 147103,
        109121, 20361,
      ],
    ],
  ];
  for (const [name, length, expected] of vectors)
    assert.deepEqual(
      fastCdcChunks(fixture(length)).map((chunk) => chunk.length),
      expected,
      name,
    );
  const bytes = fixture(2 * 1024 * 1024);
  const chunks = fastCdcChunks(bytes);
  assert.equal(
    chunks.reduce((sum, chunk) => sum + chunk.length, 0),
    bytes.length,
  );
  assert.ok(
    chunks.every(
      (chunk) => chunk.length > 0 && chunk.length <= DEFAULT_FASTCDC.maximum,
    ),
  );
  const exposedGear = fastCdcGearTableV1();
  exposedGear.fill(0);
  assert.deepEqual(
    fastCdcChunks(bytes).map((chunk) => chunk.length),
    chunks.map((chunk) => chunk.length),
  );
  assert.throws(
    () => fastCdcChunks(Uint8Array.of(1, 2), { minimum: 1, average: 1, maximum: 1 }, 1),
    /chunk count/,
  );
});

test("streaming FastCDC is partition-invariant with bounded push retention", () => {
  const bytes = fixture(3 * 1024 * 1024 + 17);
  const expected = fastCdcChunks(bytes).map(({ offset, length }) =>
    bytes.slice(offset, offset + length),
  );
  for (const partition of [1, 7, 4096, 65_537, 524_288]) {
    const stream = new StreamingFastCdc();
    const actual = [];
    for (let offset = 0; offset < bytes.length; offset += partition)
      actual.push(...stream.push(bytes.subarray(offset, offset + partition)));
    actual.push(...stream.finish());
    assert.deepEqual(
      actual.map((chunk) => Buffer.from(chunk).toString("hex")),
      expected.map((chunk) => Buffer.from(chunk).toString("hex")),
    );
    assert.equal(stream.bufferedBytes, 0);
    assert.equal(stream.capacityBytes, DEFAULT_FASTCDC.maximum);
    assert.equal(stream.maxPushBytes, DEFAULT_FASTCDC.maximum);
  }

  const rejected = new StreamingFastCdc();
  assert.throws(
    () => rejected.push(new Uint8Array(DEFAULT_FASTCDC.maximum + 1)),
    /exceeds maxPushBytes/,
  );
  assert.equal(rejected.bufferedBytes, 0);

  const prebuffered = new StreamingFastCdc({
    minimum: 1024,
    average: 1024,
    maximum: 1024,
  });
  prebuffered.drain(new Uint8Array(1023), () => {});
  const boundedOutput = prebuffered.push(Uint8Array.of(1, 2), true);
  assert.deepEqual(
    boundedOutput.map((chunk) => chunk.byteLength),
    [1024, 1],
  );
  assert.equal(prebuffered.metrics.peakPushOutputBytes, 1025);
  assert.equal(prebuffered.metrics.peakPushOutputCount, 2);

  const adversarial = fixture(16 * 1024 * 1024 + 17, 0xa5a5a5a5);
  const expectedLengths = fastCdcChunks(adversarial).map((chunk) => chunk.length);
  const drainedLengths = [];
  let drainedBytes = 0;
  const draining = new StreamingFastCdc();
  draining.drain(
    adversarial,
    (chunk) => {
      assert.ok(chunk.byteLength <= DEFAULT_FASTCDC.maximum);
      drainedLengths.push(chunk.byteLength);
      drainedBytes += chunk.byteLength;
    },
    true,
  );
  assert.deepEqual(drainedLengths, expectedLengths);
  assert.equal(drainedBytes, adversarial.byteLength);
  assert.equal(draining.bufferedBytes, 0);
  assert.equal(draining.metrics.inputBytesCopied, adversarial.byteLength);
  assert.equal(draining.metrics.outputBytesCopied, adversarial.byteLength);
  assert.ok(draining.metrics.boundaryBytesScanned <= adversarial.byteLength);
  assert.equal(draining.metrics.peakPushOutputBytes, 0);
  assert.equal(draining.metrics.peakPushOutputCount, 0);
});

test("streaming FastCDC enforces allocation, retention, terminal, and linear-work bounds", () => {
  assert.equal(MAX_STREAMING_FASTCDC_BYTES, MAX_CONTENT_OBJECT_BYTES);
  assert.throws(
    () =>
      new StreamingFastCdc({
        minimum: 1,
        average: 2,
        maximum: MAX_CONTENT_OBJECT_BYTES + 1,
      }),
    /effective content-object limit/,
  );
  const getterReads = { minimum: 0, average: 0, maximum: 0 };
  const getterConfiguration = {
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
      return getterReads.maximum === 1 ? 4 : MAX_CONTENT_OBJECT_BYTES + 1;
    },
  };
  const getterChunker = new StreamingFastCdc(getterConfiguration);
  assert.equal(getterChunker.capacityBytes, 4);
  assert.deepEqual(getterReads, { minimum: 1, average: 1, maximum: 1 });

  for (const operation of [
    (configuration) => findFastCdcBoundary(Uint8Array.of(1, 2, 3, 4), 0, configuration),
    (configuration) => fastCdcChunks(Uint8Array.of(1, 2, 3, 4), configuration),
  ]) {
    const reads = { minimum: 0, average: 0, maximum: 0 };
    operation({
      get minimum() {
        reads.minimum += 1;
        return 1;
      },
      get average() {
        reads.average += 1;
        return 2;
      },
      get maximum() {
        reads.maximum += 1;
        return 4;
      },
    });
    assert.deepEqual(reads, { minimum: 1, average: 1, maximum: 1 });
  }
  assert.throws(
    () => new StreamingFastCdc(DEFAULT_FASTCDC, MAX_STREAMING_FASTCDC_PUSH_BYTES + 1),
    /bounded push input/,
  );
  const tiny = new StreamingFastCdc(
    { minimum: 1, average: 2, maximum: 1024 * 1024 },
    MAX_DIAGNOSTIC_FASTCDC_CHUNKS - 1,
  );
  assert.throws(
    () => tiny.push(new Uint8Array(MAX_DIAGNOSTIC_FASTCDC_CHUNKS)),
    /maxPushBytes/,
  );

  const finalized = new StreamingFastCdc();
  finalized.drain(Uint8Array.of(1), () => {}, true);
  assert.equal(finalized.finalized, true);
  assert.throws(() => finalized.finish(), /finalized/);
  assert.throws(() => finalized.push(Uint8Array.of(2)), /finalized/);
  assert.throws(() => finalized.drain(Uint8Array.of(2), () => {}), /finalized/);

  const failed = new StreamingFastCdc({ minimum: 1, average: 2, maximum: 4 });
  assert.throws(
    () =>
      failed.drain(Uint8Array.of(1, 2, 3, 4), () => {
        throw new Error("consumer failure");
      }),
    /consumer failure/,
  );
  assert.throws(() => failed.push(Uint8Array.of(1)), /failed/);

  const reentrant = new StreamingFastCdc({ minimum: 1, average: 2, maximum: 4 });
  assert.throws(
    () =>
      reentrant.drain(Uint8Array.of(1, 2, 3, 4), () => {
        reentrant.drain(Uint8Array.of(9), () => {});
      }),
    /not reentrant/,
  );

  const input = fixture(256 * 1024, 0xdecafbad);
  const linear = new StreamingFastCdc({
    minimum: 1,
    average: 2,
    maximum: input.byteLength,
  });
  let outputBytes = 0;
  const started = performance.now();
  linear.drain(
    input,
    (chunk) => {
      outputBytes += chunk.byteLength;
    },
    true,
  );
  const elapsedMs = performance.now() - started;
  assert.equal(outputBytes, input.byteLength);
  assert.equal(linear.metrics.inputBytesCopied, input.byteLength);
  assert.equal(linear.metrics.outputBytesCopied, input.byteLength);
  assert.ok(linear.metrics.boundaryBytesScanned <= input.byteLength);
  assert.ok(elapsedMs < 5_000, `adversarial linear scan took ${elapsedMs}ms`);
});

test("runtime progress admission derives from the shared object ceiling", () => {
  assert.equal(MAX_STREAMING_FASTCDC_PUSH_BYTES, MAX_CONTENT_COLLECTOR_PUSH_BYTES);
  assert.equal(MAX_DIAGNOSTIC_FASTCDC_CHUNKS, MAX_CONTENT_COLLECTOR_REFERENCES);
  assert.equal(CONTENT_COLLECTOR_REFERENCE_BYTES, 16);
  const required = requiredRuntimeProgressBytes(
    DEFAULT_FILESYSTEM_LIMITS,
    DEFAULT_STORAGE_LIMITS,
    4096,
  );
  assert.equal(required, 102_273_024);
  assert.equal(
    requiredRuntimeProgressBytes(
      DEFAULT_FILESYSTEM_LIMITS,
      DEFAULT_STORAGE_LIMITS,
      8192,
    ),
    102_277_120,
  );
  assert.equal(
    requiredRuntimeProgressBytes(
      DEFAULT_FILESYSTEM_LIMITS,
      DEFAULT_STORAGE_LIMITS,
      16_384,
    ),
    102_285_312,
  );
  for (const pageBytes of [1, 4095, 4097, Number.NaN])
    assert.throws(
      () =>
        requiredRuntimeProgressBytes(
          DEFAULT_FILESYSTEM_LIMITS,
          DEFAULT_STORAGE_LIMITS,
          pageBytes,
        ),
      /cowPageBytes/,
    );
  assert.throws(
    () =>
      requiredRuntimeProgressBytes(
        DEFAULT_FILESYSTEM_LIMITS,
        { ...DEFAULT_STORAGE_LIMITS, maxManifestNodeBytes: Number.NaN },
        4096,
      ),
    /maxManifestNodeBytes/,
  );
  const getterReads = { preferred: 0, node: 0, resident: 0 };
  const getterFilesystem = {
    ...DEFAULT_FILESYSTEM_LIMITS,
    get preferredStreamChunkBytes() {
      getterReads.preferred += 1;
      return DEFAULT_FILESYSTEM_LIMITS.preferredStreamChunkBytes;
    },
  };
  const getterStorage = {
    ...DEFAULT_STORAGE_LIMITS,
    get maxManifestNodeBytes() {
      getterReads.node += 1;
      return DEFAULT_STORAGE_LIMITS.maxManifestNodeBytes;
    },
  };
  const getterRuntime = {
    ...DEFAULT_RUNTIME_LIMITS,
    get maxManagedResidentBytes() {
      getterReads.resident += 1;
      return DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes;
    },
  };
  validateRuntimeLimits(getterFilesystem, getterStorage, getterRuntime, 4096);
  assert.deepEqual(getterReads, { preferred: 1, node: 1, resident: 1 });
  assert.ok(required > MAX_CONTENT_OBJECT_BYTES * 6);
  assert.doesNotThrow(() =>
    validateRuntimeLimits(
      DEFAULT_FILESYSTEM_LIMITS,
      DEFAULT_STORAGE_LIMITS,
      { ...DEFAULT_RUNTIME_LIMITS, maxManagedResidentBytes: required },
      4096,
    ),
  );
  assert.throws(
    () =>
      validateRuntimeLimits(
        DEFAULT_FILESYSTEM_LIMITS,
        DEFAULT_STORAGE_LIMITS,
        { ...DEFAULT_RUNTIME_LIMITS, maxManagedResidentBytes: required - 1 },
        4096,
      ),
    /minimum progress working set/,
  );
});

import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import {
  MAX_CONTENT_COLLECTOR_PUSH_BYTES,
  MAX_CONTENT_COLLECTOR_REFERENCES,
  MAX_CONTENT_OBJECT_BYTES,
} from "../resources/limits.js";
import { intrinsicByteRange } from "../resources/byte-capacity.js";

export interface FastCdcConfiguration {
  readonly minimum: number;
  readonly average: number;
  readonly maximum: number;
}
export interface FastCdcChunk {
  readonly offset: number;
  readonly length: number;
}
export type FastCdcChunkConsumer = (chunk: Uint8Array) => void;
export interface StreamingFastCdcMetrics {
  readonly inputBytesCopied: number;
  readonly outputBytesCopied: number;
  readonly boundaryBytesScanned: number;
  readonly peakPushOutputBytes: number;
  readonly peakPushOutputCount: number;
}
export const DEFAULT_FASTCDC: FastCdcConfiguration = Object.freeze({
  minimum: 32_768,
  average: 131_072,
  maximum: 524_288,
});

export const MAX_STREAMING_FASTCDC_BYTES = MAX_CONTENT_OBJECT_BYTES;
export const MAX_DIAGNOSTIC_FASTCDC_CHUNKS = MAX_CONTENT_COLLECTOR_REFERENCES;
export const MAX_STREAMING_FASTCDC_PUSH_BYTES = MAX_CONTENT_COLLECTOR_PUSH_BYTES;

export function snapshotFastCdcConfiguration(
  configuration: FastCdcConfiguration,
): Readonly<FastCdcConfiguration> {
  const minimum = configuration.minimum;
  const average = configuration.average;
  const maximum = configuration.maximum;
  return Object.freeze({ minimum, average, maximum });
}

function validateOwnedFastCdcConfiguration(configuration: FastCdcConfiguration): void {
  checkedInteger(configuration.minimum, "minimum", 0xffff_ffff);
  checkedInteger(configuration.average, "average", 0xffff_ffff);
  checkedInteger(configuration.maximum, "maximum", 0xffff_ffff);
  if (
    configuration.minimum === 0 ||
    configuration.minimum > configuration.average ||
    configuration.average > configuration.maximum
  ) {
    throw new RangeError("FastCDC requires 0 < minimum <= average <= maximum");
  }
  if ((configuration.average & (configuration.average - 1)) !== 0)
    throw new RangeError("FastCDC average must be a power of two");
}

function validateOwnedSupportedFastCdcConfiguration(
  configuration: FastCdcConfiguration,
): void {
  validateOwnedFastCdcConfiguration(configuration);
  if (configuration.maximum > MAX_CONTENT_OBJECT_BYTES)
    throw new RangeError(
      `FastCDC maximum exceeds the effective content-object limit (${MAX_CONTENT_OBJECT_BYTES})`,
    );
}

const FASTCDC_GEAR_V1_PRIVATE: Uint32Array = (() => {
  const table = new Uint32Array(256);
  let seed = 0x9e3779b9;
  for (let index = 0; index < table.length; index += 1) {
    seed = (seed ^ (seed << 13)) >>> 0;
    seed = (seed ^ (seed >>> 17)) >>> 0;
    seed = (seed ^ (seed << 5)) >>> 0;
    table[index] = seed;
  }
  return table;
})();

/** Returns a defensive copy; canonical chunking never reads caller-visible storage. */
export function fastCdcGearTableV1(): Uint32Array {
  return FASTCDC_GEAR_V1_PRIVATE.slice();
}

export function validateFastCdcConfiguration(
  configuration: FastCdcConfiguration,
): void {
  validateOwnedFastCdcConfiguration(snapshotFastCdcConfiguration(configuration));
}

export function validateSupportedFastCdcConfiguration(
  configuration: FastCdcConfiguration,
): void {
  validateOwnedSupportedFastCdcConfiguration(
    snapshotFastCdcConfiguration(configuration),
  );
}

function findFastCdcBoundaryOwned(
  input: Uint8Array,
  start: number,
  configuration: FastCdcConfiguration,
): number {
  checkedInteger(start, "start", input.byteLength);
  const minimumEnd = Math.min(
    checkedAdd(start, configuration.minimum),
    input.byteLength,
  );
  const normalEnd = Math.min(
    checkedAdd(start, configuration.average),
    input.byteLength,
  );
  const maximumEnd = Math.min(
    checkedAdd(start, configuration.maximum),
    input.byteLength,
  );
  if (minimumEnd >= input.byteLength) return input.byteLength;
  const bits = Math.log2(configuration.average);
  const earlyMask = (2 ** Math.min(30, bits + 1) - 1) >>> 0;
  const lateMask = (2 ** Math.max(1, bits - 1) - 1) >>> 0;
  let gearHash = 0;
  for (let cursor = minimumEnd; cursor < maximumEnd; cursor += 1) {
    gearHash = ((gearHash << 1) + FASTCDC_GEAR_V1_PRIVATE[input[cursor]!]!) >>> 0;
    const mask = cursor < normalEnd ? earlyMask : lateMask;
    if ((gearHash & mask) === 0) return cursor + 1;
  }
  return maximumEnd;
}

export function findFastCdcBoundary(
  input: Uint8Array,
  start: number,
  configuration: FastCdcConfiguration = DEFAULT_FASTCDC,
): number {
  input = intrinsicByteRange(input);
  const owned = snapshotFastCdcConfiguration(configuration);
  validateOwnedFastCdcConfiguration(owned);
  return findFastCdcBoundaryOwned(input, start, owned);
}

export function fastCdcChunks(
  input: Uint8Array,
  configuration: FastCdcConfiguration = DEFAULT_FASTCDC,
  maxChunks = MAX_DIAGNOSTIC_FASTCDC_CHUNKS,
): FastCdcChunk[] {
  input = intrinsicByteRange(input);
  configuration = snapshotFastCdcConfiguration(configuration);
  validateOwnedFastCdcConfiguration(configuration);
  checkedInteger(maxChunks, "maxChunks", MAX_DIAGNOSTIC_FASTCDC_CHUNKS);
  if (maxChunks === 0) throw new RangeError("maxChunks must be positive");
  const chunks: FastCdcChunk[] = [];
  let offset = 0;
  while (offset < input.byteLength) {
    const boundary = findFastCdcBoundaryOwned(input, offset, configuration);
    if (chunks.length >= maxChunks)
      throw new RangeError("diagnostic FastCDC chunk count exceeds maxChunks");
    chunks.push(Object.freeze({ offset, length: boundary - offset }));
    offset = boundary;
  }
  return chunks;
}

export class StreamingFastCdc {
  readonly #configuration: FastCdcConfiguration;
  readonly #buffer: Uint8Array;
  readonly #maxPushBytes: number;
  readonly #earlyMask: number;
  readonly #lateMask: number;
  #buffered = 0;
  #gearHash = 0;
  #finalized = false;
  #failed = false;
  #inputBytesCopied = 0;
  #outputBytesCopied = 0;
  #boundaryBytesScanned = 0;
  #peakPushOutputBytes = 0;
  #peakPushOutputCount = 0;
  #inProgress = false;

  constructor(
    configuration: FastCdcConfiguration = DEFAULT_FASTCDC,
    maxPushBytes?: number,
  ) {
    configuration = snapshotFastCdcConfiguration(configuration);
    validateOwnedSupportedFastCdcConfiguration(configuration);
    maxPushBytes ??= Math.min(
      configuration.maximum,
      MAX_STREAMING_FASTCDC_PUSH_BYTES,
      configuration.minimum * (MAX_DIAGNOSTIC_FASTCDC_CHUNKS - 1),
    );
    checkedInteger(maxPushBytes, "maxPushBytes");
    if (maxPushBytes === 0) throw new RangeError("maxPushBytes must be positive");
    // Input is independently bounded. push() also preflights the prebuffer plus
    // this input against the separately charged output-byte/reference envelope.
    const boundedPushInputBytes = Math.min(
      MAX_STREAMING_FASTCDC_BYTES,
      MAX_STREAMING_FASTCDC_PUSH_BYTES,
      configuration.minimum * (MAX_DIAGNOSTIC_FASTCDC_CHUNKS - 1),
    );
    if (maxPushBytes > boundedPushInputBytes)
      throw new RangeError(
        `maxPushBytes exceeds the bounded push input/output-count limit (${boundedPushInputBytes})`,
      );
    this.#configuration = configuration;
    this.#buffer = new Uint8Array(configuration.maximum);
    this.#maxPushBytes = maxPushBytes;
    const bits = Math.log2(configuration.average);
    this.#earlyMask = (2 ** Math.min(30, bits + 1) - 1) >>> 0;
    this.#lateMask = (2 ** Math.max(1, bits - 1) - 1) >>> 0;
  }

  push(input: Uint8Array, final = false): Uint8Array[] {
    input = intrinsicByteRange(input);
    this.#assertActive();
    if (input.byteLength > this.#maxPushBytes)
      throw new RangeError(
        `streaming FastCDC push exceeds maxPushBytes (${this.#maxPushBytes})`,
      );
    const available = checkedAdd(
      this.#buffered,
      input.byteLength,
      "streaming FastCDC push available bytes",
    );
    if (available > this.#configuration.maximum + this.#maxPushBytes)
      throw new RangeError(
        "streaming FastCDC push output exceeds its charged collector envelope; use drain()",
      );
    const maximumOutputCount =
      (this.#buffered > 0 ? 1 : 0) +
      Math.ceil(input.byteLength / this.#configuration.minimum);
    if (maximumOutputCount > MAX_DIAGNOSTIC_FASTCDC_CHUNKS)
      throw new RangeError(
        "streaming FastCDC push output could exceed the chunk-count bound; use drain()",
      );
    const chunks: Uint8Array[] = [];
    let outputBytes = 0;
    this.drain(
      input,
      (chunk) => {
        outputBytes = checkedAdd(
          outputBytes,
          chunk.byteLength,
          "streaming FastCDC push output bytes",
        );
        chunks.push(chunk);
      },
      final,
    );
    this.#peakPushOutputBytes = Math.max(this.#peakPushOutputBytes, outputBytes);
    this.#peakPushOutputCount = Math.max(this.#peakPushOutputCount, chunks.length);
    return chunks;
  }

  /**
   * Consume emitted chunks immediately. Unlike push(), this path does not
   * retain all output produced by an arbitrarily large input segment.
   */
  drain(input: Uint8Array, consume: FastCdcChunkConsumer, final = false): void {
    input = intrinsicByteRange(input);
    this.#assertActive();
    const inputStart = this.#inputBytesCopied;
    const scannedStart = this.#boundaryBytesScanned;
    checkedAdd(inputStart, input.byteLength, "streaming FastCDC input bytes");
    checkedAdd(scannedStart, input.byteLength, "streaming FastCDC scanned bytes");
    this.#inProgress = true;
    let offset = 0;
    let scanned = 0;
    try {
      while (offset < input.byteLength) {
        if (this.#buffered < this.#configuration.minimum) {
          const copied = Math.min(
            this.#configuration.minimum - this.#buffered,
            input.byteLength - offset,
          );
          this.#buffer.set(input.subarray(offset, offset + copied), this.#buffered);
          this.#buffered += copied;
          offset += copied;
          if (this.#buffered === this.#configuration.maximum)
            consume(this.#emitChunk());
          continue;
        }

        const cursor = this.#buffered;
        const value = input[offset]!;
        this.#buffer[cursor] = value;
        this.#buffered += 1;
        offset += 1;
        this.#gearHash =
          ((this.#gearHash << 1) + FASTCDC_GEAR_V1_PRIVATE[value]!) >>> 0;
        scanned += 1;
        const mask =
          cursor < this.#configuration.average ? this.#earlyMask : this.#lateMask;
        if (
          (this.#gearHash & mask) === 0 ||
          this.#buffered === this.#configuration.maximum
        )
          consume(this.#emitChunk());
      }
      if (final) {
        if (this.#buffered > 0) consume(this.#emitChunk());
        this.#finalized = true;
      }
    } catch (error) {
      this.#failed = true;
      throw error;
    } finally {
      this.#inputBytesCopied = inputStart + offset;
      this.#boundaryBytesScanned = scannedStart + scanned;
      this.#inProgress = false;
    }
  }

  finish(): Uint8Array[] {
    return this.push(new Uint8Array(), true);
  }
  get bufferedBytes(): number {
    return this.#buffered;
  }
  get capacityBytes(): number {
    return this.#buffer.byteLength;
  }
  get maxPushBytes(): number {
    return this.#maxPushBytes;
  }
  get finalized(): boolean {
    return this.#finalized;
  }
  get metrics(): StreamingFastCdcMetrics {
    return Object.freeze({
      inputBytesCopied: this.#inputBytesCopied,
      outputBytesCopied: this.#outputBytesCopied,
      boundaryBytesScanned: this.#boundaryBytesScanned,
      peakPushOutputBytes: this.#peakPushOutputBytes,
      peakPushOutputCount: this.#peakPushOutputCount,
    });
  }

  #emitChunk(): Uint8Array {
    const chunk = this.#buffer.slice(0, this.#buffered);
    this.#outputBytesCopied = checkedAdd(
      this.#outputBytesCopied,
      chunk.byteLength,
      "streaming FastCDC output bytes",
    );
    this.#buffered = 0;
    this.#gearHash = 0;
    return chunk;
  }

  #assertActive(): void {
    if (this.#failed) throw new Error("streaming FastCDC is failed");
    if (this.#finalized) throw new Error("streaming FastCDC is finalized");
    if (this.#inProgress) throw new Error("streaming FastCDC is not reentrant");
  }
}

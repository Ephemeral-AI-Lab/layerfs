import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";

export interface FastCdcConfiguration {
  readonly minimum: number;
  readonly average: number;
  readonly maximum: number;
}
export interface FastCdcChunk {
  readonly offset: number;
  readonly length: number;
}
export const DEFAULT_FASTCDC: FastCdcConfiguration = Object.freeze({
  minimum: 32_768,
  average: 131_072,
  maximum: 524_288,
});

export const FASTCDC_GEAR_V1: Uint32Array = (() => {
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

export function validateFastCdcConfiguration(
  configuration: FastCdcConfiguration,
): void {
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

export function findFastCdcBoundary(
  input: Uint8Array,
  start: number,
  configuration: FastCdcConfiguration = DEFAULT_FASTCDC,
): number {
  validateFastCdcConfiguration(configuration);
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
    gearHash = ((gearHash << 1) + FASTCDC_GEAR_V1[input[cursor]!]!) >>> 0;
    const mask = cursor < normalEnd ? earlyMask : lateMask;
    if ((gearHash & mask) === 0) return cursor + 1;
  }
  return maximumEnd;
}

export function fastCdcChunks(
  input: Uint8Array,
  configuration: FastCdcConfiguration = DEFAULT_FASTCDC,
): FastCdcChunk[] {
  validateFastCdcConfiguration(configuration);
  const chunks: FastCdcChunk[] = [];
  let offset = 0;
  while (offset < input.byteLength) {
    const boundary = findFastCdcBoundary(input, offset, configuration);
    chunks.push(Object.freeze({ offset, length: boundary - offset }));
    offset = boundary;
  }
  return chunks;
}

export class StreamingFastCdc {
  readonly #configuration: FastCdcConfiguration;
  readonly #buffer: Uint8Array;
  #buffered = 0;

  constructor(configuration: FastCdcConfiguration = DEFAULT_FASTCDC) {
    validateFastCdcConfiguration(configuration);
    this.#configuration = Object.freeze({ ...configuration });
    this.#buffer = new Uint8Array(configuration.maximum);
  }

  push(input: Uint8Array, final = false): Uint8Array[] {
    const chunks: Uint8Array[] = [];
    let offset = 0;
    while (offset < input.byteLength) {
      const copied = Math.min(
        this.#buffer.byteLength - this.#buffered,
        input.byteLength - offset,
      );
      this.#buffer.set(input.subarray(offset, offset + copied), this.#buffered);
      this.#buffered += copied;
      offset += copied;
      if (this.#buffered === this.#buffer.byteLength) chunks.push(this.#emitChunk());
    }
    if (final) while (this.#buffered > 0) chunks.push(this.#emitChunk());
    return chunks;
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

  #emitChunk(): Uint8Array {
    const view = this.#buffer.subarray(0, this.#buffered);
    const boundary = findFastCdcBoundary(view, 0, this.#configuration);
    const chunk = view.slice(0, boundary);
    this.#buffer.copyWithin(0, boundary, this.#buffered);
    this.#buffered -= boundary;
    return chunk;
  }
}

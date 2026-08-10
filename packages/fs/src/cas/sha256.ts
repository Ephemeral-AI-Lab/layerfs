import {
  bytesToHex,
  copyBytes,
  equalBytes,
  freezeBytes,
  intrinsicByteLength,
  intrinsicByteRange,
} from "./bytes.js";

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
  0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
  0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
  0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
  0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
  0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
  0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
  0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
  0xc67178f2,
]);

function rotateRight(value: number, bits: number): number {
  return (value >>> bits) | (value << (32 - bits));
}

export class IncrementalSha256 {
  readonly #state = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
    0x5be0cd19,
  ]);
  readonly #buffer = new Uint8Array(64);
  readonly #words = new Uint32Array(64);
  #bufferLength = 0;
  #bytesHashed = 0;
  #finished = false;

  update(input: Uint8Array): this {
    input = intrinsicByteRange(input);
    if (this.#finished) throw new Error("SHA-256 has already been finalized");
    if (this.#bytesHashed + input.byteLength > Number.MAX_SAFE_INTEGER)
      throw new RangeError("SHA-256 input is too large");
    this.#bytesHashed += input.byteLength;
    let position = 0;
    while (position < input.byteLength) {
      const take = Math.min(64 - this.#bufferLength, input.byteLength - position);
      this.#buffer.set(input.subarray(position, position + take), this.#bufferLength);
      this.#bufferLength += take;
      position += take;
      if (this.#bufferLength === 64) {
        this.#compress(this.#buffer);
        this.#bufferLength = 0;
      }
    }
    return this;
  }

  digest(): Uint8Array {
    if (this.#finished) throw new Error("SHA-256 has already been finalized");
    this.#finished = true;
    const bitLength = BigInt(this.#bytesHashed) * 8n;
    this.#buffer[this.#bufferLength++] = 0x80;
    if (this.#bufferLength > 56) {
      this.#buffer.fill(0, this.#bufferLength);
      this.#compress(this.#buffer);
      this.#bufferLength = 0;
    }
    this.#buffer.fill(0, this.#bufferLength, 56);
    new DataView(this.#buffer.buffer).setBigUint64(56, bitLength, false);
    this.#compress(this.#buffer);
    const result = new Uint8Array(32);
    const view = new DataView(result.buffer);
    for (let index = 0; index < 8; index += 1)
      view.setUint32(index * 4, this.#state[index]!, false);
    return result;
  }

  #compress(chunk: Uint8Array): void {
    const view = new DataView(chunk.buffer, chunk.byteOffset, chunk.byteLength);
    for (let index = 0; index < 16; index += 1)
      this.#words[index] = view.getUint32(index * 4, false);
    for (let index = 16; index < 64; index += 1) {
      const a = this.#words[index - 15]!;
      const b = this.#words[index - 2]!;
      const s0 = rotateRight(a, 7) ^ rotateRight(a, 18) ^ (a >>> 3);
      const s1 = rotateRight(b, 17) ^ rotateRight(b, 19) ^ (b >>> 10);
      this.#words[index] =
        (this.#words[index - 16]! + s0 + this.#words[index - 7]! + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, h] = this.#state;
    for (let index = 0; index < 64; index += 1) {
      const s1 = rotateRight(e!, 6) ^ rotateRight(e!, 11) ^ rotateRight(e!, 25);
      const choice = (e! & f!) ^ (~e! & g!);
      const t1 = (h! + s1 + choice + K[index]! + this.#words[index]!) >>> 0;
      const s0 = rotateRight(a!, 2) ^ rotateRight(a!, 13) ^ rotateRight(a!, 22);
      const majority = (a! & b!) ^ (a! & c!) ^ (b! & c!);
      const t2 = (s0 + majority) >>> 0;
      h = g;
      g = f;
      f = e;
      e = (d! + t1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (t1 + t2) >>> 0;
    }
    this.#state[0] = (this.#state[0]! + a!) >>> 0;
    this.#state[1] = (this.#state[1]! + b!) >>> 0;
    this.#state[2] = (this.#state[2]! + c!) >>> 0;
    this.#state[3] = (this.#state[3]! + d!) >>> 0;
    this.#state[4] = (this.#state[4]! + e!) >>> 0;
    this.#state[5] = (this.#state[5]! + f!) >>> 0;
    this.#state[6] = (this.#state[6]! + g!) >>> 0;
    this.#state[7] = (this.#state[7]! + h!) >>> 0;
  }
}

export type CasObjectId = string & { readonly __casObjectId: unique symbol };
export type ManifestId = string & { readonly __manifestId: unique symbol };

export type HashFunction = (bytes: Uint8Array) => Uint8Array;

export const sha256: HashFunction = (bytes) =>
  new IncrementalSha256().update(bytes).digest();

export function sha256Hex(bytes: Uint8Array): CasObjectId {
  return bytesToHex(sha256(bytes)) as CasObjectId;
}

function validatedDigestId<T extends string>(value: string, name: string): T {
  if (!/^[0-9a-f]{64}$/u.test(value))
    throw new TypeError(`${name} must be exactly 64 lowercase hexadecimal characters`);
  return value as T;
}

export function casObjectId(value: string): CasObjectId {
  return validatedDigestId<CasObjectId>(value, "CAS object id");
}

export function manifestId(value: string): ManifestId {
  return validatedDigestId<ManifestId>(value, "manifest id");
}

export function manifestIdFromHash(hash: Uint8Array): ManifestId {
  if (intrinsicByteLength(hash) !== 32)
    throw new RangeError("manifest hash must contain exactly 32 bytes");
  return manifestId(bytesToHex(hash));
}

export interface CasObject {
  readonly id: CasObjectId;
  readonly bytes: Uint8Array;
}

export function createCasObject(bytes: Uint8Array): CasObject {
  const owned = freezeBytes(bytes);
  const id = sha256Hex(owned);
  return Object.freeze({
    id,
    get bytes(): Uint8Array {
      return copyBytes(owned);
    },
  });
}

export function verifyCasObject(
  expectedDigest: Uint8Array | string,
  bytes: Uint8Array,
): void {
  if (typeof expectedDigest === "string")
    validatedDigestId<CasObjectId>(expectedDigest, "CAS object digest");
  else if (
    !(expectedDigest instanceof Uint8Array) ||
    intrinsicByteLength(expectedDigest) !== 32
  )
    throw new TypeError("CAS object digest must contain exactly 32 bytes");
  const actual = sha256(bytes);
  const valid =
    typeof expectedDigest === "string"
      ? bytesToHex(actual) === expectedDigest
      : equalBytes(actual, expectedDigest);
  if (!valid) throw new Error("CAS object digest mismatch");
}

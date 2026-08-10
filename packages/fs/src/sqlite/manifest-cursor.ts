import { copyBytes, intrinsicByteRange } from "../cas/bytes.js";
import { verifyCasObject } from "../cas/sha256.js";
import { ManifestSequentialCursor } from "../manifests/cursor.js";
import {
  decodeManifestRoot,
  validateSupportedManifestParameters,
} from "../manifests/codec.js";
import { checkedAdd } from "../resources/safe-integers.js";

export interface SQLiteAuthenticatedManifestEntry {
  readonly hash: Uint8Array;
  readonly length: number;
  readonly offset: number;
}

export interface SQLiteManifestContentSource {
  getObject(hash: Uint8Array, expectedSize?: number): Uint8Array | undefined;
  getManifestRoot(hash: Uint8Array): Uint8Array | undefined;
  getManifestNode(hash: Uint8Array): Uint8Array | undefined;
}

export class SQLiteAuthenticatedManifestCursor {
  readonly #source: SQLiteManifestContentSource;
  readonly #cursor: ManifestSequentialCursor;
  readonly fileSize: number;
  #position: number;

  constructor(
    source: SQLiteManifestContentSource,
    manifestHash: Uint8Array,
    offset: number,
    maxDepth: number,
  ) {
    manifestHash = copyBytes(manifestHash);
    const rootBytes = source.getManifestRoot(copyBytes(manifestHash));
    if (!rootBytes) throw new Error("ECORRUPT: missing manifest root");
    const root = decodeManifestRoot(rootBytes, manifestHash);
    validateSupportedManifestParameters(root.parameters);
    if (!Number.isSafeInteger(offset) || offset < 0)
      throw new RangeError("manifest offset must be a nonnegative safe integer");
    const effectiveOffset = Math.min(offset, root.fileSize);
    this.#source = source;
    this.#cursor = new ManifestSequentialCursor(
      rootBytes,
      effectiveOffset,
      { get: (hash) => source.getManifestNode(copyBytes(hash)) },
      manifestHash,
      maxDepth,
    );
    this.fileSize = root.fileSize;
    this.#position = effectiveOffset;
  }

  get position(): number {
    return this.#position;
  }

  peekEntry(): SQLiteAuthenticatedManifestEntry | null {
    const selected = this.#cursor.peek();
    return selected
      ? Object.freeze({
          hash: copyBytes(selected.entry.hash),
          length: selected.entry.length,
          offset: selected.offset,
        })
      : null;
  }

  nextEntry(): SQLiteAuthenticatedManifestEntry | null {
    const selected = this.#cursor.next();
    if (!selected) return null;
    this.#position = checkedAdd(selected.offset, selected.entry.length);
    return Object.freeze({
      hash: copyBytes(selected.entry.hash),
      length: selected.entry.length,
      offset: selected.offset,
    });
  }

  readInto(destination: Uint8Array, destinationOffset: number, length: number): number {
    destination = intrinsicByteRange(destination);
    if (
      !Number.isSafeInteger(destinationOffset) ||
      destinationOffset < 0 ||
      !Number.isSafeInteger(length) ||
      length < 0 ||
      destinationOffset + length > destination.byteLength
    )
      throw new RangeError("invalid authenticated manifest destination range");
    const available = Math.min(length, this.fileSize - this.#position);
    let written = 0;
    while (written < available) {
      const selected = this.#cursor.peek();
      if (!selected)
        throw new Error("ECORRUPT: manifest cursor ended before root totals");
      const entryEnd = checkedAdd(selected.offset, selected.entry.length);
      if (this.#position < selected.offset || this.#position >= entryEnd)
        throw new Error("ECORRUPT: manifest cursor position is outside its entry");
      const object = this.#source.getObject(
        copyBytes(selected.entry.hash),
        selected.entry.length,
      );
      if (!object) throw new Error("ECORRUPT: missing CAS object");
      const objectBytes = intrinsicByteRange(object);
      if (objectBytes.byteLength !== selected.entry.length)
        throw new Error("ECORRUPT: CAS object length disagrees with manifest");
      verifyCasObject(selected.entry.hash, objectBytes);
      const objectOffset = this.#position - selected.offset;
      const take = Math.min(available - written, selected.entry.length - objectOffset);
      destination.set(
        intrinsicByteRange(objectBytes, objectOffset, objectOffset + take),
        destinationOffset + written,
      );
      written += take;
      this.#position = checkedAdd(this.#position, take);
      if (this.#position === entryEnd) this.nextEntry();
    }
    return written;
  }
}

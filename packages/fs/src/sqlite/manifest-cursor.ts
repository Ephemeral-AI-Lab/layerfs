import { copyBytes, intrinsicByteLength, intrinsicByteRange } from "../cas/bytes.js";
import { ManifestSequentialCursor } from "../manifests/cursor.js";
import {
  decodeManifestRoot,
  validateSupportedManifestParameters,
} from "../manifests/codec.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";

export interface SQLiteAuthenticatedManifestEntry {
  readonly hash: Uint8Array;
  readonly length: number;
  readonly offset: number;
}

export interface SQLiteManifestContentSource {
  readObjectInto(
    hash: Uint8Array,
    expectedSize: number,
    sourceOffset: number,
    destination: Uint8Array,
    destinationOffset: number,
    length: number,
  ): boolean;
  batchFetchObjects(
    requests: readonly { readonly hash: Uint8Array; readonly expectedSize: number }[],
  ): void;
  withManifestRoot<T>(
    hash: Uint8Array,
    consume: (encoded: Uint8Array) => T,
  ): T | undefined;
  withManifestNode<T>(
    hash: Uint8Array,
    consume: (encoded: Uint8Array) => T,
  ): T | undefined;
  validatedManifestDepth(hash: Uint8Array): number | undefined;
  reserveManifestCursor(bytes: number): () => void;
}

export class SQLiteAuthenticatedManifestCursor {
  #source: SQLiteManifestContentSource;
  readonly #cursor: ManifestSequentialCursor;
  readonly fileSize: number;
  #release: (() => void) | undefined;
  #position: number;

  constructor(
    source: SQLiteManifestContentSource,
    manifestHash: Uint8Array,
    offset: number,
    maxDepth: number,
    maxObjectBytes: number,
    maxManifestNodeBytes = 16_384,
  ) {
    if (intrinsicByteLength(manifestHash) !== 32)
      throw new RangeError("manifest hash must contain exactly 32 bytes");
    manifestHash = copyBytes(manifestHash);
    if (!Number.isSafeInteger(offset) || offset < 0)
      throw new RangeError("manifest offset must be a nonnegative safe integer");
    const validatedDepth = source.validatedManifestDepth(manifestHash);
    if (validatedDepth === undefined)
      throw new Error("ECORRUPT: manifest lacks a durable validation certificate");
    const release = source.reserveManifestCursor(
      checkedMultiply(
        maxDepth,
        checkedAdd(
          checkedMultiply(maxManifestNodeBytes, 4, "decoded manifest node state"),
          4_096,
          "manifest cursor frame state",
        ),
        "cursor retained state",
      ),
    );
    this.#source = source;
    let initialized:
      | {
          readonly cursor: ManifestSequentialCursor;
          readonly fileSize: number;
          readonly effectiveOffset: number;
        }
      | undefined;
    try {
      initialized = source.withManifestRoot(copyBytes(manifestHash), (rootBytes) => {
        const root = decodeManifestRoot(rootBytes, manifestHash);
        validateSupportedManifestParameters(root.parameters);
        if (root.parameters.maximum > maxObjectBytes)
          throw new RangeError(
            "manifest FastCDC maximum exceeds the durable object transaction envelope",
          );
        const effectiveOffset = Math.min(offset, root.fileSize);
        return Object.freeze({
          cursor: new ManifestSequentialCursor(
            rootBytes,
            effectiveOffset,
            {
              withNode: <T>(hash: Uint8Array, consume: (encoded: Uint8Array) => T) =>
                this.#source.withManifestNode(copyBytes(hash), consume),
            },
            manifestHash,
            maxDepth,
            validatedDepth,
          ),
          fileSize: root.fileSize,
          effectiveOffset,
        });
      });
      if (!initialized) throw new Error("ECORRUPT: missing manifest root");
    } catch (error) {
      release();
      throw error;
    }
    this.#release = release;
    this.#cursor = initialized.cursor;
    this.fileSize = initialized.fileSize;
    this.#position = initialized.effectiveOffset;
  }

  get position(): number {
    return this.#position;
  }

  bindSource(source: SQLiteManifestContentSource): void {
    this.#assertOpen();
    this.#source = source;
  }

  close(): void {
    this.#release?.();
    this.#release = undefined;
  }

  peekEntry(): SQLiteAuthenticatedManifestEntry | null {
    this.#assertOpen();
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
    this.#assertOpen();
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
    this.#assertOpen();
    destination = intrinsicByteRange(destination);
    if (
      !Number.isSafeInteger(destinationOffset) ||
      destinationOffset < 0 ||
      !Number.isSafeInteger(length) ||
      length < 0 ||
      destinationOffset + length > intrinsicByteLength(destination)
    )
      throw new RangeError("invalid authenticated manifest destination range");
    const available = Math.min(length, this.fileSize - this.#position);
    const ops: {
      readonly hash: Uint8Array;
      readonly expectedSize: number;
      readonly sourceOffset: number;
      readonly take: number;
    }[] = [];
    let written = 0;
    while (written < available) {
      const selected = this.#cursor.peek();
      if (!selected)
        throw new Error("ECORRUPT: manifest cursor ended before root totals");
      const entryEnd = checkedAdd(selected.offset, selected.entry.length);
      if (this.#position < selected.offset || this.#position >= entryEnd)
        throw new Error("ECORRUPT: manifest cursor position is outside its entry");
      const objectOffset = this.#position - selected.offset;
      const take = Math.min(available - written, selected.entry.length - objectOffset);
      ops.push({
        hash: copyBytes(selected.entry.hash),
        expectedSize: selected.entry.length,
        sourceOffset: objectOffset,
        take,
      });
      written += take;
      this.#position = checkedAdd(this.#position, take);
      if (this.#position === entryEnd) this.nextEntry();
    }
    if (ops.length) this.#source.batchFetchObjects(ops);
    let copied = 0;
    for (const op of ops) {
      if (
        !this.#source.readObjectInto(
          op.hash,
          op.expectedSize,
          op.sourceOffset,
          destination,
          destinationOffset + copied,
          op.take,
        )
      )
        throw new Error("ECORRUPT: missing CAS object");
      copied += op.take;
    }
    return written;
  }

  #assertOpen(): void {
    if (!this.#release) throw new Error("manifest cursor is closed");
  }
}

import { sha256, verifyCasObject } from "../cas/sha256.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import {
  bytesToHex,
  equalBytes,
  intrinsicByteLength,
  intrinsicByteRange,
} from "../cas/bytes.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import {
  maxPersistedContentObjectBytes,
  type StorageLimits,
} from "../resources/limits.js";
import {
  ContentCache,
  type ContentCacheKind,
  type ContentCacheReservation,
} from "../cache/content-cache.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "./usage-repository.js";
import { SQLiteAuthenticatedManifestCursor } from "./manifest-cursor.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";
interface ObjectRow extends SqliteRow {
  hash?: Uint8Array;
  size: number;
  bytes?: Uint8Array;
}
interface SequenceRow extends SqliteRow {
  next_allocation_sequence: number;
}
interface EncodedRow extends SqliteRow {
  encoded: Uint8Array;
}
interface EncodedHashRow extends SqliteRow {
  hash: Uint8Array;
  encoded: Uint8Array;
}
export interface ContentObjectInput {
  readonly hash: Uint8Array;
  readonly bytes: Uint8Array;
}
export interface ContentBatchResult {
  readonly inserted: number;
  readonly deduplicated: number;
  readonly insertedBytes: number;
}

export class ContentRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  readonly #cache: ContentCache | undefined;
  constructor(
    tx: FilesystemSQLiteTransaction,
    limits: StorageLimits,
    cache?: ContentCache,
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#cache = cache;
  }

  putObject(hash: Uint8Array, bytes: Uint8Array): boolean {
    return this.putObjectsBatch([{ hash, bytes }]).inserted === 1;
  }

  putObjectsBatch(input: readonly ContentObjectInput[]): ContentBatchResult {
    if (input.length === 0)
      return Object.freeze({ inserted: 0, deduplicated: 0, insertedBytes: 0 });
    if (input.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("content batch exceeds configured row limit");
    const unique = new Map<string, ContentObjectInput>();
    const maxObjectBytes = maxPersistedContentObjectBytes(this.#limits);
    let preflightBytes = 0;
    for (const item of input) {
      const hashBytes = intrinsicByteLength(item.hash);
      const objectBytes = intrinsicByteLength(item.bytes);
      if (hashBytes !== 32 || objectBytes > maxObjectBytes)
        throw new RangeError("object exceeds configured limit");
      preflightBytes = checkedAdd(
        preflightBytes,
        checkedAdd(objectBytes, 256, "content row envelope"),
        "content batch envelope",
      );
      if (preflightBytes > this.#limits.maxFinalTransactionBytes)
        throw new RangeError("content batch exceeds transaction byte limit");
    }
    for (const item of input) {
      verifyCasObject(item.hash, item.bytes);
      const key = bytesToHex(item.hash);
      const previous = unique.get(key);
      if (previous && !equalBytes(previous.bytes, item.bytes))
        throw new Error("ECORRUPT: duplicate batch hash has different bytes");
      unique.set(key, item);
    }
    const values = [...unique.values()];
    const inputBytes = values.reduce(
      (sum, item) => sum + intrinsicByteLength(item.bytes),
      0,
    );
    if (inputBytes + values.length * 256 > this.#limits.maxFinalTransactionBytes)
      throw new RangeError("content batch exceeds transaction byte limit");
    const placeholders = values.map(() => "?").join(",");
    const existing = this.#tx.all<ObjectRow>(
      `SELECT hash,size FROM efs_cas_objects WHERE hash IN (${placeholders})`,
      values.map((item) => item.hash),
      { maxRows: values.length, maxBytes: Math.max(1024, values.length * 96) },
    );
    const byHash = new Map(existing.map((row) => [bytesToHex(row.hash!), row]));
    const insert: ContentObjectInput[] = [];
    const uncached: ContentObjectInput[] = [];
    for (const item of values) {
      const row = byHash.get(bytesToHex(item.hash));
      if (!row) {
        insert.push(item);
        continue;
      }
      if (row.size !== intrinsicByteLength(item.bytes))
        throw new Error("ECORRUPT: CAS collision or stored size mismatch");
      const cached = this.#cache?.get("object", item.hash);
      if (cached) {
        if (!equalBytes(cached, item.bytes))
          throw new Error("ECORRUPT: cached CAS collision");
      } else uncached.push(item);
    }
    if (uncached.length) {
      const missingPlaceholders = uncached.map(() => "?").join(",");
      const expectedBytes = uncached.reduce(
        (sum, item) => sum + intrinsicByteLength(item.bytes) + 128,
        0,
      );
      const stored = this.#tx.all<ObjectRow>(
        `SELECT hash,size,bytes FROM efs_cas_objects WHERE hash IN (${missingPlaceholders})`,
        uncached.map((item) => item.hash),
        { maxRows: uncached.length, maxBytes: Math.max(1024, expectedBytes) },
      );
      const storedByHash = new Map(stored.map((row) => [bytesToHex(row.hash!), row]));
      for (const item of uncached) {
        const row = storedByHash.get(bytesToHex(item.hash));
        if (
          !row?.bytes ||
          row.size !== intrinsicByteLength(item.bytes) ||
          !equalBytes(row.bytes, item.bytes)
        )
          throw new Error("ECORRUPT: CAS collision or stored payload mismatch");
        verifyCasObject(item.hash, row.bytes);
        const reservation = this.#cache?.tryReserve(
          checkedAdd(intrinsicByteLength(row.bytes), 96),
        );
        this.#admitCache("object", item.hash, row.bytes, reservation);
      }
    }
    const insertedBytes = insert.reduce(
      (sum, item) => sum + intrinsicByteLength(item.bytes),
      0,
    );
    if (insert.length) {
      this.#admit("object_bytes", insertedBytes, "object_count", insert.length);
      const sequence = this.#allocateSequenceRange(insert.length);
      for (let index = 0; index < insert.length; index += 1) {
        const item = insert[index]!;
        this.#tx.run(
          "INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)",
          [item.hash, intrinsicByteLength(item.bytes), item.bytes, sequence + index],
        );
      }
    }
    return Object.freeze({
      inserted: insert.length,
      deduplicated: values.length - insert.length,
      insertedBytes,
    });
  }

  readObjectInto(
    hash: Uint8Array,
    expectedSize: number,
    sourceOffset: number,
    destination: Uint8Array,
    destinationOffset: number,
    length: number,
  ): boolean {
    if (!this.#cache)
      throw new Error("content reads require operation-scoped admission");
    hash = intrinsicByteRange(hash);
    destination = intrinsicByteRange(destination);
    if (intrinsicByteLength(hash) !== 32)
      throw new RangeError("content hash must contain exactly 32 bytes");
    if (
      !Number.isSafeInteger(expectedSize) ||
      expectedSize < 0 ||
      !Number.isSafeInteger(sourceOffset) ||
      sourceOffset < 0 ||
      !Number.isSafeInteger(destinationOffset) ||
      destinationOffset < 0 ||
      !Number.isSafeInteger(length) ||
      length < 0 ||
      checkedAdd(sourceOffset, length) > expectedSize ||
      checkedAdd(destinationOffset, length) > intrinsicByteLength(destination)
    )
      throw new RangeError("invalid content object read range");
    const cached = this.#cache.copyInto(
      "object",
      hash,
      expectedSize,
      sourceOffset,
      destination,
      destinationOffset,
      length,
    );
    if (cached) return true;
    const size = this.#objectSize(hash);
    if (size === undefined) return false;
    if (size !== expectedSize)
      throw new Error("ECORRUPT: stored CAS length disagrees with manifest");
    return this.#withColdObject(hash, size, (bytes) => {
      destination.set(
        intrinsicByteRange(bytes, sourceOffset, sourceOffset + length),
        destinationOffset,
      );
    });
  }

  verifyObject(hash: Uint8Array, expectedSize?: number, forceStorage = false): boolean {
    if (!this.#cache)
      throw new Error("content reads require operation-scoped admission");
    hash = intrinsicByteRange(hash);
    if (intrinsicByteLength(hash) !== 32)
      throw new RangeError("content hash must contain exactly 32 bytes");
    const size = expectedSize ?? this.#objectSize(hash);
    if (size === undefined) return false;
    if (!Number.isSafeInteger(size) || size < 0)
      throw new Error("ECORRUPT: invalid stored CAS size");
    if (!forceStorage) {
      const cached = this.#cache.containsExact("object", hash, size);
      if (cached) return true;
    }
    if (expectedSize !== undefined) {
      const storedSize = this.#objectSize(hash);
      if (storedSize === undefined) return false;
      if (storedSize !== expectedSize)
        throw new Error("ECORRUPT: stored CAS length mismatch");
    }
    return this.#withColdObject(hash, size, () => {});
  }

  #objectSize(hash: Uint8Array): number | undefined {
    const row = this.#tx.all<ObjectRow>(
      "SELECT size FROM efs_cas_objects WHERE hash=?",
      [hash],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row) return undefined;
    if (!Number.isSafeInteger(row.size) || row.size < 0)
      throw new Error("ECORRUPT: invalid stored CAS size");
    return row.size;
  }

  #withColdObject(
    hash: Uint8Array,
    size: number,
    consume: (bytes: Uint8Array) => void,
  ): boolean {
    const cache = this.#cache!;
    const transientBytes = checkedAdd(
      checkedMultiply(size, 2, "driver BLOB ownership copies"),
      128,
      "content read transient bytes",
    );
    const releaseRead = cache.reserveOperation(transientBytes);
    let reservation: ContentCacheReservation | undefined;
    try {
      const row = this.#tx.all<ObjectRow>(
        "SELECT size,bytes FROM efs_cas_objects WHERE hash=?",
        [hash],
        { maxRows: 1, maxBytes: checkedAdd(size, 128) },
      )[0];
      if (!row) return false;
      if (!row.bytes || row.size !== size || intrinsicByteLength(row.bytes) !== size)
        throw new Error("ECORRUPT: stored CAS length mismatch");
      verifyCasObject(hash, row.bytes);
      consume(row.bytes);
      reservation = cache.tryReserve(checkedAdd(size, 96));
      this.#admitCache("object", hash, row.bytes, reservation);
      reservation = undefined;
      return true;
    } catch (error) {
      reservation?.release();
      throw error;
    } finally {
      releaseRead();
    }
  }

  putManifestNode(hash: Uint8Array, encoded: Uint8Array): boolean {
    return this.putManifestNodesBatch([{ hash, encoded }]).inserted === 1;
  }

  putManifestNodesBatch(
    nodes: readonly { readonly hash: Uint8Array; readonly encoded: Uint8Array }[],
  ): ContentBatchResult {
    if (!nodes.length)
      return Object.freeze({ inserted: 0, deduplicated: 0, insertedBytes: 0 });
    if (nodes.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("manifest batch exceeds configured row limit");
    const unique = new Map<
      string,
      {
        readonly hash: Uint8Array;
        readonly encoded: Uint8Array;
        readonly decoded: ReturnType<typeof decodeManifestNode>;
      }
    >();
    let preflightBytes = 0;
    for (const node of nodes) {
      const hashBytes = intrinsicByteLength(node.hash);
      const encodedBytes = intrinsicByteLength(node.encoded);
      if (hashBytes !== 32 || encodedBytes > this.#limits.maxManifestNodeBytes)
        throw new Error("invalid manifest node digest or size");
      preflightBytes = checkedAdd(
        preflightBytes,
        checkedAdd(encodedBytes, 256, "manifest row envelope"),
        "manifest batch envelope",
      );
      if (preflightBytes > this.#limits.maxFinalTransactionBytes)
        throw new RangeError("manifest batch exceeds transaction byte limit");
    }
    for (const node of nodes) {
      if (!equalBytes(sha256(node.encoded), node.hash))
        throw new Error("invalid manifest node digest or size");
      const key = bytesToHex(node.hash);
      const previous = unique.get(key);
      if (previous && !equalBytes(previous.encoded, node.encoded))
        throw new Error("ECORRUPT: duplicate manifest hash has different bytes");
      unique.set(key, {
        ...node,
        decoded: decodeManifestNode(node.encoded, node.hash),
      });
    }
    const values = [...unique.values()];
    const placeholders = values.map(() => "?").join(",");
    const existing = this.#tx.all<EncodedHashRow>(
      `SELECT hash,encoded FROM efs_manifest_nodes WHERE hash IN (${placeholders})`,
      values.map((node) => node.hash),
      {
        maxRows: values.length,
        maxBytes: Math.max(
          1024,
          values.reduce((sum, node) => sum + intrinsicByteLength(node.encoded) + 96, 0),
        ),
      },
    );
    const byHash = new Map(existing.map((row) => [bytesToHex(row.hash), row.encoded]));
    const insert = values.filter((node) => {
      const prior = byHash.get(bytesToHex(node.hash));
      if (prior && !equalBytes(prior, node.encoded))
        throw new Error("ECORRUPT: manifest node collision");
      return !prior;
    });
    const insertedBytes = insert.reduce(
      (sum, node) => sum + intrinsicByteLength(node.encoded),
      0,
    );
    if (insertedBytes > this.#limits.maxFinalTransactionBytes)
      throw new RangeError("manifest batch exceeds transaction byte limit");
    if (insert.length) {
      this.#admit(
        "manifest_node_bytes",
        insertedBytes,
        "manifest_node_count",
        insert.length,
      );
      const sequence = this.#allocateSequenceRange(insert.length);
      for (let index = 0; index < insert.length; index += 1) {
        const node = insert[index]!;
        this.#tx.run(
          "INSERT INTO efs_manifest_nodes(hash,kind,logical_bytes,entry_count,encoded,allocation_sequence) VALUES(?,?,?,?,?,?)",
          [
            node.hash,
            node.decoded.kind === "leaf" ? 0 : 1,
            node.decoded.span,
            node.decoded.entryCount,
            node.encoded,
            sequence + index,
          ],
        );
      }
    }
    return Object.freeze({
      inserted: insert.length,
      deduplicated: values.length - insert.length,
      insertedBytes,
    });
  }

  putManifestRoot(hash: Uint8Array, encoded: Uint8Array): boolean {
    if (
      intrinsicByteLength(encoded) > this.#limits.maxManifestNodeBytes ||
      !equalBytes(sha256(encoded), hash)
    )
      throw new Error("invalid manifest root digest or size");
    const root = decodeManifestRoot(encoded, hash);
    if (root.parameters.maximum > maxPersistedContentObjectBytes(this.#limits))
      throw new RangeError(
        "manifest FastCDC maximum exceeds the durable object transaction envelope",
      );
    const existing = this.#tx.all<EncodedRow>(
      "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
      [hash],
      { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 128 },
    )[0];
    if (existing) {
      if (!equalBytes(existing.encoded, encoded))
        throw new Error("ECORRUPT: manifest root collision");
      return false;
    }
    this.#admit(
      "manifest_root_bytes",
      intrinsicByteLength(encoded),
      "manifest_root_count",
      1,
    );
    this.#tx.run(
      "INSERT INTO efs_manifest_roots(hash,root_node_hash,file_size,entry_count,chunk_min,chunk_avg,chunk_max,encoded,allocation_sequence) VALUES(?,?,?,?,?,?,?,?,?)",
      [
        hash,
        root.rootNodeHash,
        root.fileSize,
        root.entryCount,
        root.parameters.minimum,
        root.parameters.average,
        root.parameters.maximum,
        encoded,
        this.#allocateSequenceRange(1),
      ],
    );
    return true;
  }

  getManifestRoot(hash: Uint8Array): Uint8Array | undefined {
    return this.#getEncoded("manifest-root", "efs_manifest_roots", hash);
  }
  getManifestNode(hash: Uint8Array): Uint8Array | undefined {
    return this.#getEncoded("manifest-node", "efs_manifest_nodes", hash);
  }
  openManifestCursor(
    manifestHash: Uint8Array,
    offset: number,
  ): SQLiteAuthenticatedManifestCursor {
    return new SQLiteAuthenticatedManifestCursor(
      this,
      manifestHash,
      offset,
      this.#limits.maxManifestDepth,
      maxPersistedContentObjectBytes(this.#limits),
    );
  }

  #getEncoded(
    kind: ContentCacheKind,
    table: "efs_manifest_roots" | "efs_manifest_nodes",
    hash: Uint8Array,
  ): Uint8Array | undefined {
    const cached = this.#cache?.get(kind, hash);
    if (cached) return cached;
    const reservation = this.#cache?.reserve(this.#limits.maxManifestNodeBytes + 96);
    try {
      const encoded = this.#tx.all<EncodedRow>(
        `SELECT encoded FROM ${table} WHERE hash=?`,
        [hash],
        { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 128 },
      )[0]?.encoded;
      if (!encoded) {
        reservation?.release();
        return undefined;
      }
      this.#admitCache(kind, hash, encoded, reservation);
      return encoded;
    } catch (error) {
      reservation?.release();
      throw error;
    }
  }
  #admitCache(
    kind: ContentCacheKind,
    hash: Uint8Array,
    bytes: Uint8Array,
    reservation: ContentCacheReservation | undefined,
  ): void {
    if (this.#cache && reservation) this.#cache.admit(kind, hash, bytes, reservation);
  }
  #allocateSequenceRange(count: number): number {
    const row = this.#tx.all<SequenceRow>(
      "SELECT next_allocation_sequence FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row || !Number.isSafeInteger(row.next_allocation_sequence))
      throw new Error("ECORRUPT: invalid allocation sequence");
    this.#tx.run(
      "UPDATE efs_meta SET next_allocation_sequence=next_allocation_sequence+? WHERE singleton=1",
      [count],
    );
    return row.next_allocation_sequence;
  }
  #admit(
    byteColumn: "object_bytes" | "manifest_root_bytes" | "manifest_node_bytes",
    bytes: number,
    countColumn: "object_count" | "manifest_root_count" | "manifest_node_count",
    count: number,
  ): void {
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        [byteColumn]: bytes,
        [countColumn]: count,
        charged_metadata_bytes: count * CHARGED_ROW_BYTES,
      },
      "durable content",
    );
  }
}

import { sha256, verifyCasObject } from "../cas/sha256.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import { equalBytes } from "../utils/bytes.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "../sqlite-driver.js";
import type { StorageLimits } from "../resources/limits.js";

interface UsageRow extends SqliteRow { object_count: number; object_bytes: number; manifest_root_count: number; manifest_root_bytes: number; manifest_node_count: number; manifest_node_bytes: number; charged_metadata_bytes: number }
interface ObjectRow extends SqliteRow { size: number; bytes: Uint8Array }
interface SequenceRow extends SqliteRow { next_allocation_sequence: number }
interface EncodedRow extends SqliteRow { encoded: Uint8Array }

export class ContentRepository {
  readonly #tx: FilesystemSQLiteTransaction; readonly #limits: StorageLimits;
  constructor(tx: FilesystemSQLiteTransaction, limits: StorageLimits) { this.#tx = tx; this.#limits = limits; }

  putObject(hash: Uint8Array, bytes: Uint8Array): boolean {
    if (hash.byteLength !== 32 || bytes.byteLength > this.#limits.maxWriteBytes) throw new RangeError("object exceeds configured limit");
    verifyCasObject(hash, bytes);
    const existing = this.#tx.all<ObjectRow>("SELECT size,bytes FROM efs_cas_objects WHERE hash=?", [hash], { maxRows: 1, maxBytes: Math.max(1024, bytes.byteLength + 128) })[0];
    if (existing) {
      if (existing.size !== bytes.byteLength || !equalBytes(existing.bytes, bytes)) throw new Error("ECORRUPT: CAS collision or stored payload mismatch");
      return false;
    }
    this.#admit("object_bytes", bytes.byteLength, "object_count", 1);
    const sequence = this.#allocateSequence();
    this.#tx.run("INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)", [hash, bytes.byteLength, bytes.slice(), sequence]);
    return true;
  }

  getObject(hash: Uint8Array): Uint8Array | undefined {
    const row = this.#tx.all<ObjectRow>("SELECT size,bytes FROM efs_cas_objects WHERE hash=?", [hash], { maxRows: 1, maxBytes: this.#limits.maxWriteBytes + 128 })[0];
    if (!row) return undefined;
    if (row.size !== row.bytes.byteLength) throw new Error("ECORRUPT: stored CAS length mismatch");
    verifyCasObject(hash, row.bytes); return row.bytes;
  }

  putManifestNode(hash: Uint8Array, encoded: Uint8Array): boolean {
    if (encoded.byteLength > this.#limits.maxManifestNodeBytes || !equalBytes(sha256(encoded), hash)) throw new Error("invalid manifest node digest or size");
    const node = decodeManifestNode(encoded, hash);
    const existing = this.#tx.all<EncodedRow>("SELECT encoded FROM efs_manifest_nodes WHERE hash=?", [hash], { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 128 })[0];
    if (existing) { if (!equalBytes(existing.encoded, encoded)) throw new Error("ECORRUPT: manifest node collision"); return false; }
    this.#admit("manifest_node_bytes", encoded.byteLength, "manifest_node_count", 1);
    this.#tx.run("INSERT INTO efs_manifest_nodes(hash,kind,logical_bytes,entry_count,encoded,allocation_sequence) VALUES(?,?,?,?,?,?)", [hash, node.kind === "leaf" ? 0 : 1, node.span, node.entryCount, encoded, this.#allocateSequence()]);
    return true;
  }

  putManifestRoot(hash: Uint8Array, encoded: Uint8Array): boolean {
    if (encoded.byteLength > this.#limits.maxManifestNodeBytes || !equalBytes(sha256(encoded), hash)) throw new Error("invalid manifest root digest or size");
    const root = decodeManifestRoot(encoded, hash);
    const existing = this.#tx.all<EncodedRow>("SELECT encoded FROM efs_manifest_roots WHERE hash=?", [hash], { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 128 })[0];
    if (existing) { if (!equalBytes(existing.encoded, encoded)) throw new Error("ECORRUPT: manifest root collision"); return false; }
    this.#admit("manifest_root_bytes", encoded.byteLength, "manifest_root_count", 1);
    this.#tx.run("INSERT INTO efs_manifest_roots(hash,root_node_hash,file_size,entry_count,chunk_min,chunk_avg,chunk_max,encoded,allocation_sequence) VALUES(?,?,?,?,?,?,?,?,?)", [hash, root.rootNodeHash, root.fileSize, root.entryCount, root.parameters.minimum, root.parameters.average, root.parameters.maximum, encoded, this.#allocateSequence()]);
    return true;
  }

  getManifestRoot(hash: Uint8Array): Uint8Array | undefined { return this.#getEncoded("efs_manifest_roots", hash); }
  getManifestNode(hash: Uint8Array): Uint8Array | undefined { return this.#getEncoded("efs_manifest_nodes", hash); }

  #getEncoded(table: "efs_manifest_roots" | "efs_manifest_nodes", hash: Uint8Array): Uint8Array | undefined {
    return this.#tx.all<EncodedRow>(`SELECT encoded FROM ${table} WHERE hash=?`, [hash], { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 128 })[0]?.encoded;
  }
  #allocateSequence(): number {
    const row = this.#tx.all<SequenceRow>("SELECT next_allocation_sequence FROM efs_meta WHERE singleton=1", [], { maxRows: 1, maxBytes: 1024 })[0];
    if (!row || !Number.isSafeInteger(row.next_allocation_sequence)) throw new Error("ECORRUPT: invalid allocation sequence");
    this.#tx.run("UPDATE efs_meta SET next_allocation_sequence=next_allocation_sequence+1 WHERE singleton=1"); return row.next_allocation_sequence;
  }
  #admit(byteColumn: "object_bytes" | "manifest_root_bytes" | "manifest_node_bytes", bytes: number, countColumn: "object_count" | "manifest_root_count" | "manifest_node_count", count: number): void {
    const usage = this.#tx.all<UsageRow>("SELECT object_count,object_bytes,manifest_root_count,manifest_root_bytes,manifest_node_count,manifest_node_bytes,charged_metadata_bytes FROM efs_usage WHERE singleton=1", [], { maxRows: 1, maxBytes: 2048 })[0];
    if (!usage) throw new Error("ECORRUPT: missing usage singleton");
    const managed = usage.object_bytes + usage.manifest_root_bytes + usage.manifest_node_bytes;
    if (managed + bytes > this.#limits.maxManagedPayloadBytes - this.#limits.maintenanceReserveBytes || usage.charged_metadata_bytes + 96 > this.#limits.maxChargedMetadataBytes) throw new Error("ENOSPC: durable content quota exceeded");
    const allowedBytes = new Set(["object_bytes", "manifest_root_bytes", "manifest_node_bytes"]); const allowedCounts = new Set(["object_count", "manifest_root_count", "manifest_node_count"]);
    if (!allowedBytes.has(byteColumn) || !allowedCounts.has(countColumn)) throw new Error("invalid usage column");
    this.#tx.run(`UPDATE efs_usage SET ${byteColumn}=${byteColumn}+?,${countColumn}=${countColumn}+?,charged_metadata_bytes=charged_metadata_bytes+96 WHERE singleton=1`, [bytes, count]);
  }
}

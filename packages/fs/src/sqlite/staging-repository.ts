import { equalBytes } from "../utils/bytes.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "../sqlite-driver.js";

export interface ClosureCertificate { readonly leaseId: string; readonly manifestHash: Uint8Array; readonly chainDigest: Uint8Array; readonly objectCount: number; readonly objectBytes: number; readonly nodeCount: number; readonly nodeBytes: number }
interface CertificateRow extends SqliteRow { manifest_hash: Uint8Array; chain_digest: Uint8Array; object_count: number; object_bytes: number; node_count: number; node_bytes: number; sealed: number }

export class StagingRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  constructor(tx: FilesystemSQLiteTransaction) { this.#tx = tx; }
  seal(certificate: ClosureCertificate): void {
    if (certificate.manifestHash.byteLength !== 32 || certificate.chainDigest.byteLength !== 32) throw new RangeError("closure certificate hashes must be 32 bytes");
    for (const value of [certificate.objectCount, certificate.objectBytes, certificate.nodeCount, certificate.nodeBytes]) if (!Number.isSafeInteger(value) || value < 0) throw new RangeError("invalid closure certificate counter");
    this.#tx.run("INSERT INTO efs_staging_certificates(lease_id,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,sealed) VALUES(?,?,?,?,?,?,?,1) ON CONFLICT(lease_id) DO UPDATE SET manifest_hash=excluded.manifest_hash,chain_digest=excluded.chain_digest,object_count=excluded.object_count,object_bytes=excluded.object_bytes,node_count=excluded.node_count,node_bytes=excluded.node_bytes,sealed=1", [certificate.leaseId, certificate.manifestHash, certificate.chainDigest, certificate.objectCount, certificate.objectBytes, certificate.nodeCount, certificate.nodeBytes]);
  }
  validateSealed(certificate: ClosureCertificate): void {
    const row = this.#tx.all<CertificateRow>("SELECT manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,sealed FROM efs_staging_certificates WHERE lease_id=?", [certificate.leaseId], { maxRows: 1, maxBytes: 2048 })[0];
    if (!row || row.sealed !== 1 || !equalBytes(row.manifest_hash, certificate.manifestHash) || !equalBytes(row.chain_digest, certificate.chainDigest) || row.object_count !== certificate.objectCount || row.object_bytes !== certificate.objectBytes || row.node_count !== certificate.nodeCount || row.node_bytes !== certificate.nodeBytes) throw new Error("ECORRUPT: staged closure certificate mismatch");
  }
}


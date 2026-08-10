import { sha256 } from "../cas/sha256.js";
import { concatBytes, equalBytes, utf8 } from "../utils/bytes.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "../sqlite-driver.js";

export type StagingMemberKind = "object" | "manifest-root" | "manifest-node";
export interface StagingMember { readonly kind: StagingMemberKind; readonly hash: Uint8Array; readonly size: number }
export interface ClosureCertificate {
  readonly leaseId: string;
  readonly ownerNonce: Uint8Array;
  readonly manifestHash: Uint8Array;
  readonly chainDigest: Uint8Array;
  readonly objectCount: number;
  readonly objectBytes: number;
  readonly nodeCount: number;
  readonly nodeBytes: number;
  readonly membershipCount: number;
}
interface CertificateRow extends SqliteRow { owner_nonce: Uint8Array; manifest_hash: Uint8Array | null; chain_digest: Uint8Array; object_count: number; object_bytes: number; node_count: number; node_bytes: number; membership_count: number; next_sequence: number; sealed: number; verified: number; expires_at_ms?: number; state?: number; lease_nonce?: Uint8Array; rooted?: number }

export const EMPTY_STAGING_CHAIN = sha256(utf8("efs-staging-chain-v1"));

function memberKind(kind: StagingMemberKind): number { return kind === "object" ? 0 : kind === "manifest-root" ? 1 : 2; }
function extendChain(previous: Uint8Array, sequence: number, member: StagingMember): Uint8Array {
  const encoded = new Uint8Array(49);
  const view = new DataView(encoded.buffer);
  encoded[0] = memberKind(member.kind); encoded.set(member.hash, 1);
  view.setBigUint64(33, BigInt(sequence), true); view.setBigUint64(41, BigInt(member.size), true);
  return sha256(concatBytes([previous, encoded]));
}
function counters(values: readonly number[]): void { for (const value of values) if (!Number.isSafeInteger(value) || value < 0) throw new RangeError("invalid closure certificate counter"); }

export class StagingRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  constructor(tx: FilesystemSQLiteTransaction) { this.#tx = tx; }

  begin(options: { readonly leaseId: string; readonly ownerId: string; readonly ownerNonce: Uint8Array; readonly now: number; readonly expiresAt: number; readonly kind?: number; readonly branchId?: string; readonly generation?: number }): void {
    if (!options.leaseId || !options.ownerId || options.ownerNonce.byteLength < 16) throw new RangeError("staging lease identity or owner nonce is invalid");
    counters([options.now, options.expiresAt]); if (options.expiresAt <= options.now) throw new RangeError("staging lease expiry must be in the future");
    this.#tx.run("INSERT INTO efs_leases(id,kind,owner_id,owner_nonce,branch_id,generation,created_at_ms,last_renewal_at_ms,expires_at_ms,state) VALUES(?,?,?,?,?,?,?,?,?,0)", [options.leaseId, options.kind ?? 1, options.ownerId, options.ownerNonce, options.branchId ?? null, options.generation ?? null, options.now, options.now, options.expiresAt]);
    this.#tx.run("INSERT INTO efs_staging_certificates(lease_id,owner_nonce,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified) VALUES(?,?,NULL,?,0,0,0,0,0,0,0,0)", [options.leaseId, options.ownerNonce, EMPTY_STAGING_CHAIN]);
  }

  appendBatch(leaseId: string, ownerNonce: Uint8Array, members: readonly StagingMember[]): ClosureCertificate {
    if (members.length === 0) return this.snapshot(leaseId, ownerNonce);
    const row = this.#row(leaseId);
    if (!equalBytes(row.owner_nonce, ownerNonce) || row.sealed !== 0) throw new Error("ECORRUPT: staging owner mismatch or certificate already sealed");
    const seen = new Set<string>(); let chain = row.chain_digest; let sequence = row.next_sequence;
    let objectCount = row.object_count; let objectBytes = row.object_bytes; let nodeCount = row.node_count; let nodeBytes = row.node_bytes;
    for (const member of members) {
      if (member.hash.byteLength !== 32) throw new RangeError("staging member hash must be 32 bytes"); counters([member.size]);
      const key = `${member.kind}:${Array.from(member.hash, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
      if (seen.has(key)) throw new Error("duplicate staging member in one batch"); seen.add(key);
      if (member.kind === "object") {
        const inserted = this.#tx.run("INSERT OR IGNORE INTO efs_lease_objects(lease_id,object_hash,sequence,size) VALUES(?,?,?,?)", [leaseId, member.hash, sequence, member.size]);
        if (!inserted.changes) continue; objectCount += 1; objectBytes += member.size;
      } else {
        const inserted = this.#tx.run("INSERT OR IGNORE INTO efs_lease_staged_manifests(lease_id,kind,manifest_hash,sequence,size) VALUES(?,?,?,?,?)", [leaseId, member.kind === "manifest-root" ? 0 : 1, member.hash, sequence, member.size]);
        if (!inserted.changes) continue; nodeCount += 1; nodeBytes += member.size;
      }
      chain = extendChain(chain, sequence, member); sequence += 1;
    }
    this.#tx.run("UPDATE efs_staging_certificates SET chain_digest=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=? WHERE lease_id=? AND sealed=0", [chain, objectCount, objectBytes, nodeCount, nodeBytes, sequence, sequence, leaseId]);
    return Object.freeze({ leaseId, ownerNonce: ownerNonce.slice(), manifestHash: new Uint8Array(32), chainDigest: chain, objectCount, objectBytes, nodeCount, nodeBytes, membershipCount: sequence });
  }

  snapshot(leaseId: string, ownerNonce: Uint8Array): ClosureCertificate {
    const row = this.#row(leaseId); if (!equalBytes(row.owner_nonce, ownerNonce)) throw new Error("ECORRUPT: staging owner mismatch");
    return Object.freeze({ leaseId, ownerNonce: ownerNonce.slice(), manifestHash: row.manifest_hash?.slice() ?? new Uint8Array(32), chainDigest: row.chain_digest, objectCount: row.object_count, objectBytes: row.object_bytes, nodeCount: row.node_count, nodeBytes: row.node_bytes, membershipCount: row.membership_count });
  }

  seal(certificate: ClosureCertificate): void {
    this.#validateShape(certificate);
    const row = this.#row(certificate.leaseId);
    if (!equalBytes(row.owner_nonce, certificate.ownerNonce) || !equalBytes(row.chain_digest, certificate.chainDigest) || row.object_count !== certificate.objectCount || row.object_bytes !== certificate.objectBytes || row.node_count !== certificate.nodeCount || row.node_bytes !== certificate.nodeBytes || row.membership_count !== certificate.membershipCount || row.next_sequence !== certificate.membershipCount || row.sealed !== 0) throw new Error("ECORRUPT: staged closure certificate mismatch");
    const root = this.#tx.all("SELECT hash FROM efs_manifest_roots WHERE hash=?", [certificate.manifestHash], { maxRows: 1, maxBytes: 128 })[0];
    const membership = this.#tx.all("SELECT manifest_hash FROM efs_lease_staged_manifests WHERE lease_id=? AND kind=0 AND manifest_hash=?", [certificate.leaseId, certificate.manifestHash], { maxRows: 1, maxBytes: 128 })[0];
    if (!root || !membership) throw new Error("ECORRUPT: staged root is absent from the verified membership");
    this.#tx.run("INSERT INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)", [certificate.leaseId, certificate.manifestHash]);
    this.#tx.run("UPDATE efs_staging_certificates SET manifest_hash=?,sealed=1,verified=1 WHERE lease_id=? AND sealed=0", [certificate.manifestHash, certificate.leaseId]);
    this.#tx.run("UPDATE efs_leases SET state=1 WHERE id=? AND state=0", [certificate.leaseId]);
  }

  validateSealed(certificate: ClosureCertificate, now = 0): void {
    this.#validateShape(certificate); counters([now]);
    const row = this.#tx.all<CertificateRow>("SELECT c.owner_nonce,c.manifest_hash,c.chain_digest,c.object_count,c.object_bytes,c.node_count,c.node_bytes,c.membership_count,c.next_sequence,c.sealed,c.verified,l.owner_nonce lease_nonce,l.expires_at_ms,l.state,CASE WHEN m.manifest_hash IS NULL THEN 0 ELSE 1 END rooted FROM efs_staging_certificates c JOIN efs_leases l ON l.id=c.lease_id LEFT JOIN efs_lease_manifests m ON m.lease_id=c.lease_id AND m.manifest_hash=c.manifest_hash WHERE c.lease_id=?", [certificate.leaseId], { maxRows: 1, maxBytes: 4096 })[0];
    if (!row || row.sealed !== 1 || row.verified !== 1 || row.state !== 1 || row.rooted !== 1 || row.expires_at_ms! < now || !equalBytes(row.owner_nonce, certificate.ownerNonce) || !equalBytes(row.lease_nonce!, certificate.ownerNonce) || !row.manifest_hash || !equalBytes(row.manifest_hash, certificate.manifestHash) || !equalBytes(row.chain_digest, certificate.chainDigest) || row.object_count !== certificate.objectCount || row.object_bytes !== certificate.objectBytes || row.node_count !== certificate.nodeCount || row.node_bytes !== certificate.nodeBytes || row.membership_count !== certificate.membershipCount || row.next_sequence !== certificate.membershipCount) throw new Error("ECORRUPT: sealed closure certificate mismatch");
  }

  #row(leaseId: string): CertificateRow {
    const row = this.#tx.all<CertificateRow>("SELECT owner_nonce,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified FROM efs_staging_certificates WHERE lease_id=?", [leaseId], { maxRows: 1, maxBytes: 4096 })[0];
    if (!row) throw new Error("ECORRUPT: staging certificate is missing"); return row;
  }
  #validateShape(certificate: ClosureCertificate): void {
    if (certificate.ownerNonce.byteLength < 16 || certificate.manifestHash.byteLength !== 32 || certificate.chainDigest.byteLength !== 32) throw new RangeError("closure certificate hashes or owner nonce are invalid");
    counters([certificate.objectCount, certificate.objectBytes, certificate.nodeCount, certificate.nodeBytes, certificate.membershipCount]);
    if (certificate.membershipCount !== certificate.objectCount + certificate.nodeCount) throw new RangeError("closure certificate membership count mismatch");
  }
}

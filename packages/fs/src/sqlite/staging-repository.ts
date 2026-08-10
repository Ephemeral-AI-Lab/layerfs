import { sha256 } from "../cas/sha256.js";
import { equalBytes } from "../cas/bytes.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import { checkedAdd } from "../resources/safe-integers.js";
import type { StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "./usage-repository.js";

export type StagingMemberKind = "object" | "manifest-root" | "manifest-node";
export interface StagingMember {
  readonly kind: StagingMemberKind;
  readonly hash: Uint8Array;
  readonly size: number;
}
export interface StagingEntryRow extends SqliteRow {
  entry_index: number;
  object_hash: Uint8Array;
  length: number;
}
export interface StagingLevelRow extends SqliteRow {
  record_index: number;
  node_hash: Uint8Array;
  span: number;
  entry_count: number;
}
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
interface CertificateRow extends SqliteRow {
  owner_nonce: Uint8Array;
  manifest_hash: Uint8Array | null;
  chain_digest: Uint8Array;
  object_count: number;
  object_bytes: number;
  node_count: number;
  node_bytes: number;
  membership_count: number;
  next_sequence: number;
  sealed: number;
  verified: number;
  expires_at_ms?: number;
  state?: number;
  lease_nonce?: Uint8Array;
  rooted?: number;
}
interface ReconciliationRow extends SqliteRow {
  owner_nonce: Uint8Array;
  manifest_hash: Uint8Array;
  next_sequence: number;
  object_count: number;
  object_bytes: number;
  node_count: number;
  node_bytes: number;
  membership_count: number;
  complete: number;
}
interface QueueRow extends SqliteRow {
  kind: number;
  hash: Uint8Array;
  sequence: number;
  declared_size: number;
  declared_span: number | null;
  declared_entry_count: number | null;
  edge_cursor: number;
}
interface BackingRow extends SqliteRow {
  stored_size: number;
  membership_size: number;
  encoded?: Uint8Array;
}
interface LeaseChargeRow extends SqliteRow {
  state: number;
  owner_nonce: Uint8Array;
  staged_bytes: number;
}
interface ExpiredLeaseRow extends LeaseChargeRow {
  id: string;
}

export interface ReconciliationProgress {
  readonly processed: number;
  readonly complete: boolean;
}

export const EMPTY_STAGING_CHAIN = sha256(encodeUtf8("efs-staging-chain-v1"));

function memberKind(kind: StagingMemberKind): number {
  return kind === "object" ? 0 : kind === "manifest-root" ? 1 : 2;
}
function extendChain(
  previous: Uint8Array,
  sequence: number,
  member: StagingMember,
): Uint8Array {
  const encoded = new Uint8Array(49);
  const view = new DataView(encoded.buffer);
  encoded[0] = memberKind(member.kind);
  encoded.set(member.hash, 1);
  view.setBigUint64(33, BigInt(sequence), true);
  view.setBigUint64(41, BigInt(member.size), true);
  const chained = new Uint8Array(previous.byteLength + encoded.byteLength);
  chained.set(previous);
  chained.set(encoded, previous.byteLength);
  return sha256(chained);
}
function counters(values: readonly number[]): void {
  for (const value of values)
    if (!Number.isSafeInteger(value) || value < 0)
      throw new RangeError("invalid closure certificate counter");
}

export class StagingRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  constructor(tx: FilesystemSQLiteTransaction, limits: StorageLimits) {
    this.#tx = tx;
    this.#limits = limits;
  }

  begin(options: {
    readonly leaseId: string;
    readonly ownerId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
    readonly expiresAt: number;
    readonly kind?: number;
    readonly branchId?: string;
    readonly generation?: number;
  }): void {
    if (!options.leaseId || !options.ownerId || options.ownerNonce.byteLength < 16)
      throw new RangeError("staging lease identity or owner nonce is invalid");
    counters([options.now, options.expiresAt]);
    if (options.expiresAt <= options.now)
      throw new RangeError("staging lease expiry must be in the future");
    this.#tx.run(
      "INSERT INTO efs_leases(id,kind,owner_id,owner_nonce,branch_id,generation,created_at_ms,last_renewal_at_ms,expires_at_ms,state) VALUES(?,?,?,?,?,?,?,?,?,0)",
      [
        options.leaseId,
        options.kind ?? 1,
        options.ownerId,
        options.ownerNonce,
        options.branchId ?? null,
        options.generation ?? null,
        options.now,
        options.now,
        options.expiresAt,
      ],
    );
    this.#tx.run(
      "INSERT INTO efs_staging_certificates(lease_id,owner_nonce,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified) VALUES(?,?,NULL,?,0,0,0,0,0,0,0,0)",
      [options.leaseId, options.ownerNonce, EMPTY_STAGING_CHAIN],
    );
  }

  putEntry(
    leaseId: string,
    entryIndex: number,
    objectHash: Uint8Array,
    length: number,
  ): void {
    this.#tx.run(
      "INSERT INTO efs_staging_entries(lease_id,entry_index,object_hash,length) VALUES(?,?,?,?)",
      [leaseId, entryIndex, objectHash, length],
    );
  }
  entriesAfter(
    leaseId: string,
    cursor: number,
    limit: number,
    maxBytes: number,
  ): readonly StagingEntryRow[] {
    return this.#tx.all<StagingEntryRow>(
      "SELECT entry_index,object_hash,length FROM efs_staging_entries WHERE lease_id=? AND entry_index>? ORDER BY entry_index LIMIT ?",
      [leaseId, cursor, limit],
      { maxRows: limit, maxBytes },
    );
  }
  putLevelRecord(
    leaseId: string,
    level: number,
    recordIndex: number,
    nodeHash: Uint8Array,
    span: number,
    entryCount: number,
  ): void {
    this.#tx.run(
      "INSERT INTO efs_staging_level_records(lease_id,level,record_index,node_hash,span,entry_count) VALUES(?,?,?,?,?,?)",
      [leaseId, level, recordIndex, nodeHash, span, entryCount],
    );
  }
  levelRecordsAfter(
    leaseId: string,
    level: number,
    cursor: number,
    limit: number,
    maxBytes: number,
  ): readonly StagingLevelRow[] {
    return this.#tx.all<StagingLevelRow>(
      "SELECT record_index,node_hash,span,entry_count FROM efs_staging_level_records WHERE lease_id=? AND level=? AND record_index>? ORDER BY record_index LIMIT ?",
      [leaseId, level, cursor, limit],
      { maxRows: limit, maxBytes },
    );
  }
  bumpRoot(kind: number, id: string): void {
    const rootId = encodeUtf8(id);
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: CHARGED_ROW_BYTES + rootId.byteLength },
      "root journal",
    );
    this.#tx.run(
      "UPDATE efs_meta SET root_mutation_generation=root_mutation_generation+1 WHERE singleton=1",
    );
    const generation = this.#tx.all<{ root_mutation_generation: number } & SqliteRow>(
      "SELECT root_mutation_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.root_mutation_generation;
    if (!Number.isSafeInteger(generation))
      throw new Error("ECORRUPT: invalid root mutation generation");
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
      [generation!, kind, rootId],
    );
  }
  release(leaseId: string, ownerNonce: Uint8Array, requireSealed: boolean): boolean {
    const charge = this.#leaseCharge(leaseId);
    if (!charge || !equalBytes(charge.owner_nonce, ownerNonce)) return false;
    const state = requireSealed ? "state=1" : "state IN (0,1)";
    const result = this.#tx.run(
      `UPDATE efs_leases SET state=2 WHERE id=? AND owner_nonce=? AND ${state}`,
      [leaseId, ownerNonce],
    );
    if (result.changes) {
      this.#releaseStagingBytes(charge.staged_bytes);
      this.bumpRoot(6, leaseId);
    }
    return result.changes === 1;
  }
  delete(leaseId: string, ownerNonce: Uint8Array): boolean {
    const charge = this.#leaseCharge(leaseId);
    if (!charge || !equalBytes(charge.owner_nonce, ownerNonce)) return false;
    const result = this.#tx.run("DELETE FROM efs_leases WHERE id=? AND owner_nonce=?", [
      leaseId,
      ownerNonce,
    ]);
    if (result.changes) {
      if (charge.state === 0 || charge.state === 1)
        this.#releaseStagingBytes(charge.staged_bytes);
      this.bumpRoot(6, leaseId);
    }
    return result.changes === 1;
  }
  acquireReadLease(
    leaseId: string,
    ownerId: string,
    manifestHash: Uint8Array,
    expiresAt: number,
  ): void {
    this.#tx.run(
      "INSERT INTO efs_leases(id,kind,owner_id,expires_at_ms,state) VALUES(?,0,?,?,1)",
      [leaseId, ownerId, expiresAt],
    );
    this.#tx.run(
      "INSERT INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)",
      [leaseId, manifestHash],
    );
    this.bumpRoot(2, leaseId);
  }
  releaseReadLease(leaseId: string, ownerId: string): boolean {
    const result = this.#tx.run("DELETE FROM efs_leases WHERE id=? AND owner_id=?", [
      leaseId,
      ownerId,
    ]);
    if (result.changes) this.bumpRoot(3, leaseId);
    return result.changes === 1;
  }

  expireBatch(now: number, limit: number): number {
    counters([now, limit]);
    if (
      limit <= 0 ||
      limit > this.#limits.maxGcBatchSize ||
      limit > this.#limits.maxQueryBatchSize
    )
      throw new RangeError("invalid expired-lease batch limit");
    const rows = this.#tx.all<ExpiredLeaseRow>(
      "SELECT l.id,l.state,l.owner_nonce,COALESCE(c.object_bytes+c.node_bytes,0) staged_bytes FROM efs_leases l LEFT JOIN efs_staging_certificates c ON c.lease_id=l.id WHERE l.expires_at_ms<? OR l.state=2 ORDER BY l.id LIMIT ?",
      [now, limit],
      { maxRows: limit, maxBytes: Math.max(1024, limit * 256) },
    );
    let releasedBytes = 0;
    let deleted = 0;
    for (const row of rows) {
      const result = this.#tx.run(
        "DELETE FROM efs_leases WHERE id=? AND (expires_at_ms<? OR state=2)",
        [row.id, now],
      );
      if (result.changes) {
        deleted += 1;
        if (row.state === 0 || row.state === 1)
          releasedBytes = checkedAdd(releasedBytes, row.staged_bytes);
      }
    }
    this.#releaseStagingBytes(releasedBytes);
    if (deleted) this.bumpRoot(6, `expired:${now}`);
    return deleted;
  }

  appendBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
  ): ClosureCertificate {
    if (members.length === 0) return this.snapshot(leaseId, ownerNonce);
    if (members.length > this.#limits.maxQueryBatchSize)
      throw new RangeError(
        "staging membership batch exceeds configured query/binding limit",
      );
    const row = this.#row(leaseId);
    if (!equalBytes(row.owner_nonce, ownerNonce) || row.sealed !== 0)
      throw new Error("ECORRUPT: staging owner mismatch or certificate already sealed");
    if (
      this.#tx.all(
        "SELECT lease_id FROM efs_staging_reconciliations WHERE lease_id=?",
        [leaseId],
        { maxRows: 1, maxBytes: 128 },
      ).length
    )
      throw new Error(
        "ECORRUPT: staged closure cannot change after reconciliation begins",
      );
    const seen = new Set<string>();
    let chain = row.chain_digest;
    let sequence = row.next_sequence;
    let objectCount = row.object_count;
    let objectBytes = row.object_bytes;
    let nodeCount = row.node_count;
    let nodeBytes = row.node_bytes;
    let stagedDelta = 0;
    for (const member of members) {
      if (member.hash.byteLength !== 32)
        throw new RangeError("staging member hash must be 32 bytes");
      counters([member.size]);
      this.#verifyMemberBacking(leaseId, member);
      const key = `${member.kind}:${Array.from(member.hash, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
      if (seen.has(key)) throw new Error("duplicate staging member in one batch");
      seen.add(key);
      if (member.kind === "object") {
        const inserted = this.#tx.run(
          "INSERT OR IGNORE INTO efs_lease_objects(lease_id,object_hash,sequence,size) VALUES(?,?,?,?)",
          [leaseId, member.hash, sequence, member.size],
        );
        if (!inserted.changes) continue;
        objectCount += 1;
        objectBytes = checkedAdd(objectBytes, member.size);
      } else {
        const inserted = this.#tx.run(
          "INSERT OR IGNORE INTO efs_lease_staged_manifests(lease_id,kind,manifest_hash,sequence,size) VALUES(?,?,?,?,?)",
          [
            leaseId,
            member.kind === "manifest-root" ? 0 : 1,
            member.hash,
            sequence,
            member.size,
          ],
        );
        if (!inserted.changes) continue;
        nodeCount += 1;
        nodeBytes = checkedAdd(nodeBytes, member.size);
      }
      stagedDelta = checkedAdd(stagedDelta, member.size);
      chain = extendChain(chain, sequence, member);
      sequence += 1;
    }
    this.#admitStagingBytes(stagedDelta);
    this.#tx.run(
      "UPDATE efs_staging_certificates SET chain_digest=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=? WHERE lease_id=? AND sealed=0",
      [
        chain,
        objectCount,
        objectBytes,
        nodeCount,
        nodeBytes,
        sequence,
        sequence,
        leaseId,
      ],
    );
    return Object.freeze({
      leaseId,
      ownerNonce: ownerNonce.slice(),
      manifestHash: new Uint8Array(32),
      chainDigest: chain,
      objectCount,
      objectBytes,
      nodeCount,
      nodeBytes,
      membershipCount: sequence,
    });
  }

  snapshot(leaseId: string, ownerNonce: Uint8Array): ClosureCertificate {
    const row = this.#row(leaseId);
    if (!equalBytes(row.owner_nonce, ownerNonce))
      throw new Error("ECORRUPT: staging owner mismatch");
    return Object.freeze({
      leaseId,
      ownerNonce: ownerNonce.slice(),
      manifestHash: row.manifest_hash?.slice() ?? new Uint8Array(32),
      chainDigest: row.chain_digest,
      objectCount: row.object_count,
      objectBytes: row.object_bytes,
      nodeCount: row.node_count,
      nodeBytes: row.node_bytes,
      membershipCount: row.membership_count,
    });
  }

  beginReconciliation(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void {
    if (manifestHash.byteLength !== 32)
      throw new RangeError("manifest root hash must be 32 bytes");
    const certificate = this.#row(leaseId);
    if (!equalBytes(certificate.owner_nonce, ownerNonce) || certificate.sealed !== 0)
      throw new Error("ECORRUPT: staging owner mismatch or certificate already sealed");
    const existing = this.#reconciliation(leaseId);
    if (existing) {
      if (
        !equalBytes(existing.owner_nonce, ownerNonce) ||
        !equalBytes(existing.manifest_hash, manifestHash)
      )
        throw new Error("ECORRUPT: reconciliation identity mismatch");
      return;
    }
    this.#tx.run(
      "INSERT INTO efs_staging_reconciliations(lease_id,owner_nonce,manifest_hash,next_sequence,object_count,object_bytes,node_count,node_bytes,membership_count,complete) VALUES(?,?,?,0,0,0,0,0,0,0)",
      [leaseId, ownerNonce, manifestHash],
    );
    this.#enqueueVerified(leaseId, 1, manifestHash, undefined, undefined);
  }

  reconcileBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    workLimit: number,
  ): ReconciliationProgress {
    if (
      !Number.isSafeInteger(workLimit) ||
      workLimit <= 0 ||
      workLimit > this.#limits.maxQueryBatchSize
    )
      throw new RangeError("invalid reconciliation work limit");
    const state = this.#reconciliation(leaseId);
    if (!state || !equalBytes(state.owner_nonce, ownerNonce))
      throw new Error("ECORRUPT: reconciliation owner mismatch");
    if (state.complete === 1) return Object.freeze({ processed: 0, complete: true });
    const queue = this.#tx.all<QueueRow>(
      "SELECT kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor FROM efs_staging_reconciliation_queue WHERE lease_id=? AND processed=0 ORDER BY sequence LIMIT ?",
      [leaseId, workLimit],
      { maxRows: workLimit, maxBytes: Math.max(1024, workLimit * 320) },
    );
    let remaining = workLimit;
    let processed = 0;
    const objects = queue.filter((item) => item.kind === 0);
    if (objects.length) {
      this.#tx.run(
        "UPDATE efs_staging_reconciliation_queue SET processed=1 WHERE lease_id=? AND kind=0 AND processed=0 AND sequence>=? AND sequence<=?",
        [leaseId, objects[0]!.sequence, objects.at(-1)!.sequence],
      );
      remaining -= objects.length;
      processed += objects.length;
    }
    for (const item of queue) {
      if (remaining <= 0) break;
      if (item.kind === 0) continue;
      const backing = this.#manifestBacking(leaseId, item.kind, item.hash);
      if (!backing.encoded || backing.stored_size !== item.declared_size)
        throw new Error("ECORRUPT: reconciled manifest size changed");
      if (item.kind === 1) {
        const root = decodeManifestRoot(backing.encoded, item.hash);
        this.#enqueueVerified(
          leaseId,
          2,
          root.rootNodeHash,
          root.fileSize,
          root.entryCount,
        );
        this.#tx.run(
          "UPDATE efs_staging_reconciliation_queue SET edge_cursor=1,processed=1 WHERE lease_id=? AND kind=1 AND hash=? AND processed=0",
          [leaseId, item.hash],
        );
        remaining -= 1;
        processed += 1;
        continue;
      }
      const node = decodeManifestNode(backing.encoded, item.hash);
      if (
        item.declared_span !== node.span ||
        item.declared_entry_count !== node.entryCount
      )
        throw new Error("ECORRUPT: manifest child declaration mismatch");
      const edgeCount =
        node.kind === "leaf" ? node.entries.length : node.children.length;
      const end = Math.min(edgeCount, item.edge_cursor + remaining);
      if (node.kind === "leaf") {
        for (let index = item.edge_cursor; index < end; index += 1) {
          const edge = node.entries[index]!;
          this.#enqueueVerified(leaseId, 0, edge.hash, edge.length, 1);
          remaining -= 1;
          processed += 1;
        }
      } else {
        for (let index = item.edge_cursor; index < end; index += 1) {
          const edge = node.children[index]!;
          this.#enqueueVerified(leaseId, 2, edge.hash, edge.span, edge.entryCount);
          remaining -= 1;
          processed += 1;
        }
      }
      this.#tx.run(
        "UPDATE efs_staging_reconciliation_queue SET edge_cursor=?,processed=? WHERE lease_id=? AND kind=2 AND hash=? AND processed=0",
        [end, end === edgeCount ? 1 : 0, leaseId, item.hash],
      );
    }
    const pending =
      this.#tx.all(
        "SELECT sequence FROM efs_staging_reconciliation_queue WHERE lease_id=? AND processed=0 LIMIT 1",
        [leaseId],
        { maxRows: 1, maxBytes: 128 },
      ).length !== 0;
    if (!pending) {
      const reconciled = this.#reconciliation(leaseId)!;
      const certificate = this.#row(leaseId);
      if (
        reconciled.object_count !== certificate.object_count ||
        reconciled.object_bytes !== certificate.object_bytes ||
        reconciled.node_count !== certificate.node_count ||
        reconciled.node_bytes !== certificate.node_bytes ||
        reconciled.membership_count !== certificate.membership_count ||
        reconciled.next_sequence !== certificate.membership_count
      )
        throw new Error(
          "ECORRUPT: complete manifest closure differs from staged membership",
        );
      this.#tx.run(
        "UPDATE efs_staging_reconciliations SET complete=1 WHERE lease_id=? AND complete=0",
        [leaseId],
      );
    }
    return Object.freeze({ processed, complete: !pending });
  }

  seal(certificate: ClosureCertificate): void {
    this.#validateShape(certificate);
    const row = this.#row(certificate.leaseId);
    if (
      !equalBytes(row.owner_nonce, certificate.ownerNonce) ||
      !equalBytes(row.chain_digest, certificate.chainDigest) ||
      row.object_count !== certificate.objectCount ||
      row.object_bytes !== certificate.objectBytes ||
      row.node_count !== certificate.nodeCount ||
      row.node_bytes !== certificate.nodeBytes ||
      row.membership_count !== certificate.membershipCount ||
      row.next_sequence !== certificate.membershipCount ||
      row.sealed !== 0
    )
      throw new Error("ECORRUPT: staged closure certificate mismatch");
    const reconciliation = this.#reconciliation(certificate.leaseId);
    if (
      !reconciliation ||
      reconciliation.complete !== 1 ||
      !equalBytes(reconciliation.owner_nonce, certificate.ownerNonce) ||
      !equalBytes(reconciliation.manifest_hash, certificate.manifestHash) ||
      reconciliation.object_count !== certificate.objectCount ||
      reconciliation.object_bytes !== certificate.objectBytes ||
      reconciliation.node_count !== certificate.nodeCount ||
      reconciliation.node_bytes !== certificate.nodeBytes ||
      reconciliation.membership_count !== certificate.membershipCount ||
      reconciliation.next_sequence !== certificate.membershipCount
    )
      throw new Error(
        "ECORRUPT: staged closure reconciliation is incomplete or mismatched",
      );
    this.#tx.run(
      "INSERT INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)",
      [certificate.leaseId, certificate.manifestHash],
    );
    this.#tx.run(
      "UPDATE efs_staging_certificates SET manifest_hash=?,sealed=1,verified=1 WHERE lease_id=? AND sealed=0",
      [certificate.manifestHash, certificate.leaseId],
    );
    this.#tx.run("UPDATE efs_leases SET state=1 WHERE id=? AND state=0", [
      certificate.leaseId,
    ]);
  }

  validateSealed(certificate: ClosureCertificate, now = 0): void {
    this.#validateShape(certificate);
    counters([now]);
    const row = this.#tx.all<CertificateRow>(
      "SELECT c.owner_nonce,c.manifest_hash,c.chain_digest,c.object_count,c.object_bytes,c.node_count,c.node_bytes,c.membership_count,c.next_sequence,c.sealed,c.verified,l.owner_nonce lease_nonce,l.expires_at_ms,l.state,CASE WHEN m.manifest_hash IS NULL THEN 0 ELSE 1 END rooted FROM efs_staging_certificates c JOIN efs_leases l ON l.id=c.lease_id LEFT JOIN efs_lease_manifests m ON m.lease_id=c.lease_id AND m.manifest_hash=c.manifest_hash WHERE c.lease_id=?",
      [certificate.leaseId],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (
      !row ||
      row.sealed !== 1 ||
      row.verified !== 1 ||
      row.state !== 1 ||
      row.rooted !== 1 ||
      row.expires_at_ms! < now ||
      !equalBytes(row.owner_nonce, certificate.ownerNonce) ||
      !equalBytes(row.lease_nonce!, certificate.ownerNonce) ||
      !row.manifest_hash ||
      !equalBytes(row.manifest_hash, certificate.manifestHash) ||
      !equalBytes(row.chain_digest, certificate.chainDigest) ||
      row.object_count !== certificate.objectCount ||
      row.object_bytes !== certificate.objectBytes ||
      row.node_count !== certificate.nodeCount ||
      row.node_bytes !== certificate.nodeBytes ||
      row.membership_count !== certificate.membershipCount ||
      row.next_sequence !== certificate.membershipCount
    )
      throw new Error("ECORRUPT: sealed closure certificate mismatch");
  }

  #verifyMemberBacking(_leaseId: string, member: StagingMember): void {
    const row =
      member.kind === "object"
        ? this.#tx.all<{ stored_size: number } & SqliteRow>(
            "SELECT size stored_size FROM efs_cas_objects WHERE hash=?",
            [member.hash],
            { maxRows: 1, maxBytes: 128 },
          )[0]
        : member.kind === "manifest-root"
          ? this.#tx.all<{ stored_size: number } & SqliteRow>(
              "SELECT length(encoded) stored_size FROM efs_manifest_roots WHERE hash=?",
              [member.hash],
              { maxRows: 1, maxBytes: 128 },
            )[0]
          : this.#tx.all<{ stored_size: number } & SqliteRow>(
              "SELECT length(encoded) stored_size FROM efs_manifest_nodes WHERE hash=?",
              [member.hash],
              { maxRows: 1, maxBytes: 128 },
            )[0];
    if (!row || row.stored_size !== member.size)
      throw new Error("ECORRUPT: staged membership does not match immutable content");
  }

  #manifestBacking(leaseId: string, kind: number, hash: Uint8Array): BackingRow {
    const row =
      kind === 1
        ? this.#tx.all<BackingRow>(
            "SELECT length(r.encoded) stored_size,m.size membership_size,r.encoded FROM efs_manifest_roots r JOIN efs_lease_staged_manifests m ON m.lease_id=? AND m.kind=0 AND m.manifest_hash=r.hash WHERE r.hash=?",
            [leaseId, hash],
            { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 256 },
          )[0]
        : this.#tx.all<BackingRow>(
            "SELECT length(n.encoded) stored_size,m.size membership_size,n.encoded FROM efs_manifest_nodes n JOIN efs_lease_staged_manifests m ON m.lease_id=? AND m.kind=1 AND m.manifest_hash=n.hash WHERE n.hash=?",
            [leaseId, hash],
            { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 256 },
          )[0];
    if (!row || row.stored_size !== row.membership_size)
      throw new Error(
        "ECORRUPT: manifest closure member is absent or has a mismatched size",
      );
    return row;
  }

  #enqueueVerified(
    leaseId: string,
    kind: number,
    hash: Uint8Array,
    declaredSpan: number | undefined,
    declaredEntryCount: number | undefined,
  ): void {
    let size: number;
    if (kind === 0) {
      const row = this.#tx.all<BackingRow>(
        "SELECT o.size stored_size,m.size membership_size FROM efs_cas_objects o JOIN efs_lease_objects m ON m.lease_id=? AND m.object_hash=o.hash WHERE o.hash=?",
        [leaseId, hash],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      if (
        !row ||
        row.stored_size !== row.membership_size ||
        row.stored_size !== declaredSpan
      )
        throw new Error(
          "ECORRUPT: object closure member is absent or has a mismatched size",
        );
      size = row.stored_size;
    } else {
      const row = this.#manifestBacking(leaseId, kind, hash);
      size = row.stored_size;
      if (kind === 1) decodeManifestRoot(row.encoded!, hash);
      else {
        const node = decodeManifestNode(row.encoded!, hash);
        if (node.span !== declaredSpan || node.entryCount !== declaredEntryCount)
          throw new Error("ECORRUPT: manifest child declaration mismatch");
      }
    }
    const state = this.#reconciliation(leaseId);
    if (!state) throw new Error("ECORRUPT: missing staging reconciliation");
    const inserted = this.#tx.run(
      "INSERT OR IGNORE INTO efs_staging_reconciliation_queue(lease_id,kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor,processed) VALUES(?,?,?,?,?,?,?,0,0)",
      [
        leaseId,
        kind,
        hash,
        state.next_sequence,
        size,
        declaredSpan ?? null,
        declaredEntryCount ?? null,
      ],
    );
    if (!inserted.changes) {
      const existing = this.#tx.all<QueueRow>(
        "SELECT kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor FROM efs_staging_reconciliation_queue WHERE lease_id=? AND kind=? AND hash=?",
        [leaseId, kind, hash],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      if (
        !existing ||
        existing.declared_size !== size ||
        existing.declared_span !== (declaredSpan ?? null) ||
        existing.declared_entry_count !== (declaredEntryCount ?? null)
      )
        throw new Error("ECORRUPT: repeated manifest closure edge disagrees");
      return;
    }
    const object = kind === 0 ? 1 : 0;
    const node = kind === 0 ? 0 : 1;
    this.#tx.run(
      "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+1,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+?,node_bytes=node_bytes+?,membership_count=membership_count+1 WHERE lease_id=? AND complete=0",
      [object, object ? size : 0, node, node ? size : 0, leaseId],
    );
  }

  #reconciliation(leaseId: string): ReconciliationRow | undefined {
    return this.#tx.all<ReconciliationRow>(
      "SELECT owner_nonce,manifest_hash,next_sequence,object_count,object_bytes,node_count,node_bytes,membership_count,complete FROM efs_staging_reconciliations WHERE lease_id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 512 },
    )[0];
  }

  #leaseCharge(leaseId: string): LeaseChargeRow | undefined {
    return this.#tx.all<LeaseChargeRow>(
      "SELECT l.state,l.owner_nonce,COALESCE(c.object_bytes+c.node_bytes,0) staged_bytes FROM efs_leases l LEFT JOIN efs_staging_certificates c ON c.lease_id=l.id WHERE l.id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 256 },
    )[0];
  }

  #admitStagingBytes(bytes: number): void {
    if (bytes === 0) return;
    new UsageRepository(this.#tx, this.#limits).apply(
      { staging_bytes: bytes },
      "staging payload",
    );
  }

  #releaseStagingBytes(bytes: number): void {
    if (bytes === 0) return;
    new UsageRepository(this.#tx, this.#limits).apply(
      { staging_bytes: -bytes },
      "staging payload release",
    );
  }

  #row(leaseId: string): CertificateRow {
    const row = this.#tx.all<CertificateRow>(
      "SELECT owner_nonce,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified FROM efs_staging_certificates WHERE lease_id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!row) throw new Error("ECORRUPT: staging certificate is missing");
    return row;
  }
  #validateShape(certificate: ClosureCertificate): void {
    if (
      certificate.ownerNonce.byteLength < 16 ||
      certificate.manifestHash.byteLength !== 32 ||
      certificate.chainDigest.byteLength !== 32
    )
      throw new RangeError("closure certificate hashes or owner nonce are invalid");
    counters([
      certificate.objectCount,
      certificate.objectBytes,
      certificate.nodeCount,
      certificate.nodeBytes,
      certificate.membershipCount,
    ]);
    if (certificate.membershipCount !== certificate.objectCount + certificate.nodeCount)
      throw new RangeError("closure certificate membership count mismatch");
  }
}

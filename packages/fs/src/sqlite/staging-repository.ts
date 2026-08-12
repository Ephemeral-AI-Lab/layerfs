import { sha256, type HashFunction } from "../cas/sha256.js";
import {
  bytesToHex,
  copyBytes,
  equalBytes,
  intrinsicByteLength,
} from "../cas/bytes.js";
import { encodeUtf8, utf8ByteLength } from "../namespace/utf8.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  validateSupportedManifestParameters,
  type ManifestChild,
} from "../manifests/codec.js";
import { validateCanonicalManifestNode } from "../manifests/cursor.js";
import { checkedAdd } from "../resources/safe-integers.js";
import {
  MAINTENANCE_GC_EMERGENCY_BYTES,
  MAINTENANCE_TOTAL_EMERGENCY_BYTES,
  type StorageLimits,
} from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow, SqliteValue } from "./driver.js";
import {
  applyChargedMetadata,
  beginMetadataChargeBatch,
  beginUsageMutationBatch,
  CHARGED_ROW_BYTES,
  flushMetadataChargeBatch,
  flushUsageMutationBatch,
  UsageRepository,
} from "./usage-repository.js";
import {
  ManifestTreeRepository,
  type ReusedSubtreeCacheMetadata,
  type ManifestSubtreeSummary,
} from "./manifest-tree-repository.js";
import { advanceRootMutationGeneration } from "./namespace-repository.js";
import { ContentRepository } from "./content-repository.js";
import type { ContentCache } from "../cache/content-cache.js";

interface ValidatedSealedLease {
  readonly leaseId: string;
  readonly ownerNonce: Uint8Array;
  readonly stagedBytes: number;
  readonly ingestReservationBytes: number;
  readonly metadataReservationBytes: number;
}

export type StagingMemberKind = "object" | "manifest-root" | "manifest-node";
export interface StagingMember {
  readonly kind: StagingMemberKind;
  readonly hash: Uint8Array;
  readonly size: number;
  /**
   * Count-only members are already-durable objects referenced by the rebuilt
   * closure: they extend the chain and the certificate counts, but they get
   * no `efs_lease_objects` row, no metadata charge, and no staging-byte
   * admission (they are quota-neutral). They are protected mid-edit by the
   * source-manifest lease link and the GC source-closure marking, never by
   * their own membership row - count-only members must already be durable.
   */
  readonly counted?: boolean;
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
  /** Commutative XOR fold of every chain member hash (the closure binding). */
  readonly chainFold: Uint8Array;
  readonly objectCount: number;
  readonly objectBytes: number;
  readonly nodeCount: number;
  readonly nodeBytes: number;
  readonly membershipCount: number;
}
export interface ReconcileBatchOptions {
  /** The caller already authenticated durable count-only boundary objects. */
  readonly skipObjectBackingCheck?: boolean;
}
interface VerifiedFreshBacking {
  readonly objectSizes?: ReadonlyMap<string, number>;
  readonly nodeSizes?: ReadonlyMap<string, number>;
  /** A freshly inserted/collision-checked root is already size-authenticated. */
  readonly rootSizes?: ReadonlyMap<string, number>;
}
interface CertificateRow extends SqliteRow {
  owner_nonce: Uint8Array;
  manifest_hash: Uint8Array | null;
  chain_digest: Uint8Array;
  chain_fold: Uint8Array;
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
  validated_depth?: number | null;
  ingest_reservation_bytes: number;
  metadata_reservation_bytes: number;
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
  leaf_depth: number | null;
  closure_fold: Uint8Array;
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
}
interface LeaseChargeRow extends SqliteRow {
  state: number;
  owner_nonce: Uint8Array;
  staged_bytes: number;
  ingest_reservation_bytes: number;
  metadata_reservation_bytes: number;
}
interface ExpiredLeaseRow extends LeaseChargeRow {
  id: string;
}
interface CleanupRow extends SqliteRow {
  lease_id: string;
  phase: number;
}
interface ReusedSubtreeRow extends SqliteRow {
  source_manifest_hash: Uint8Array;
  source_path: Uint8Array;
  span: number;
  entry_count: number;
  validated_nonfinal_leaf_delta: number | null;
  validated_final_leaf_delta: number | null;
  summary_usable: number;
}
interface ReusableSummaryRow extends SqliteRow {
  object_count: number;
  object_bytes: number;
  node_count: number;
  node_bytes: number;
  membership_count: number;
  closure_fold: Uint8Array;
}
interface ValidationQueueRow extends SqliteRow {
  path: Uint8Array;
  node_hash: Uint8Array;
  declared_span: number;
  declared_entry_count: number;
  depth: number;
  final_at_level: number;
  edge_cursor: number;
}

export interface ReconciliationProgress {
  readonly processed: number;
  readonly complete: boolean;
}
export interface LeaseCleanupProgress {
  readonly worked: boolean;
  readonly deletedRows: number;
  readonly deletedLeases: number;
}

const CLEANUP_DELETE_STATEMENTS = Object.freeze([
  "DELETE FROM efs_staging_entries WHERE lease_id=? AND entry_index IN (SELECT entry_index FROM efs_staging_entries WHERE lease_id=? ORDER BY entry_index LIMIT ?)",
  "DELETE FROM efs_staging_level_records WHERE lease_id=? AND (level,record_index) IN (SELECT level,record_index FROM efs_staging_level_records WHERE lease_id=? ORDER BY level,record_index LIMIT ?)",
  "DELETE FROM efs_staging_reconciliation_queue WHERE lease_id=? AND (kind,hash) IN (SELECT kind,hash FROM efs_staging_reconciliation_queue WHERE lease_id=? ORDER BY kind,hash LIMIT ?)",
  "DELETE FROM efs_staging_manifest_validation_queue WHERE lease_id=? AND path IN (SELECT path FROM efs_staging_manifest_validation_queue WHERE lease_id=? ORDER BY path LIMIT ?)",
  "DELETE FROM efs_staging_reused_subtrees WHERE lease_id=? AND node_hash IN (SELECT node_hash FROM efs_staging_reused_subtrees WHERE lease_id=? ORDER BY node_hash LIMIT ?)",
  "DELETE FROM efs_lease_objects WHERE lease_id=? AND object_hash IN (SELECT object_hash FROM efs_lease_objects WHERE lease_id=? ORDER BY object_hash LIMIT ?)",
  "DELETE FROM efs_lease_staged_manifests WHERE lease_id=? AND (kind,manifest_hash) IN (SELECT kind,manifest_hash FROM efs_lease_staged_manifests WHERE lease_id=? ORDER BY kind,manifest_hash LIMIT ?)",
  "DELETE FROM efs_lease_manifests WHERE lease_id=? AND manifest_hash IN (SELECT manifest_hash FROM efs_lease_manifests WHERE lease_id=? ORDER BY manifest_hash LIMIT ?)",
  "DELETE FROM efs_lease_cow_pages WHERE lease_id=? AND (branch_id,inode_id,page_index,generation) IN (SELECT branch_id,inode_id,page_index,generation FROM efs_lease_cow_pages WHERE lease_id=? ORDER BY branch_id,inode_id,page_index,generation LIMIT ?)",
  "DELETE FROM efs_lease_patches WHERE lease_id=? AND (branch_id,inode_id,sequence) IN (SELECT branch_id,inode_id,sequence FROM efs_lease_patches WHERE lease_id=? ORDER BY branch_id,inode_id,sequence LIMIT ?)",
] as const);
const MAX_STAGING_ID_BYTES = 128;

function stagingId(value: string, label: string): void {
  const bytes = utf8ByteLength(value);
  if (!value || bytes > MAX_STAGING_ID_BYTES)
    throw new RangeError(
      `${label} must contain 1..${MAX_STAGING_ID_BYTES} UTF-8 bytes`,
    );
}

export const EMPTY_STAGING_CHAIN = sha256(encodeUtf8("efs-staging-chain-v1"));

function memberKind(kind: StagingMemberKind): number {
  return kind === "object" ? 0 : kind === "manifest-root" ? 1 : 2;
}
function membershipInsertChunks<T>(
  members: readonly T[],
  bindingsPerRow: number,
  limit: number,
): readonly (readonly T[])[] {
  const rowsPerChunk = Math.max(1, Math.floor((limit - 4) / bindingsPerRow));
  const chunks: T[][] = [];
  for (let index = 0; index < members.length; index += rowsPerChunk)
    chunks.push(members.slice(index, index + rowsPerChunk));
  return Object.freeze(chunks);
}

function extendChain(
  previous: Uint8Array,
  sequence: number,
  member: StagingMember,
  hashBytes: HashFunction,
  scratch?: Uint8Array,
): Uint8Array {
  if (intrinsicByteLength(previous) !== 32 || intrinsicByteLength(member.hash) !== 32)
    throw new Error("ECORRUPT: staging chain member hash must contain 32 bytes");
  const chained = scratch ?? new Uint8Array(81);
  if (intrinsicByteLength(chained) !== 81)
    throw new Error("ECORRUPT: staging chain scratch must contain 81 bytes");
  chained.set(previous, 0);
  chained[32] = memberKind(member.kind);
  chained.set(member.hash, 33);
  const view = new DataView(chained.buffer, chained.byteOffset, chained.byteLength);
  view.setBigUint64(65, BigInt(sequence), true);
  view.setBigUint64(73, BigInt(member.size), true);
  return hashBytes(chained);
}

function extendSummaryChain(
  previous: Uint8Array,
  sequence: number,
  summary: ManifestSubtreeSummary,
  hashBytes: HashFunction,
): Uint8Array {
  const encoded = new Uint8Array(49);
  const view = new DataView(encoded.buffer);
  encoded[0] = 3;
  encoded.set(summary.chainDigest, 1);
  view.setBigUint64(33, BigInt(sequence), true);
  view.setBigUint64(41, BigInt(summary.membershipCount), true);
  return hashBytes(new Uint8Array([...previous, ...encoded]));
}

/** Commutative XOR fold over member/edge hashes (the closure binding). */
function foldHashes(fold: Uint8Array, hash: Uint8Array): Uint8Array {
  if (intrinsicByteLength(fold) !== 32 || intrinsicByteLength(hash) !== 32)
    throw new Error("ECORRUPT: closure fold hashes must contain 32 bytes");
  const out = copyBytes(fold);
  for (let index = 0; index < 32; index += 1) out[index]! ^= hash[index]!;
  return out;
}

export const EMPTY_CLOSURE_FOLD = new Uint8Array(32);
function counters(values: readonly number[]): void {
  for (const value of values)
    if (!Number.isSafeInteger(value) || value < 0)
      throw new RangeError("invalid closure certificate counter");
}

export class StagingRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  readonly #cache: ContentCache | undefined;
  readonly #content: ContentRepository;
  readonly #hashBytes: HashFunction;
  #batchedIngestBytes = 0;
  #batchedStagedBytes = 0;
  #batchedIngestReservationBytes = 0;
  #batchedMetadataReservationBytes = 0;
  #ingestReservationLease: string | undefined;
  #ingestReservationBytes: number | undefined;
  #batchIngestAccounting = false;
  #batchMetadataAccounting = false;
  #metadataReservationLease: string | undefined;
  #metadataReservationBytes: number | undefined;
  #leaseExpiresAt: number | undefined;
  #batchedCertificateDirty = false;
  readonly #certificateCache = new Map<string, CertificateRow>();
  /**
   * Reconciliation runs inside one IMMEDIATE write transaction for the local
   * rebuild path. Keep its mutable aggregate in memory for that transaction;
   * every SQL mutation below patches this cache immediately after the same
   * mutation. This removes a read-after-write query from every queue edge while
   * retaining the database row as the durable source of truth at commit.
   */
  readonly #reconciliationCache = new Map<string, ReconciliationRow | undefined>();
  /** Queue membership is immutable for a lease once inserted. */
  readonly #reconciliationQueueCache = new Map<string, QueueRow>();
  /** Summary-backed reused nodes are aggregated once in the merged local tx. */
  readonly #aggregatedSummaryCache = new Set<string>();
  /** Immutable rows revisited by enqueue, summary, and validation phases. */
  readonly #reusedSubtreeCache = new Map<string, ReusedSubtreeRow | undefined>();
  readonly #manifestBackingCache = new Map<string, BackingRow>();
  readonly #reusedSummaryCache = new Map<string, ReusableSummaryRow | undefined>();
  readonly #batchedReconciliationLeases = new Set<string>();
  /** Fully validated fresh and source-authenticated objects for the durable local path. */
  readonly #trustedObjects = new Map<
    string,
    { readonly hash: Uint8Array; readonly length: number }
  >();
  readonly #trustedObjectsAccounted = new Set<string>();
  constructor(
    tx: FilesystemSQLiteTransaction,
    limits: StorageLimits,
    cache?: ContentCache,
    hashBytes: HashFunction = sha256,
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#cache = cache;
    this.#hashBytes = hashBytes;
    this.#content = new ContentRepository(tx, limits, cache, hashBytes);
  }

  /** The manifest repository may update the same certificate in this tx. */
  invalidateCertificateCache(leaseId?: string): void {
    if (leaseId === undefined) this.#certificateCache.clear();
    else this.#certificateCache.delete(leaseId);
  }

  applyCertificatePatch(
    leaseId: string,
    patch: {
      readonly chainDigest: Uint8Array;
      readonly chainFold: Uint8Array;
      readonly objectCount: number;
      readonly objectBytes: number;
      readonly nodeCount: number;
      readonly nodeBytes: number;
      readonly membershipCount: number;
    },
  ): void {
    const row = this.#row(leaseId);
    if (row.sealed !== 0) throw new Error("ECORRUPT: certificate is already sealed");
    this.#cacheCertificatePatch(leaseId, {
      chain_digest: copyBytes(patch.chainDigest),
      chain_fold: copyBytes(patch.chainFold),
      object_count: patch.objectCount,
      object_bytes: patch.objectBytes,
      node_count: patch.nodeCount,
      node_bytes: patch.nodeBytes,
      membership_count: patch.membershipCount,
      next_sequence: patch.membershipCount,
    });
    if (!this.#batchIngestAccounting)
      this.#tx.run(
        "UPDATE efs_staging_certificates SET chain_digest=?,chain_fold=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=? WHERE lease_id=? AND sealed=0",
        [
          patch.chainDigest,
          patch.chainFold,
          patch.objectCount,
          patch.objectBytes,
          patch.nodeCount,
          patch.nodeBytes,
          patch.membershipCount,
          patch.membershipCount,
          leaseId,
        ],
      );
  }

  #cacheCertificatePatch(leaseId: string, patch: Partial<CertificateRow>): void {
    const current = this.#certificateCache.get(leaseId);
    if (current) {
      this.#certificateCache.set(leaseId, { ...current, ...patch } as CertificateRow);
      if (this.#batchIngestAccounting) this.#batchedCertificateDirty = true;
    }
  }

  /**
   * Enables the durable local-rebuild transfer path. Its ingest reservation
   * and staged-byte changes are exact opposites within one write transaction,
   * so the usage counters can be updated once at the transaction boundary.
   */
  enableBatchedIngestAccounting(): void {
    this.#batchIngestAccounting = true;
    this.#batchMetadataAccounting = true;
    beginMetadataChargeBatch(this.#tx);
    beginUsageMutationBatch(this.#tx, this.#limits);
  }

  flushBatchedIngestAccounting(includeCertificate = true): void {
    if (!this.#batchIngestAccounting) return;
    if (includeCertificate) this.flushBatchedCertificate();
    if (this.#batchedIngestBytes !== 0) {
      const bytes = this.#batchedIngestBytes;
      this.#batchedIngestBytes = 0;
      new UsageRepository(this.#tx, this.#limits).apply(
        { staging_bytes: bytes, ingest_reservation_bytes: -bytes },
        "durable local-rebuild ingest transfer",
      );
    }
    flushMetadataChargeBatch(this.#tx, this.#limits);
    for (const leaseId of this.#batchedReconciliationLeases)
      this.#flushBatchedReconciliation(leaseId);
  }

  flushBatchedUsageAccounting(): void {
    flushUsageMutationBatch(this.#tx, this.#limits);
  }

  flushBatchedCertificate(): void {
    if (!this.#batchIngestAccounting || !this.#batchedCertificateDirty) return;
    const leaseId = this.#ingestReservationLease ?? this.#metadataReservationLease;
    const row = leaseId ? this.#certificateCache.get(leaseId) : undefined;
    if (leaseId && row) {
      this.#tx.run(
        "UPDATE efs_staging_certificates SET chain_digest=?,chain_fold=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=?,ingest_reservation_bytes=?,metadata_reservation_bytes=? WHERE lease_id=? AND sealed=0",
        [
          row.chain_digest,
          row.chain_fold,
          row.object_count,
          row.object_bytes,
          row.node_count,
          row.node_bytes,
          row.membership_count,
          row.next_sequence,
          row.ingest_reservation_bytes,
          row.metadata_reservation_bytes,
          leaseId,
        ],
      );
      this.#batchedIngestReservationBytes = 0;
      this.#batchedMetadataReservationBytes = 0;
      this.#batchedCertificateDirty = false;
    }
  }

  #updateReconciliationAggregate(
    leaseId: string,
    sql: string,
    bindings: readonly SqliteValue[],
  ): void {
    if (this.#batchIngestAccounting) {
      this.#batchedReconciliationLeases.add(leaseId);
      return;
    }
    this.#tx.run(sql, bindings);
  }

  #flushBatchedReconciliation(leaseId: string): void {
    if (!this.#batchedReconciliationLeases.has(leaseId)) return;
    const state = this.#reconciliation(leaseId);
    if (!state) throw new Error("ECORRUPT: missing staging reconciliation");
    this.#tx.run(
      "UPDATE efs_staging_reconciliations SET next_sequence=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,closure_fold=? WHERE lease_id=?",
      [
        state.next_sequence,
        state.object_count,
        state.object_bytes,
        state.node_count,
        state.node_bytes,
        state.membership_count,
        state.closure_fold,
        leaseId,
      ],
    );
    this.#batchedReconciliationLeases.delete(leaseId);
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
    readonly ingestReservationBytes?: number;
    readonly metadataReservationBytes?: number;
  }): void {
    this.#batchedCertificateDirty = false;
    this.#batchedStagedBytes = 0;
    this.#leaseExpiresAt = options.expiresAt;
    this.#certificateCache.delete(options.leaseId);
    this.#reconciliationCache.delete(options.leaseId);
    for (const key of this.#reconciliationQueueCache.keys())
      if (key.startsWith(`${options.leaseId}:`))
        this.#reconciliationQueueCache.delete(key);
    for (const key of this.#aggregatedSummaryCache)
      if (key.startsWith(`${options.leaseId}:`))
        this.#aggregatedSummaryCache.delete(key);
    for (const key of this.#reusedSubtreeCache.keys())
      if (key.startsWith(`${options.leaseId}:`)) this.#reusedSubtreeCache.delete(key);
    for (const key of this.#manifestBackingCache.keys())
      if (key.startsWith(`${options.leaseId}:`)) this.#manifestBackingCache.delete(key);
    for (const key of this.#reusedSummaryCache.keys())
      if (key.startsWith(`${options.leaseId}:`)) this.#reusedSummaryCache.delete(key);
    stagingId(options.leaseId, "staging lease id");
    stagingId(options.ownerId, "staging owner id");
    if (options.branchId !== undefined)
      stagingId(options.branchId, "staging branch id");
    if (intrinsicByteLength(options.ownerNonce) !== 16)
      throw new RangeError("staging lease identity or owner nonce is invalid");
    counters([options.now, options.expiresAt]);
    counters([
      options.ingestReservationBytes ?? 0,
      options.metadataReservationBytes ?? 0,
    ]);
    if (options.expiresAt <= options.now)
      throw new RangeError("staging lease expiry must be in the future");
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        charged_metadata_bytes:
          2 * CHARGED_ROW_BYTES + (options.metadataReservationBytes ?? 0),
      },
      "staging lease, certificate, and metadata envelope",
    );
    if (options.ingestReservationBytes)
      new UsageRepository(this.#tx, this.#limits).apply(
        { ingest_reservation_bytes: options.ingestReservationBytes },
        "declared streamed-ingest envelope",
      );
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
      "INSERT INTO efs_staging_certificates(lease_id,owner_nonce,manifest_hash,chain_digest,chain_fold,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified,ingest_reservation_bytes,metadata_reservation_bytes) VALUES(?,?,NULL,?,?,0,0,0,0,0,0,0,0,?,?)",
      [
        options.leaseId,
        options.ownerNonce,
        EMPTY_STAGING_CHAIN,
        EMPTY_CLOSURE_FOLD,
        options.ingestReservationBytes ?? 0,
        options.metadataReservationBytes ?? 0,
      ],
    );
    this.#metadataReservationLease = options.leaseId;
    this.#metadataReservationBytes = options.metadataReservationBytes ?? 0;
    this.#ingestReservationLease = options.leaseId;
    this.#ingestReservationBytes = options.ingestReservationBytes ?? 0;
    // The certificate row was inserted from these exact values above. Seed
    // the transaction-local cache so the first reservation/member operation
    // does not immediately reread the row it just created.
    this.#certificateCache.set(
      options.leaseId,
      Object.freeze({
        owner_nonce: copyBytes(options.ownerNonce),
        manifest_hash: null,
        chain_digest: copyBytes(EMPTY_STAGING_CHAIN),
        chain_fold: copyBytes(EMPTY_CLOSURE_FOLD),
        object_count: 0,
        object_bytes: 0,
        node_count: 0,
        node_bytes: 0,
        membership_count: 0,
        next_sequence: 0,
        sealed: 0,
        verified: 0,
        ingest_reservation_bytes: options.ingestReservationBytes ?? 0,
        metadata_reservation_bytes: options.metadataReservationBytes ?? 0,
      }),
    );
  }

  consumeMetadataReservation(
    leaseId: string,
    ownerNonce: Uint8Array,
    bytes: number,
  ): void {
    counters([bytes]);
    if (bytes === 0) return;
    const row = this.#row(leaseId);
    if (!equalBytes(row.owner_nonce, ownerNonce) || row.sealed !== 0)
      throw new Error("ECORRUPT: metadata reservation owner mismatch");
    this.#metadataReservationLease = leaseId;
    const available =
      this.#metadataReservation(leaseId) ?? row.metadata_reservation_bytes;
    if (bytes > available)
      throw new Error("ENOSPC: durable metadata exceeds its declared envelope");
    this.#metadataReservationBytes = available - bytes;
    this.#cacheCertificatePatch(leaseId, {
      metadata_reservation_bytes: available - bytes,
    });
    if (this.#batchMetadataAccounting) {
      this.#batchedMetadataReservationBytes = checkedAdd(
        this.#batchedMetadataReservationBytes,
        bytes,
        "batched metadata reservation consumption",
      );
    } else {
      this.#tx.run(
        "UPDATE efs_staging_certificates SET metadata_reservation_bytes=metadata_reservation_bytes-? WHERE lease_id=? AND sealed=0",
        [bytes, leaseId],
      );
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: -bytes },
        "durable metadata reservation consumption",
      );
    }
  }

  consumeIngestReservation(
    leaseId: string,
    ownerNonce: Uint8Array,
    bytes: number,
  ): void {
    counters([bytes]);
    if (bytes === 0) return;
    const row = this.#row(leaseId);
    if (!equalBytes(row.owner_nonce, ownerNonce) || row.sealed !== 0)
      throw new Error("ECORRUPT: streamed-ingest reservation owner mismatch");
    this.#ingestReservationLease = leaseId;
    const available =
      this.#batchIngestAccounting && this.#ingestReservationBytes !== undefined
        ? this.#ingestReservationBytes
        : row.ingest_reservation_bytes;
    if (bytes > available)
      throw new Error("ENOSPC: streamed ingest exceeds its declared durable envelope");
    this.#ingestReservationBytes = available - bytes;
    this.#cacheCertificatePatch(leaseId, {
      ingest_reservation_bytes: available - bytes,
    });
    if (this.#batchIngestAccounting) {
      this.#batchedIngestReservationBytes = checkedAdd(
        this.#batchedIngestReservationBytes,
        bytes,
        "batched ingest reservation consumption",
      );
      this.#batchedIngestBytes = checkedAdd(
        this.#batchedIngestBytes,
        bytes,
        "batched ingest transfer",
      );
    } else {
      this.#tx.run(
        "UPDATE efs_staging_certificates SET ingest_reservation_bytes=ingest_reservation_bytes-? WHERE lease_id=? AND sealed=0",
        [bytes, leaseId],
      );
      new UsageRepository(this.#tx, this.#limits).apply(
        { ingest_reservation_bytes: -bytes },
        "streamed-ingest reservation consumption",
      );
    }
  }

  putEntry(
    leaseId: string,
    entryIndex: number,
    objectHash: Uint8Array,
    length: number,
  ): void {
    this.putEntriesBatch(leaseId, [Object.freeze({ entryIndex, objectHash, length })]);
  }
  putEntriesBatch(
    leaseId: string,
    entries: readonly {
      readonly entryIndex: number;
      readonly objectHash: Uint8Array;
      readonly length: number;
    }[],
  ): void {
    if (entries.length === 0) return;
    if (entries.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("staging entry batch exceeds configured row limit");
    this.#changeMetadataRows(entries.length, "staging entry", leaseId);
    this.#tx.run(
      `INSERT INTO efs_staging_entries(lease_id,entry_index,object_hash,length) VALUES ${entries
        .map(() => "(?,?,?,?)")
        .join(",")}`,
      entries.flatMap((entry) => [
        leaseId,
        entry.entryIndex,
        entry.objectHash,
        entry.length,
      ]),
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
    this.putLevelRecordsBatch(leaseId, level, [
      Object.freeze({ recordIndex, nodeHash, span, entryCount }),
    ]);
  }
  putLevelRecordsBatch(
    leaseId: string,
    level: number,
    records: readonly {
      readonly recordIndex: number;
      readonly nodeHash: Uint8Array;
      readonly span: number;
      readonly entryCount: number;
    }[],
  ): void {
    if (records.length === 0) return;
    if (records.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("staging level-record batch exceeds configured row limit");
    this.#changeMetadataRows(records.length, "staging level record", leaseId);
    this.#tx.run(
      `INSERT INTO efs_staging_level_records(lease_id,level,record_index,node_hash,span,entry_count) VALUES ${records
        .map(() => "(?,?,?,?,?,?)")
        .join(",")}`,
      records.flatMap((record) => [
        leaseId,
        level,
        record.recordIndex,
        record.nodeHash,
        record.span,
        record.entryCount,
      ]),
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
      { maintenance_bytes: CHARGED_ROW_BYTES + intrinsicByteLength(rootId) },
      "root journal",
      { preserveMaintenanceBytes: MAINTENANCE_TOTAL_EMERGENCY_BYTES },
    );
    const generation = advanceRootMutationGeneration(this.#tx);
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
      [generation!, kind, rootId],
    );
  }

  /** Optimistic root-generation handoff for a trusted local edit. */
  bumpRootFromSnapshot(kind: number, id: string, expectedGeneration: number): void {
    if (!Number.isSafeInteger(expectedGeneration) || expectedGeneration < 0)
      throw new RangeError("invalid root mutation generation snapshot");
    const rootId = encodeUtf8(id);
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: CHARGED_ROW_BYTES + intrinsicByteLength(rootId) },
      "root journal",
      { preserveMaintenanceBytes: MAINTENANCE_TOTAL_EMERGENCY_BYTES },
    );
    const next = expectedGeneration + 1;
    if (!Number.isSafeInteger(next))
      throw new Error("ENOSPC: root mutation generation space exhausted");
    const updated = this.#tx.run(
      "UPDATE efs_meta SET root_mutation_generation=? WHERE singleton=1 AND root_mutation_generation=?",
      [next, expectedGeneration],
    );
    const generation =
      updated.changes === 1
        ? next
        : advanceRootMutationGeneration(this.#tx);
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
      [generation, kind, rootId],
    );
  }
  release(
    leaseId: string,
    ownerNonce: Uint8Array,
    requireSealed: boolean,
    validated?: ValidatedSealedLease,
  ): boolean {
    const charge =
      validated?.leaseId === leaseId && equalBytes(validated.ownerNonce, ownerNonce)
        ? {
            owner_nonce: validated.ownerNonce,
            state: 1,
            staged_bytes: validated.stagedBytes,
            ingest_reservation_bytes: validated.ingestReservationBytes,
            metadata_reservation_bytes: validated.metadataReservationBytes,
          }
        : this.#leaseCharge(leaseId);
    if (!charge || !equalBytes(charge.owner_nonce, ownerNonce)) return false;
    const releasable = requireSealed
      ? charge.state === 1
      : charge.state === 0 || charge.state === 1;
    if (releasable) {
      this.#scheduleCleanup(leaseId, ownerNonce, charge.staged_bytes, 0);
      const result = this.#tx.run(
        "UPDATE efs_leases SET state=2 WHERE id=? AND owner_nonce=? AND state IN (0,1)",
        [leaseId, ownerNonce],
      );
      if (result.changes !== 1)
        throw new Error("ECORRUPT: staging lease tombstone changed unexpectedly");
      this.#releaseLeaseReservations(
        leaseId,
        charge.staged_bytes,
        charge.ingest_reservation_bytes,
        charge.metadata_reservation_bytes,
      );
      this.bumpRoot(6, leaseId);
    } else if (charge.state === 2) {
      this.#scheduleCleanup(leaseId, ownerNonce, 0, 0);
    }
    return releasable;
  }
  delete(leaseId: string, ownerNonce: Uint8Array): boolean {
    const charge = this.#leaseCharge(leaseId);
    if (!charge || !equalBytes(charge.owner_nonce, ownerNonce)) return false;
    const releasable = charge.state === 0 || charge.state === 1;
    if (releasable) {
      this.#scheduleCleanup(leaseId, ownerNonce, charge.staged_bytes, 0);
      const result = this.#tx.run(
        "UPDATE efs_leases SET state=2 WHERE id=? AND owner_nonce=? AND state IN (0,1)",
        [leaseId, ownerNonce],
      );
      if (result.changes !== 1)
        throw new Error("ECORRUPT: staging lease tombstone changed unexpectedly");
      if (charge.state === 0 || charge.state === 1)
        this.#releaseLeaseReservations(
          leaseId,
          charge.staged_bytes,
          charge.ingest_reservation_bytes,
          charge.metadata_reservation_bytes,
        );
      this.bumpRoot(6, leaseId);
    } else if (charge.state === 2) {
      this.#scheduleCleanup(leaseId, ownerNonce, 0, 0);
    }
    return releasable;
  }
  acquireReadLease(
    leaseId: string,
    ownerId: string,
    manifestHash: Uint8Array,
    expiresAt: number,
  ): void {
    stagingId(leaseId, "read lease id");
    stagingId(ownerId, "read lease owner id");
    if (intrinsicByteLength(manifestHash) !== 32)
      throw new RangeError("read lease manifest hash must contain exactly 32 bytes");
    this.#changeMetadataRows(2, "read lease and root link");
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
    const lease = this.#tx.all<{ owner_nonce: Uint8Array; state: number } & SqliteRow>(
      "SELECT owner_nonce,state FROM efs_leases WHERE id=? AND owner_id=?",
      [leaseId, ownerId],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    if (!lease) return false;
    const releasable = lease.state === 0 || lease.state === 1;
    if (releasable) {
      this.#scheduleCleanup(leaseId, lease.owner_nonce, 0, 0);
      const result = this.#tx.run(
        "UPDATE efs_leases SET state=2 WHERE id=? AND owner_id=? AND state IN (0,1)",
        [leaseId, ownerId],
      );
      if (result.changes !== 1)
        throw new Error("ECORRUPT: read lease tombstone changed unexpectedly");
      this.bumpRoot(3, leaseId);
    } else if (lease.state === 2) {
      this.#scheduleCleanup(leaseId, lease.owner_nonce, 0, 0);
    }
    return releasable;
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
      "SELECT l.id,l.state,l.owner_nonce,COALESCE(c.node_bytes+(SELECT coalesce(sum(o.size),0) FROM efs_lease_objects o WHERE o.lease_id=l.id),0) staged_bytes,COALESCE(c.ingest_reservation_bytes,0) ingest_reservation_bytes,COALESCE(c.metadata_reservation_bytes,0) metadata_reservation_bytes FROM efs_leases l LEFT JOIN efs_staging_certificates c ON c.lease_id=l.id LEFT JOIN efs_lease_cleanups x ON x.lease_id=l.id WHERE x.lease_id IS NULL AND (l.expires_at_ms<? OR l.state=2) ORDER BY l.id LIMIT ?",
      [now, limit],
      { maxRows: limit, maxBytes: Math.max(1024, limit * 256) },
    );
    let releasedBytes = 0;
    let releasedIngestBytes = 0;
    let releasedMetadataBytes = 0;
    let tombstoned = 0;
    for (const row of rows) {
      this.#scheduleCleanup(
        row.id,
        row.owner_nonce,
        row.state === 0 || row.state === 1 ? row.staged_bytes : 0,
        now,
      );
      const result = this.#tx.run(
        "UPDATE efs_leases SET state=2 WHERE id=? AND (expires_at_ms<? OR state=2)",
        [row.id, now],
      );
      if (result.changes) {
        if (row.ingest_reservation_bytes || row.metadata_reservation_bytes)
          this.#tx.run(
            "UPDATE efs_staging_certificates SET ingest_reservation_bytes=0,metadata_reservation_bytes=0 WHERE lease_id=? AND sealed=0",
            [row.id],
          );
        tombstoned += 1;
        if (row.state === 0 || row.state === 1)
          releasedBytes = checkedAdd(releasedBytes, row.staged_bytes);
        if (row.state === 0 || row.state === 1)
          releasedIngestBytes = checkedAdd(
            releasedIngestBytes,
            row.ingest_reservation_bytes,
          );
        if (row.state === 0 || row.state === 1)
          releasedMetadataBytes = checkedAdd(
            releasedMetadataBytes,
            row.metadata_reservation_bytes,
          );
      } else {
        throw new Error("ECORRUPT: expired lease tombstone changed unexpectedly");
      }
    }
    this.#releaseLeaseReservations(
      "",
      releasedBytes,
      releasedIngestBytes,
      releasedMetadataBytes,
    );
    if (tombstoned) this.bumpRoot(6, `expired:${now}`);
    return tombstoned;
  }

  cleanupBatch(limit: number): LeaseCleanupProgress {
    counters([limit]);
    if (
      limit <= 0 ||
      limit > this.#limits.maxGcBatchSize ||
      limit > this.#limits.maxQueryBatchSize
    )
      throw new RangeError("invalid lease-cleanup batch limit");
    const cleanup = this.#tx.all<CleanupRow>(
      "SELECT lease_id,phase FROM efs_lease_cleanups ORDER BY lease_id LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    if (!cleanup)
      return Object.freeze({ worked: false, deletedRows: 0, deletedLeases: 0 });
    if (!Number.isSafeInteger(cleanup.phase) || cleanup.phase < 0 || cleanup.phase > 12)
      throw new Error("ECORRUPT: invalid lease cleanup phase");
    if (cleanup.phase < CLEANUP_DELETE_STATEMENTS.length) {
      const deletedRows = this.#tx.run(CLEANUP_DELETE_STATEMENTS[cleanup.phase]!, [
        cleanup.lease_id,
        cleanup.lease_id,
        limit,
      ]).changes;
      this.#changeMetadataRows(-deletedRows, "bounded lease child cleanup");
      if (deletedRows < limit) this.#advanceCleanup(cleanup.lease_id, cleanup.phase);
      return Object.freeze({ worked: true, deletedRows, deletedLeases: 0 });
    }
    if (cleanup.phase === 10) {
      const deletedRows = this.#tx.run(
        "DELETE FROM efs_staging_reconciliations WHERE lease_id=?",
        [cleanup.lease_id],
      ).changes;
      this.#changeMetadataRows(-deletedRows, "staging reconciliation cleanup");
      this.#advanceCleanup(cleanup.lease_id, cleanup.phase);
      return Object.freeze({ worked: true, deletedRows, deletedLeases: 0 });
    }
    if (cleanup.phase === 11) {
      const deletedRows = this.#tx.run(
        "DELETE FROM efs_staging_workspaces WHERE lease_id=?",
        [cleanup.lease_id],
      ).changes;
      this.#changeMetadataRows(-deletedRows, "staging workspace cleanup");
      this.#advanceCleanup(cleanup.lease_id, cleanup.phase);
      return Object.freeze({ worked: true, deletedRows, deletedLeases: 0 });
    }
    const remaining = this.#tx.all<{ count: number } & SqliteRow>(
      "SELECT (SELECT count(*) FROM efs_staging_entries WHERE lease_id=?)+(SELECT count(*) FROM efs_staging_level_records WHERE lease_id=?)+(SELECT count(*) FROM efs_staging_reconciliation_queue WHERE lease_id=?)+(SELECT count(*) FROM efs_staging_manifest_validation_queue WHERE lease_id=?)+(SELECT count(*) FROM efs_staging_reused_subtrees WHERE lease_id=?)+(SELECT count(*) FROM efs_lease_objects WHERE lease_id=?)+(SELECT count(*) FROM efs_lease_staged_manifests WHERE lease_id=?)+(SELECT count(*) FROM efs_lease_manifests WHERE lease_id=?)+(SELECT count(*) FROM efs_lease_cow_pages WHERE lease_id=?)+(SELECT count(*) FROM efs_lease_patches WHERE lease_id=?)+(SELECT count(*) FROM efs_staging_reconciliations WHERE lease_id=?)+(SELECT count(*) FROM efs_staging_workspaces WHERE lease_id=?) count",
      Array.from({ length: 12 }, () => cleanup.lease_id),
      { maxRows: 1, maxBytes: 256 },
    )[0]?.count;
    if (remaining !== 0)
      throw new Error("ECORRUPT: lease cleanup reached parent with live children");
    const deletedRows = this.#tx.run(
      "DELETE FROM efs_staging_certificates WHERE lease_id=?",
      [cleanup.lease_id],
    ).changes;
    const deletedLeases = this.#tx.run(
      "DELETE FROM efs_leases WHERE id=? AND state=2",
      [cleanup.lease_id],
    ).changes;
    if (deletedLeases !== 1)
      throw new Error("ECORRUPT: tombstoned lease disappeared during cleanup");
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        maintenance_bytes: -CHARGED_ROW_BYTES,
        charged_metadata_bytes: -(deletedRows + deletedLeases) * CHARGED_ROW_BYTES,
      },
      "lease cleanup completion",
    );
    return Object.freeze({ worked: true, deletedRows, deletedLeases });
  }

  appendBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
  ): ClosureCertificate {
    return this.#appendBatch(leaseId, ownerNonce, members, true);
  }

  /**
   * Append members freshly produced by a local rebuild in a new staging
   * lease. Lease-local membership probes are redundant for this narrow
   * caller, but immutable backing validation remains enabled and the
   * INSERT OR IGNORE change check still rejects an unexpected duplicate.
   * Generic callers must use appendBatch().
   */
  appendFreshBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
    verifiedBacking?: VerifiedFreshBacking,
  ): ClosureCertificate {
    if (members.some((member) => member.counted === true))
      throw new Error("ECORRUPT: fresh local batch cannot contain count-only members");
    return this.#appendBatch(
      leaseId,
      ownerNonce,
      members,
      true,
      verifiedBacking?.nodeSizes,
      true,
      verifiedBacking?.objectSizes,
      verifiedBacking?.rootSizes,
    );
  }

  /**
   * Append already-authenticated boundary objects of a local rebuild without
   * rescanning CAS. The caller must have derived these members from the
   * authenticated source manifest protected by the same lease; the regular
   * appendBatch API remains the validating path for arbitrary callers.
   */
  appendCountedBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
  ): ClosureCertificate {
    if (members.some((member) => member.kind !== "object" || member.counted !== true))
      throw new Error("ECORRUPT: trusted counted batch contains a non-counted member");
    return this.#appendBatch(leaseId, ownerNonce, members, false);
  }

  /**
   * Append source-authenticated manifest nodes after one immutable-content
   * size lookup. This is intentionally narrower than appendBatch: callers
   * still provide only source-authenticated reused nodes, while this method
   * verifies that every hash exists and that its declared size matches the
   * immutable node before the normal staging admission and closure accounting
   * run.
   */
  appendReusedManifestBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    nodeHashes: readonly Uint8Array[],
  ): ClosureCertificate & {
    readonly verifiedNodeSizes: ReadonlyMap<string, number>;
  } {
    return this.#appendReusedManifestBatch(
      leaseId,
      ownerNonce,
      nodeHashes,
      false,
    );
  }

  /**
   * Append source-authenticated reused nodes for the local rebuild path. The
   * caller has just created the lease and has not appended these nodes before,
   * so the lease-local duplicate-membership probe is unnecessary. Immutable
   * node existence and size validation remain enabled.
   */
  appendTrustedReusedManifestBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    nodeHashes: readonly Uint8Array[],
  ): ClosureCertificate & {
    readonly verifiedNodeSizes: ReadonlyMap<string, number>;
  } {
    return this.#appendReusedManifestBatch(
      leaseId,
      ownerNonce,
      nodeHashes,
      true,
    );
  }

  #appendReusedManifestBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    nodeHashes: readonly Uint8Array[],
    skipExistingMembershipCheck: boolean,
  ): ClosureCertificate & {
    readonly verifiedNodeSizes: ReadonlyMap<string, number>;
  } {
    const hashes = [
      ...new Map(nodeHashes.map((hash) => [bytesToHex(hash), hash])).values(),
    ];
    if (hashes.length !== nodeHashes.length)
      throw new Error("ECORRUPT: duplicate reused manifest node");
    if (hashes.some((hash) => intrinsicByteLength(hash) !== 32))
      throw new RangeError("reused manifest node hash must be 32 bytes");
    if (hashes.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("reused manifest node batch exceeds query limit");
    if (hashes.length === 0)
      return Object.freeze({
        ...this.snapshot(leaseId, ownerNonce),
        verifiedNodeSizes: new Map<string, number>(),
      });
    const placeholders = hashes.map(() => "?").join(",");
    const rows = this.#tx.all<{ hash?: Uint8Array; size: number } & SqliteRow>(
      `SELECT hash,length(encoded) size FROM efs_manifest_nodes WHERE hash IN (${placeholders})`,
      hashes,
      {
        maxRows: hashes.length + 1,
        maxBytes: Math.max(4096, hashes.length * 96),
      },
    );
    const sizes = new Map(rows.map((row) => [bytesToHex(row.hash!), row.size]));
    if (sizes.size !== hashes.length)
      throw new Error("ECORRUPT: reused subtree node is missing");
    const members = hashes.map((hash) => ({
      kind: "manifest-node" as const,
      hash,
      size: sizes.get(bytesToHex(hash))!,
    }));
    const certificate = this.#appendBatch(
      leaseId,
      ownerNonce,
      members,
      true,
      sizes,
      skipExistingMembershipCheck,
    );
    return Object.freeze({ ...certificate, verifiedNodeSizes: sizes });
  }

  registerTrustedObjects(
    objects: readonly { readonly hash: Uint8Array; readonly length: number }[],
  ): void {
    for (const object of objects) {
      if (intrinsicByteLength(object.hash) !== 32)
        throw new RangeError(
          "trusted counted object hash must contain exactly 32 bytes",
        );
      if (!Number.isSafeInteger(object.length) || object.length < 0)
        throw new RangeError(
          "trusted counted object length must be a nonnegative safe integer",
        );
      const key = bytesToHex(object.hash);
      const existing = this.#trustedObjects.get(key);
      if (existing && existing.length !== object.length)
        throw new Error("ECORRUPT: trusted counted object size changed");
      this.#trustedObjects.set(
        key,
        Object.freeze({ hash: copyBytes(object.hash), length: object.length }),
      );
    }
  }

  cacheReusedSubtreeMetadata(
    leaseId: string,
    nodeHashes: readonly Uint8Array[],
    metadata?: readonly ReusedSubtreeCacheMetadata[],
    verifiedNodeSizes?: ReadonlyMap<string, number>,
  ): void {
    const hashes = [
      ...new Map(nodeHashes.map((hash) => [bytesToHex(hash), hash])).values(),
    ];
    if (hashes.length === 0) return;
    if (hashes.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("reused subtree metadata batch exceeds query limit");
    if (metadata !== undefined) {
      if (!verifiedNodeSizes)
        throw new Error("ECORRUPT: reused subtree metadata lacks verified sizes");
      const metadataByHash = new Map(
        metadata.map((value) => [bytesToHex(value.nodeHash), value]),
      );
      if (metadataByHash.size !== hashes.length)
        throw new Error("ECORRUPT: reused subtree metadata batch is incomplete");
      for (const hash of hashes) {
        const key = bytesToHex(hash);
        const value = metadataByHash.get(key);
        const size = verifiedNodeSizes.get(key);
        if (!value || size === undefined)
          throw new Error(
            "ECORRUPT: reused subtree metadata is missing verified backing",
          );
        if (
          intrinsicByteLength(value.nodeHash) !== 32 ||
          intrinsicByteLength(value.sourceManifestHash) !== 32 ||
          intrinsicByteLength(value.sourcePath) > this.#limits.maxManifestDepth ||
          value.sourcePath.some((index) => index > 255) ||
          !Number.isSafeInteger(value.span) ||
          value.span < 0 ||
          !Number.isSafeInteger(value.entryCount) ||
          value.entryCount < 0
        )
          throw new Error("ECORRUPT: invalid reused subtree metadata");
        this.#manifestBackingCache.set(
          `${leaseId}:2:${key}`,
          Object.freeze({ stored_size: size, membership_size: size }),
        );
        this.#reusedSubtreeCache.set(
          `${leaseId}:${key}`,
          Object.freeze({
            source_manifest_hash: copyBytes(value.sourceManifestHash),
            source_path: copyBytes(value.sourcePath),
            span: value.span,
            entry_count: value.entryCount,
            validated_nonfinal_leaf_delta: value.validatedNonfinalLeafDelta,
            validated_final_leaf_delta: value.validatedFinalLeafDelta,
            summary_usable: value.summaryUsable ? 1 : 0,
          }),
        );
        const summary = value.summary;
        this.#reusedSummaryCache.set(
          `${leaseId}:2:${key}`,
          summary
            ? Object.freeze({
                object_count: summary.objectCount,
                object_bytes: summary.objectBytes,
                node_count: summary.nodeCount,
                node_bytes: summary.nodeBytes,
                membership_count: summary.membershipCount,
                closure_fold: copyBytes(summary.closureFold),
              })
            : undefined,
        );
      }
      return;
    }
    const placeholders = hashes.map(() => "?").join(",");
    const rows = this.#tx.all<
      {
        node_hash?: Uint8Array;
        stored_size: number;
        membership_size: number;
        source_manifest_hash: Uint8Array | null;
        source_path: Uint8Array | null;
        span: number | null;
        entry_count: number | null;
        validated_nonfinal_leaf_delta: number | null;
        validated_final_leaf_delta: number | null;
        summary_usable: number | null;
        object_count: number | null;
        object_bytes: number | null;
        node_count: number | null;
        node_bytes: number | null;
        membership_count: number | null;
        closure_fold: Uint8Array | null;
      } & SqliteRow
    >(
      `SELECT n.hash node_hash,length(n.encoded) stored_size,m.size membership_size,r.source_manifest_hash,r.source_path,r.span,r.entry_count,r.validated_nonfinal_leaf_delta,r.validated_final_leaf_delta,r.summary_usable,s.object_count,s.object_bytes,s.node_count,s.node_bytes,s.membership_count,s.closure_fold FROM efs_manifest_nodes n JOIN efs_lease_staged_manifests m ON m.lease_id=? AND m.kind=1 AND m.manifest_hash=n.hash LEFT JOIN efs_staging_reused_subtrees r ON r.lease_id=? AND r.node_hash=n.hash LEFT JOIN efs_manifest_subtree_summaries s ON s.node_hash=n.hash WHERE n.hash IN (${placeholders})`,
      [leaseId, leaseId, ...hashes],
      {
        maxRows: hashes.length + 1,
        maxBytes: Math.max(4096, hashes.length * 4096),
      },
    );
    const rowsByHash = new Map(rows.map((row) => [bytesToHex(row.node_hash!), row]));
    for (const hash of hashes) {
      const row = rowsByHash.get(bytesToHex(hash));
      if (!row || row.stored_size !== row.membership_size)
        throw new Error(
          "ECORRUPT: reused subtree metadata is absent or has a mismatched size",
        );
      const key = `${leaseId}:2:${bytesToHex(hash)}`;
      this.#manifestBackingCache.set(
        key,
        Object.freeze({
          stored_size: row.stored_size,
          membership_size: row.membership_size,
        }),
      );
      const claim =
        row.source_manifest_hash &&
        row.source_path &&
        row.span !== null &&
        row.entry_count !== null &&
        row.summary_usable !== null
          ? Object.freeze({
              source_manifest_hash: copyBytes(row.source_manifest_hash),
              source_path: copyBytes(row.source_path),
              span: row.span,
              entry_count: row.entry_count,
              validated_nonfinal_leaf_delta: row.validated_nonfinal_leaf_delta,
              validated_final_leaf_delta: row.validated_final_leaf_delta,
              summary_usable: row.summary_usable,
            })
          : undefined;
      this.#reusedSubtreeCache.set(`${leaseId}:${bytesToHex(hash)}`, claim);
      const summary =
        row.object_count !== null &&
        row.object_bytes !== null &&
        row.node_count !== null &&
        row.node_bytes !== null &&
        row.membership_count !== null &&
        row.closure_fold !== null
          ? Object.freeze({
              object_count: row.object_count,
              object_bytes: row.object_bytes,
              node_count: row.node_count,
              node_bytes: row.node_bytes,
              membership_count: row.membership_count,
              closure_fold: copyBytes(row.closure_fold),
            })
          : undefined;
      this.#reusedSummaryCache.set(key, summary);
    }
  }

  #appendBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
    validateCountedMembers: boolean,
    verifiedNodeSizes?: ReadonlyMap<string, number>,
    skipExistingMembershipCheck = false,
    verifiedObjectSizes?: ReadonlyMap<string, number>,
    verifiedRootSizes?: ReadonlyMap<string, number>,
  ): ClosureCertificate {
    if (members.length === 0) return this.snapshot(leaseId, ownerNonce);
    if (members.length > this.#limits.maxQueryBatchSize)
      throw new RangeError(
        "staging membership batch exceeds configured query/binding limit",
      );
    const row = this.#row(leaseId);
    if (!equalBytes(row.owner_nonce, ownerNonce) || row.sealed !== 0)
      throw new Error("ECORRUPT: staging owner mismatch or certificate already sealed");
    if (this.#reconciliation(leaseId))
      throw new Error(
        "ECORRUPT: staged closure cannot change after reconciliation begins",
      );
    const seen = new Set<string>();
    let chain = row.chain_digest;
    let chainFold = row.chain_fold;
    let sequence = row.next_sequence;
    let objectCount = row.object_count;
    let objectBytes = row.object_bytes;
    let nodeCount = row.node_count;
    let nodeBytes = row.node_bytes;
    let stagedDelta = 0;
    let insertedRows = 0;
    // Every chain member has the same 32-byte previous digest plus the
    // canonical 49-byte member record. Reuse the input buffer; hashBytes is
    // synchronous and returns an owned digest before the next member writes it.
    const chainScratch = new Uint8Array(81);
    const objects: StagingMember[] = [];
    const nodes: StagingMember[] = [];
    for (const member of members) {
      if (intrinsicByteLength(member.hash) !== 32)
        throw new RangeError("staging member hash must be 32 bytes");
      counters([member.size]);
      const key = `${member.kind}:${Array.from(member.hash, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
      if (seen.has(key)) throw new Error("duplicate staging member in one batch");
      seen.add(key);
      if (member.kind === "object") objects.push(member);
      else nodes.push(member);
    }
    // Count-only members (the durable boundary records of the rebuilt
    // closure) are split out first: they extend the chain and the counts but
    // produce no membership row, no metadata charge, and no staging-byte
    // admission. The regular appendBatch path checks their CAS backing and
    // rejects an existing full member; the local-rebuild trusted path has
    // already authenticated them through the protected source closure.
    const countedObjects = objects.filter((member) => member.counted === true);
    const fullObjects = objects.filter((member) => member.counted !== true);
    if (countedObjects.length) {
      if (validateCountedMembers) {
        const existing = this.#existingObjectMembers(leaseId, countedObjects);
        for (const member of countedObjects)
          if (existing.has(bytesToHex(member.hash)))
            throw new Error(
              "ECORRUPT: counted closure member is already a full staged member",
            );
      }
      if (validateCountedMembers) this.#verifyCountedBacking(countedObjects);
      for (const member of countedObjects) {
        objectCount += 1;
        objectBytes = checkedAdd(objectBytes, member.size);
        chain = extendChain(chain, sequence, member, this.#hashBytes, chainScratch);
        chainFold = foldHashes(chainFold, member.hash);
        sequence += 1;
      }
    }
    // Membership writes are batched per binding budget: one existing-member
    // lookup and one multi-row insert per chunk instead of per-member
    // statements. The write transaction is IMMEDIATE, so the pre-query and
    // the insert observe the same durable state.
    const existingObjects = skipExistingMembershipCheck
      ? new Set<string>()
      : this.#existingObjectMembers(leaseId, fullObjects);
    const freshObjects = fullObjects.filter(
      (member) => !existingObjects.has(bytesToHex(member.hash)),
    );
    if (verifiedObjectSizes) {
      for (const member of freshObjects)
        if (verifiedObjectSizes.get(bytesToHex(member.hash)) !== member.size)
          throw new Error(
            "ECORRUPT: staged membership does not match immutable content",
          );
    } else this.#verifyObjectBacking(freshObjects);
    for (const chunk of membershipInsertChunks(
      freshObjects,
      4,
      this.#limits.maxQueryBatchSize,
    )) {
      const inserted = this.#tx.run(
        `INSERT OR IGNORE INTO efs_lease_objects(lease_id,object_hash,sequence,size) VALUES ${chunk
          .map(() => "(?,?,?,?)")
          .join(",")}`,
        chunk.flatMap((member, index) => [
          leaseId,
          member.hash,
          sequence + index,
          member.size,
        ]),
      );
      if (inserted.changes !== chunk.length)
        throw new Error("ECORRUPT: staging membership changed during batched insert");
      for (const member of chunk) {
        objectCount += 1;
        objectBytes = checkedAdd(objectBytes, member.size);
        stagedDelta = checkedAdd(stagedDelta, member.size);
        insertedRows += 1;
        chain = extendChain(chain, sequence, member, this.#hashBytes, chainScratch);
        chainFold = foldHashes(chainFold, member.hash);
        sequence += 1;
      }
    }
    const existingNodes = skipExistingMembershipCheck
      ? new Set<string>()
      : this.#existingNodeMembers(leaseId, nodes);
    const freshNodes = nodes.filter(
      (member) => !existingNodes.has(bytesToHex(member.hash)),
    );
    this.#verifyNodeBacking(freshNodes, verifiedNodeSizes, verifiedRootSizes);
    // The backing query above is authoritative for this write transaction.
    // Seed the immutable-node caches while the verified sizes are already in
    // hand; reconciliation otherwise re-joins the same node for every queue
    // edge merely to discover that it is not a reused claim.
    for (const member of freshNodes) {
      if (member.kind !== "manifest-node") continue;
      const hash = bytesToHex(member.hash);
      const key = `${leaseId}:2:${hash}`;
      this.#manifestBackingCache.set(
        key,
        Object.freeze({ stored_size: member.size, membership_size: member.size }),
      );
      this.#reusedSubtreeCache.set(`${leaseId}:${hash}`, undefined);
      this.#reusedSummaryCache.set(key, undefined);
    }
    for (const chunk of membershipInsertChunks(
      freshNodes,
      5,
      this.#limits.maxQueryBatchSize,
    )) {
      const inserted = this.#tx.run(
        `INSERT OR IGNORE INTO efs_lease_staged_manifests(lease_id,kind,manifest_hash,sequence,size) VALUES ${chunk
          .map(() => "(?,?,?,?,?)")
          .join(",")}`,
        chunk.flatMap((member, index) => [
          leaseId,
          member.kind === "manifest-root" ? 0 : 1,
          member.hash,
          sequence + index,
          member.size,
        ]),
      );
      if (inserted.changes !== chunk.length)
        throw new Error("ECORRUPT: staging membership changed during batched insert");
      for (const member of chunk) {
        nodeCount += 1;
        nodeBytes = checkedAdd(nodeBytes, member.size);
        stagedDelta = checkedAdd(stagedDelta, member.size);
        insertedRows += 1;
        chain = extendChain(chain, sequence, member, this.#hashBytes, chainScratch);
        chainFold = foldHashes(chainFold, member.hash);
        sequence += 1;
      }
    }
    if (row.ingest_reservation_bytes)
      this.consumeIngestReservation(leaseId, ownerNonce, stagedDelta);
    this.#admitStagingBytes(stagedDelta);
    this.#changeMetadataRows(insertedRows, "staging membership", leaseId);
    if (!this.#batchIngestAccounting)
      this.#tx.run(
        "UPDATE efs_staging_certificates SET chain_digest=?,chain_fold=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=? WHERE lease_id=? AND sealed=0",
        [
          chain,
          chainFold,
          objectCount,
          objectBytes,
          nodeCount,
          nodeBytes,
          sequence,
          sequence,
          leaseId,
        ],
      );
    this.#cacheCertificatePatch(leaseId, {
      chain_digest: chain,
      chain_fold: chainFold,
      object_count: objectCount,
      object_bytes: objectBytes,
      node_count: nodeCount,
      node_bytes: nodeBytes,
      membership_count: sequence,
      next_sequence: sequence,
    });
    return Object.freeze({
      leaseId,
      ownerNonce: copyBytes(ownerNonce),
      manifestHash: new Uint8Array(32),
      chainDigest: chain,
      chainFold,
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
      ownerNonce: copyBytes(ownerNonce),
      manifestHash: row.manifest_hash?.slice() ?? new Uint8Array(32),
      chainDigest: row.chain_digest,
      chainFold: row.chain_fold,
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
    if (intrinsicByteLength(manifestHash) !== 32)
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
    this.#changeMetadataRows(1, "staging reconciliation state", leaseId);
    this.#tx.run(
      "INSERT INTO efs_staging_reconciliations(lease_id,owner_nonce,manifest_hash,next_sequence,object_count,object_bytes,node_count,node_bytes,membership_count,complete) VALUES(?,?,?,0,0,0,0,0,0,0)",
      [leaseId, ownerNonce, manifestHash],
    );
    this.#reconciliationCache.set(
      leaseId,
      Object.freeze({
        owner_nonce: copyBytes(ownerNonce),
        manifest_hash: copyBytes(manifestHash),
        next_sequence: 0,
        object_count: 0,
        object_bytes: 0,
        node_count: 0,
        node_bytes: 0,
        membership_count: 0,
        complete: 0,
        leaf_depth: null,
        closure_fold: copyBytes(EMPTY_CLOSURE_FOLD),
      }),
    );
    this.#enqueueVerified(leaseId, 1, manifestHash, undefined, undefined);
  }

  /**
   * Start the local merged-rebuild proof without creating reconciliation
   * queues. The caller is restricted to the trusted completion method below;
   * generic and streamed staging always uses beginReconciliation().
   */
  beginTrustedReconciliation(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void {
    if (!this.#batchIngestAccounting)
      throw new Error("ECORRUPT: trusted reconciliation requires merged accounting");
    if (intrinsicByteLength(manifestHash) !== 32)
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
    this.#changeMetadataRows(1, "staging reconciliation state", leaseId);
    this.#tx.run(
      "INSERT INTO efs_staging_reconciliations(lease_id,owner_nonce,manifest_hash,next_sequence,object_count,object_bytes,node_count,node_bytes,membership_count,complete) VALUES(?,?,?,0,0,0,0,0,0,0)",
      [leaseId, ownerNonce, manifestHash],
    );
    this.#reconciliationCache.set(
      leaseId,
      Object.freeze({
        owner_nonce: copyBytes(ownerNonce),
        manifest_hash: copyBytes(manifestHash),
        next_sequence: 0,
        object_count: 0,
        object_bytes: 0,
        node_count: 0,
        node_bytes: 0,
        membership_count: 0,
        complete: 0,
        leaf_depth: null,
        closure_fold: copyBytes(EMPTY_CLOSURE_FOLD),
      }),
    );
  }

  /**
   * Complete a local rebuild by validating its fresh spine and authenticated
   * reused boundaries directly. This keeps the durable certificate checks
   * (counts, bytes, membership cardinality, closure fold, canonical nodes,
   * and balanced leaf depth) while avoiding transient queue rows. A missing
   * reusable summary deliberately falls back to the generic queued proof.
   */
  completeTrustedLocalReconciliation(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
    freshNodeHashes: readonly Uint8Array[],
    rootSize: number,
    leafDepth: number,
  ): ReconciliationProgress {
    if (!this.#batchIngestAccounting)
      throw new Error("ECORRUPT: trusted reconciliation requires merged accounting");
    if (
      intrinsicByteLength(manifestHash) !== 32 ||
      !Number.isSafeInteger(rootSize) ||
      rootSize < 0 ||
      rootSize > this.#limits.maxManifestNodeBytes ||
      !Number.isSafeInteger(leafDepth) ||
      leafDepth < 1 ||
      leafDepth > this.#limits.maxManifestDepth
    )
      throw new RangeError("invalid trusted reconciliation identity");
    if (freshNodeHashes.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("trusted reconciliation exceeds its node limit");
    const state = this.#reconciliation(leaseId);
    const certificate = this.#row(leaseId);
    if (
      !state ||
      !equalBytes(state.owner_nonce, ownerNonce) ||
      !equalBytes(state.manifest_hash, manifestHash) ||
      state.complete !== 0 ||
      certificate.sealed !== 0
    )
      throw new Error("ECORRUPT: trusted reconciliation owner or state mismatch");
    if (
      state.next_sequence !== 0 ||
      state.object_count !== 0 ||
      state.object_bytes !== 0 ||
      state.node_count !== 0 ||
      state.node_bytes !== 0 ||
      state.membership_count !== 0 ||
      !equalBytes(state.closure_fold, EMPTY_CLOSURE_FOLD)
    )
      throw new Error("ECORRUPT: trusted reconciliation was partially populated");
    const decodeRoot = this.#content.withManifestRoot(manifestHash, (encoded) =>
      decodeManifestRoot(encoded, manifestHash),
    );
    if (!decodeRoot) throw new Error("ECORRUPT: trusted manifest root is missing");
    validateSupportedManifestParameters(decodeRoot.parameters);
    if (
      decodeRoot.fileSize > this.#limits.maxFileBytes ||
      decodeRoot.entryCount > this.#limits.maxManifestEntries ||
      (decodeRoot.fileSize === 0) !== (decodeRoot.entryCount === 0)
    )
      throw new Error("ECORRUPT: trusted manifest root totals are invalid");

    const fresh = new Set<string>();
    for (const hash of freshNodeHashes) {
      if (intrinsicByteLength(hash) !== 32)
        throw new RangeError("trusted fresh node hash must contain 32 bytes");
      const key = bytesToHex(hash);
      if (!fresh.add(key)) throw new Error("ECORRUPT: duplicate trusted fresh node");
    }
    const claimRows = this.#tx.all<{ node_hash?: Uint8Array } & SqliteRow>(
      "SELECT node_hash FROM efs_staging_reused_subtrees WHERE lease_id=?",
      [leaseId],
      {
        maxRows: this.#limits.maxQueryBatchSize + 1,
        maxBytes: Math.max(1024, this.#limits.maxQueryBatchSize * 64),
      },
    );
    if (claimRows.length > this.#limits.maxQueryBatchSize)
      throw new RangeError("trusted reconciliation exceeds its claim limit");
    const claimBytes = new Map(
      claimRows.map((row) => [bytesToHex(row.node_hash!), row.node_hash!]),
    );
    const claims = new Set(claimBytes.keys());
    const queuedFallback = (): ReconciliationProgress => {
      this.#enqueueVerified(leaseId, 1, manifestHash, undefined, undefined);
      let complete = false;
      let processed = 0;
      while (!complete) {
        const progress = this.reconcileBatch(
          leaseId,
          ownerNonce,
          Math.max(1, Math.floor((this.#limits.maxFinalTransactionRows - 12) / 5)),
          { skipObjectBackingCheck: true },
        );
        processed += progress.processed;
        complete = progress.complete;
      }
      return Object.freeze({ processed, complete: true });
    };
    // A summary is the only compact representation of a reused subtree's
    // exact descendant closure. If one claim lacks it, retain the generic
    // queued proof inside this same transaction; this is still a safe local
    // path, but it is deliberately not called queue-free.
    for (const hash of claims) {
      const metadata = this.#loadReusedNodeMetadata(leaseId, claimBytes.get(hash)!);
      if (!metadata.claim || metadata.claim.summary_usable !== 1 || !metadata.summary)
        return queuedFallback();
    }
    // The fresh spine was already inserted and size-authenticated by the
    // trusted staging path. Warm its immutable encoded rows in one bounded
    // batch; visit() still decodes and canonical-validates every node.
    this.#content.warmManifestNodeBatch(freshNodeHashes);
    const visitedFresh = new Set<string>();
    const visitedClaims = new Set<string>();
    const visitedObjects = new Set<string>();
    const freshNodeBytes = new Map<string, number>();
    let objectCount = 0;
    let objectBytes = 0;
    let nodeCount = 0;
    let nodeBytes = 0;
    let membershipCount = 0;
    let closureFold = copyBytes(EMPTY_CLOSURE_FOLD);
    let observedLeafDepth: number | undefined;
    const addObject = (hash: Uint8Array, length: number): void => {
      const key = bytesToHex(hash);
      const trusted = this.#trustedObjects.get(key);
      if (!trusted || trusted.length !== length)
        throw new Error("ECORRUPT: trusted closure object is not authenticated");
      if (visitedObjects.has(key)) return;
      visitedObjects.add(key);
      objectCount += 1;
      objectBytes = checkedAdd(objectBytes, length, "trusted closure object bytes");
      membershipCount += 1;
      closureFold = foldHashes(closureFold, hash);
    };
    const addSummary = (
      hash: Uint8Array,
      size: number,
      depth: number,
      finalAtLevel: boolean,
      expected: ManifestChild,
    ): void => {
      const key = bytesToHex(hash);
      const metadata = this.#loadReusedNodeMetadata(leaseId, hash);
      const claim = metadata.claim;
      if (
        !claim ||
        !claims.has(key) ||
        claim.span !== expected.span ||
        claim.entry_count !== expected.entryCount ||
        metadata.backing.stored_size !== size ||
        claim.summary_usable !== 1
      )
        throw new RangeError("trusted reconciliation requires an authenticated reusable summary");
      const delta = finalAtLevel
        ? (claim.validated_nonfinal_leaf_delta ?? claim.validated_final_leaf_delta)
        : claim.validated_nonfinal_leaf_delta;
      if (delta === null || delta === undefined || depth + delta !== leafDepth)
        throw new Error("ECORRUPT: trusted reused subtree has invalid leaf depth");
      if (visitedClaims.has(key)) return;
      const summary = metadata.summary;
      if (
        !summary ||
        !Number.isSafeInteger(summary.object_count) ||
        !Number.isSafeInteger(summary.object_bytes) ||
        !Number.isSafeInteger(summary.node_count) ||
        !Number.isSafeInteger(summary.node_bytes) ||
        !Number.isSafeInteger(summary.membership_count) ||
        summary.object_count < 0 ||
        summary.node_count < 0 ||
        summary.object_count + summary.node_count !== summary.membership_count ||
        intrinsicByteLength(summary.closure_fold) !== 32
      )
        throw new RangeError("trusted reconciliation reusable summary is invalid");
      visitedClaims.add(key);
      nodeCount += 1;
      nodeBytes = checkedAdd(nodeBytes, size, "trusted closure node bytes");
      membershipCount += 1;
      closureFold = foldHashes(closureFold, hash);
      objectCount = checkedAdd(objectCount, summary.object_count, "trusted summary objects");
      objectBytes = checkedAdd(objectBytes, summary.object_bytes, "trusted summary object bytes");
      nodeCount = checkedAdd(nodeCount, summary.node_count, "trusted summary nodes");
      nodeBytes = checkedAdd(nodeBytes, summary.node_bytes, "trusted summary node bytes");
      membershipCount = checkedAdd(
        membershipCount,
        summary.membership_count,
        "trusted summary membership",
      );
      closureFold = foldHashes(closureFold, summary.closure_fold);
    };
    const seenNodes = new Set<string>();
    const visit = (
      hash: Uint8Array,
      depth: number,
      finalAtLevel: boolean,
      rootNode: boolean,
      expected?: ManifestChild,
    ): void => {
      const key = bytesToHex(hash);
      if (!fresh.has(key)) {
        const backing = this.#manifestBacking(leaseId, 2, hash);
        addSummary(
          hash,
          backing.stored_size,
          depth,
          finalAtLevel,
          expected ?? {
            hash,
            span: decodeRoot.fileSize,
            entryCount: decodeRoot.entryCount,
          },
        );
        return;
      }
      const node = this.#content.withManifestNode(hash, (encoded) => {
        freshNodeBytes.set(key, intrinsicByteLength(encoded));
        return decodeManifestNode(encoded, hash);
      });
      if (!node) throw new Error("ECORRUPT: trusted fresh manifest node is missing");
      if (
        expected &&
        (node.span !== expected.span || node.entryCount !== expected.entryCount)
      )
        throw new Error("ECORRUPT: trusted fresh node totals mismatch");
      validateCanonicalManifestNode(node, decodeRoot.parameters, finalAtLevel, rootNode);
      if (!seenNodes.has(key)) {
        seenNodes.add(key);
        nodeCount += 1;
        nodeBytes = checkedAdd(nodeBytes, freshNodeBytes.get(key)!, "trusted fresh node bytes");
        membershipCount += 1;
        closureFold = foldHashes(closureFold, hash);
      }
      if (node.kind === "leaf") {
        if (observedLeafDepth === undefined) observedLeafDepth = depth;
        else if (observedLeafDepth !== depth)
          throw new Error("ECORRUPT: trusted manifest tree is unbalanced");
        for (const entry of node.entries) addObject(entry.hash, entry.length);
        return;
      }
      for (let index = 0; index < node.children.length; index += 1) {
        const child = node.children[index]!;
        visit(
          child.hash,
          depth + 1,
          finalAtLevel && index === node.children.length - 1,
          false,
          child,
        );
      }
    };
    nodeCount = 1;
    nodeBytes = rootSize;
    membershipCount = 1;
    closureFold = foldHashes(closureFold, manifestHash);
    visit(decodeRoot.rootNodeHash, 1, true, true);
    if (observedLeafDepth !== undefined && observedLeafDepth !== leafDepth)
      throw new Error("ECORRUPT: trusted manifest leaf depth disagrees");
    if (observedLeafDepth === undefined && visitedClaims.size === 0)
      throw new Error("ECORRUPT: trusted manifest has no leaf proof");
    for (const key of fresh) if (!seenNodes.has(key))
      throw new Error("ECORRUPT: trusted fresh node is unreachable");
    for (const key of claims) if (!visitedClaims.has(key))
      throw new Error("ECORRUPT: trusted reused node is unreachable");
    if (
      objectCount !== certificate.object_count ||
      objectBytes !== certificate.object_bytes ||
      nodeCount !== certificate.node_count ||
      nodeBytes !== certificate.node_bytes ||
      membershipCount !== certificate.membership_count ||
      membershipCount !== objectCount + nodeCount ||
      !equalBytes(closureFold, certificate.chain_fold)
    )
      throw new Error("ECORRUPT: trusted manifest closure differs from staged membership");
    const validation = this.#tx.run(
      "INSERT OR IGNORE INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
      [manifestHash, leafDepth],
    );
    this.#changeMetadataRows(validation.changes, "manifest validation certificate", leaseId);
    // A newly inserted certificate is already checked by SQLite's typed
    // binding and tree-depth constraint.  Only re-read when INSERT OR IGNORE
    // found an existing row, where its immutable value still needs to be
    // compared.  This avoids one SELECT on every fresh local rebuild while
    // retaining collision validation.
    if (validation.changes === 0) {
      const certified = this.#content.validatedManifestDepth(manifestHash);
      if (certified !== leafDepth)
        throw new Error("ECORRUPT: trusted manifest validation certificate disagrees");
    }
    this.#tx.run(
      "UPDATE efs_staging_reconciliations SET next_sequence=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,closure_fold=?,leaf_depth=?,complete=1 WHERE lease_id=? AND complete=0",
      [
        membershipCount,
        objectCount,
        objectBytes,
        nodeCount,
        nodeBytes,
        membershipCount,
        closureFold,
        leafDepth,
        leaseId,
      ],
    );
    this.#cacheReconciliationPatch(leaseId, {
      next_sequence: membershipCount,
      object_count: objectCount,
      object_bytes: objectBytes,
      node_count: nodeCount,
      node_bytes: nodeBytes,
      membership_count: membershipCount,
      closure_fold: copyBytes(closureFold),
      leaf_depth: leafDepth,
      complete: 1,
    });
    return Object.freeze({
      processed: freshNodeHashes.length + claimRows.length + visitedObjects.size,
      complete: true,
    });
  }

  reconcileBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    workLimit: number,
    options: ReconcileBatchOptions = {},
  ): ReconciliationProgress {
    if (
      !Number.isSafeInteger(workLimit) ||
      workLimit <= 0 ||
      workLimit > Math.floor((this.#limits.maxFinalTransactionRows - 12) / 5)
    )
      throw new RangeError("invalid reconciliation work limit");
    // Keep the result page bounded by the adapter query limit. The local
    // trusted path may skip the CAS backing probe, but it must not enlarge the
    // queue result envelope beyond the same row/byte admission profile.
    const queryLimit = Math.min(
      workLimit,
      this.#batchIngestAccounting
        ? this.#limits.maxQueryBatchSize * 4
        : this.#limits.maxQueryBatchSize,
    );
    const state = this.#reconciliation(leaseId);
    if (!state || !equalBytes(state.owner_nonce, ownerNonce))
      throw new Error("ECORRUPT: reconciliation owner mismatch");
    if (state.complete === 1) return Object.freeze({ processed: 0, complete: true });
    const queue = this.#tx.all<QueueRow>(
      "SELECT kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor FROM efs_staging_reconciliation_queue WHERE lease_id=? AND processed=0 ORDER BY sequence LIMIT ?",
      [leaseId, queryLimit],
      { maxRows: queryLimit, maxBytes: Math.max(1024, queryLimit * 320) },
    );
    const nodeHashes = queue
      .filter((item) => item.kind === 2 && item.hash !== undefined)
      .map((item) => item.hash!);
    // Fresh nodes still go through the full decode and canonical validation
    // below. Warm their immutable encoded rows in one bounded IN query so the
    // per-node validation loop does not reopen one SQLite read per node.
    this.#content.warmManifestNodeBatch(nodeHashes);
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
    // Reused subtrees are authenticated when their claims are registered.
    // Aggregate all summary-backed queue members visible in this bounded
    // batch with one summary lookup and one reconciliation update. The queue
    // member itself was already counted by #enqueueVerified; the summary
    // contributes only its descendants, exactly as the previous per-claim
    // path did.
    const summaryCandidates: Array<{ readonly hash: Uint8Array }> = [];
    for (const item of queue) {
      if (remaining <= 0 || item.kind !== 2 || item.edge_cursor !== 1 || !item.hash)
        continue;
      const backing = this.#manifestBacking(leaseId, item.kind, item.hash);
      if (backing.stored_size !== item.declared_size)
        throw new Error("ECORRUPT: reconciled manifest size changed");
      const claim = this.#reusedSubtree(leaseId, item.hash);
      if (claim?.summary_usable === 1) summaryCandidates.push({ hash: item.hash });
    }
    const summaryProcessed = new Set<string>();
    if (summaryCandidates.length) {
      const summaryPlaceholders = summaryCandidates.map(() => "?").join(",");
      const summariesByHash = new Map<string, ReusableSummaryRow>();
      const missing: Uint8Array[] = [];
      for (const candidate of summaryCandidates) {
        const key = `${leaseId}:2:${bytesToHex(candidate.hash)}`;
        if (this.#reusedSummaryCache.has(key)) {
          const cached = this.#reusedSummaryCache.get(key);
          if (!cached)
            throw new Error("ECORRUPT: reusable subtree summary is missing or invalid");
          summariesByHash.set(bytesToHex(candidate.hash), cached);
        } else missing.push(candidate.hash);
      }
      if (missing.length) {
        const placeholders = missing.map(() => "?").join(",");
        const summaries = this.#tx.all<
          {
            node_hash?: Uint8Array;
            object_count: number;
            object_bytes: number;
            node_count: number;
            node_bytes: number;
            membership_count: number;
            closure_fold: Uint8Array;
            chain_digest: Uint8Array;
          } & SqliteRow
        >(
          `SELECT node_hash,object_count,object_bytes,node_count,node_bytes,membership_count,closure_fold,chain_digest FROM efs_manifest_subtree_summaries WHERE node_hash IN (${placeholders})`,
          missing,
          {
            maxRows: missing.length + 1,
            maxBytes: Math.max(4096, missing.length * 4096),
          },
        );
        for (const summary of summaries) {
          const key = bytesToHex(summary.node_hash!);
          const cached = Object.freeze({
            object_count: summary.object_count,
            object_bytes: summary.object_bytes,
            node_count: summary.node_count,
            node_bytes: summary.node_bytes,
            membership_count: summary.membership_count,
            closure_fold: copyBytes(summary.closure_fold),
          });
          summariesByHash.set(key, cached);
          this.#reusedSummaryCache.set(`${leaseId}:2:${key}`, cached);
        }
      }
      let objectCount = 0;
      let objectBytes = 0;
      let nodeCount = 0;
      let nodeBytes = 0;
      let membershipCount = 0;
      const closureFold = new Uint8Array(32);
      for (const candidate of summaryCandidates) {
        const summary = summariesByHash.get(bytesToHex(candidate.hash));
        if (
          !summary ||
          !Number.isSafeInteger(summary.object_count) ||
          !Number.isSafeInteger(summary.object_bytes) ||
          !Number.isSafeInteger(summary.node_count) ||
          !Number.isSafeInteger(summary.node_bytes) ||
          !Number.isSafeInteger(summary.membership_count) ||
          intrinsicByteLength(summary.closure_fold) !== 32
        )
          throw new Error("ECORRUPT: reusable subtree summary is missing or invalid");
        objectCount = checkedAdd(objectCount, summary.object_count);
        objectBytes = checkedAdd(objectBytes, summary.object_bytes);
        nodeCount = checkedAdd(nodeCount, summary.node_count);
        nodeBytes = checkedAdd(nodeBytes, summary.node_bytes);
        membershipCount = checkedAdd(membershipCount, summary.membership_count);
        closureFold.set(foldHashes(closureFold, summary.closure_fold));
        summaryProcessed.add(bytesToHex(candidate.hash));
      }
      const current = this.#reconciliation(leaseId)!;
      this.#updateReconciliationAggregate(
        leaseId,
        "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+?,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+?,node_bytes=node_bytes+?,membership_count=membership_count+?,closure_fold=? WHERE lease_id=? AND complete=0",
        [
          membershipCount,
          objectCount,
          objectBytes,
          nodeCount,
          nodeBytes,
          membershipCount,
          foldHashes(current.closure_fold, closureFold),
          leaseId,
        ],
      );
      this.#cacheReconciliationPatch(leaseId, {
        next_sequence: current.next_sequence + membershipCount,
        object_count: current.object_count + objectCount,
        object_bytes: current.object_bytes + objectBytes,
        node_count: current.node_count + nodeCount,
        node_bytes: current.node_bytes + nodeBytes,
        membership_count: current.membership_count + membershipCount,
        closure_fold: foldHashes(current.closure_fold, closureFold),
      });
      this.#tx.run(
        `UPDATE efs_staging_reconciliation_queue SET processed=1 WHERE lease_id=? AND kind=2 AND processed=0 AND hash IN (${summaryPlaceholders})`,
        [leaseId, ...summaryCandidates.map((candidate) => candidate.hash)],
      );
      remaining -= summaryCandidates.length;
      processed += summaryCandidates.length;
    }
    for (const item of queue) {
      if (remaining <= 0) break;
      if (item.kind === 0) continue;
      if (item.hash && summaryProcessed.has(bytesToHex(item.hash))) continue;
      const backing = this.#manifestBacking(leaseId, item.kind, item.hash);
      if (backing.stored_size !== item.declared_size)
        throw new Error("ECORRUPT: reconciled manifest size changed");
      if (item.kind === 1) {
        const root = this.#content.withManifestRoot(item.hash, (encoded) =>
          decodeManifestRoot(encoded, item.hash),
        );
        if (!root) throw new Error("ECORRUPT: reconciled manifest root is missing");
        if (
          root.fileSize > this.#limits.maxFileBytes ||
          root.entryCount > this.#limits.maxManifestEntries
        )
          throw new RangeError("manifest root exceeds configured storage limits");
        this.#enqueueVerified(
          leaseId,
          2,
          root.rootNodeHash,
          root.fileSize,
          root.entryCount,
        );
        this.#enqueueValidation(
          leaseId,
          new Uint8Array(0),
          root.rootNodeHash,
          root.fileSize,
          root.entryCount,
          1,
          true,
        );
        this.#tx.run(
          "UPDATE efs_staging_reconciliation_queue SET edge_cursor=1,processed=1 WHERE lease_id=? AND kind=1 AND hash=? AND processed=0",
          [leaseId, item.hash],
        );
        remaining -= 1;
        processed += 1;
        continue;
      }
      const node = this.#content.withManifestNode(item.hash, (encoded) =>
        decodeManifestNode(encoded, item.hash),
      );
      if (!node) throw new Error("ECORRUPT: reconciled manifest node is missing");
      if (
        item.declared_span !== node.span ||
        item.declared_entry_count !== node.entryCount
      )
        throw new Error("ECORRUPT: manifest child declaration mismatch");
      const edgeCount =
        node.kind === "leaf" ? node.entries.length : node.children.length;
      let end = item.edge_cursor;
      let edgeUnits = remaining * 4;
      if (node.kind === "leaf") {
        // Batch the leaf's object edges: one queued lookup with hash IN (...),
        // one backing lookup with hash IN (...), and one multi-row queue insert,
        // instead of four statements per edge.
        const edges = node.entries;
        const remainingEdges = edges
          .slice(end)
          .filter((edge) => !this.#trustedObjects.has(bytesToHex(edge.hash)));
        const queuedByHash = new Map<string, QueueRow>();
        if (remainingEdges.length) {
          const placeholders = remainingEdges.map(() => "?").join(",");
          const rows = this.#tx.all<QueueRow>(
            `SELECT kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor FROM efs_staging_reconciliation_queue WHERE lease_id=? AND kind=? AND hash IN (${placeholders})`,
            [leaseId, 0, ...remainingEdges.map((edge) => edge.hash)],
            {
              maxRows: remainingEdges.length + 1,
              maxBytes: Math.max(1024, remainingEdges.length * 320 + 512),
            },
          );
          for (const row of rows) queuedByHash.set(bytesToHex(row.hash!), row);
        }
        const seen = new Set<string>();
        const seenLengths = new Map<string, number>();
        const trustedEdges: Array<{
          readonly hash: Uint8Array;
          readonly length: number;
        }> = [];
        const newEdges: Array<{ readonly hash: Uint8Array; readonly length: number }> =
          [];
        while (end < edgeCount && edgeUnits >= 4) {
          const edge = edges[end]!;
          const key = bytesToHex(edge.hash);
          const trusted = this.#trustedObjects.get(key);
          if (trusted) {
            if (trusted.length !== edge.length)
              throw new Error("ECORRUPT: trusted counted object size disagrees");
            if (!this.#trustedObjectsAccounted.has(key)) {
              this.#trustedObjectsAccounted.add(key);
              trustedEdges.push(trusted);
            }
            // The source-manifest protection and the local rebuild's full
            // leaf validation already authenticated this member. It is
            // present in the certificate, so only the reconciliation totals
            // need updating; no transient object queue row is required.
            edgeUnits -= 1;
          } else {
            const queued = queuedByHash.get(key);
            if (queued) {
              if (
                queued.declared_span !== edge.length ||
                queued.declared_entry_count !== 1
              )
                throw new Error("ECORRUPT: repeated manifest closure edge disagrees");
              edgeUnits -= 1;
            } else if (seen.has(key)) {
              if (seenLengths.get(key) !== edge.length)
                throw new Error("ECORRUPT: repeated manifest edge length disagrees");
              edgeUnits -= 1;
            } else {
              seen.add(key);
              seenLengths.set(key, edge.length);
              newEdges.push(edge);
              edgeUnits -= 4;
            }
          }
          processed += 1;
          end += 1;
        }
        if (trustedEdges.length) {
          let folded: Uint8Array = EMPTY_CLOSURE_FOLD;
          let bytes = 0;
          for (const edge of trustedEdges) {
            folded = foldHashes(folded, edge.hash);
            bytes = checkedAdd(bytes, edge.length);
          }
          const currentState = this.#reconciliation(leaseId)!;
          const closureFold = foldHashes(currentState.closure_fold, folded);
          this.#updateReconciliationAggregate(
            leaseId,
            "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+?,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+0,node_bytes=node_bytes+0,membership_count=membership_count+?,closure_fold=? WHERE lease_id=? AND complete=0",
            [
              trustedEdges.length,
              trustedEdges.length,
              bytes,
              trustedEdges.length,
              closureFold,
              leaseId,
            ],
          );
          this.#cacheReconciliationPatch(leaseId, {
            next_sequence: currentState.next_sequence + trustedEdges.length,
            object_count: currentState.object_count + trustedEdges.length,
            object_bytes: currentState.object_bytes + bytes,
            membership_count: currentState.membership_count + trustedEdges.length,
            closure_fold: closureFold,
          });
        }
        if (newEdges.length) {
          const placeholders = newEdges.map(() => "?").join(",");
          // Generic/streamed staging keeps the CAS size check here. A
          // durable local rebuild may skip it: its new full objects were
          // checked by appendBatch and its count-only boundary objects were
          // authenticated against the protected source closure before this
          // reconciliation begins. The closure fold and certificate totals
          // still bind the discovered edge set to the staged chain.
          if (!options.skipObjectBackingCheck) {
            const backing = this.#tx.all<
              { hash?: Uint8Array; size: number } & SqliteRow
            >(
              `SELECT hash,size FROM efs_cas_objects WHERE hash IN (${placeholders})`,
              newEdges.map((edge) => edge.hash),
              {
                maxRows: newEdges.length + 1,
                maxBytes: Math.max(1024, newEdges.length * 192 + 256),
              },
            );
            const sizes = new Map(
              backing.map((row) => [bytesToHex(row.hash!), row.size]),
            );
            for (const edge of newEdges)
              if (sizes.get(bytesToHex(edge.hash)) !== edge.length)
                throw new Error(
                  "ECORRUPT: object closure member is absent or has a mismatched size",
                );
          }
          let folded: Uint8Array = EMPTY_CLOSURE_FOLD;
          for (const edge of newEdges) folded = foldHashes(folded, edge.hash);
          const currentState = this.#reconciliation(leaseId)!;
          const inserted = this.#tx.run(
            `INSERT OR IGNORE INTO efs_staging_reconciliation_queue(lease_id,kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor,processed) VALUES ${newEdges
              .map(() => "(?,?,?,?,?,?,?,0,0)")
              .join(",")}`,
            newEdges.flatMap((edge, index) => [
              leaseId,
              0,
              edge.hash,
              currentState.next_sequence + index,
              edge.length,
              edge.length,
              1,
            ]),
          );
          if (inserted.changes !== newEdges.length)
            throw new Error(
              "ECORRUPT: reconciliation queue changed during batched insertion",
            );
          for (const [index, edge] of newEdges.entries()) {
            const cacheKey = this.#queueKey(leaseId, 0, edge.hash);
            const queueRow = Object.freeze({
              kind: 0,
              hash: copyBytes(edge.hash),
              sequence: currentState.next_sequence + index,
              declared_size: edge.length,
              declared_span: edge.length,
              declared_entry_count: 1,
              edge_cursor: 0,
            });
            this.#reconciliationQueueCache.set(cacheKey, queueRow);
          }
          const insertedBytes = newEdges.reduce(
            (sum, edge) => checkedAdd(sum, edge.length),
            0,
          );
          this.#changeMetadataRows(
            newEdges.length,
            "staging reconciliation queue",
            leaseId,
          );
          const closureFold = foldHashes(currentState.closure_fold, folded);
          this.#updateReconciliationAggregate(
            leaseId,
            "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+?,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+0,node_bytes=node_bytes+0,membership_count=membership_count+?,closure_fold=? WHERE lease_id=? AND complete=0",
            [
              newEdges.length,
              newEdges.length,
              insertedBytes,
              newEdges.length,
              closureFold,
              leaseId,
            ],
          );
          this.#cacheReconciliationPatch(leaseId, {
            next_sequence: currentState.next_sequence + newEdges.length,
            object_count: currentState.object_count + newEdges.length,
            object_bytes: currentState.object_bytes + insertedBytes,
            membership_count: currentState.membership_count + newEdges.length,
            closure_fold: closureFold,
          });
        }
      } else {
        while (end < edgeCount && edgeUnits >= 4) {
          const edge = node.children[end]!;
          const inserted = this.#enqueueVerified(
            leaseId,
            2,
            edge.hash,
            edge.span,
            edge.entryCount,
          );
          edgeUnits -= inserted ? 4 : 1;
          processed += 1;
          end += 1;
        }
      }
      remaining = Math.floor(edgeUnits / 4);
      this.#tx.run(
        "UPDATE efs_staging_reconciliation_queue SET edge_cursor=?,processed=? WHERE lease_id=? AND kind=2 AND hash=? AND processed=0",
        [end, end === edgeCount ? 1 : 0, leaseId, item.hash],
      );
    }
    let pending =
      this.#tx.all(
        "SELECT sequence FROM efs_staging_reconciliation_queue WHERE lease_id=? AND processed=0 LIMIT 1",
        [leaseId],
        { maxRows: 1, maxBytes: 128 },
      ).length !== 0;
    if (!pending) {
      if (remaining > 0) {
        const validation = this.#validateManifestBatch(
          leaseId,
          state.manifest_hash,
          remaining,
        );
        processed += validation.processed;
        pending = !validation.complete;
      } else {
        pending =
          this.#tx.all(
            "SELECT path FROM efs_staging_manifest_validation_queue WHERE lease_id=? AND processed=0 LIMIT 1",
            [leaseId],
            { maxRows: 1, maxBytes: 128 },
          ).length !== 0;
      }
    }
    if (!pending) {
      const reconciled = this.#reconciliation(leaseId)!;
      const certificate = this.#row(leaseId);
      if (
        reconciled.object_count !== certificate.object_count ||
        reconciled.object_bytes !== certificate.object_bytes ||
        reconciled.node_count !== certificate.node_count ||
        reconciled.node_bytes !== certificate.node_bytes ||
        reconciled.membership_count !== certificate.membership_count ||
        reconciled.next_sequence !== certificate.membership_count ||
        !equalBytes(reconciled.closure_fold, certificate.chain_fold)
      ) {
        throw new Error(
          `ECORRUPT: complete manifest closure differs from staged membership (reconciled=${reconciled.object_count}/${reconciled.object_bytes}/${reconciled.node_count}/${reconciled.node_bytes}/${reconciled.membership_count}, certificate=${certificate.object_count}/${certificate.object_bytes}/${certificate.node_count}/${certificate.node_bytes}/${certificate.membership_count})`,
        );
      }
      if (reconciled.leaf_depth === null)
        throw new Error("ECORRUPT: manifest validation did not reach a leaf");
      const validation = this.#tx.run(
        "INSERT OR IGNORE INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
        [state.manifest_hash, reconciled.leaf_depth],
      );
      this.#changeMetadataRows(
        validation.changes,
        "manifest validation certificate",
        leaseId,
      );
      const certified = this.#content.validatedManifestDepth(state.manifest_hash);
      if (certified !== reconciled.leaf_depth)
        throw new Error("ECORRUPT: manifest validation certificate disagrees");
      this.#tx.run(
        "UPDATE efs_staging_reconciliations SET complete=1 WHERE lease_id=? AND complete=0",
        [leaseId],
      );
      this.#cacheReconciliationPatch(leaseId, { complete: 1 });
    }
    // The merged local-rebuild transaction keeps the reconciliation aggregate
    // in the in-memory cache across bounded batches.  `seal()` flushes that
    // cache before it validates the certificate, so writing the same aggregate
    // after every batch only adds redundant UPDATEs.  Standalone/generic
    // reconciliation has no deferred aggregate and is unchanged.
    if (!this.#batchIngestAccounting)
      for (const pendingLeaseId of this.#batchedReconciliationLeases)
        this.#flushBatchedReconciliation(pendingLeaseId);
    return Object.freeze({ processed, complete: !pending });
  }

  seal(certificate: ClosureCertificate): void {
    this.#validateShape(certificate);
    const row = this.#row(certificate.leaseId);
    if (
      !equalBytes(row.owner_nonce, certificate.ownerNonce) ||
      !equalBytes(row.chain_digest, certificate.chainDigest) ||
      !equalBytes(row.chain_fold, certificate.chainFold) ||
      row.object_count !== certificate.objectCount ||
      row.object_bytes !== certificate.objectBytes ||
      row.node_count !== certificate.nodeCount ||
      row.node_bytes !== certificate.nodeBytes ||
      row.membership_count !== certificate.membershipCount ||
      row.next_sequence !== certificate.membershipCount ||
      row.sealed !== 0
    )
      throw new Error("ECORRUPT: staged closure certificate mismatch");
    if (row.ingest_reservation_bytes)
      this.consumeIngestReservation(
        certificate.leaseId,
        certificate.ownerNonce,
        row.ingest_reservation_bytes,
      );
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
      reconciliation.next_sequence !== certificate.membershipCount ||
      !equalBytes(reconciliation.closure_fold, certificate.chainFold)
    )
      throw new Error(
        "ECORRUPT: staged closure reconciliation is incomplete or mismatched",
      );
    const rooted = this.#tx.run(
      "INSERT OR IGNORE INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)",
      [certificate.leaseId, certificate.manifestHash],
    );
    this.#changeMetadataRows(
      rooted.changes,
      "sealed staging root link",
      certificate.leaseId,
    );
    const remainingMetadataReservation = this.#row(
      certificate.leaseId,
    ).metadata_reservation_bytes;
    if (remainingMetadataReservation)
      this.consumeMetadataReservation(
        certificate.leaseId,
        certificate.ownerNonce,
        remainingMetadataReservation,
      );
    // The local merged path keeps certificate, reconciliation, and reservation
    // state authoritative in memory for this transaction. Flush their durable
    // accounting once, after all seal-time mutations, instead of publishing
    // the same aggregate before each validation step.
    this.flushBatchedIngestAccounting(false);
    const finalRow = this.#row(certificate.leaseId);
    this.#tx.run(
      "UPDATE efs_staging_certificates SET manifest_hash=?,chain_digest=?,chain_fold=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=?,ingest_reservation_bytes=?,metadata_reservation_bytes=?,sealed=1,verified=1 WHERE lease_id=? AND sealed=0",
      [
        certificate.manifestHash,
        finalRow.chain_digest,
        finalRow.chain_fold,
        finalRow.object_count,
        finalRow.object_bytes,
        finalRow.node_count,
        finalRow.node_bytes,
        finalRow.membership_count,
        finalRow.next_sequence,
        finalRow.ingest_reservation_bytes,
        finalRow.metadata_reservation_bytes,
        certificate.leaseId,
      ],
    );
    this.#tx.run("UPDATE efs_leases SET state=1 WHERE id=? AND state=0", [
      certificate.leaseId,
    ]);
    this.invalidateCertificateCache(certificate.leaseId);
  }

  /**
   * Seal a merged local-rebuild lease and carry the proof needed by the
   * immediate inode commit.  `seal()` has already checked the certificate,
   * complete reconciliation, rooted manifest, and zero reservations; the
   * only validation data not retained by that path is the exact staged-byte
   * total, which is counted as memberships are admitted in this transaction.
   * Generic and streamed callers continue to use `validateSealed()`.
   */
  sealAndValidate(
    certificate: ClosureCertificate,
    now: number,
  ): {
    readonly leaseId: string;
    readonly ownerNonce: Uint8Array;
    readonly stagedBytes: number;
    readonly ingestReservationBytes: number;
    readonly metadataReservationBytes: number;
    readonly expiresAtMs?: number;
  } {
    counters([now]);
    if (!this.#batchIngestAccounting)
      throw new Error("ECORRUPT: local sealed-lease proof requires batched accounting");
    if (this.#leaseExpiresAt !== undefined && this.#leaseExpiresAt < now)
      throw new Error("ECORRUPT: staging lease expired before sealing");
    const stagedBytes = this.#batchedStagedBytes;
    this.seal(certificate);
    this.#batchedStagedBytes = 0;
    return Object.freeze({
      leaseId: certificate.leaseId,
      ownerNonce: copyBytes(certificate.ownerNonce),
      stagedBytes,
      ingestReservationBytes: 0,
      metadataReservationBytes: 0,
      ...(this.#leaseExpiresAt === undefined
        ? {}
        : { expiresAtMs: this.#leaseExpiresAt }),
    });
  }

  validateSealed(certificate: ClosureCertificate, now = 0): ValidatedSealedLease {
    this.#validateShape(certificate);
    counters([now]);
    const row = this.#tx.all<CertificateRow & { staged_bytes: number }>(
      "SELECT c.owner_nonce,c.manifest_hash,c.chain_digest,c.chain_fold,c.object_count,c.object_bytes,c.node_count,c.node_bytes,c.membership_count,c.next_sequence,c.sealed,c.verified,c.ingest_reservation_bytes,c.metadata_reservation_bytes,l.owner_nonce lease_nonce,l.expires_at_ms,l.state,CASE WHEN m.manifest_hash IS NULL THEN 0 ELSE 1 END rooted,v.tree_depth validated_depth,COALESCE((SELECT coalesce(sum(o.size),0) FROM efs_lease_objects o WHERE o.lease_id=c.lease_id)+(SELECT coalesce(sum(sm.size),0) FROM efs_lease_staged_manifests sm WHERE sm.lease_id=c.lease_id),0) staged_bytes FROM efs_staging_certificates c JOIN efs_leases l ON l.id=c.lease_id LEFT JOIN efs_lease_manifests m ON m.lease_id=c.lease_id AND m.manifest_hash=c.manifest_hash LEFT JOIN efs_manifest_validations v ON v.manifest_hash=c.manifest_hash WHERE c.lease_id=?",
      [certificate.leaseId],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (
      !row ||
      row.sealed !== 1 ||
      row.verified !== 1 ||
      row.state !== 1 ||
      row.rooted !== 1 ||
      row.ingest_reservation_bytes !== 0 ||
      row.metadata_reservation_bytes !== 0 ||
      !Number.isSafeInteger(row.validated_depth) ||
      row.validated_depth! < 1 ||
      row.validated_depth! > this.#limits.maxManifestDepth ||
      row.expires_at_ms! < now ||
      !equalBytes(row.owner_nonce, certificate.ownerNonce) ||
      !equalBytes(row.lease_nonce!, certificate.ownerNonce) ||
      !row.manifest_hash ||
      !equalBytes(row.manifest_hash, certificate.manifestHash) ||
      !equalBytes(row.chain_digest, certificate.chainDigest) ||
      !equalBytes(row.chain_fold, certificate.chainFold) ||
      row.object_count !== certificate.objectCount ||
      row.object_bytes !== certificate.objectBytes ||
      row.node_count !== certificate.nodeCount ||
      row.node_bytes !== certificate.nodeBytes ||
      row.membership_count !== certificate.membershipCount ||
      row.next_sequence !== certificate.membershipCount
    )
      throw new Error("ECORRUPT: sealed closure certificate mismatch");
    return Object.freeze({
      leaseId: certificate.leaseId,
      ownerNonce: copyBytes(certificate.ownerNonce),
      stagedBytes: row.staged_bytes,
      ingestReservationBytes: row.ingest_reservation_bytes,
      metadataReservationBytes: row.metadata_reservation_bytes,
    });
  }

  #existingObjectMembers(
    leaseId: string,
    members: readonly StagingMember[],
  ): Set<string> {
    const existing = new Set<string>();
    for (const chunk of membershipInsertChunks(
      members,
      1,
      this.#limits.maxQueryBatchSize,
    )) {
      const placeholders = chunk.map(() => "?").join(",");
      const rows = this.#tx.all<{ object_hash?: Uint8Array } & SqliteRow>(
        `SELECT object_hash FROM efs_lease_objects WHERE lease_id=? AND object_hash IN (${placeholders})`,
        [leaseId, ...chunk.map((member) => member.hash)],
        {
          maxRows: chunk.length + 1,
          maxBytes: Math.max(1024, chunk.length * 96),
        },
      );
      for (const row of rows) existing.add(bytesToHex(row.object_hash!));
    }
    return existing;
  }

  #verifyObjectBacking(members: readonly StagingMember[]): void {
    for (const chunk of membershipInsertChunks(
      members,
      1,
      this.#limits.maxQueryBatchSize,
    )) {
      const placeholders = chunk.map(() => "?").join(",");
      const rows = this.#tx.all<{ hash?: Uint8Array; size: number } & SqliteRow>(
        `SELECT hash,size FROM efs_cas_objects WHERE hash IN (${placeholders})`,
        chunk.map((member) => member.hash),
        {
          maxRows: chunk.length + 1,
          maxBytes: Math.max(1024, chunk.length * 96),
        },
      );
      const sizes = new Map(rows.map((row) => [bytesToHex(row.hash!), row.size]));
      for (const member of chunk)
        if (sizes.get(bytesToHex(member.hash)) !== member.size)
          throw new Error(
            "ECORRUPT: staged membership does not match immutable content",
          );
    }
  }

  /**
   * Count-only members are already-durable objects: the CAS-only size check
   * binds them to their immutable content without a membership row. The
   * closure fold binds the closure to the chain at completion, so a
   * same-size durable substitute cannot slip through the relaxed check.
   */
  #verifyCountedBacking(members: readonly StagingMember[]): void {
    for (const chunk of membershipInsertChunks(
      members,
      1,
      this.#limits.maxQueryBatchSize,
    )) {
      const placeholders = chunk.map(() => "?").join(",");
      const rows = this.#tx.all<{ hash?: Uint8Array; size: number } & SqliteRow>(
        `SELECT hash,size FROM efs_cas_objects WHERE hash IN (${placeholders})`,
        chunk.map((member) => member.hash),
        {
          maxRows: chunk.length + 1,
          maxBytes: Math.max(1024, chunk.length * 96),
        },
      );
      const sizes = new Map(rows.map((row) => [bytesToHex(row.hash!), row.size]));
      for (const member of chunk)
        if (sizes.get(bytesToHex(member.hash)) !== member.size)
          throw new Error(
            "ECORRUPT: counted closure member is not backed by durable content",
          );
    }
  }

  #existingNodeMembers(
    leaseId: string,
    members: readonly StagingMember[],
  ): Set<string> {
    const existing = new Set<string>();
    for (const chunk of membershipInsertChunks(
      members,
      1,
      this.#limits.maxQueryBatchSize,
    )) {
      const placeholders = chunk.map(() => "?").join(",");
      const rows = this.#tx.all<{ manifest_hash?: Uint8Array } & SqliteRow>(
        `SELECT manifest_hash FROM efs_lease_staged_manifests WHERE lease_id=? AND manifest_hash IN (${placeholders})`,
        [leaseId, ...chunk.map((member) => member.hash)],
        {
          maxRows: chunk.length + 1,
          maxBytes: Math.max(1024, chunk.length * 96),
        },
      );
      for (const row of rows) existing.add(bytesToHex(row.manifest_hash!));
    }
    return existing;
  }

  #verifyNodeBacking(
    members: readonly StagingMember[],
    verifiedNodeSizes?: ReadonlyMap<string, number>,
    verifiedRootSizes?: ReadonlyMap<string, number>,
  ): void {
    const roots = members.filter((member) => member.kind === "manifest-root");
    const nodes = members.filter((member) => member.kind === "manifest-node");
    const rootsRequiringLookup = roots.filter((member) => {
      const verifiedSize = verifiedRootSizes?.get(bytesToHex(member.hash));
      if (verifiedSize === undefined) return true;
      if (verifiedSize !== member.size)
        throw new Error("ECORRUPT: staged membership does not match immutable content");
      return false;
    });
    for (const chunk of membershipInsertChunks(
      rootsRequiringLookup,
      1,
      this.#limits.maxQueryBatchSize,
    )) {
      const placeholders = chunk.map(() => "?").join(",");
      const rows = this.#tx.all<{ hash?: Uint8Array; size: number } & SqliteRow>(
        `SELECT hash,length(encoded) size FROM efs_manifest_roots WHERE hash IN (${placeholders})`,
        chunk.map((member) => member.hash),
        {
          maxRows: chunk.length + 1,
          maxBytes: Math.max(1024, chunk.length * 96),
        },
      );
      const sizes = new Map(rows.map((row) => [bytesToHex(row.hash!), row.size]));
      for (const member of chunk)
        if (sizes.get(bytesToHex(member.hash)) !== member.size)
          throw new Error(
            "ECORRUPT: staged membership does not match immutable content",
          );
    }
    const nodesRequiringLookup = nodes.filter((member) => {
      const verifiedSize = verifiedNodeSizes?.get(bytesToHex(member.hash));
      if (verifiedSize === undefined) return true;
      if (verifiedSize !== member.size)
        throw new Error("ECORRUPT: staged membership does not match immutable content");
      return false;
    });
    for (const chunk of membershipInsertChunks(
      nodesRequiringLookup,
      1,
      this.#limits.maxQueryBatchSize,
    )) {
      const placeholders = chunk.map(() => "?").join(",");
      const rows = this.#tx.all<{ hash?: Uint8Array; size: number } & SqliteRow>(
        `SELECT hash,length(encoded) size FROM efs_manifest_nodes WHERE hash IN (${placeholders})`,
        chunk.map((member) => member.hash),
        {
          maxRows: chunk.length + 1,
          maxBytes: Math.max(1024, chunk.length * 96),
        },
      );
      const sizes = new Map(rows.map((row) => [bytesToHex(row.hash!), row.size]));
      for (const member of chunk)
        if (sizes.get(bytesToHex(member.hash)) !== member.size)
          throw new Error(
            "ECORRUPT: staged membership does not match immutable content",
          );
    }
  }

  #reusedSubtree(leaseId: string, nodeHash: Uint8Array): ReusedSubtreeRow | undefined {
    const key = `${leaseId}:${bytesToHex(nodeHash)}`;
    if (this.#reusedSubtreeCache.has(key)) return this.#reusedSubtreeCache.get(key);
    const row = this.#tx.all<ReusedSubtreeRow>(
      "SELECT source_manifest_hash,source_path,span,entry_count,validated_nonfinal_leaf_delta,validated_final_leaf_delta,summary_usable FROM efs_staging_reused_subtrees WHERE lease_id=? AND node_hash=?",
      [leaseId, nodeHash],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    this.#reusedSubtreeCache.set(key, row);
    return row;
  }

  #cacheReusedSubtreePatch(
    leaseId: string,
    nodeHash: Uint8Array,
    patch: Partial<ReusedSubtreeRow>,
  ): void {
    const key = `${leaseId}:${bytesToHex(nodeHash)}`;
    if (!this.#reusedSubtreeCache.has(key)) return;
    const current = this.#reusedSubtreeCache.get(key);
    if (current)
      this.#reusedSubtreeCache.set(
        key,
        Object.freeze({ ...current, ...patch } as ReusedSubtreeRow),
      );
  }

  #manifestBacking(leaseId: string, kind: number, hash: Uint8Array): BackingRow {
    const key = `${leaseId}:${kind}:${bytesToHex(hash)}`;
    const cached = this.#manifestBackingCache.get(key);
    if (cached) return cached;
    const row =
      kind === 1
        ? this.#tx.all<BackingRow>(
            "SELECT length(r.encoded) stored_size,m.size membership_size FROM efs_manifest_roots r JOIN efs_lease_staged_manifests m ON m.lease_id=? AND m.kind=0 AND m.manifest_hash=r.hash WHERE r.hash=?",
            [leaseId, hash],
            { maxRows: 1, maxBytes: 256 },
          )[0]
        : this.#tx.all<BackingRow>(
            "SELECT length(n.encoded) stored_size,m.size membership_size FROM efs_manifest_nodes n JOIN efs_lease_staged_manifests m ON m.lease_id=? AND m.kind=1 AND m.manifest_hash=n.hash WHERE n.hash=?",
            [leaseId, hash],
            { maxRows: 1, maxBytes: 256 },
          )[0];
    if (!row || row.stored_size !== row.membership_size)
      throw new Error(
        "ECORRUPT: manifest closure member is absent or has a mismatched size",
      );
    this.#manifestBackingCache.set(key, row);
    return row;
  }

  #loadReusedNodeMetadata(
    leaseId: string,
    hash: Uint8Array,
  ): {
    readonly backing: BackingRow;
    readonly claim: ReusedSubtreeRow | undefined;
    readonly summary: ReusableSummaryRow | undefined;
  } {
    const key = `${leaseId}:2:${bytesToHex(hash)}`;
    const backing = this.#manifestBackingCache.get(key);
    const claim = this.#reusedSubtreeCache.get(`${leaseId}:${bytesToHex(hash)}`);
    if (backing && this.#reusedSubtreeCache.has(`${leaseId}:${bytesToHex(hash)}`))
      return {
        backing,
        claim,
        summary: this.#reusedSummaryCache.get(key),
      };
    const row = this.#tx.all<
      {
        stored_size: number;
        membership_size: number;
        source_manifest_hash: Uint8Array | null;
        source_path: Uint8Array | null;
        span: number | null;
        entry_count: number | null;
        validated_nonfinal_leaf_delta: number | null;
        validated_final_leaf_delta: number | null;
        summary_usable: number | null;
        object_count: number | null;
        object_bytes: number | null;
        node_count: number | null;
        node_bytes: number | null;
        membership_count: number | null;
        closure_fold: Uint8Array | null;
      } & SqliteRow
    >(
      "SELECT length(n.encoded) stored_size,m.size membership_size,r.source_manifest_hash,r.source_path,r.span,r.entry_count,r.validated_nonfinal_leaf_delta,r.validated_final_leaf_delta,r.summary_usable,s.object_count,s.object_bytes,s.node_count,s.node_bytes,s.membership_count,s.closure_fold FROM efs_manifest_nodes n JOIN efs_lease_staged_manifests m ON m.lease_id=? AND m.kind=1 AND m.manifest_hash=n.hash LEFT JOIN efs_staging_reused_subtrees r ON r.lease_id=? AND r.node_hash=n.hash LEFT JOIN efs_manifest_subtree_summaries s ON s.node_hash=n.hash WHERE n.hash=?",
      [leaseId, leaseId, hash],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!row || row.stored_size !== row.membership_size)
      throw new Error(
        "ECORRUPT: manifest closure member is absent or has a mismatched size",
      );
    const cachedBacking = Object.freeze({
      stored_size: row.stored_size,
      membership_size: row.membership_size,
    });
    this.#manifestBackingCache.set(key, cachedBacking);
    const cacheKey = `${leaseId}:${bytesToHex(hash)}`;
    const cachedClaim =
      row.source_manifest_hash &&
      row.source_path &&
      row.span !== null &&
      row.entry_count !== null &&
      row.validated_nonfinal_leaf_delta !== undefined &&
      row.validated_final_leaf_delta !== undefined &&
      row.summary_usable !== null
        ? Object.freeze({
            source_manifest_hash: copyBytes(row.source_manifest_hash),
            source_path: copyBytes(row.source_path),
            span: row.span,
            entry_count: row.entry_count,
            validated_nonfinal_leaf_delta: row.validated_nonfinal_leaf_delta,
            validated_final_leaf_delta: row.validated_final_leaf_delta,
            summary_usable: row.summary_usable,
          })
        : undefined;
    this.#reusedSubtreeCache.set(cacheKey, cachedClaim);
    const cachedSummary =
      row.object_count !== null &&
      row.object_bytes !== null &&
      row.node_count !== null &&
      row.node_bytes !== null &&
      row.membership_count !== null &&
      row.closure_fold !== null
        ? Object.freeze({
            object_count: row.object_count,
            object_bytes: row.object_bytes,
            node_count: row.node_count,
            node_bytes: row.node_bytes,
            membership_count: row.membership_count,
            closure_fold: copyBytes(row.closure_fold),
          })
        : undefined;
    this.#reusedSummaryCache.set(key, cachedSummary);
    return { backing: cachedBacking, claim: cachedClaim, summary: cachedSummary };
  }

  #queueKey(leaseId: string, kind: number, hash: Uint8Array): string {
    return `${leaseId}:${kind}:${bytesToHex(hash)}`;
  }

  #enqueueVerified(
    leaseId: string,
    kind: number,
    hash: Uint8Array,
    declaredSpan: number | undefined,
    declaredEntryCount: number | undefined,
  ): boolean {
    // A manifest is a DAG. Repeated edges to a queue member that was already
    // authenticated need only agree with its immutable declaration; fetching
    // and decoding the same BLOB once per sibling would turn highly deduplicated
    // trees into avoidable fanout work.
    const cacheKey = `${leaseId}:${kind}:${bytesToHex(hash)}`;
    const queued = this.#reconciliationQueueCache.get(cacheKey);
    if (queued) {
      if (
        queued.declared_span !== (declaredSpan ?? null) ||
        queued.declared_entry_count !== (declaredEntryCount ?? null)
      )
        throw new Error("ECORRUPT: repeated manifest closure edge disagrees");
      return false;
    }
    let size: number;
    let reused = false;
    let summaryUsable = false;
    let reusedSummary: ReusableSummaryRow | undefined;
    if (kind === 0) {
      // CAS-only backing (no membership JOIN): count-only closure members
      // carry no `efs_lease_objects` row; the closure fold binds the closure
      // to the chain at completion.
      const row = this.#tx.all<{ stored_size: number } & SqliteRow>(
        "SELECT size stored_size FROM efs_cas_objects WHERE hash=?",
        [hash],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      if (!row || row.stored_size !== declaredSpan)
        throw new Error(
          "ECORRUPT: object closure member is absent or has a mismatched size",
        );
      size = row.stored_size;
    } else if (kind === 1) {
      const row = this.#manifestBacking(leaseId, kind, hash);
      size = row.stored_size;
      if (
        !this.#content.withManifestRoot(hash, (encoded) => {
          decodeManifestRoot(encoded, hash);
          return true;
        })
      )
        throw new Error("ECORRUPT: closure manifest root is missing");
    } else {
      const metadata = this.#loadReusedNodeMetadata(leaseId, hash);
      size = metadata.backing.stored_size;
      const claim = metadata.claim;
      reusedSummary = metadata.summary;
      if (claim) {
        if (claim.span !== declaredSpan || claim.entry_count !== declaredEntryCount)
          throw new Error("ECORRUPT: reused subtree declaration mismatch");
        if (
          claim.validated_nonfinal_leaf_delta === null &&
          claim.validated_final_leaf_delta === null
        )
          throw new Error("ECORRUPT: reused subtree lacks authenticated depth");
        reused = true;
        summaryUsable = claim.summary_usable === 1;
      } else {
        const node = this.#content.withManifestNode(hash, (encoded) =>
          decodeManifestNode(encoded, hash),
        );
        if (!node) throw new Error("ECORRUPT: closure manifest node is missing");
        if (node.span !== declaredSpan || node.entryCount !== declaredEntryCount)
          throw new Error("ECORRUPT: manifest child declaration mismatch");
      }
    }
    const state = this.#reconciliation(leaseId);
    if (!state) throw new Error("ECORRUPT: missing staging reconciliation");
    if (summaryUsable && this.#batchIngestAccounting) {
      if (this.#aggregatedSummaryCache.has(cacheKey)) return false;
      const summary = reusedSummary;
      if (
        !summary ||
        !Number.isSafeInteger(summary.object_count) ||
        !Number.isSafeInteger(summary.object_bytes) ||
        !Number.isSafeInteger(summary.node_count) ||
        !Number.isSafeInteger(summary.node_bytes) ||
        !Number.isSafeInteger(summary.membership_count) ||
        intrinsicByteLength(summary.closure_fold) !== 32
      )
        throw new Error("ECORRUPT: reusable subtree summary is missing or invalid");
      const rootFold = foldHashes(state.closure_fold, hash);
      const combinedFold = foldHashes(rootFold, summary.closure_fold);
      const membershipCount = checkedAdd(
        summary.membership_count,
        1,
        "summary-backed closure membership count",
      );
      this.#updateReconciliationAggregate(
        leaseId,
        "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+?,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+?,node_bytes=node_bytes+?,membership_count=membership_count+?,closure_fold=? WHERE lease_id=? AND complete=0",
        [
          membershipCount,
          summary.object_count,
          summary.object_bytes,
          summary.node_count + 1,
          summary.node_bytes + size,
          membershipCount,
          combinedFold,
          leaseId,
        ],
      );
      this.#cacheReconciliationPatch(leaseId, {
        next_sequence: state.next_sequence + membershipCount,
        object_count: state.object_count + summary.object_count,
        object_bytes: state.object_bytes + summary.object_bytes,
        node_count: state.node_count + summary.node_count + 1,
        node_bytes: state.node_bytes + summary.node_bytes + size,
        membership_count: state.membership_count + membershipCount,
        closure_fold: combinedFold,
      });
      this.#aggregatedSummaryCache.add(cacheKey);
      return true;
    }
    const inserted = this.#tx.run(
      "INSERT OR IGNORE INTO efs_staging_reconciliation_queue(lease_id,kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor,processed) VALUES(?,?,?,?,?,?,?,0,?)",
      [
        leaseId,
        kind,
        hash,
        state.next_sequence,
        size,
        declaredSpan ?? null,
        declaredEntryCount ?? null,
        reused ? 1 : 0,
      ],
    );
    if (!inserted.changes) {
      const existing = this.#tx.all<QueueRow>(
        "SELECT kind,hash,sequence,declared_size,declared_span,declared_entry_count,edge_cursor FROM efs_staging_reconciliation_queue WHERE lease_id=? AND kind=? AND hash=?",
        [leaseId, kind, hash],
        { maxRows: 1, maxBytes: 512 },
      )[0];
      if (!existing)
        throw new Error("ECORRUPT: reconciliation queue changed unexpectedly");
      if (
        existing.declared_span !== (declaredSpan ?? null) ||
        existing.declared_entry_count !== (declaredEntryCount ?? null)
      )
        throw new Error("ECORRUPT: repeated manifest closure edge disagrees");
      this.#reconciliationQueueCache.set(cacheKey, existing);
      return false;
    }
    this.#reconciliationQueueCache.set(
      cacheKey,
      Object.freeze({
        kind,
        hash: copyBytes(hash),
        sequence: state.next_sequence,
        declared_size: size,
        declared_span: declaredSpan ?? null,
        declared_entry_count: declaredEntryCount ?? null,
        edge_cursor: reused ? 1 : 0,
      }),
    );
    this.#changeMetadataRows(1, "staging reconciliation queue", leaseId);
    const object = kind === 0 ? 1 : 0;
    const node = kind === 0 ? 0 : 1;
    const closureFold = foldHashes(state.closure_fold, hash);
    this.#updateReconciliationAggregate(
      leaseId,
      "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+1,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+?,node_bytes=node_bytes+?,membership_count=membership_count+1,closure_fold=? WHERE lease_id=? AND complete=0",
      [object, object ? size : 0, node, node ? size : 0, closureFold, leaseId],
    );
    this.#cacheReconciliationPatch(leaseId, {
      next_sequence: state.next_sequence + 1,
      object_count: state.object_count + object,
      object_bytes: state.object_bytes + (object ? size : 0),
      node_count: state.node_count + node,
      node_bytes: state.node_bytes + (node ? size : 0),
      membership_count: state.membership_count + 1,
      closure_fold: closureFold,
    });
    if (summaryUsable) {
      const summary = reusedSummary;
      if (
        !summary ||
        !Number.isSafeInteger(summary.object_count) ||
        !Number.isSafeInteger(summary.object_bytes) ||
        !Number.isSafeInteger(summary.node_count) ||
        !Number.isSafeInteger(summary.node_bytes) ||
        !Number.isSafeInteger(summary.membership_count) ||
        intrinsicByteLength(summary.closure_fold) !== 32
      )
        throw new Error("ECORRUPT: reusable subtree summary is missing or invalid");
      const summaryFold = foldHashes(closureFold, summary.closure_fold);
      this.#updateReconciliationAggregate(
        leaseId,
        "UPDATE efs_staging_reconciliations SET next_sequence=next_sequence+?,object_count=object_count+?,object_bytes=object_bytes+?,node_count=node_count+?,node_bytes=node_bytes+?,membership_count=membership_count+?,closure_fold=? WHERE lease_id=? AND complete=0",
        [
          summary.membership_count,
          summary.object_count,
          summary.object_bytes,
          summary.node_count,
          summary.node_bytes,
          summary.membership_count,
          summaryFold,
          leaseId,
        ],
      );
      this.#cacheReconciliationPatch(leaseId, {
        next_sequence: state.next_sequence + 1 + summary.membership_count,
        object_count: state.object_count + object + summary.object_count,
        object_bytes: state.object_bytes + (object ? size : 0) + summary.object_bytes,
        node_count: state.node_count + node + summary.node_count,
        node_bytes: state.node_bytes + (node ? size : 0) + summary.node_bytes,
        membership_count: state.membership_count + 1 + summary.membership_count,
        closure_fold: summaryFold,
      });
    }
    return true;
  }

  #enqueueValidation(
    leaseId: string,
    path: Uint8Array,
    nodeHash: Uint8Array,
    declaredSpan: number,
    declaredEntryCount: number,
    depth: number,
    finalAtLevel: boolean,
  ): void {
    if (
      intrinsicByteLength(path) !== depth - 1 ||
      depth > this.#limits.maxManifestDepth
    )
      throw new Error("ECORRUPT: manifest validation path exceeds configured depth");
    const inserted = this.#tx.run(
      "INSERT INTO efs_staging_manifest_validation_queue(lease_id,path,node_hash,declared_span,declared_entry_count,depth,final_at_level,edge_cursor,processed) VALUES(?,?,?,?,?,?,?,0,0)",
      [
        leaseId,
        path,
        nodeHash,
        declaredSpan,
        declaredEntryCount,
        depth,
        finalAtLevel ? 1 : 0,
      ],
    );
    if (inserted.changes !== 1)
      throw new Error("ECORRUPT: manifest validation path changed unexpectedly");
    this.#changeMetadataRows(1, "manifest validation queue", leaseId);
  }

  #validateManifestBatch(
    leaseId: string,
    manifestHash: Uint8Array,
    workLimit: number,
  ): { readonly processed: number; readonly complete: boolean } {
    const root = this.#content.withManifestRoot(manifestHash, (encoded) =>
      decodeManifestRoot(encoded, manifestHash),
    );
    if (!root) throw new Error("ECORRUPT: reconciled manifest root is missing");
    validateSupportedManifestParameters(root.parameters);
    let processed = 0;
    const pageSize = Math.min(
      workLimit,
      this.#batchIngestAccounting
        ? this.#limits.maxQueryBatchSize * 4
        : this.#limits.maxQueryBatchSize,
    );
    let page: readonly ValidationQueueRow[] = [];
    let pageIndex = 0;
    const loadPage = (): void => {
      page = this.#tx.all<ValidationQueueRow>(
        "SELECT path,node_hash,declared_span,declared_entry_count,depth,final_at_level,edge_cursor FROM efs_staging_manifest_validation_queue WHERE lease_id=? AND processed=0 ORDER BY path LIMIT ?",
        [leaseId, pageSize],
        { maxRows: pageSize, maxBytes: Math.max(1024, pageSize * 512) },
      );
      pageIndex = 0;
      this.#content.warmManifestNodeBatch(page.map((item) => item.node_hash));
    };
    while (processed < workLimit) {
      if (pageIndex >= page.length) loadPage();
      const item = page[pageIndex++];
      if (!item) return Object.freeze({ processed, complete: true });

      const claim = this.#reusedSubtree(leaseId, item.node_hash);
      if (claim) {
        if (
          claim.span !== item.declared_span ||
          claim.entry_count !== item.declared_entry_count
        )
          throw new Error("ECORRUPT: reused validation declaration mismatch");
        const cachedLeafDelta =
          item.final_at_level === 1
            ? (claim.validated_nonfinal_leaf_delta ?? claim.validated_final_leaf_delta)
            : claim.validated_nonfinal_leaf_delta;
        if (cachedLeafDelta !== null) {
          this.#recordLeafDepth(
            leaseId,
            checkedAdd(
              item.depth,
              cachedLeafDelta,
              "cached reused manifest validation depth",
            ),
          );
          this.#tx.run(
            "UPDATE efs_staging_manifest_validation_queue SET processed=1 WHERE lease_id=? AND path=? AND processed=0",
            [leaseId, item.path],
          );
          processed += 1;
          continue;
        }
        const authenticated = new ManifestTreeRepository(
          this.#tx,
          this.#limits,
          this.#cache,
        ).authenticateNodePath(
          claim.source_manifest_hash,
          Array.from(claim.source_path),
        );
        if (
          equalBytes(authenticated.hash, item.node_hash) &&
          (!authenticated.finalAtLevel || item.final_at_level === 1)
        ) {
          const leafDepth = checkedAdd(
            item.depth,
            authenticated.treeDepth - authenticated.depth,
            "reused manifest validation depth",
          );
          this.#recordLeafDepth(leaseId, leafDepth);
          const column =
            item.final_at_level === 1
              ? "validated_final_leaf_delta"
              : "validated_nonfinal_leaf_delta";
          this.#tx.run(
            `UPDATE efs_staging_reused_subtrees SET ${column}=? WHERE lease_id=? AND node_hash=? AND ${column} IS NULL`,
            [authenticated.treeDepth - authenticated.depth, leaseId, item.node_hash],
          );
          this.#cacheReusedSubtreePatch(leaseId, item.node_hash, {
            [column]: authenticated.treeDepth - authenticated.depth,
          });
          this.#tx.run(
            "UPDATE efs_staging_manifest_validation_queue SET processed=1 WHERE lease_id=? AND path=? AND processed=0",
            [leaseId, item.path],
          );
          processed += 1;
          continue;
        }
      }

      const node = this.#content.withManifestNode(item.node_hash, (encoded) =>
        decodeManifestNode(encoded, item.node_hash),
      );
      if (!node) throw new Error("ECORRUPT: validated manifest node is missing");
      if (
        node.span !== item.declared_span ||
        node.entryCount !== item.declared_entry_count
      )
        throw new Error("ECORRUPT: manifest validation child totals mismatch");
      validateCanonicalManifestNode(
        node,
        root.parameters,
        item.final_at_level === 1,
        item.depth === 1,
      );
      if (node.kind === "leaf") {
        this.#recordLeafDepth(leaseId, item.depth);
        this.#tx.run(
          "UPDATE efs_staging_manifest_validation_queue SET processed=1 WHERE lease_id=? AND path=? AND processed=0",
          [leaseId, item.path],
        );
        processed += 1;
        continue;
      }
      if (item.depth >= this.#limits.maxManifestDepth)
        throw new Error("ECORRUPT: manifest validation exceeds configured depth");
      const end = Math.min(
        node.children.length,
        item.edge_cursor + workLimit - processed,
      );
      for (let index = item.edge_cursor; index < end; index += 1) {
        const child = node.children[index]!;
        const childPath = new Uint8Array(intrinsicByteLength(item.path) + 1);
        childPath.set(item.path);
        childPath[childPath.length - 1] = index;
        const childFinalAtLevel =
          item.final_at_level === 1 && index === node.children.length - 1;
        const claim = this.#reusedSubtree(leaseId, child.hash);
        const cachedLeafDelta = claim
          ? childFinalAtLevel
            ? (claim.validated_nonfinal_leaf_delta ?? claim.validated_final_leaf_delta)
            : claim.validated_nonfinal_leaf_delta
          : undefined;
        if (cachedLeafDelta !== null && cachedLeafDelta !== undefined)
          this.#recordLeafDepth(
            leaseId,
            checkedAdd(
              item.depth + 1,
              cachedLeafDelta,
              "cached reused manifest validation depth",
            ),
          );
        else
          this.#enqueueValidation(
            leaseId,
            childPath,
            child.hash,
            child.span,
            child.entryCount,
            item.depth + 1,
            childFinalAtLevel,
          );
        processed += 1;
      }
      this.#tx.run(
        "UPDATE efs_staging_manifest_validation_queue SET edge_cursor=?,processed=? WHERE lease_id=? AND path=? AND processed=0",
        [end, end === node.children.length ? 1 : 0, leaseId, item.path],
      );
    }
    const pending =
      this.#tx.all(
        "SELECT path FROM efs_staging_manifest_validation_queue WHERE lease_id=? AND processed=0 LIMIT 1",
        [leaseId],
        { maxRows: 1, maxBytes: 128 },
      ).length !== 0;
    return Object.freeze({ processed, complete: !pending });
  }

  #recordLeafDepth(leaseId: string, depth: number): void {
    if (depth < 1 || depth > this.#limits.maxManifestDepth)
      throw new Error("ECORRUPT: manifest leaf depth exceeds configured maximum");
    const cached = this.#reconciliationCache.get(leaseId)?.leaf_depth;
    if (cached !== undefined && cached !== null) {
      if (cached !== depth) throw new Error("ECORRUPT: unbalanced manifest tree");
      return;
    }
    const changed = this.#tx.run(
      "UPDATE efs_staging_reconciliations SET leaf_depth=? WHERE lease_id=? AND leaf_depth IS NULL AND complete=0",
      [depth, leaseId],
    );
    if (changed.changes) this.#cacheReconciliationPatch(leaseId, { leaf_depth: depth });
    if (changed.changes === 0) {
      const current = this.#reconciliation(leaseId)?.leaf_depth;
      if (current !== depth) throw new Error("ECORRUPT: unbalanced manifest tree");
    }
  }

  #reconciliation(leaseId: string): ReconciliationRow | undefined {
    if (this.#reconciliationCache.has(leaseId))
      return this.#reconciliationCache.get(leaseId);
    const row = this.#tx.all<ReconciliationRow>(
      "SELECT owner_nonce,manifest_hash,next_sequence,object_count,object_bytes,node_count,node_bytes,membership_count,complete,leaf_depth,closure_fold FROM efs_staging_reconciliations WHERE lease_id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    this.#reconciliationCache.set(leaseId, row);
    return row;
  }

  #cacheReconciliationPatch(leaseId: string, patch: Partial<ReconciliationRow>): void {
    if (!this.#reconciliationCache.has(leaseId)) return;
    const current = this.#reconciliationCache.get(leaseId);
    if (current)
      this.#reconciliationCache.set(
        leaseId,
        Object.freeze({ ...current, ...patch } as ReconciliationRow),
      );
  }

  #leaseCharge(leaseId: string): LeaseChargeRow | undefined {
    return this.#tx.all<LeaseChargeRow>(
      "SELECT l.state,l.owner_nonce,COALESCE((SELECT coalesce(sum(o.size),0) FROM efs_lease_objects o WHERE o.lease_id=l.id)+(SELECT coalesce(sum(m.size),0) FROM efs_lease_staged_manifests m WHERE m.lease_id=l.id),0) staged_bytes,COALESCE(c.ingest_reservation_bytes,0) ingest_reservation_bytes,COALESCE(c.metadata_reservation_bytes,0) metadata_reservation_bytes FROM efs_leases l LEFT JOIN efs_staging_certificates c ON c.lease_id=l.id WHERE l.id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 256 },
    )[0];
  }

  #scheduleCleanup(
    leaseId: string,
    ownerNonce: Uint8Array,
    releasedStagingBytes: number,
    tombstonedAt: number,
  ): void {
    const inserted = this.#tx.run(
      "INSERT OR IGNORE INTO efs_lease_cleanups(lease_id,owner_nonce,phase,cursor_text,cursor_blob,released_staging_bytes,tombstoned_at_ms) VALUES(?,?,0,NULL,NULL,?,?)",
      [leaseId, ownerNonce, releasedStagingBytes, tombstonedAt],
    );
    if (inserted.changes)
      new UsageRepository(this.#tx, this.#limits).apply(
        { maintenance_bytes: CHARGED_ROW_BYTES },
        "lease cleanup state",
        { preserveMaintenanceBytes: MAINTENANCE_GC_EMERGENCY_BYTES },
      );
  }

  #advanceCleanup(leaseId: string, phase: number): void {
    const result = this.#tx.run(
      "UPDATE efs_lease_cleanups SET phase=phase+1,cursor_text=NULL,cursor_blob=NULL WHERE lease_id=? AND phase=?",
      [leaseId, phase],
    );
    if (result.changes !== 1)
      throw new Error("ECORRUPT: lease cleanup phase changed unexpectedly");
  }

  #admitStagingBytes(bytes: number): void {
    if (bytes === 0) return;
    if (this.#batchIngestAccounting) {
      this.#batchedStagedBytes = checkedAdd(
        this.#batchedStagedBytes,
        bytes,
        "batched staged bytes",
      );
      return;
    }
    new UsageRepository(this.#tx, this.#limits).apply(
      { staging_bytes: bytes },
      "staging payload",
    );
  }

  #releaseLeaseReservations(
    leaseId: string,
    stagingBytes: number,
    ingestBytes: number,
    metadataBytes = 0,
  ): void {
    if (stagingBytes === 0 && ingestBytes === 0 && metadataBytes === 0) return;
    if (leaseId && (ingestBytes !== 0 || metadataBytes !== 0)) {
      this.#tx.run(
        "UPDATE efs_staging_certificates SET ingest_reservation_bytes=0,metadata_reservation_bytes=0 WHERE lease_id=?",
        [leaseId],
      );
      this.#cacheCertificatePatch(leaseId, {
        ingest_reservation_bytes: 0,
        metadata_reservation_bytes: 0,
      });
    }
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        staging_bytes: -stagingBytes,
        ingest_reservation_bytes: -ingestBytes,
        charged_metadata_bytes: -metadataBytes,
      },
      "staging lease reservation release",
    );
  }

  #changeMetadataRows(rows: number, reason: string, leaseId?: string): void {
    if (rows === 0) return;
    let bytes = rows * CHARGED_ROW_BYTES;
    if (rows > 0 && leaseId) {
      const available = this.#metadataReservation(leaseId);
      if (available !== undefined) {
        const credit = Math.min(bytes, available);
        if (credit) {
          bytes -= credit;
          this.#metadataReservationLease = leaseId;
          this.#metadataReservationBytes = available - credit;
          this.#cacheCertificatePatch(leaseId, {
            metadata_reservation_bytes: available - credit,
          });
          if (this.#batchMetadataAccounting)
            this.#batchedMetadataReservationBytes = checkedAdd(
              this.#batchedMetadataReservationBytes,
              credit,
              "batched metadata row reservation",
            );
          else
            this.#tx.run(
              "UPDATE efs_staging_certificates SET metadata_reservation_bytes=metadata_reservation_bytes-? WHERE lease_id=? AND sealed=0",
              [credit, leaseId],
            );
        }
      }
    }
    if (bytes === 0) {
      // Moving charge from a lease reservation to its durable metadata row
      // leaves the aggregate charged-metadata counter unchanged. The row
      // insertion and reservation decrement are in this same transaction, so
      // there is no usage-counter mutation to publish here.
      return;
    }
    applyChargedMetadata(this.#tx, this.#limits, bytes, reason);
  }

  #row(leaseId: string): CertificateRow {
    const cached = this.#certificateCache.get(leaseId);
    if (cached) return cached;
    const row = this.#tx.all<CertificateRow>(
      "SELECT owner_nonce,manifest_hash,chain_digest,chain_fold,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified,ingest_reservation_bytes,metadata_reservation_bytes FROM efs_staging_certificates WHERE lease_id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!row) throw new Error("ECORRUPT: staging certificate is missing");
    this.#certificateCache.set(leaseId, row);
    return row;
  }
  #metadataReservation(leaseId: string): number | undefined {
    if (this.#metadataReservationLease === leaseId)
      return this.#metadataReservationBytes;
    const bytes = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT metadata_reservation_bytes bytes FROM efs_staging_certificates WHERE lease_id=? AND sealed=0",
      [leaseId],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.bytes;
    this.#metadataReservationLease = leaseId;
    this.#metadataReservationBytes = bytes;
    return bytes;
  }
  #validateShape(certificate: ClosureCertificate): void {
    if (
      intrinsicByteLength(certificate.ownerNonce) !== 16 ||
      intrinsicByteLength(certificate.manifestHash) !== 32 ||
      intrinsicByteLength(certificate.chainDigest) !== 32
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

import type {
  BranchConfiguration,
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
import type { CanonicalPath } from "../namespace/paths.js";
import type { CowPage, CowPageBytes } from "../cow/pages.js";
import type { ContentCache } from "../cache/content-cache.js";
import type { ManifestNode, ManifestParameters } from "../manifests/codec.js";
import type { HashFunction } from "../cas/sha256.js";

export type StorageTransactionMode = "read" | "write" | "exclusive";
export interface StorageWorkBudget {
  readonly maxRows: number;
  readonly maxBytes: number;
  readonly maxStatements?: number;
  readonly maxElapsedMs?: number;
  readonly maxResultRows?: number;
  readonly maxResultBytes?: number;
}
export interface StorageAdapterCapabilities {
  readonly maxBlobBytes: number;
  readonly maxBindings: number;
  readonly durability: "acknowledged" | "relaxed-test";
  readonly journalMode: "wal" | "rollback" | "runtime-managed";
  readonly memoryPolicy: "configured" | "runtime-managed";
  readonly cacheTargetBytes?: number;
  readonly mmapLimitBytes?: number;
  readonly maxPhysicalDatabaseBytes: number;
  readonly maxJournalBytes: number;
  readonly physicalQuotaPolicy: "driver-enforced" | "runtime-enforced";
  readonly journalQuotaPolicy: "checkpoint-backpressure" | "runtime-enforced";
  readonly journalSizeLimitIsHard: false;
}
export interface StoragePhysicalFiles {
  readonly mainFileBytes?: number;
  readonly walBytes?: number;
}
export interface StorageCheckpointResult {
  readonly mode: "passive" | "restart" | "truncate";
  readonly busy: number;
  readonly logFrames: number;
  readonly checkpointedFrames: number;
  readonly walBytes?: number;
}
export interface StorageMetadata {
  readonly filesystemId: string;
  readonly mainRevision: number;
  readonly rootInode: string;
  readonly cowPageBytes: CowPageBytes;
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
export interface AuthenticatedManifestCursorSource {
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
  withManifestNode<T>(
    hash: Uint8Array,
    consume: (encoded: Uint8Array) => T,
  ): T | undefined;
}
export interface AuthenticatedManifestCursor {
  readonly fileSize: number;
  readonly position: number;
  peekEntry(): AuthenticatedManifestEntry | null;
  nextEntry(): AuthenticatedManifestEntry | null;
  readInto(destination: Uint8Array, destinationOffset: number, length: number): number;
  /**
   * Rebind the cursor's content source to the current storage transaction.
   * Carried cursors outlive any single transaction; every readInto call must
   * run against a live transaction, so the stream rebinds before each pull.
   */
  bindSource(source: AuthenticatedManifestCursorSource): void;
  close(): void;
}
export interface AuthenticatedManifestEntry {
  readonly hash: Uint8Array;
  readonly length: number;
  readonly offset: number;
}
export interface ContentStore {
  putObject(hash: Uint8Array, bytes: Uint8Array): boolean;
  putObjectsBatch(
    input: readonly ContentObjectInput[],
    trustedDigests?: boolean,
  ): ContentBatchResult;
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
  verifyObject(
    hash: Uint8Array,
    expectedSize?: number,
    forceStorage?: boolean,
  ): boolean;
  putManifestNode(hash: Uint8Array, encoded: Uint8Array): boolean;
  putManifestNodesBatch(
    nodes: readonly { readonly hash: Uint8Array; readonly encoded: Uint8Array }[],
  ): ContentBatchResult;
  putManifestRoot(hash: Uint8Array, encoded: Uint8Array): boolean;
  withManifestRoot<T>(
    hash: Uint8Array,
    consume: (encoded: Uint8Array) => T,
  ): T | undefined;
  withManifestNode<T>(
    hash: Uint8Array,
    consume: (encoded: Uint8Array) => T,
  ): T | undefined;
  openManifestCursor(
    manifestHash: Uint8Array,
    offset: number,
  ): AuthenticatedManifestCursor;
}

export interface AuthenticatedManifestTreePathNode {
  readonly hash: Uint8Array;
  readonly path: readonly number[];
  readonly offset: number;
  readonly finalAtLevel: boolean;
  readonly node: ManifestNode;
  readonly selectedChildIndex?: number;
}
export interface AuthenticatedManifestTreePath {
  readonly manifestHash: Uint8Array;
  readonly parameters: ManifestParameters;
  readonly fileSize: number;
  readonly entryCount: number;
  readonly nodesRead: number;
  readonly nodes: readonly AuthenticatedManifestTreePathNode[];
  readonly leafOffset: number;
  readonly entryIndex: number;
  readonly entryOffset: number;
}
export interface ManifestTreeStore {
  pathAtOffset(manifestHash: Uint8Array, offset: number): AuthenticatedManifestTreePath;
  recordSubtreeSummaries(
    nodes: readonly { readonly hash: Uint8Array; readonly encoded: Uint8Array }[],
  ): void;
  protectSourceManifest(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void;
  registerReusedSubtrees(
    leaseId: string,
    ownerNonce: Uint8Array,
    sourceManifestHash: Uint8Array,
    claims: readonly {
      readonly sourcePath: readonly number[];
      readonly nodeHash: Uint8Array;
      readonly span: number;
      readonly entryCount: number;
    }[],
    options?: {
      readonly knownObjectHashes?: readonly Uint8Array[];
      readonly knownNodeHashes?: readonly Uint8Array[];
      /** The same transaction already called protectSourceManifest. */
      readonly sourceManifestProtected?: boolean;
      /** Disable summary aggregation when overlap state cannot span batches. */
      readonly allowSummaries?: boolean;
      readonly certificateState?: {
        readonly chainDigest: Uint8Array;
        readonly chainFold: Uint8Array;
        readonly objectCount: number;
        readonly objectBytes: number;
        readonly nodeCount: number;
        readonly nodeBytes: number;
        readonly membershipCount: number;
      };
      readonly deferCertificateWrite?: boolean;
      readonly certificatePatch?: {
        value?: {
          readonly chainDigest: Uint8Array;
          readonly chainFold: Uint8Array;
          readonly objectCount: number;
          readonly objectBytes: number;
          readonly nodeCount: number;
          readonly nodeBytes: number;
          readonly membershipCount: number;
        };
      };
      /** Source-authenticated proof supplied by the bounded local path. */
      readonly authenticatedClaims?: readonly {
        readonly sourcePath: readonly number[];
        readonly nodeHash: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
        readonly sourceFinalAtLevel: boolean;
        readonly sourceLeafDelta: number;
      }[];
    },
  ): readonly {
    readonly nodeHash: Uint8Array;
    readonly sourceManifestHash: Uint8Array;
    readonly sourcePath: Uint8Array;
    readonly span: number;
    readonly entryCount: number;
    readonly validatedNonfinalLeafDelta: number | null;
    readonly validatedFinalLeafDelta: number | null;
    readonly summaryUsable: boolean;
    readonly summary?: {
      readonly objectCount: number;
      readonly objectBytes: number;
      readonly nodeCount: number;
      readonly nodeBytes: number;
      readonly membershipCount: number;
      readonly closureFold: Uint8Array;
    };
  }[];
}

export interface InodeRow {
  readonly id: string;
  readonly type: number;
  readonly mode: number;
  readonly birthtime_ms: number;
  readonly mtime_ms: number;
  readonly ctime_ms: number;
  readonly nlink: number;
  readonly size: number | null;
  readonly manifest_hash: Uint8Array | null;
  readonly symlink_target: string | null;
  readonly token: number;
}
export interface EntryRow {
  readonly parent_inode: string;
  readonly name_sort: Uint8Array;
  readonly name: string | null;
  readonly inode_id: string | null;
  readonly token: number;
}
export interface ChildRow {
  readonly name: string;
  readonly name_sort: Uint8Array;
  readonly inode_id: string;
  readonly token: number;
  readonly type: number;
}
export interface ResolvedPath {
  readonly path: CanonicalPath;
  readonly inode: InodeRow;
  readonly parentInode: string | null;
  readonly name: string;
  readonly nameSort: Uint8Array | null;
  readonly entryToken: number | null;
  /** Read-snapshot namespace state, when supplied by the SQLite resolver. */
  readonly mainRevision?: number;
  readonly rootMutationGeneration?: number;
}
export interface NamespaceStore {
  meta(): {
    readonly root_inode: string;
    readonly main_revision: number;
    readonly root_mutation_generation: number;
  };
  inode(id: string): InodeRow | undefined;
  entry(parentInode: string, nameSort: Uint8Array): EntryRow | undefined;
  resolve(input: string | CanonicalPath, followFinal?: boolean): ResolvedPath;
  resolveOptional(
    input: string | CanonicalPath,
    followFinal?: boolean,
  ): ResolvedPath | undefined;
  resolveParent(path: CanonicalPath): {
    readonly parent: ResolvedPath;
    readonly name: string;
    readonly nameSort: Uint8Array;
  };
  nextRevision(now: number, changeCount: number, writer?: string): number;
  /** Optimistic local-edit handoff; falls back internally if the snapshot is stale. */
  nextRevisionFromSnapshot?(
    now: number,
    changeCount: number,
    mainRevision: number,
    rootMutationGeneration: number,
    writer?: string,
  ): number;
  recordInode(revision: number, inodeId: string, tombstone?: boolean): void;
  /** Records a just-allocated file revision from its already-updated inode state. */
  recordFileContentRevision?(revision: number, inode: InodeRow): void;
  recordEntry(
    revision: number,
    parentInode: string,
    nameSort: Uint8Array,
    tombstone?: boolean,
  ): void;
  putEntry(
    parentInode: string,
    nameSort: Uint8Array,
    name: string | null,
    inodeId: string | null,
    token: number,
  ): void;
  children(
    parentInode: string,
    limit: number,
    maxBytes: number,
    startAfter?: Uint8Array,
  ): readonly ChildRow[];
  childCount(parentInode: string): number;
  linkCount(inodeId: string): number;
  createInode(value: {
    readonly id: string;
    readonly type: number;
    readonly mode: number;
    readonly now: number;
    readonly revision: number;
    readonly size?: number | null;
    readonly manifestHash?: Uint8Array | null;
    readonly symlinkTarget?: string | null;
  }): void;
  upsertInode(value: {
    readonly id: string;
    readonly type: number;
    readonly mode: number;
    readonly birthtimeMs: number;
    readonly mtimeMs: number;
    readonly ctimeMs: number;
    readonly nlink: number;
    readonly size: number | null;
    readonly manifestHash: Uint8Array | null;
    readonly symlinkTarget: string | null;
    readonly token: number;
  }): void;
  setFileContent(
    id: string,
    size: number,
    manifestHash: Uint8Array,
    mtime: number,
    ctime: number,
    token: number,
    expectedToken?: number,
  ): number;
  setMode(id: string, mode: number, ctime: number, token: number): void;
  incrementLinks(id: string, ctime: number, token: number): void;
  decrementLinks(id: string, ctime: number, token: number): void;
  setLinks(id: string, count: number, ctime: number, token: number): void;
  touch(id: string, mtime: number, ctime: number, token: number): void;
  deleteEntriesUnder(parentInode: string, tombstonesOnly?: boolean): void;
  deleteInode(id: string): void;
  bumpRoot(kind: number, id: string, mayRemoveRoots?: boolean): void;
}

export interface BranchRow {
  readonly id: string;
  readonly base_revision: number;
  readonly state: number;
  readonly generation: number;
  readonly created_at_ms: number;
  readonly terminal_at_ms: number | null;
  readonly merged_revision: number | null;
}
export interface BranchHistoryRow {
  readonly tombstone: number;
  readonly encoded: Uint8Array | null;
}
export interface BranchHistoryEntryRow {
  readonly name_sort: Uint8Array;
  readonly tombstone: number;
  readonly encoded: Uint8Array | null;
}
export interface BranchChangeRow {
  readonly path: Uint8Array;
  readonly expected_token: number | null;
  readonly kind: number;
  readonly encoded: Uint8Array | null;
}
export interface BranchResultRow {
  readonly branch_id: string;
  readonly generation: number;
  readonly reservation_nonce: Uint8Array;
  readonly outcome: number;
  readonly encoded: Uint8Array | null;
  readonly expires_at_ms: number | null;
}
export interface BranchStore {
  rootInodeId(): string;
  historyEntries(
    parentInode: string,
    revision: number,
  ): readonly BranchHistoryEntryRow[];
  historicEntry(
    parentInode: string,
    nameSort: Uint8Array,
    revision: number,
  ): BranchHistoryRow | undefined;
  historicInode(inodeId: string, revision: number): BranchHistoryRow | undefined;
  inodeOverlay(
    branchId: string,
    inodeId: string,
    maxBytes: number,
  ): Uint8Array | undefined;
  change(branchId: string, path: Uint8Array): BranchChangeRow | undefined;
  changes(branchId: string): readonly BranchChangeRow[];
  activeCount(): number;
  headRevision(): number;
  revisionExists(revision: number): boolean;
  create(id: string, baseRevision: number, now: number): BranchRow;
  row(id: string): BranchRow | undefined;
  operationResult(operationId: string, maxBytes: number): BranchResultRow | undefined;
  reserveOperation(
    operationId: string,
    branchId: string,
    generation: number,
    now: number,
    reservationExpiresAt: number,
    reservationNonce: Uint8Array,
  ): void;
  reclaimOperation(
    operationId: string,
    branchId: string,
    generation: number,
    now: number,
    reservationExpiresAt: number,
    reservationNonce: Uint8Array,
  ): boolean;
  expireOperation(operationId: string, reservationNonce: Uint8Array, now: number): void;
  releaseOperation(operationId: string, reservationNonce?: Uint8Array): void;
  putChange(
    branchId: string,
    path: Uint8Array,
    expectedToken: number | null,
    kind: number,
    encoded: Uint8Array | null,
  ): void;
  putInodeExpectation(
    branchId: string,
    inodeId: string,
    expectedToken: number | null,
  ): void;
  setManifestRoot(branchId: string, path: Uint8Array, manifestHash?: Uint8Array): void;
  changeCount(branchId: string): number;
  changeBytes(branchId: string): number;
  changePathBytes(branchId: string): number;
  subtreeChanged(inodeId: string, baseRevision: number): boolean;
  incrementGeneration(branchId: string): void;
  putInodeOverlay(
    branchId: string,
    inodeId: string,
    expectedToken: number | null,
    encoded: Uint8Array,
  ): void;
  finish(
    branchId: string,
    state: 1 | 2,
    now: number,
    mergedRevision?: number | null,
  ): void;
  terminalCleanupRows(branchId: string): number;
  clearChanges(branchId: string): void;
  storeResult(
    operationId: string,
    outcome: number,
    encoded: Uint8Array,
    expiresAt: number,
    revision: number | null,
  ): void;
  pruneExpiredResults(now: number, limit: number): number;
  pruneTerminalBranches(now: number, retentionMs: number, limit: number): number;
  maintainRevisionRetention(
    maxRetainedRevisions: number,
    now: number,
    limit: number,
  ): number;
}

export type StagingMemberKind = "object" | "manifest-root" | "manifest-node";
export interface StagingMember {
  readonly kind: StagingMemberKind;
  readonly hash: Uint8Array;
  readonly size: number;
  /**
   * Count-only members are already-durable objects referenced by the rebuilt
   * closure: they extend the chain and the certificate counts, but they get
   * no membership row, no metadata charge, and no staging-byte admission.
   */
  readonly counted?: boolean;
}
export interface StagingEntryRow {
  readonly entry_index: number;
  readonly object_hash: Uint8Array;
  readonly length: number;
}
export interface StagingLevelRow {
  readonly record_index: number;
  readonly node_hash: Uint8Array;
  readonly span: number;
  readonly entry_count: number;
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

export interface ValidatedSealedLease {
  readonly leaseId: string;
  readonly ownerNonce: Uint8Array;
  readonly stagedBytes: number;
  readonly ingestReservationBytes: number;
  readonly metadataReservationBytes: number;
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
export interface StagingStore {
  invalidateCertificateCache(leaseId?: string): void;
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
  ): void;
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
  }): void;
  consumeIngestReservation(
    leaseId: string,
    ownerNonce: Uint8Array,
    bytes: number,
  ): void;
  consumeMetadataReservation(
    leaseId: string,
    ownerNonce: Uint8Array,
    bytes: number,
  ): void;
  putEntry(
    leaseId: string,
    entryIndex: number,
    objectHash: Uint8Array,
    length: number,
  ): void;
  putEntriesBatch(
    leaseId: string,
    entries: readonly {
      readonly entryIndex: number;
      readonly objectHash: Uint8Array;
      readonly length: number;
    }[],
  ): void;
  entriesAfter(
    leaseId: string,
    cursor: number,
    limit: number,
    maxBytes: number,
  ): readonly StagingEntryRow[];
  putLevelRecord(
    leaseId: string,
    level: number,
    recordIndex: number,
    nodeHash: Uint8Array,
    span: number,
    entryCount: number,
  ): void;
  putLevelRecordsBatch(
    leaseId: string,
    level: number,
    records: readonly {
      readonly recordIndex: number;
      readonly nodeHash: Uint8Array;
      readonly span: number;
      readonly entryCount: number;
    }[],
  ): void;
  levelRecordsAfter(
    leaseId: string,
    level: number,
    cursor: number,
    limit: number,
    maxBytes: number,
  ): readonly StagingLevelRow[];
  bumpRoot(kind: number, id: string, mayRemoveRoots?: boolean): void;
  release(
    leaseId: string,
    ownerNonce: Uint8Array,
    requireSealed: boolean,
    validated?: ValidatedSealedLease,
  ): boolean;
  delete(leaseId: string, ownerNonce: Uint8Array): boolean;
  acquireReadLease(
    leaseId: string,
    ownerId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
    expiresAt: number,
    branchId?: string,
    generation?: number,
  ): void;
  renewReadLease(
    leaseId: string,
    ownerId: string,
    ownerNonce: Uint8Array,
    priorExpiresAt: number,
    now: number,
    expiresAt: number,
  ): boolean;
  releaseReadLease(leaseId: string, ownerId: string, ownerNonce: Uint8Array): boolean;
  expireBatch(now: number, limit: number): number;
  cleanupBatch(limit: number): LeaseCleanupProgress;
  appendBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
  ): ClosureCertificate;
  /** Append source-manifest boundary objects whose durability was authenticated by the caller. */
  appendCountedBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
  ): ClosureCertificate;
  /** Cache metadata for source-authenticated reused nodes registered in this transaction. */
  cacheReusedSubtreeMetadata(
    leaseId: string,
    nodeHashes: readonly Uint8Array[],
    metadata?: readonly {
      readonly nodeHash: Uint8Array;
      readonly sourceManifestHash: Uint8Array;
      readonly sourcePath: Uint8Array;
      readonly span: number;
      readonly entryCount: number;
      readonly validatedNonfinalLeafDelta: number | null;
      readonly validatedFinalLeafDelta: number | null;
      readonly summaryUsable: boolean;
      readonly summary?: {
        readonly objectCount: number;
        readonly objectBytes: number;
        readonly nodeCount: number;
        readonly nodeBytes: number;
        readonly membershipCount: number;
        readonly closureFold: Uint8Array;
      };
    }[],
    verifiedNodeSizes?: ReadonlyMap<string, number>,
  ): void;
  /** Register local-path objects already authenticated before reconciliation. */
  registerTrustedObjects(
    objects: readonly { readonly hash: Uint8Array; readonly length: number }[],
  ): void;
  flushBatchedCertificate(): void;
  snapshot(leaseId: string, ownerNonce: Uint8Array): ClosureCertificate;
  beginReconciliation(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void;
  /** Local merged rebuild fast path; generic callers retain queued validation. */
  beginTrustedReconciliation?(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void;
  reconcileBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    workLimit: number,
    options?: { readonly skipObjectBackingCheck?: boolean },
  ): ReconciliationProgress;
  /** Complete a locally authenticated manifest without materializing queues. */
  completeTrustedLocalReconciliation?(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
    freshNodeHashes: readonly Uint8Array[],
    rootSize: number,
    leafDepth: number,
  ): ReconciliationProgress;
  seal(certificate: ClosureCertificate): void;
  validateSealed(certificate: ClosureCertificate, now?: number): ValidatedSealedLease;
}

export interface GcRunRow {
  readonly id: string;
  readonly state: number;
  readonly high_water: number;
  readonly root_generation: number;
  readonly cursor_kind: number;
  readonly cursor_value: Uint8Array | null;
  readonly examined_roots: number;
  readonly deleted_roots: number;
  readonly examined_nodes: number;
  readonly deleted_nodes: number;
  readonly examined_objects: number;
  readonly deleted_objects: number;
  readonly reclaimed_object_bytes: number;
  readonly reclaimed_manifest_bytes: number;
  readonly reclaimed_overlay_bytes: number;
}
export interface GcMarkRow {
  readonly kind: number;
  readonly hash: Uint8Array;
  readonly edge_cursor: number;
  readonly payload_size: number;
}
export interface PayloadRow {
  readonly hash: Uint8Array;
  readonly size: number;
  readonly allocation_sequence: number;
}
export interface StorageSnapshotRow {
  readonly object_count: number;
  readonly object_bytes: number;
  readonly manifest_root_count: number;
  readonly manifest_root_bytes: number;
  readonly manifest_node_count: number;
  readonly manifest_node_bytes: number;
  readonly page_bytes: number;
  readonly patch_bytes: number;
  readonly result_bytes: number;
  readonly charged_metadata_bytes: number;
  readonly generation: number;
  readonly logical_bytes: number;
  readonly revisions: number;
}
export interface StorageSnapshotRunRow {
  readonly state: number;
  readonly high_water: number;
  readonly root_generation: number;
  readonly last_root_removal_generation: number;
  readonly evaluation_time_ms: number;
  readonly next_root_expiry_ms: number | null;
  readonly root_kind: number;
  readonly root_cursor: Uint8Array | null;
  readonly mark_kind: number;
  readonly mark_cursor: Uint8Array | null;
  readonly stored_kind: number;
  readonly stored_cursor: number;
  readonly logical_cursor: string;
  readonly logical_complete: number;
  readonly logical_bytes: number;
  readonly overlay_kind: number;
  readonly overlay_branch_cursor: string;
  readonly overlay_inode_cursor: string;
  readonly overlay_sequence_cursor: number;
  readonly overlay_index_cursor: number;
  readonly stored_page_bytes: number;
  readonly stored_patch_bytes: number;
  readonly reclaimable_overlay_bytes: number;
  readonly result_bytes: number;
  readonly charged_metadata_bytes: number;
  readonly revision_count: number;
  readonly stored_object_count: number;
  readonly stored_object_bytes: number;
  readonly stored_manifest_root_count: number;
  readonly stored_manifest_root_bytes: number;
  readonly stored_manifest_node_count: number;
  readonly stored_manifest_node_bytes: number;
  readonly reachable_object_count: number;
  readonly reachable_object_bytes: number;
  readonly reachable_manifest_root_count: number;
  readonly reachable_manifest_root_bytes: number;
  readonly reachable_manifest_node_count: number;
  readonly reachable_manifest_node_bytes: number;
  readonly branch_exclusive_object_bytes: number;
  readonly branch_exclusive_manifest_root_bytes: number;
  readonly branch_exclusive_manifest_node_bytes: number;
  readonly committed_batches: number;
  readonly created_at_ms: number;
  readonly updated_at_ms: number;
  readonly current?: number;
}
export interface StorageSnapshotMarkRow {
  readonly kind: number;
  readonly hash: Uint8Array;
  readonly edge_cursor: number;
  readonly accounted: number;
  readonly scope_mask: number;
  readonly payload_size: number;
}
export interface StoragePayloadRow {
  readonly hash: Uint8Array;
  readonly size: number;
  readonly allocation_sequence: number;
  readonly scope_mask: number;
}
export interface StorageInodeRow {
  readonly id: string;
  readonly size: number | null;
}
export interface HashRow {
  readonly hash: Uint8Array;
  readonly encoded: Uint8Array;
}
export interface InodeVerifyRow {
  readonly id: string;
  readonly type: number;
  readonly size: number | null;
  readonly manifest_hash: Uint8Array | null;
  readonly nlink: number;
  readonly actual_links: number;
}
export interface UsageVerificationState {
  readonly mutationSequence: number;
  readonly counters: readonly number[];
}
export interface UsageVerificationBatch {
  readonly checkedRows: number;
  readonly deltas: readonly number[];
  readonly nextKey: string | null;
  readonly complete: boolean;
}
export interface MaintenanceStore {
  beginRun(runId: string, now: number): void;
  abandonRun(runId: string, completeState: number, abandonedState: number): void;
  resumeAbandonedRun(
    runId: string,
    abandonedState: number,
    cleanupMarksState: number,
  ): void;
  run(id: string): GcRunRow | undefined;
  activeRun(): GcRunRow | undefined;
  snapshot(): StorageSnapshotRow | undefined;
  physical(): {
    readonly pageCount: number;
    readonly pageSize: number;
    readonly freePages: number;
  };
  generation(): number;
  hashes(
    kind: "roots" | "nodes",
    after: Uint8Array,
    limit: number,
    maxBytes: number,
  ): readonly HashRow[];
  objects(after: Uint8Array, limit: number, maxBytes: number): readonly PayloadRow[];
  inodes(after: string, limit: number, maxBytes: number): readonly InodeVerifyRow[];
  pendingMarks(runId: string, limit: number, maxBytes: number): readonly GcMarkRow[];
  addMark(runId: string, kind: number, hash: Uint8Array): void;
  advanceMark(
    runId: string,
    kind: number,
    hash: Uint8Array,
    edgeCursor: number,
    processed: boolean,
  ): void;
  addExamined(runId: string, roots: number, nodes: number, objects: number): void;
  seedRootsBatch(runId: string, limit: number, maxBytes: number): boolean;
  sweepCandidates(
    runId: string,
    state: number,
    highWater: number,
    limit: number,
    maxBytes: number,
  ): readonly PayloadRow[];
  reconcileSweepGeneration(runId: string, state: number): boolean;
  applySweep(
    runId: string,
    state: number,
    rows: readonly PayloadRow[],
    completeState: number,
  ): void;
  cleanupMarks(runId: string, limit: number, nextState: number): boolean;
  cleanupRootJournal(runId: string, limit: number, nextState: number): boolean;
  cleanupTerminalRuns(
    runId: string,
    limit: number,
    completeState: number,
    abandonedState: number,
    nextState: number,
  ): boolean;
  usageVerificationState(): UsageVerificationState;
  usageVerificationPhaseCount(): number;
  usageVerificationBatch(
    phase: number,
    afterKey: string | null,
    limit: number,
    maxBytes: number,
  ): UsageVerificationBatch;
  storageSnapshot(): StorageSnapshotRunRow | undefined;
  storageSnapshotCurrent(now: number): boolean;
  storageSnapshotResult(now: number): StorageSnapshotRunRow | undefined;
  beginStorageSnapshot(now: number): void;
  recordStorageSnapshotBatch(): void;
  storageRootBatch(limit: number, maxBytes: number, now: number): boolean;
  storageMarks(limit: number, maxBytes: number): readonly StorageSnapshotMarkRow[];
  addStorageMark(kind: number, hash: Uint8Array, scopeMask: number): boolean;
  accountStorageMark(kind: number, hash: Uint8Array, payloadBytes: number): boolean;
  storagePayloadSize(kind: number, hash: Uint8Array): number | undefined;
  advanceStorageMark(
    kind: number,
    hash: Uint8Array,
    edgeCursor: number,
    processed: boolean,
  ): void;
  reconcileStorageSnapshotGeneration(now: number): boolean;
  finishStorageMarking(now: number): boolean;
  storageStoredBatch(limit: number, maxBytes: number, now: number): boolean;
  storageLogicalBatch(limit: number, maxBytes: number, now: number): boolean;
  cleanupStorageMarks(limit: number, maxBytes: number, now: number): boolean;
  resetStorageMarksBatch(limit: number, maxBytes: number): boolean;
  addReclaimedOverlayBytes(runId: string, bytes: number): void;
}

export interface PersistedPatch {
  readonly sequence: number;
  readonly generation: number;
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertLength: number;
  readonly segments: readonly Uint8Array[];
}

export interface OverlayStore {
  writePages(
    branchId: string,
    inodeId: string,
    fileSize: number,
    pages: readonly CowPage[],
    now: number,
  ): number;
  headPages(
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
  ): readonly CowPage[];
  leasedPages(
    leaseId: string,
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
    baseGeneration?: number,
    ownerNonce?: Uint8Array,
  ): readonly CowPage[];
  leaseMembershipFits(
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
    baseGeneration: number,
    includePages: boolean,
    includePatches: boolean,
  ): boolean;
  pinHeads(
    leaseId: string,
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
    ownerNonce: Uint8Array,
  ): number;
  pinPatches(
    leaseId: string,
    branchId: string,
    inodeId: string,
    ownerNonce: Uint8Array,
    baseGeneration?: number,
  ): number;
  leasedPatches(
    leaseId: string,
    branchId: string,
    inodeId: string,
    ownerNonce?: Uint8Array,
    baseGeneration?: number,
  ): readonly PersistedPatch[];
  hasPages(branchId: string, inodeId: string): boolean;
  hasPatchesAfter(branchId: string, inodeId: string, baseGeneration: number): boolean;
  appendPatch(
    branchId: string,
    inodeId: string,
    currentSize: number,
    offset: number,
    deleteLength: number,
    segments: readonly Uint8Array[],
  ): number;
  patches(
    branchId: string,
    inodeId: string,
    minimumGeneration?: number,
    minimumSequence?: number,
  ): readonly PersistedPatch[];
  clearPages(branchId: string, inodeId: string): void;
  clearPatches(branchId: string, inodeId: string): void;
  cleanupUnleased(limit: number): {
    readonly worked: boolean;
    readonly reclaimedPayloadBytes: number;
  };
}

export interface StorageTransactionPorts {
  content(limits: StorageLimits, cache?: ContentCache): ContentStore;
  manifestTree(limits: StorageLimits, cache?: ContentCache): ManifestTreeStore;
  namespace(
    filesystem: FilesystemLimits,
    storage: StorageLimits,
    syscall: string,
  ): NamespaceStore;
  branches(limits: StorageLimits): BranchStore;
  staging(limits: StorageLimits, cache?: ContentCache): StagingStore;
  maintenance(limits: StorageLimits): MaintenanceStore;
  overlay(limits: StorageLimits, pageBytes: CowPageBytes): OverlayStore;
}
export interface OperationsStorage {
  readonly readOnly: boolean;
  readonly capabilities: StorageAdapterCapabilities;
  /**
   * Synchronous SHA-256 hashing capability injected by the host adapter.
   * Hosts that can provide a synchronous native hasher (node:crypto on Node)
   * do so; every other host falls back to the byte-identical pure-JS
   * implementation in `cas/sha256.ts`, so digests never depend on the host.
   */
  readonly hashBytes: HashFunction;
  /**
   * Optional asynchronous SHA-256 hasher (WebCrypto on workerd) used by the
   * streaming write pipeline to hash chunk batches concurrently with bounded
   * parallelism. Digest output is byte-identical to `hashBytes`.
   */
  readonly hashBytesAsync?: (bytes: Uint8Array) => Promise<Uint8Array>;
  initialize(options?: {
    readonly cowPageBytes?: CowPageBytes;
    readonly now?: number;
    readonly maxManifestEntries?: number;
    readonly maxManifestDepth?: number;
    readonly maxFileBytes?: number;
    readonly maxContentObjectBytes?: number;
    readonly writerProfile?: string;
  }): StorageMetadata;
  transaction<T>(
    mode: StorageTransactionMode,
    budget: StorageWorkBudget,
    callback: (ports: StorageTransactionPorts) => T,
  ): T;
  physicalStorage(): StoragePhysicalFiles;
  checkpoint(
    mode?: "passive" | "restart" | "truncate",
  ): StorageCheckpointResult | undefined;
  close(): void | Promise<void>;
}

export interface OperationsContext {
  readonly storage: OperationsStorage;
  readonly filesystem: FilesystemLimits;
  readonly durable: StorageLimits;
  readonly runtime: RuntimeLimits;
  readonly branches: BranchConfiguration;
}

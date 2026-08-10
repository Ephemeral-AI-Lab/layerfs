import type {
  BranchConfiguration,
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
import type { CanonicalPath } from "../namespace/paths.js";
import type { CowPage, CowPageBytes } from "../cow/pages.js";
import type { ContentCache } from "../cache/content-cache.js";

export type StorageTransactionMode = "read" | "write" | "exclusive";
export interface StorageWorkBudget {
  readonly maxRows: number;
  readonly maxBytes: number;
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
export interface ContentStore {
  putObject(hash: Uint8Array, bytes: Uint8Array): boolean;
  putObjectsBatch(input: readonly ContentObjectInput[]): ContentBatchResult;
  getObject(hash: Uint8Array, expectedSize?: number): Uint8Array | undefined;
  putManifestNode(hash: Uint8Array, encoded: Uint8Array): boolean;
  putManifestNodesBatch(
    nodes: readonly { readonly hash: Uint8Array; readonly encoded: Uint8Array }[],
  ): ContentBatchResult;
  putManifestRoot(hash: Uint8Array, encoded: Uint8Array): boolean;
  getManifestRoot(hash: Uint8Array): Uint8Array | undefined;
  getManifestNode(hash: Uint8Array): Uint8Array | undefined;
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
  recordInode(revision: number, inodeId: string, tombstone?: boolean): void;
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
  bumpRoot(kind: number, id: string): void;
}

export interface BranchRow {
  readonly id: string;
  readonly base_revision: number;
  readonly state: number;
  readonly generation: number;
  readonly created_at_ms: number;
  readonly terminal_at_ms: number | null;
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
  ): void;
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
  incrementGeneration(branchId: string): void;
  finish(branchId: string, state: 1 | 2, now: number): void;
  clearChanges(branchId: string): void;
  storeResult(
    operationId: string,
    outcome: number,
    encoded: Uint8Array,
    expiresAt: number,
  ): void;
}

export type StagingMemberKind = "object" | "manifest-root" | "manifest-node";
export interface StagingMember {
  readonly kind: StagingMemberKind;
  readonly hash: Uint8Array;
  readonly size: number;
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
  readonly objectCount: number;
  readonly objectBytes: number;
  readonly nodeCount: number;
  readonly nodeBytes: number;
  readonly membershipCount: number;
}
export interface ReconciliationProgress {
  readonly processed: number;
  readonly complete: boolean;
}
export interface StagingStore {
  begin(options: {
    readonly leaseId: string;
    readonly ownerId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
    readonly expiresAt: number;
    readonly kind?: number;
    readonly branchId?: string;
    readonly generation?: number;
  }): void;
  putEntry(
    leaseId: string,
    entryIndex: number,
    objectHash: Uint8Array,
    length: number,
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
  levelRecordsAfter(
    leaseId: string,
    level: number,
    cursor: number,
    limit: number,
    maxBytes: number,
  ): readonly StagingLevelRow[];
  bumpRoot(kind: number, id: string): void;
  release(leaseId: string, ownerNonce: Uint8Array, requireSealed: boolean): boolean;
  delete(leaseId: string, ownerNonce: Uint8Array): boolean;
  acquireReadLease(
    leaseId: string,
    ownerId: string,
    manifestHash: Uint8Array,
    expiresAt: number,
  ): void;
  releaseReadLease(leaseId: string, ownerId: string): boolean;
  expireBatch(now: number, limit: number): number;
  appendBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    members: readonly StagingMember[],
  ): ClosureCertificate;
  snapshot(leaseId: string, ownerNonce: Uint8Array): ClosureCertificate;
  beginReconciliation(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void;
  reconcileBatch(
    leaseId: string,
    ownerNonce: Uint8Array,
    workLimit: number,
  ): ReconciliationProgress;
  seal(certificate: ClosureCertificate): void;
  validateSealed(certificate: ClosureCertificate, now?: number): void;
}

export interface GcRunRow {
  readonly id: string;
  readonly state: number;
  readonly high_water: number;
  readonly root_generation: number;
  readonly examined_roots: number;
  readonly deleted_roots: number;
  readonly examined_nodes: number;
  readonly deleted_nodes: number;
  readonly examined_objects: number;
  readonly deleted_objects: number;
  readonly reclaimed_object_bytes: number;
  readonly reclaimed_manifest_bytes: number;
}
export interface GcMarkRow {
  readonly kind: number;
  readonly hash: Uint8Array;
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
  readonly charged_metadata_bytes: number;
  readonly generation: number;
  readonly logical_bytes: number;
  readonly revisions: number;
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
export interface MaintenanceStore {
  beginRun(runId: string, now: number): void;
  abandonRun(runId: string, completeState: number, abandonedState: number): void;
  run(id: string): GcRunRow | undefined;
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
  markProcessed(runId: string, kind: number, hash: Uint8Array): void;
  addExamined(runId: string, roots: number, nodes: number, objects: number): void;
  reconcileRoots(runId: string): void;
  sweepCandidates(
    runId: string,
    state: number,
    highWater: number,
    limit: number,
    maxBytes: number,
  ): readonly PayloadRow[];
  applySweep(
    runId: string,
    state: number,
    rows: readonly PayloadRow[],
    completeState: number,
  ): void;
}

export interface OverlayStore {
  writePages(
    branchId: string,
    inodeId: string,
    fileSize: number,
    pages: readonly CowPage[],
    now: number,
  ): number;
}

export interface StorageTransactionPorts {
  content(limits: StorageLimits, cache?: ContentCache): ContentStore;
  namespace(limits: FilesystemLimits, syscall: string): NamespaceStore;
  branches(limits: StorageLimits): BranchStore;
  staging(limits: StorageLimits): StagingStore;
  maintenance(limits: StorageLimits): MaintenanceStore;
  overlay(limits: StorageLimits, pageBytes: CowPageBytes): OverlayStore;
}
export interface OperationsStorage {
  readonly readOnly: boolean;
  readonly capabilities: StorageAdapterCapabilities;
  initialize(options?: {
    readonly cowPageBytes?: CowPageBytes;
    readonly now?: number;
  }): StorageMetadata;
  transaction<T>(
    mode: StorageTransactionMode,
    budget: StorageWorkBudget,
    callback: (ports: StorageTransactionPorts) => T,
  ): T;
  close(): void | Promise<void>;
}

export interface OperationsContext {
  readonly storage: OperationsStorage;
  readonly filesystem: FilesystemLimits;
  readonly durable: StorageLimits;
  readonly runtime: RuntimeLimits;
  readonly branches: BranchConfiguration;
}

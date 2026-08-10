/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./integrations/node-vfs; entry: packages/fs/dist/integrations/node-vfs.d.ts */

/* ===== packages/fs/dist/cache/content-cache.d.ts ===== */
import { AdmissionController } from "../resources/limits.js";
export type ContentCacheKind = "object" | "manifest-root" | "manifest-node";
export interface ContentCacheMetrics {
    readonly bytes: number;
    readonly highWaterBytes: number;
    readonly hits: number;
    readonly misses: number;
    readonly admissions: number;
    readonly bypasses: number;
    readonly evictions: number;
}
export interface ContentCacheReservation {
    readonly weight: number;
    release(): void;
}
export declare class ContentCache {
    #private;
    constructor(limitBytes: number, admission: AdmissionController);
    get(kind: ContentCacheKind, hash: Uint8Array): Uint8Array | undefined;
    reserve(weight: number): ContentCacheReservation | undefined;
    admit(kind: ContentCacheKind, hash: Uint8Array, bytes: Uint8Array, reservation: ContentCacheReservation): void;
    makeRoom(additionalBytes: number): void;
    clear(): void;
    metrics(): ContentCacheMetrics;
}

/* ===== packages/fs/dist/cow/pages.d.ts ===== */
export type CowPageBytes = 4096 | 8192 | 16384;
/** 64 MiB at 4 KiB plus both partial endpoints. */
export declare const MAX_COW_PAGES_PER_WRITE = 16385;
export declare const MAX_DIRTY_RANGES = 16384;
export interface DirtyRange {
    readonly start: number;
    readonly end: number;
}
export interface CowPage {
    readonly index: number;
    readonly bytes: Uint8Array;
}
export type CowPageIndex = number & {
    readonly __cowPageIndex: unique symbol;
};
export interface CowPageKey {
    readonly branchId: string;
    readonly inodeId: string;
    readonly pageIndex: CowPageIndex;
}
export declare function validateCowPageBytes(value: number): asserts value is CowPageBytes;
export declare function cowPageIndex(value: number): CowPageIndex;
export declare function createCowPageKey(branchId: string, inodeId: string, index: number): CowPageKey;
export declare function pageIndex(offset: number, pageBytes: CowPageBytes): CowPageIndex;
export declare function pageRange(offset: number, length: number, pageBytes: CowPageBytes, maxPages?: number): readonly number[];
export declare function mergeDirtyRanges(ranges: readonly DirtyRange[], maxRanges?: number): DirtyRange[];
export declare function writeCowPages(base: Uint8Array, offset: number, content: Uint8Array, pageBytes: CowPageBytes): CowPage[];
export declare function overlayCowPages(base: Uint8Array, pages: readonly CowPage[], pageBytes: CowPageBytes, logicalSize?: number, maxPages?: number): Uint8Array;

/* ===== packages/fs/dist/filesystem/errors.d.ts ===== */
export type FilesystemErrorCode = "EINVAL" | "ENOENT" | "ENOTDIR" | "EISDIR" | "EEXIST" | "ENOTEMPTY" | "ELOOP" | "EPERM" | "EROFS" | "EBADF" | "EAGAIN" | "EBUSY" | "EFBIG" | "ENOSPC" | "ECORRUPT" | "ESCHEMA" | "EIO";
export declare class FilesystemError extends Error {
    readonly name: "FilesystemError";
    readonly code: FilesystemErrorCode;
    readonly syscall?: string;
    readonly path?: string;
    readonly destination?: string;
    constructor(code: FilesystemErrorCode, message: string, options?: {
        syscall?: string;
        path?: string;
        destination?: string;
        cause?: unknown;
    });
}
export declare function fsError(code: FilesystemErrorCode, syscall: string, path: string | undefined, detail: string, cause?: unknown): FilesystemError;
export declare function mapStorageError(error: unknown, syscall: string, path?: string): never;
export declare function abortError(): DOMException;

/* ===== packages/fs/dist/filesystem/types.d.ts ===== */
import type { FilesystemSQLiteDriver, SQLiteDriverCapabilities } from "../sqlite/driver.js";
import type { BranchConfiguration, FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { CowPageBytes } from "../cow/pages.js";
import type { FilesystemErrorCode } from "./errors.js";
export type FileType = "file" | "directory" | "symlink";
export type FileContent = string | Uint8Array | ReadableStream<Uint8Array>;
export interface FileStat {
    readonly id: string;
    readonly name: string;
    readonly type: FileType;
    readonly mode: number;
    readonly size: number;
    readonly nlink: number;
    readonly mtimeMs: number;
    readonly ctimeMs: number;
    readonly birthtimeMs: number;
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
}
export interface DirectoryEntry {
    readonly name: string;
    readonly parentPath: string;
    readonly type: FileType;
    isFile(): boolean;
    isDirectory(): boolean;
    isSymbolicLink(): boolean;
}
export interface ReadTextOptions {
    readonly encoding: "utf8";
}
export interface ReadRangeOptions {
    readonly offset: number;
    readonly length: number;
}
export interface ReadStreamOptions {
    readonly offset?: number;
    readonly length?: number;
    readonly signal?: AbortSignal;
}
export interface WriteFileOptions {
    readonly mode?: number;
    readonly exclusive?: boolean;
    readonly signal?: AbortSignal;
}
export interface MkdirOptions {
    readonly recursive?: boolean;
    readonly mode?: number;
}
export interface ReaddirOptions {
    readonly limit?: number;
    readonly startAfter?: string;
}
export interface RmOptions {
    readonly recursive?: boolean;
    readonly force?: boolean;
}
export interface StorageFormatOptions {
    readonly cowPageBytes?: CowPageBytes;
}
export interface StorageFormat {
    readonly cowPageBytes: CowPageBytes;
    readonly hashAlgorithm: "sha256";
    readonly chunkerAlgorithm: "fastcdc-v1";
    readonly manifestFormat: "efs-merkle-manifest-v1";
}
export interface EffectiveLimit {
    readonly domain: "filesystem" | "storage" | "branch" | "runtime";
    readonly name: string;
    readonly value: number;
    readonly scope: "persisted" | "runtime";
    readonly constrainedBy: "configuration" | "format" | "adapter";
}
export interface FilesystemCapabilities {
    readonly adapter: SQLiteDriverCapabilities;
    readonly filesystem: Readonly<FilesystemLimits>;
    readonly storage: Readonly<StorageLimits>;
    readonly branch: Readonly<BranchConfiguration>;
    readonly runtime: Readonly<RuntimeLimits>;
    readonly format: Readonly<StorageFormat>;
    readonly effectiveLimits: readonly EffectiveLimit[];
    readonly readOnly: boolean;
}
export interface FilesystemObservation {
    readonly type: "operation" | "integrity" | "maintenance";
    readonly operation: string;
    readonly outcome: "success" | "error";
    readonly elapsedMs: number;
    readonly counters: Readonly<Record<string, number>>;
    readonly errorCode?: FilesystemErrorCode;
}
export type FilesystemObserver = (event: FilesystemObservation) => void;
export interface GarbageCollectionOptions {
    readonly runId?: string;
    readonly maxBatches?: number;
    readonly signal?: AbortSignal;
}
export interface GarbageCollectionResult {
    readonly runId: string;
    readonly state: "complete" | "paused" | "abandoned";
    readonly examinedManifestRootCount: number;
    readonly deletedManifestRootCount: number;
    readonly examinedManifestNodeCount: number;
    readonly deletedManifestNodeCount: number;
    readonly examinedManifestCount: number;
    readonly deletedManifestCount: number;
    readonly examinedObjectCount: number;
    readonly deletedObjectCount: number;
    readonly reclaimedObjectPayloadBytes: number;
    readonly reclaimedManifestPayloadBytes: number;
    readonly reclaimedBranchOverlayPayloadBytes: number;
    readonly committedBatches: number;
    readonly elapsedMs: number;
}
export interface PhysicalStorageSnapshot {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
    readonly freelistBytes?: number;
}
export interface StorageSnapshot {
    readonly rootMutationGeneration: number;
    readonly mainLogicalBytes: number;
    readonly storedObjectPayloadBytes: number;
    readonly storedManifestPayloadBytes: number;
    readonly reachableObjectPayloadBytes: number;
    readonly reachableManifestPayloadBytes: number;
    readonly reclaimablePayloadBytes: number;
    readonly branchPageBytes: number;
    readonly branchPatchBytes: number;
    readonly branchExclusiveObjectBytes: number;
    readonly branchExclusiveManifestBytes: number;
    readonly branchExclusivePayloadBytes: number;
    readonly objectCount: number;
    readonly manifestRootCount: number;
    readonly manifestNodeCount: number;
    readonly manifestCount: number;
    readonly chargedMetadataBytes: number;
    readonly revisionCount: number;
    readonly includesNamespaceMetadata: boolean;
    readonly includesOperationResults: boolean;
    readonly physical?: PhysicalStorageSnapshot;
}
export type VerificationScope = "metadata" | "namespace" | "manifests" | "objects" | "head";
export interface VerificationOptions {
    readonly scopes?: readonly VerificationScope[];
    readonly cursor?: string;
    readonly maxEntities?: number;
    readonly signal?: AbortSignal;
}
export interface VerificationResult {
    readonly rootMutationGeneration: number;
    readonly checkedEntities: number;
    readonly complete: boolean;
    readonly nextCursor: string | null;
}
export interface FilesystemMaintenance {
    collectGarbage(options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
    snapshotStorage(): Promise<StorageSnapshot>;
    verify(options?: VerificationOptions): Promise<VerificationResult>;
}
export interface OpenFilesystemOptions {
    readonly database: FilesystemSQLiteDriver;
    readonly clock?: () => number;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly branch?: Partial<BranchConfiguration>;
    readonly observer?: FilesystemObserver;
    readonly ownsDatabase?: boolean;
}
export interface EphemeralFilesystem {
    readFile(path: string): Promise<Uint8Array>;
    readFile(path: string, options: ReadTextOptions): Promise<string>;
    readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array>;
    readStream(path: string, options?: ReadStreamOptions): Promise<ReadableStream<Uint8Array>>;
    writeFile(path: string, content: FileContent, options?: WriteFileOptions): Promise<void>;
    writeRange(path: string, offset: number, content: Uint8Array): Promise<void>;
    replaceRange(path: string, offset: number, deleteLength: number, insertBytes: Uint8Array): Promise<void>;
    truncate(path: string, size?: number): Promise<void>;
    mkdir(path: string, options?: MkdirOptions): Promise<void>;
    readdir(path: string, options?: ReaddirOptions): Promise<DirectoryEntry[]>;
    stat(path: string): Promise<FileStat>;
    lstat(path: string): Promise<FileStat>;
    chmod(path: string, mode: number): Promise<void>;
    link(existingPath: string, newPath: string): Promise<void>;
    symlink(target: string, path: string): Promise<void>;
    readlink(path: string): Promise<string>;
    rename(oldPath: string, newPath: string): Promise<void>;
    unlink(path: string): Promise<void>;
    rm(path: string, options?: RmOptions): Promise<void>;
    close(): Promise<void>;
}

/* ===== packages/fs/dist/integrations/node-vfs.d.ts ===== */
import type { StorageFormatOptions } from "../filesystem/types.js";
import { type NodeVfsFilesystemBridge, type SyncPreparedContent } from "../operations/node-vfs-bridge.js";
import type { FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteDriver } from "../sqlite/driver.js";
/** Public composition-root options for the synchronous Node VFS bridge. */
export interface CreateNodeVfsBridgeOptions {
    readonly database: FilesystemSQLiteDriver;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly clock?: () => number;
}
/** Compose the public bridge with the private SQLite storage implementation. */
export declare function createNodeVfsBridge(options: CreateNodeVfsBridgeOptions): NodeVfsFilesystemBridge;
export type { NodeVfsFilesystemBridge, SyncPreparedContent };

/* ===== packages/fs/dist/manifests/codec.d.ts ===== */
export declare const ROOT_ENVELOPE_BYTES = 68;
export declare const NODE_HEADER_BYTES = 32;
export declare const LEAF_RECORD_BYTES = 36;
export declare const INTERNAL_RECORD_BYTES = 48;
export declare const MAX_MANIFEST_ENTRY_COUNT = 4294967295;
export declare const MAX_MANIFEST_NODE_BYTES: number;
export interface ManifestParameters {
    readonly minimum: number;
    readonly average: number;
    readonly maximum: number;
}
export interface ManifestRoot {
    readonly parameters: ManifestParameters;
    readonly fileSize: number;
    readonly entryCount: number;
    readonly rootNodeHash: Uint8Array;
}
export interface ManifestEntry {
    readonly hash: Uint8Array;
    readonly length: number;
}
export interface ManifestChild {
    readonly hash: Uint8Array;
    readonly span: number;
    readonly entryCount: number;
}
export interface ManifestLeaf {
    readonly kind: "leaf";
    readonly span: number;
    readonly entryCount: number;
    readonly entries: readonly ManifestEntry[];
}
export interface ManifestInternal {
    readonly kind: "internal";
    readonly span: number;
    readonly entryCount: number;
    readonly children: readonly ManifestChild[];
}
export type ManifestNode = ManifestLeaf | ManifestInternal;
export declare function snapshotManifestParameters(parameters: ManifestParameters): Readonly<ManifestParameters>;
export declare function validateManifestParameters(parameters: ManifestParameters): void;
/**
 * Validates parameters that this runtime may use to construct or materialize
 * content. Binary inspection remains format-complete for valid uint32 values.
 */
export declare function validateSupportedManifestParameters(parameters: ManifestParameters): void;
export declare function encodeManifestRoot(root: ManifestRoot): Uint8Array;
export declare function decodeManifestRoot(bytes: Uint8Array, expectedHash?: Uint8Array): ManifestRoot;
export declare function encodeManifestNode(node: ManifestNode): Uint8Array;
export declare function decodeManifestNode(bytes: Uint8Array, expectedHash?: Uint8Array): ManifestNode;

/* ===== packages/fs/dist/namespace/paths.d.ts ===== */
import type { FilesystemLimits } from "../resources/limits.js";
export interface CanonicalPath {
    readonly value: string;
    readonly segments: readonly string[];
    readonly encodedSegments: readonly Uint8Array[];
}
export declare function canonicalizePath(input: string, limits: FilesystemLimits, syscall: string): CanonicalPath;
export declare function validateName(name: string, limits: FilesystemLimits, syscall: string): Uint8Array;
export declare function validateSymlinkTarget(target: string, limits: FilesystemLimits, syscall: string): void;
export declare function compareUtf8(left: string, right: string): number;
export declare function assertCanonicalNameBytes(name: string, bytes: Uint8Array): void;

/* ===== packages/fs/dist/operations/node-vfs-bridge.d.ts ===== */
import { type FilesystemLimits, type RuntimeLimits, type StorageLimits } from "../resources/limits.js";
import type { DirectoryEntry, FileStat, StorageFormatOptions } from "../filesystem/types.js";
import type { OperationsStorage } from "./storage-ports.js";
export interface SyncPreparedContent {
    readonly manifestHash: Uint8Array;
    readonly size: number;
}
export interface NodeVfsOperationsBridgeOptions {
    readonly port: OperationsStorage;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly clock?: () => number;
}
export interface NodeVfsFilesystemBridge {
    readonly filesystemLimits: Readonly<FilesystemLimits>;
    readonly storageLimits: Readonly<StorageLimits>;
    readonly runtimeLimits: Readonly<RuntimeLimits>;
    readonly cowPageBytes: 4096 | 8192 | 16384;
    existsSync(path: string): boolean;
    statSync(path: string, followFinal?: boolean): FileStat;
    readdirSync(path: string): DirectoryEntry[];
    readlinkSync(path: string): string;
    readIntoSync(path: string, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    readFileSync(path: string): Uint8Array;
    prepareContentSync(bytes: Uint8Array): SyncPreparedContent;
    readPreparedIntoSync(prepared: SyncPreparedContent, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    commitPreparedSync(path: string, prepared: SyncPreparedContent, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
    }): void;
    writeFileSync(path: string, bytes: Uint8Array, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
    }): void;
    mkdirSync(path: string, options?: {
        recursive?: boolean;
        mode?: number;
    }): void;
    chmodSync(path: string, mode: number): void;
    linkSync(existingPath: string, newPath: string): void;
    symlinkSync(target: string, path: string): void;
    renameSync(oldPath: string, newPath: string): void;
    unlinkSync(path: string): void;
    rmdirSync(path: string): void;
}
export declare function createNodeVfsOperationsBridge(options: NodeVfsOperationsBridgeOptions): NodeVfsFilesystemBridge;

/* ===== packages/fs/dist/operations/storage-ports.d.ts ===== */
import type { BranchConfiguration, FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { CanonicalPath } from "../namespace/paths.js";
import type { CowPage, CowPageBytes } from "../cow/pages.js";
import type { ContentCache } from "../cache/content-cache.js";
import type { ManifestNode, ManifestParameters } from "../manifests/codec.js";
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
export interface AuthenticatedManifestCursor {
    readonly fileSize: number;
    readonly position: number;
    peekEntry(): AuthenticatedManifestEntry | null;
    nextEntry(): AuthenticatedManifestEntry | null;
    readInto(destination: Uint8Array, destinationOffset: number, length: number): number;
}
export interface AuthenticatedManifestEntry {
    readonly hash: Uint8Array;
    readonly length: number;
    readonly offset: number;
}
export interface ContentStore {
    putObject(hash: Uint8Array, bytes: Uint8Array): boolean;
    putObjectsBatch(input: readonly ContentObjectInput[]): ContentBatchResult;
    getObject(hash: Uint8Array, expectedSize?: number): Uint8Array | undefined;
    putManifestNode(hash: Uint8Array, encoded: Uint8Array): boolean;
    putManifestNodesBatch(nodes: readonly {
        readonly hash: Uint8Array;
        readonly encoded: Uint8Array;
    }[]): ContentBatchResult;
    putManifestRoot(hash: Uint8Array, encoded: Uint8Array): boolean;
    getManifestRoot(hash: Uint8Array): Uint8Array | undefined;
    getManifestNode(hash: Uint8Array): Uint8Array | undefined;
    openManifestCursor(manifestHash: Uint8Array, offset: number): AuthenticatedManifestCursor;
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
    protectSourceManifest(leaseId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array): void;
    registerReusedSubtrees(leaseId: string, ownerNonce: Uint8Array, sourceManifestHash: Uint8Array, claims: readonly {
        readonly sourcePath: readonly number[];
        readonly nodeHash: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
    }[]): void;
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
    resolveOptional(input: string | CanonicalPath, followFinal?: boolean): ResolvedPath | undefined;
    resolveParent(path: CanonicalPath): {
        readonly parent: ResolvedPath;
        readonly name: string;
        readonly nameSort: Uint8Array;
    };
    nextRevision(now: number, changeCount: number, writer?: string): number;
    recordInode(revision: number, inodeId: string, tombstone?: boolean): void;
    recordEntry(revision: number, parentInode: string, nameSort: Uint8Array, tombstone?: boolean): void;
    putEntry(parentInode: string, nameSort: Uint8Array, name: string | null, inodeId: string | null, token: number): void;
    children(parentInode: string, limit: number, maxBytes: number, startAfter?: Uint8Array): readonly ChildRow[];
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
    setFileContent(id: string, size: number, manifestHash: Uint8Array, mtime: number, ctime: number, token: number, expectedToken?: number): number;
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
    historyEntries(parentInode: string, revision: number): readonly BranchHistoryEntryRow[];
    historicEntry(parentInode: string, nameSort: Uint8Array, revision: number): BranchHistoryRow | undefined;
    historicInode(inodeId: string, revision: number): BranchHistoryRow | undefined;
    change(branchId: string, path: Uint8Array): BranchChangeRow | undefined;
    changes(branchId: string): readonly BranchChangeRow[];
    activeCount(): number;
    headRevision(): number;
    revisionExists(revision: number): boolean;
    create(id: string, baseRevision: number, now: number): BranchRow;
    row(id: string): BranchRow | undefined;
    operationResult(operationId: string, maxBytes: number): BranchResultRow | undefined;
    reserveOperation(operationId: string, branchId: string, generation: number, now: number): void;
    putChange(branchId: string, path: Uint8Array, expectedToken: number | null, kind: number, encoded: Uint8Array | null): void;
    putInodeExpectation(branchId: string, inodeId: string, expectedToken: number | null): void;
    setManifestRoot(branchId: string, path: Uint8Array, manifestHash?: Uint8Array): void;
    changeCount(branchId: string): number;
    incrementGeneration(branchId: string): void;
    finish(branchId: string, state: 1 | 2, now: number): void;
    clearChanges(branchId: string): void;
    storeResult(operationId: string, outcome: number, encoded: Uint8Array, expiresAt: number): void;
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
export interface LeaseCleanupProgress {
    readonly worked: boolean;
    readonly deletedRows: number;
    readonly deletedLeases: number;
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
    putEntry(leaseId: string, entryIndex: number, objectHash: Uint8Array, length: number): void;
    entriesAfter(leaseId: string, cursor: number, limit: number, maxBytes: number): readonly StagingEntryRow[];
    putLevelRecord(leaseId: string, level: number, recordIndex: number, nodeHash: Uint8Array, span: number, entryCount: number): void;
    levelRecordsAfter(leaseId: string, level: number, cursor: number, limit: number, maxBytes: number): readonly StagingLevelRow[];
    bumpRoot(kind: number, id: string): void;
    release(leaseId: string, ownerNonce: Uint8Array, requireSealed: boolean): boolean;
    delete(leaseId: string, ownerNonce: Uint8Array): boolean;
    acquireReadLease(leaseId: string, ownerId: string, manifestHash: Uint8Array, expiresAt: number): void;
    releaseReadLease(leaseId: string, ownerId: string): boolean;
    expireBatch(now: number, limit: number): number;
    cleanupBatch(limit: number): LeaseCleanupProgress;
    appendBatch(leaseId: string, ownerNonce: Uint8Array, members: readonly StagingMember[]): ClosureCertificate;
    snapshot(leaseId: string, ownerNonce: Uint8Array): ClosureCertificate;
    beginReconciliation(leaseId: string, ownerNonce: Uint8Array, manifestHash: Uint8Array): void;
    reconcileBatch(leaseId: string, ownerNonce: Uint8Array, workLimit: number): ReconciliationProgress;
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
    hashes(kind: "roots" | "nodes", after: Uint8Array, limit: number, maxBytes: number): readonly HashRow[];
    objects(after: Uint8Array, limit: number, maxBytes: number): readonly PayloadRow[];
    inodes(after: string, limit: number, maxBytes: number): readonly InodeVerifyRow[];
    pendingMarks(runId: string, limit: number, maxBytes: number): readonly GcMarkRow[];
    addMark(runId: string, kind: number, hash: Uint8Array): void;
    markProcessed(runId: string, kind: number, hash: Uint8Array): void;
    addExamined(runId: string, roots: number, nodes: number, objects: number): void;
    reconcileRoots(runId: string): void;
    sweepCandidates(runId: string, state: number, highWater: number, limit: number, maxBytes: number): readonly PayloadRow[];
    applySweep(runId: string, state: number, rows: readonly PayloadRow[], completeState: number): void;
}
export interface OverlayStore {
    writePages(branchId: string, inodeId: string, fileSize: number, pages: readonly CowPage[], now: number): number;
}
export interface StorageTransactionPorts {
    content(limits: StorageLimits, cache?: ContentCache): ContentStore;
    manifestTree(limits: StorageLimits, cache?: ContentCache): ManifestTreeStore;
    namespace(filesystem: FilesystemLimits, storage: StorageLimits, syscall: string): NamespaceStore;
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
    transaction<T>(mode: StorageTransactionMode, budget: StorageWorkBudget, callback: (ports: StorageTransactionPorts) => T): T;
    physicalStorage(): StoragePhysicalFiles;
    checkpoint(mode?: "passive" | "restart" | "truncate"): StorageCheckpointResult | undefined;
    close(): void | Promise<void>;
}
export interface OperationsContext {
    readonly storage: OperationsStorage;
    readonly filesystem: FilesystemLimits;
    readonly durable: StorageLimits;
    readonly runtime: RuntimeLimits;
    readonly branches: BranchConfiguration;
}

/* ===== packages/fs/dist/resources/limits.d.ts ===== */
export interface FilesystemLimits {
    readonly maxPathBytes: number;
    readonly maxNameBytes: number;
    readonly maxSymlinkTargetBytes: number;
    readonly maxSymlinkTraversals: number;
    readonly maxMaterializedBytes: number;
    readonly preferredStreamChunkBytes: number;
    readonly maxAtomicTreeEntries: number;
    readonly maxReaddirEntries: number;
}
export interface StorageLimits {
    readonly maxManifestEntries: number;
    readonly maxManifestNodeBytes: number;
    readonly maxManifestDepth: number;
    readonly maxFileBytes: number;
    readonly maxWriteBytes: number;
    readonly maxManagedPayloadBytes: number;
    readonly maxChargedMetadataBytes: number;
    readonly maxPhysicalDatabaseBytes: number;
    readonly maxJournalBytes: number;
    readonly maxStagingPayloadBytes: number;
    readonly maxBranchOverlayBytes: number;
    readonly maxMaintenanceBytes: number;
    readonly maintenanceReserveBytes: number;
    readonly maxPermanentIdentifiers: number;
    readonly maxFinalTransactionRows: number;
    readonly maxFinalTransactionBytes: number;
    readonly maxRevisionReplaySteps: number;
    readonly maxPatchesPerFile: number;
    readonly maxPatchBytesPerFile: number;
    readonly maxQueryBatchSize: number;
    readonly maxGcBatchSize: number;
    readonly maxRetainedRevisions: number;
    readonly readLeaseMs: number;
    readonly stagingLeaseMs: number;
}
export interface RuntimeLimits {
    readonly maxManagedResidentBytes: number;
    readonly maxCacheBytes: number;
    readonly maxPendingWriteBytes: number;
    readonly maxWriteSessionBytes: number;
    readonly maxPrefetchBytes: number;
    readonly maxQueryBatchBytes: number;
    readonly maxPreparedResultBytes: number;
    readonly maxConcurrentStreams: number;
    readonly maxConcurrentOperations: number;
    readonly maxOpenBranchHandles: number;
    readonly maxOpenNodeVfsSessions: number;
}
export interface BranchConfiguration {
    readonly maxBranchIdBytes: number;
    readonly maxOperationIdBytes: number;
    readonly maxActiveBranches: number;
    readonly maxChangedPathsPerBranch: number;
    readonly maxChangedPathBytes: number;
    readonly maxConflictsPerPublication: number;
    readonly maxConflictResultBytes: number;
    readonly terminalBranchRetentionMs: number;
    readonly publicationResultRetentionMs: number;
}
/** Structural adapter limits consumed by resource policy without depending on SQLite. */
export interface StorageAdapterLimits {
    readonly maxBlobBytes: number;
    readonly maxBindings: number;
    readonly maxPhysicalDatabaseBytes: number;
    readonly maxJournalBytes: number;
}
/** Hard version-0.1 content-object/streaming CDC allocation ceiling. */
export declare const MAX_CONTENT_OBJECT_BYTES: number;
/** Conservative per-object binding/row/index envelope in a durable transaction. */
export declare const CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES = 256;
export declare function maxPersistedContentObjectBytes(storage: Pick<StorageLimits, "maxFinalTransactionBytes">): number;
/** Additional caller input one collecting FastCDC push may return with a prebuffer. */
export declare const MAX_CONTENT_COLLECTOR_PUSH_BYTES: number;
/** Maximum retained chunk references returned by one collecting push call. */
export declare const MAX_CONTENT_COLLECTOR_REFERENCES = 16384;
/** Conservative allocated-capacity charge for one JavaScript array element slot. */
export declare const CONTENT_COLLECTOR_REFERENCE_BYTES = 16;
/**
 * Source/carry, chunker, emitted chunk, sink handoff, retained object, and
 * replacement-window copies may coexist in the bounded rebuild pipeline.
 */
export declare const MAX_CONTENT_WORKING_SET_COPIES = 6;
export declare const MIN_CANONICAL_MANIFEST_NODE_BYTES = 9248;
export declare const DEFAULT_FILESYSTEM_LIMITS: FilesystemLimits;
export declare const DEFAULT_STORAGE_LIMITS: StorageLimits;
export declare const DEFAULT_RUNTIME_LIMITS: RuntimeLimits;
export declare const DEFAULT_BRANCH_CONFIGURATION: BranchConfiguration;
export declare function resolveLimits<T extends object>(defaults: T, configured?: Partial<T>): Readonly<T>;
export declare function constrainStorageLimits(configured: Partial<StorageLimits> | undefined, adapter: StorageAdapterLimits): Readonly<StorageLimits>;
export declare function validateRuntimeLimits(filesystem: FilesystemLimits, storage: StorageLimits, runtime: RuntimeLimits, cowPageBytes: number): void;
export declare function requiredRuntimeProgressBytes(filesystem: FilesystemLimits, storage: StorageLimits, cowPageBytes: number): number;
export declare class AdmissionController {
    #private;
    constructor(limit: number);
    reserve(bytes: number): () => void;
    get usedBytes(): number;
    get peakBytes(): number;
    get limitBytes(): number;
}

/* ===== packages/fs/dist/sqlite/driver.d.ts ===== */
export type SqliteValue = null | string | number | Uint8Array;
export type SqliteBindings = readonly SqliteValue[];
export type SqliteRow = Readonly<Record<string, SqliteValue>>;
export interface SqliteRunResult {
    readonly changes: number;
    readonly lastInsertRowid?: number;
}
export interface QueryBudget {
    readonly maxRows: number;
    readonly maxBytes: number;
}
export interface FilesystemSQLiteTransaction {
    readonly scope: symbol;
    run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
    all<Row extends SqliteRow = SqliteRow>(sql: string, bindings: SqliteBindings, budget: QueryBudget): readonly Row[];
}
export type TransactionMode = "read" | "write" | "exclusive";
export interface SQLiteDriverCapabilities {
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
export interface SQLitePhysicalStorage {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
}
export interface SQLiteCheckpointResult {
    readonly mode: "passive" | "restart" | "truncate";
    readonly busy: number;
    readonly logFrames: number;
    readonly checkpointedFrames: number;
    readonly walBytes?: number;
}
export interface FilesystemSQLiteDriver {
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    physicalStorage?(): SQLitePhysicalStorage;
    checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
    close(): void | Promise<void>;
}

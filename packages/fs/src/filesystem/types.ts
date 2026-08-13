import type {
  FilesystemSQLiteDriver,
  SQLiteDriverCapabilities,
} from "../sqlite/driver.js";
import type {
  BranchConfiguration,
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
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
  /** Required upper bound for a streamed write; buffered values infer their length. */
  readonly maxBytes?: number;
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
  readonly phase:
    | "marking"
    | "sweeping-manifest-roots"
    | "sweeping-manifest-nodes"
    | "sweeping-objects"
    | "cleaning-marks"
    | "cleaning-root-journal"
    | "cleaning-terminal-runs"
    | "complete"
    | "abandoned";
  readonly progressCursor: string | null;
  /** Exact when zero; null means the remaining total is not boundedly knowable yet. */
  readonly remainingWork: number | null;
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
export interface StorageSnapshotOptions {
  readonly maxBatches?: number;
  readonly signal?: AbortSignal;
}
export interface PhysicalStorageSnapshot {
  readonly mainFileBytes?: number;
  readonly walBytes?: number;
  readonly freelistBytes?: number;
}
export interface StorageSnapshot {
  readonly state: "complete" | "paused";
  readonly phase:
    | "roots"
    | "marking"
    | "stored-payload"
    | "logical-namespace"
    | "branch-overlays"
    | "mark-cleanup"
    | "mark-reset"
    | "complete";
  readonly progressCursor: string | null;
  /** Exact when zero; null means the remaining total is not boundedly knowable yet. */
  readonly remainingWork: number | null;
  readonly committedBatches: number;
  readonly batchSize: number;
  readonly elapsedMs: number;
  readonly peakManagedResidentBytes: number;
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
  readonly operationResultPayloadBytes: number;
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
export type VerificationScope =
  "metadata" | "namespace" | "manifests" | "objects" | "head";
export interface VerificationOptions {
  readonly scopes?: readonly VerificationScope[];
  readonly cursor?: string;
  readonly maxEntities?: number;
  readonly signal?: AbortSignal;
}
export interface VerificationResult {
  readonly rootMutationGeneration: number;
  readonly phase: "roots" | "nodes" | "objects" | "inodes" | "usage" | "complete";
  readonly progressCursor: string | null;
  readonly remainingWork: number | null;
  readonly committedBatches: 0;
  readonly elapsedMs: number;
  readonly peakManagedResidentBytes: number;
  readonly checkedEntities: number;
  readonly complete: boolean;
  readonly nextCursor: string | null;
}
export interface FilesystemMaintenance {
  collectGarbage(options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
  snapshotStorage(options?: StorageSnapshotOptions): Promise<StorageSnapshot>;
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
  readStream(
    path: string,
    options?: ReadStreamOptions,
  ): Promise<ReadableStream<Uint8Array>>;
  writeFile(
    path: string,
    content: FileContent,
    options?: WriteFileOptions,
  ): Promise<void>;
  writeRange(path: string, offset: number, content: Uint8Array): Promise<void>;
  replaceRange(
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): Promise<void>;
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
export interface EphemeralFilesystemAdministration {
  readonly capabilities: FilesystemCapabilities;
  readonly maintenance: FilesystemMaintenance;
}

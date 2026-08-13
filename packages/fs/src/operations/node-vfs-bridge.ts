import {
  DEFAULT_BRANCH_CONFIGURATION,
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  AdmissionController,
  constrainStorageLimits,
  maxPersistedContentObjectBytes,
  persistedWriterProfile,
  resolveLimits,
  validateRuntimeLimits,
  type FilesystemLimits,
  type RuntimeLimits,
  type StorageLimits,
} from "../resources/limits.js";
import { ContentCache } from "../cache/content-cache.js";
import {
  canonicalizePath,
  validateSymlinkTarget,
  type CanonicalPath,
} from "../namespace/paths.js";
import { readManifestInto } from "../operations/manifest-io.js";
import type {
  DirectoryEntry,
  FileStat,
  StorageFormatOptions,
} from "../filesystem/types.js";
import { fsError, mapStorageError } from "../filesystem/errors.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import {
  copyBytes,
  equalBytes,
  intrinsicByteLength,
  intrinsicByteRange,
} from "../cas/bytes.js";
import {
  prepareContentSourceSync,
  type SynchronousContentSource,
} from "./streaming-prepare.js";
import type {
  AuthenticatedManifestCursor,
  ClosureCertificate,
  InodeRow,
  NamespaceStore,
  OperationsStorage,
  StorageTransactionPorts,
} from "./storage-ports.js";

/** Opaque durable content owned by the core bridge. */
export interface NodeVfsPreparedContent {
  readonly size: number;
  /** Bounded source bytes read while applying page-local edits. */
  readonly editSourceBytes?: number;
}
export interface NodeVfsOverwriteEdit {
  readonly offset: number;
  readonly source: SynchronousContentSource;
}
export interface NodeVfsCommitResult {
  readonly pinned: NodeVfsPinnedReadBridge;
}
export interface SyncPreparedContent {
  readonly manifestHash: Uint8Array;
  readonly size: number;
  readonly certificate: ClosureCertificate;
  /** Source token captured by a bounded edit preparation. */
  readonly expectedToken?: number;
  readonly preparationMode?: "local-rebuild" | "durable-path-copy";
  readonly sourceBytesRead?: number;
}
export interface NodeVfsPinnedReadBridge {
  readonly canonicalPath: string;
  readonly inodeId: string;
  readonly stat: FileStat;
  readonly size: number;
  readIntoSync(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  closeSync(): void;
}
export interface NodeVfsManagedSlab {
  readonly bytes: Uint8Array;
  release(): void;
}
export interface NodeVfsManagedMemorySnapshot {
  readonly usedBytes: number;
  readonly peakBytes: number;
  readonly limitBytes: number;
}
export interface NodeVfsResolvedPath {
  readonly canonicalPath: string;
  readonly stat: FileStat;
}
export interface NodeVfsOperationsBridgeOptions {
  readonly port: OperationsStorage;
  readonly filesystem?: Partial<FilesystemLimits>;
  readonly storage?: Partial<StorageLimits>;
  readonly runtime?: Partial<RuntimeLimits>;
  readonly format?: StorageFormatOptions;
  readonly clock?: () => number;
  /** Core-owned bounded COW preparation; never exposed outside this bridge. */
  readonly prepareOverwriteSync?: (
    path: string,
    offset: number,
    source: SynchronousContentSource,
  ) => SyncPreparedContent | undefined;
  readonly prepareOverwritesSync?: (
    path: string,
    edits: readonly NodeVfsOverwriteEdit[],
  ) => SyncPreparedContent | undefined;
  /** Existing filesystem resources supplied by the core composition root. */
  readonly shared?: {
    readonly filesystemLimits: Readonly<FilesystemLimits>;
    readonly storageLimits: Readonly<StorageLimits>;
    readonly runtimeLimits: Readonly<RuntimeLimits>;
    readonly cowPageBytes: 4096 | 8192 | 16384;
    readonly admission: AdmissionController;
    readonly cache: ContentCache;
  };
}
export interface NodeVfsFilesystemBridge {
  readonly filesystemLimits: Readonly<FilesystemLimits>;
  readonly storageLimits: Readonly<StorageLimits>;
  readonly runtimeLimits: Readonly<RuntimeLimits>;
  readonly cowPageBytes: 4096 | 8192 | 16384;
  canonicalPathSync(path: string, syscall?: string): string;
  resolvePathSync(path: string, followFinal?: boolean): NodeVfsResolvedPath;
  openPinnedReadSync(path: string): NodeVfsPinnedReadBridge;
  acquireSlabSync(
    source: Uint8Array,
    sourceOffset: number,
    length: number,
  ): NodeVfsManagedSlab | undefined;
  reserveControlSync(bytes: number): (() => void) | undefined;
  managedMemorySync(): NodeVfsManagedMemorySnapshot;
  existsSync(path: string): boolean;
  statSync(path: string, followFinal?: boolean): FileStat;
  readdirSync(path: string): DirectoryEntry[];
  readlinkSync(path: string): string;
  readIntoSync(
    path: string,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  readRangeSync(path: string, position: number, length: number): Uint8Array;
  readFileSync(path: string): Uint8Array;
  prepareContentSync(bytes: Uint8Array): NodeVfsPreparedContent;
  prepareContentSourceSync(source: SynchronousContentSource): NodeVfsPreparedContent;
  prepareOverwriteSync(
    path: string,
    offset: number,
    source: SynchronousContentSource,
  ): NodeVfsPreparedContent | undefined;
  prepareOverwritesSync(
    path: string,
    edits: readonly NodeVfsOverwriteEdit[],
  ): NodeVfsPreparedContent | undefined;
  abortPreparedSync(prepared: NodeVfsPreparedContent): void;
  readPreparedIntoSync(
    prepared: NodeVfsPreparedContent,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  commitPreparedSync(
    path: string,
    prepared: NodeVfsPreparedContent,
    options?: {
      create?: boolean;
      exclusive?: boolean;
      mode?: number;
      inodeId?: string;
      aliases?: readonly string[];
    },
  ): NodeVfsCommitResult;
  writeFileSync(
    path: string,
    bytes: Uint8Array,
    options?: { create?: boolean; exclusive?: boolean; mode?: number },
  ): void;
  mkdirSync(path: string, options?: { recursive?: boolean; mode?: number }): void;
  chmodSync(path: string, mode: number): void;
  linkSync(existingPath: string, newPath: string): void;
  symlinkSync(target: string, path: string): void;
  renameSync(oldPath: string, newPath: string): void;
  unlinkSync(path: string): void;
  rmdirSync(path: string): void;
}

function kind(value: number): "file" | "directory" | "symlink" {
  return value === 0
    ? "file"
    : value === 1
      ? "directory"
      : value === 2
        ? "symlink"
        : (() => {
            throw new Error("ECORRUPT: invalid inode type");
          })();
}
function stat(inode: InodeRow, name: string): FileStat {
  const type = kind(inode.type);
  const size =
    type === "file"
      ? inode.size!
      : type === "symlink"
        ? encodeUtf8(inode.symlink_target ?? "").length
        : 0;
  return Object.freeze({
    id: inode.id,
    name,
    type,
    mode: inode.mode,
    size,
    nlink: inode.nlink,
    mtimeMs: inode.mtime_ms,
    ctimeMs: inode.ctime_ms,
    birthtimeMs: inode.birthtime_ms,
    isFile: () => type === "file",
    isDirectory: () => type === "directory",
    isSymbolicLink: () => type === "symlink",
  });
}

function validatedMode(mode: number | undefined, fallback: number): number {
  const value = mode ?? fallback;
  if (!Number.isSafeInteger(value) || value < 0)
    throw fsError(
      "EINVAL",
      "nodeVfs",
      undefined,
      "mode must be a nonnegative safe integer",
    );
  return value & 0o7777;
}

class Bridge implements NodeVfsFilesystemBridge {
  readonly filesystemLimits: Readonly<FilesystemLimits>;
  readonly storageLimits: Readonly<StorageLimits>;
  readonly runtimeLimits: Readonly<RuntimeLimits>;
  readonly cowPageBytes: 4096 | 8192 | 16384;
  readonly #port: OperationsStorage;
  readonly #clock: () => number;
  readonly #admission: AdmissionController;
  readonly #cache: ContentCache;
  readonly #prepareOverwriteSync:
    | ((
        path: string,
        offset: number,
        source: SynchronousContentSource,
      ) => SyncPreparedContent | undefined)
    | undefined;
  readonly #prepareOverwritesSync:
    | ((
        path: string,
        edits: readonly NodeVfsOverwriteEdit[],
      ) => SyncPreparedContent | undefined)
    | undefined;
  readonly #prepared = new WeakMap<NodeVfsPreparedContent, SyncPreparedContent>();
  constructor(options: NodeVfsOperationsBridgeOptions) {
    this.#port = options.port;
    this.#clock = options.clock ?? Date.now;
    this.#prepareOverwriteSync = options.prepareOverwriteSync;
    this.#prepareOverwritesSync = options.prepareOverwritesSync;
    if (options.shared) {
      this.filesystemLimits = options.shared.filesystemLimits;
      this.storageLimits = options.shared.storageLimits;
      this.runtimeLimits = options.shared.runtimeLimits;
      this.cowPageBytes = options.shared.cowPageBytes;
      this.#admission = options.shared.admission;
      this.#cache = options.shared.cache;
      return;
    }
    this.filesystemLimits = resolveLimits(
      DEFAULT_FILESYSTEM_LIMITS,
      options.filesystem,
    );
    this.storageLimits = constrainStorageLimits(
      options.storage,
      options.port.capabilities,
    );
    this.runtimeLimits = resolveLimits(DEFAULT_RUNTIME_LIMITS, options.runtime);
    for (const [domain, values] of [
      ["filesystem", this.filesystemLimits],
      ["runtime", this.runtimeLimits],
      ["branch", DEFAULT_BRANCH_CONFIGURATION],
    ] as const)
      for (const [name, value] of Object.entries(values))
        if (!Number.isSafeInteger(value) || value <= 0)
          throw new RangeError(`${domain}.${name} must be a positive safe integer`);
    validateRuntimeLimits(
      this.filesystemLimits,
      this.storageLimits,
      this.runtimeLimits,
      options.format?.cowPageBytes ?? 16_384,
    );
    this.#admission = new AdmissionController(
      this.runtimeLimits.maxManagedResidentBytes,
    );
    this.#cache = new ContentCache(this.runtimeLimits.maxCacheBytes, this.#admission);
    this.cowPageBytes = options.port.initialize({
      ...(options.format?.cowPageBytes === undefined
        ? {}
        : { cowPageBytes: options.format.cowPageBytes }),
      now: this.#now(),
      maxManifestEntries: this.storageLimits.maxManifestEntries,
      maxManifestDepth: this.storageLimits.maxManifestDepth,
      maxFileBytes: this.storageLimits.maxFileBytes,
      maxContentObjectBytes: maxPersistedContentObjectBytes(this.storageLimits),
      writerProfile: persistedWriterProfile(
        this.filesystemLimits,
        this.storageLimits,
        DEFAULT_BRANCH_CONFIGURATION,
      ),
    }).cowPageBytes;
    validateRuntimeLimits(
      this.filesystemLimits,
      this.storageLimits,
      this.runtimeLimits,
      this.cowPageBytes,
    );
  }
  canonicalPathSync(path: string, syscall = "nodeVfs"): string {
    return canonicalizePath(path, this.filesystemLimits, syscall).value;
  }
  resolvePathSync(path: string, followFinal = true): NodeVfsResolvedPath {
    const canonical = canonicalizePath(path, this.filesystemLimits, "resolvePathSync");
    return this.#read(
      (tx) => {
        const selected = tx
          .namespace(this.filesystemLimits, this.storageLimits, "resolvePathSync")
          .resolve(canonical, followFinal);
        return Object.freeze({
          canonicalPath: canonical.value,
          stat: stat(selected.inode, canonical.segments.at(-1) ?? ""),
        });
      },
      "resolvePathSync",
      canonical.value,
    );
  }
  openPinnedReadSync(path: string): NodeVfsPinnedReadBridge {
    const canonical = canonicalizePath(path, this.filesystemLimits, "openFileSync");
    const leaseId = globalThis.crypto.randomUUID();
    const ownerId = globalThis.crypto.randomUUID();
    const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
    let expiresAt = 0;
    const selected = this.#write(
      (tx) => {
        const inode = tx
          .namespace(this.filesystemLimits, this.storageLimits, "openFileSync")
          .resolve(canonical, true).inode;
        if (inode.type !== 0 || !inode.manifest_hash)
          throw fsError(
            inode.type === 1 ? "EISDIR" : "EINVAL",
            "openFileSync",
            canonical.value,
            "path is not a regular file",
          );
        const manifestHash = copyBytes(inode.manifest_hash);
        expiresAt = this.#now() + this.storageLimits.readLeaseMs;
        tx.staging(this.storageLimits).acquireReadLease(
          leaseId,
          ownerId,
          ownerNonce,
          manifestHash,
          expiresAt,
        );
        return Object.freeze({
          inodeId: inode.id,
          manifestHash,
          size: inode.size!,
          stat: stat(inode, canonical.segments.at(-1) ?? ""),
        });
      },
      "openFileSync",
      canonical.value,
    );
    return this.#makePinnedRead(canonical.value, {
      ...selected,
      leaseId,
      ownerId,
      ownerNonce,
      expiresAt,
    });
  }
  acquireSlabSync(
    source: Uint8Array,
    sourceOffset: number,
    length: number,
  ): NodeVfsManagedSlab | undefined {
    source = intrinsicByteRange(source);
    if (
      !Number.isSafeInteger(sourceOffset) ||
      sourceOffset < 0 ||
      !Number.isSafeInteger(length) ||
      length < 0 ||
      sourceOffset + length > source.byteLength
    )
      throw new RangeError("invalid Node VFS slab source range");
    this.#cache.makeRoom(length);
    let release: (() => void) | undefined;
    try {
      release = this.#admission.reserve(length);
    } catch (error) {
      if (error instanceof RangeError) return undefined;
      throw error;
    }
    let bytes: Uint8Array;
    try {
      bytes = copyBytes(source, sourceOffset, sourceOffset + length);
    } catch (error) {
      release();
      throw error;
    }
    let active = true;
    return Object.freeze({
      bytes,
      release: () => {
        if (!active) return;
        active = false;
        release!();
      },
    });
  }
  reserveControlSync(bytes: number): (() => void) | undefined {
    if (!Number.isSafeInteger(bytes) || bytes < 0)
      throw new RangeError("invalid Node VFS control reservation");
    this.#cache.makeRoom(bytes);
    try {
      return this.#admission.reserve(bytes);
    } catch (error) {
      if (error instanceof RangeError) return undefined;
      throw error;
    }
  }
  managedMemorySync(): NodeVfsManagedMemorySnapshot {
    return Object.freeze({
      usedBytes: this.#admission.usedBytes,
      peakBytes: this.#admission.peakBytes,
      limitBytes: this.#admission.limitBytes,
    });
  }
  existsSync(path: string): boolean {
    try {
      this.statSync(path);
      return true;
    } catch (error) {
      if (error instanceof Error && "code" in error && error.code === "ENOENT")
        return false;
      throw error;
    }
  }
  statSync(path: string, followFinal = true): FileStat {
    const canonical = canonicalizePath(path, this.filesystemLimits, "statSync");
    return this.#read((tx) => {
      const value = tx
        .namespace(this.filesystemLimits, this.storageLimits, "statSync")
        .resolve(canonical, followFinal);
      return stat(value.inode, canonical.segments.at(-1) ?? "");
    });
  }
  readdirSync(path: string): DirectoryEntry[] {
    const canonical = canonicalizePath(path, this.filesystemLimits, "readdirSync");
    return this.#read((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "readdirSync");
      const selected = ns.resolve(canonical, true);
      if (selected.inode.type !== 1)
        throw fsError(
          "ENOTDIR",
          "readdirSync",
          canonical.value,
          "path is not a directory",
        );
      const rows = ns.children(
        selected.inode.id,
        this.filesystemLimits.maxReaddirEntries,
        this.runtimeLimits.maxQueryBatchBytes,
      );
      return rows.map((row) => {
        const type = kind(row.type);
        return Object.freeze({
          name: row.name,
          parentPath: canonical.value,
          type,
          isFile: () => type === "file",
          isDirectory: () => type === "directory",
          isSymbolicLink: () => type === "symlink",
        });
      });
    });
  }
  readlinkSync(path: string): string {
    const canonical = canonicalizePath(path, this.filesystemLimits, "readlinkSync");
    return this.#read((tx) => {
      const value = tx
        .namespace(this.filesystemLimits, this.storageLimits, "readlinkSync")
        .resolve(canonical, false);
      if (value.inode.type !== 2 || value.inode.symlink_target === null)
        throw fsError(
          "EINVAL",
          "readlinkSync",
          canonical.value,
          "path is not a symbolic link",
        );
      return value.inode.symlink_target;
    });
  }
  readIntoSync(
    path: string,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    const canonical = canonicalizePath(path, this.filesystemLimits, "readIntoSync");
    destination = intrinsicByteRange(destination);
    this.#validateReadRange(
      destination,
      destinationOffset,
      position,
      length,
      "readIntoSync",
      canonical.value,
    );
    return this.#read(
      (tx) => {
        const inode = tx
          .namespace(this.filesystemLimits, this.storageLimits, "readIntoSync")
          .resolve(canonical, true).inode;
        if (inode.type !== 0 || !inode.manifest_hash)
          throw fsError(
            inode.type === 1 ? "EISDIR" : "EINVAL",
            "readIntoSync",
            canonical.value,
            "path is not a file",
          );
        return readManifestInto(
          tx.content(this.storageLimits, this.#cache),
          inode.manifest_hash,
          position,
          destination,
          destinationOffset,
          length,
        );
      },
      "readIntoSync",
      canonical.value,
    );
  }
  readRangeSync(path: string, position: number, length: number): Uint8Array {
    this.#validateMaterializedRange(position, length, "readRangeSync", path);
    const output = new Uint8Array(length);
    const read = this.readIntoSync(path, output, 0, position, length);
    return read === length ? output : output.slice(0, read);
  }
  readFileSync(path: string): Uint8Array {
    const size = this.statSync(path).size;
    if (size > this.filesystemLimits.maxMaterializedBytes)
      throw fsError(
        "EFBIG",
        "readFileSync",
        path,
        "file exceeds synchronous materialization limit",
      );
    return this.readRangeSync(path, 0, size);
  }
  prepareContentSync(bytes: Uint8Array): NodeVfsPreparedContent {
    bytes = intrinsicByteRange(bytes);
    return this.prepareContentSourceSync({
      size: intrinsicByteLength(bytes),
      readInto: (destination, destinationOffset, position, length) => {
        destination.set(
          intrinsicByteRange(bytes, position, position + length),
          destinationOffset,
        );
        return length;
      },
    });
  }
  prepareContentSourceSync(source: SynchronousContentSource): NodeVfsPreparedContent {
    try {
      const prepared = prepareContentSourceSync(
        this.#port,
        source,
        this.storageLimits,
        this.runtimeLimits,
        this.#admission,
        this.#cache,
        this.#clock,
      );
      return this.#wrapPrepared(
        Object.freeze({
          manifestHash: prepared.hash,
          size: prepared.size,
          certificate: prepared.certificate,
        }),
      );
    } catch (error) {
      if (
        error instanceof RangeError &&
        /managed resident|memory limit|admit synchronous/i.test(error.message)
      )
        throw fsError(
          "EAGAIN",
          "stagePrefixSync",
          undefined,
          "aggregate managed-memory pressure could not be relieved",
          error,
        );
      mapStorageError(error, "stagePrefixSync");
    }
  }
  prepareOverwriteSync(
    path: string,
    offset: number,
    source: SynchronousContentSource,
  ): NodeVfsPreparedContent | undefined {
    if (!this.#prepareOverwriteSync) return undefined;
    const prepared = this.#prepareOverwriteSync(path, offset, source);
    return prepared ? this.#wrapPrepared(prepared) : undefined;
  }
  prepareOverwritesSync(
    path: string,
    edits: readonly NodeVfsOverwriteEdit[],
  ): NodeVfsPreparedContent | undefined {
    if (!this.#prepareOverwritesSync) return undefined;
    const prepared = this.#prepareOverwritesSync(path, edits);
    return prepared ? this.#wrapPrepared(prepared) : undefined;
  }
  abortPreparedSync(handle: NodeVfsPreparedContent): void {
    const prepared = this.#requirePrepared(handle);
    this.#write((tx) => {
      tx.staging(this.storageLimits, this.#cache).release(
        prepared.certificate.leaseId,
        prepared.certificate.ownerNonce,
        false,
      );
    }, "abortSync");
    this.#prepared.delete(handle);
  }
  readPreparedIntoSync(
    handle: NodeVfsPreparedContent,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    const prepared = this.#requirePrepared(handle);
    destination = intrinsicByteRange(destination);
    this.#validateReadRange(
      destination,
      destinationOffset,
      position,
      length,
      "readIntoSync",
    );
    return this.#read(
      (tx) =>
        readManifestInto(
          tx.content(this.storageLimits, this.#cache),
          prepared.manifestHash,
          position,
          destination,
          destinationOffset,
          length,
        ),
      "readIntoSync",
    );
  }
  commitPreparedSync(
    path: string,
    handle: NodeVfsPreparedContent,
    options: {
      create?: boolean;
      exclusive?: boolean;
      mode?: number;
      inodeId?: string;
      aliases?: readonly string[];
    } = {},
  ): NodeVfsCommitResult {
    const prepared = this.#requirePrepared(handle);
    const canonical = canonicalizePath(
      path,
      this.filesystemLimits,
      "commitVisibleSync",
    );
    if (canonical.value === "/")
      throw fsError(
        "EISDIR",
        "commitVisibleSync",
        canonical.value,
        "root is a directory",
      );
    const aliases = (options.aliases ?? []).map((alias) =>
      canonicalizePath(alias, this.filesystemLimits, "commitVisibleSync"),
    );
    const mode = validatedMode(options.mode, 0o644);
    const leaseId = globalThis.crypto.randomUUID();
    const ownerId = globalThis.crypto.randomUUID();
    const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
    let selected;
    try {
      selected = this.#write(
        (tx) => {
          const ns = tx.namespace(
            this.filesystemLimits,
            this.storageLimits,
            "commitVisibleSync",
          );
          const existing = ns.resolveOptional(canonical, true);
          const alreadyCommitted =
            existing?.inode.type === 0 &&
            existing.inode.id === options.inodeId &&
            existing.inode.size === prepared.size &&
            existing.inode.manifest_hash !== null &&
            equalBytes(existing.inode.manifest_hash, prepared.manifestHash);
          if (!alreadyCommitted)
            tx.staging(this.storageLimits, this.#cache).validateSealed(
              prepared.certificate,
              this.#now(),
            );
          if (
            options.inodeId !== undefined &&
            existing?.inode.id !== options.inodeId &&
            !(options.create && !existing)
          )
            throw fsError(
              "EBUSY",
              "commitVisibleSync",
              canonical.value,
              "open inode identity no longer matches the commit path",
            );
          if (options.exclusive && existing && !alreadyCommitted)
            throw fsError(
              "EEXIST",
              "commitVisibleSync",
              canonical.value,
              "destination exists",
            );
          if (!existing && options.create === false)
            throw fsError(
              "ENOENT",
              "commitVisibleSync",
              canonical.value,
              "file does not exist",
            );
          if (existing?.inode.type === 1)
            throw fsError(
              "EISDIR",
              "commitVisibleSync",
              canonical.value,
              "destination is a directory",
            );
          const now = this.#now();
          let revision: number | undefined;
          let committedInodeId: string;
          if (alreadyCommitted) {
            committedInodeId = existing!.inode.id;
          } else if (existing) {
            revision = ns.nextRevision(now, 1, "node-vfs");
            committedInodeId = existing.inode.id;
            if (
              ns.setFileContent(
                existing.inode.id,
                prepared.size,
                prepared.manifestHash,
                now,
                now,
                revision,
                prepared.expectedToken,
              ) !== 1
            )
              throw fsError(
                "EAGAIN",
                "commitVisibleSync",
                canonical.value,
                "file changed while content was prepared",
              );
            ns.recordInode(revision, existing.inode.id);
          } else {
            const parent = ns.resolveParent(canonical);
            revision = ns.nextRevision(now, 3 + aliases.length * 3, "node-vfs");
            const id = options.inodeId ?? globalThis.crypto.randomUUID();
            committedInodeId = id;
            ns.createInode({
              id,
              type: 0,
              mode,
              now,
              revision: revision!,
              size: prepared.size,
              manifestHash: prepared.manifestHash,
            });
            ns.putEntry(
              parent.parent.inode.id,
              parent.nameSort,
              parent.name,
              id,
              revision!,
            );
            ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
            ns.recordInode(revision, id);
            this.#touch(tx, ns, parent.parent.inode, now, revision);
          }
          for (const alias of alreadyCommitted ? [] : aliases) {
            if (alias.value === canonical.value) continue;
            if (ns.resolveOptional(alias, false))
              throw fsError(
                "EEXIST",
                "commitVisibleSync",
                alias.value,
                "pending hard-link alias already exists",
              );
            const parent = ns.resolveParent(alias);
            ns.putEntry(
              parent.parent.inode.id,
              parent.nameSort,
              parent.name,
              committedInodeId,
              revision!,
            );
            ns.incrementLinks(committedInodeId, now, revision!);
            ns.recordEntry(revision!, parent.parent.inode.id, parent.nameSort);
            ns.recordInode(revision!, committedInodeId);
            this.#touch(tx, ns, parent.parent.inode, now, revision!);
          }
          tx.staging(this.storageLimits, this.#cache).release(
            prepared.certificate.leaseId,
            prepared.certificate.ownerNonce,
            true,
          );
          const committed = ns.inode(committedInodeId);
          if (!committed?.manifest_hash)
            throw new Error("ECORRUPT: committed Node VFS inode is missing content");
          const expiresAt = now + this.storageLimits.readLeaseMs;
          tx.staging(this.storageLimits).acquireReadLease(
            leaseId,
            ownerId,
            ownerNonce,
            committed.manifest_hash,
            expiresAt,
          );
          return Object.freeze({
            inodeId: committed.id,
            manifestHash: copyBytes(committed.manifest_hash),
            size: committed.size!,
            stat: stat(committed, canonical.segments.at(-1) ?? ""),
            leaseId,
            ownerId,
            ownerNonce,
            expiresAt,
          });
        },
        "commitVisibleSync",
        canonical.value,
      );
    } catch (error) {
      const visible = this.#read(
        (tx) => {
          const inode = tx
            .namespace(this.filesystemLimits, this.storageLimits, "commitVisibleSync")
            .resolveOptional(canonical, true)?.inode;
          return Boolean(
            inode?.type === 0 &&
            (options.inodeId === undefined || inode.id === options.inodeId) &&
            inode.size === prepared.size &&
            inode.manifest_hash !== null &&
            equalBytes(inode.manifest_hash, prepared.manifestHash),
          );
        },
        "commitVisibleSync",
        canonical.value,
      );
      if (!visible) throw error;
      // Resolve an ambiguous adapter outcome through the same inode/content
      // identity and acquire the replacement pin before reporting success.
      return this.commitPreparedSync(path, handle, options);
    }
    this.#prepared.delete(handle);
    return Object.freeze({ pinned: this.#makePinnedRead(canonical.value, selected) });
  }
  writeFileSync(
    path: string,
    bytes: Uint8Array,
    options?: { create?: boolean; exclusive?: boolean; mode?: number },
  ): void {
    this.commitPreparedSync(path, this.prepareContentSync(bytes), options);
  }
  mkdirSync(path: string, options: { recursive?: boolean; mode?: number } = {}): void {
    const mode = validatedMode(options.mode, 0o755);
    const canonical = canonicalizePath(path, this.filesystemLimits, "mkdirSync");
    if (canonical.value === "/") {
      if (options.recursive) return;
      throw fsError("EEXIST", "mkdirSync", canonical.value, "root exists");
    }
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "mkdirSync");
      if (ns.resolveOptional(canonical, false)) {
        if (options.recursive) return;
        throw fsError("EEXIST", "mkdirSync", canonical.value, "destination exists");
      }
      const prefixes = options.recursive
        ? canonical.segments.map(
            (_, index) => `/${canonical.segments.slice(0, index + 1).join("/")}`,
          )
        : [canonical.value];
      const now = this.#now();
      let revision: number | undefined;
      for (const prefix of prefixes) {
        if (ns.resolveOptional(prefix, false)) continue;
        const current = canonicalizePath(prefix, this.filesystemLimits, "mkdirSync");
        const parent = ns.resolveParent(current);
        revision ??= ns.nextRevision(now, prefixes.length * 3, "node-vfs");
        const id = globalThis.crypto.randomUUID();
        ns.createInode({
          id,
          type: 1,
          mode,
          now,
          revision,
        });
        ns.putEntry(parent.parent.inode.id, parent.nameSort, parent.name, id, revision);
        ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
        ns.recordInode(revision, id);
        this.#touch(tx, ns, parent.parent.inode, now, revision);
      }
    });
  }
  chmodSync(path: string, mode: number): void {
    mode = validatedMode(mode, 0);
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "chmodSync");
      const value = ns.resolve(path, true);
      const now = this.#now();
      const revision = ns.nextRevision(now, 1, "node-vfs");
      ns.setMode(value.inode.id, mode & 0o7777, now, revision);
      ns.recordInode(revision, value.inode.id);
    });
  }
  linkSync(existingPath: string, newPath: string): void {
    const checkedDestination = canonicalizePath(
      newPath,
      this.filesystemLimits,
      "linkSync",
    );
    if (checkedDestination.value === "/")
      throw fsError("EPERM", "linkSync", "/", "root cannot be replaced");
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "linkSync");
      const source = ns.resolve(existingPath, true);
      if (source.inode.type !== 0)
        throw fsError(
          "EPERM",
          "linkSync",
          existingPath,
          "only files can be hard linked",
        );
      const destination = checkedDestination;
      if (ns.resolveOptional(destination, false))
        throw fsError("EEXIST", "linkSync", destination.value, "destination exists");
      const parent = ns.resolveParent(destination);
      const now = this.#now();
      const revision = ns.nextRevision(now, 3, "node-vfs");
      ns.putEntry(
        parent.parent.inode.id,
        parent.nameSort,
        parent.name,
        source.inode.id,
        revision,
      );
      ns.incrementLinks(source.inode.id, now, revision);
      ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
      ns.recordInode(revision, source.inode.id);
      this.#touch(tx, ns, parent.parent.inode, now, revision);
    });
  }
  symlinkSync(target: string, path: string): void {
    validateSymlinkTarget(target, this.filesystemLimits, "symlinkSync");
    const checkedDestination = canonicalizePath(
      path,
      this.filesystemLimits,
      "symlinkSync",
    );
    if (checkedDestination.value === "/")
      throw fsError("EPERM", "symlinkSync", "/", "root cannot be replaced");
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "symlinkSync");
      const destination = checkedDestination;
      if (ns.resolveOptional(destination, false))
        throw fsError("EEXIST", "symlinkSync", destination.value, "destination exists");
      const parent = ns.resolveParent(destination);
      const now = this.#now();
      const revision = ns.nextRevision(now, 3, "node-vfs");
      const id = globalThis.crypto.randomUUID();
      ns.createInode({
        id,
        type: 2,
        mode: 0o777,
        now,
        revision,
        symlinkTarget: target,
      });
      ns.putEntry(parent.parent.inode.id, parent.nameSort, parent.name, id, revision);
      ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
      ns.recordInode(revision, id);
      this.#touch(tx, ns, parent.parent.inode, now, revision);
    });
  }
  renameSync(oldPath: string, newPath: string): void {
    const sourcePath = canonicalizePath(oldPath, this.filesystemLimits, "renameSync");
    const destination = canonicalizePath(newPath, this.filesystemLimits, "renameSync");
    if (sourcePath.value === "/" || destination.value === "/")
      throw fsError(
        "EPERM",
        "renameSync",
        sourcePath.value,
        "root cannot be renamed or replaced",
      );
    if (sourcePath.value === destination.value) return;
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "renameSync");
      const source = ns.resolve(sourcePath, false);
      const target = ns.resolveOptional(destination, false);
      const parent = ns.resolveParent(destination);
      if (source.inode.type === 1) {
        for (let index = 1; index < destination.segments.length; index += 1) {
          const prefix = `/${destination.segments.slice(0, index).join("/")}`;
          if (ns.resolve(prefix, true).inode.id === source.inode.id)
            throw fsError(
              "EINVAL",
              "renameSync",
              sourcePath.value,
              "directory cannot be moved into itself",
            );
        }
      }
      if (target) {
        if (source.inode.type === 1 && target.inode.type !== 1)
          throw fsError(
            "ENOTDIR",
            "renameSync",
            destination.value,
            "cannot replace non-directory with directory",
          );
        if (source.inode.type !== 1 && target.inode.type === 1)
          throw fsError(
            "EISDIR",
            "renameSync",
            destination.value,
            "cannot replace directory with non-directory",
          );
        if (target.inode.type === 1 && ns.childCount(target.inode.id) > 0)
          throw fsError(
            "ENOTEMPTY",
            "renameSync",
            destination.value,
            "destination directory is not empty",
          );
      }
      const now = this.#now();
      const revision = ns.nextRevision(now, 7, "node-vfs");
      ns.putEntry(source.parentInode!, source.nameSort!, null, null, revision);
      ns.recordEntry(revision, source.parentInode!, source.nameSort!, true);
      if (target?.inode.id === source.inode.id) {
        ns.decrementLinks(source.inode.id, now, revision);
        ns.recordInode(revision, source.inode.id);
      } else {
        if (target) {
          ns.putEntry(target.parentInode!, target.nameSort!, null, null, revision);
          ns.recordEntry(revision, target.parentInode!, target.nameSort!, true);
          if (target.inode.type !== 1 && target.inode.nlink > 1) {
            ns.decrementLinks(target.inode.id, now, revision);
            ns.recordInode(revision, target.inode.id);
          } else {
            ns.deleteInode(target.inode.id);
            ns.recordInode(revision, target.inode.id, true);
          }
        }
        ns.putEntry(
          parent.parent.inode.id,
          parent.nameSort,
          parent.name,
          source.inode.id,
          revision,
        );
        ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
      }
      const sourceParent = ns.inode(source.parentInode!);
      if (sourceParent) this.#touch(tx, ns, sourceParent, now, revision);
      if (parent.parent.inode.id !== source.parentInode)
        this.#touch(tx, ns, parent.parent.inode, now, revision);
    });
  }
  unlinkSync(path: string): void {
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "unlinkSync");
      const value = ns.resolve(path, false);
      if (value.inode.type === 1)
        throw fsError("EISDIR", "unlinkSync", path, "path is a directory");
      this.#unlink(tx, ns, value.path, false, "unlinkSync");
    });
  }
  rmdirSync(path: string): void {
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "rmdirSync");
      const value = ns.resolve(path, false);
      if (value.inode.type !== 1)
        throw fsError("ENOTDIR", "rmdirSync", path, "path is not a directory");
      if (ns.childCount(value.inode.id))
        throw fsError("ENOTEMPTY", "rmdirSync", path, "directory is not empty");
      this.#unlink(tx, ns, value.path, true, "rmdirSync");
    });
  }
  #unlink(
    _tx: StorageTransactionPorts,
    ns: NamespaceStore,
    path: CanonicalPath,
    directory: boolean,
    syscall: string,
  ): void {
    const value = ns.resolve(path, false);
    const now = this.#now();
    const revision = ns.nextRevision(now, 3, "node-vfs");
    ns.putEntry(value.parentInode!, value.nameSort!, null, null, revision);
    ns.recordEntry(revision, value.parentInode!, value.nameSort!, true);
    if (!directory && value.inode.nlink > 1) {
      ns.decrementLinks(value.inode.id, now, revision);
      ns.recordInode(revision, value.inode.id);
    } else {
      if (directory) ns.deleteEntriesUnder(value.inode.id);
      ns.deleteInode(value.inode.id);
      ns.recordInode(revision, value.inode.id, true);
    }
  }
  #touch(
    _tx: StorageTransactionPorts,
    ns: NamespaceStore,
    inode: InodeRow,
    now: number,
    revision: number,
  ): void {
    ns.touch(inode.id, now, now, revision);
    ns.recordInode(revision, inode.id);
  }
  #wrapPrepared(prepared: SyncPreparedContent): NodeVfsPreparedContent {
    const handle = Object.freeze({
      size: prepared.size,
      ...(prepared.sourceBytesRead === undefined
        ? {}
        : { editSourceBytes: prepared.sourceBytesRead }),
    });
    this.#prepared.set(handle, prepared);
    return handle;
  }
  #requirePrepared(handle: NodeVfsPreparedContent): SyncPreparedContent {
    const prepared = this.#prepared.get(handle);
    if (!prepared)
      throw fsError(
        "EINVAL",
        "nodeVfs",
        undefined,
        "unknown or consumed prepared content",
      );
    return prepared;
  }
  #makePinnedRead(
    canonicalPath: string,
    selected: {
      readonly inodeId: string;
      readonly manifestHash: Uint8Array;
      readonly size: number;
      readonly stat: FileStat;
      readonly leaseId: string;
      readonly ownerId: string;
      readonly ownerNonce: Uint8Array;
      readonly expiresAt: number;
    },
  ): NodeVfsPinnedReadBridge {
    let expiresAt = selected.expiresAt;
    let cursor: AuthenticatedManifestCursor | undefined;
    let closed = false;
    const renewIfNeeded = (): void => {
      const now = this.#now();
      if (now + Math.floor(this.storageLimits.readLeaseMs / 3) < expiresAt) return;
      const next = Math.max(now, expiresAt) + this.storageLimits.readLeaseMs;
      const renewed = this.#write(
        (tx) =>
          tx
            .staging(this.storageLimits)
            .renewReadLease(
              selected.leaseId,
              selected.ownerId,
              selected.ownerNonce,
              expiresAt,
              now,
              next,
            ),
        "readIntoSync",
        canonicalPath,
      );
      if (!renewed)
        throw fsError(
          "EBUSY",
          "readIntoSync",
          canonicalPath,
          "pinned read lease expired or changed owner",
        );
      expiresAt = next;
    };
    return Object.freeze({
      canonicalPath,
      inodeId: selected.inodeId,
      stat: selected.stat,
      size: selected.size,
      readIntoSync: (
        destination: Uint8Array,
        destinationOffset: number,
        position: number,
        length: number,
      ): number => {
        if (closed)
          throw fsError(
            "EBADF",
            "readIntoSync",
            canonicalPath,
            "pinned read session is closed",
          );
        destination = intrinsicByteRange(destination);
        this.#validateReadRange(
          destination,
          destinationOffset,
          position,
          length,
          "readIntoSync",
          canonicalPath,
        );
        renewIfNeeded();
        return this.#read(
          (tx) => {
            const content = tx.content(this.storageLimits, this.#cache);
            if (!cursor || cursor.position !== Math.min(position, selected.size)) {
              cursor?.close();
              cursor = content.openManifestCursor(selected.manifestHash, position);
            } else cursor.bindSource(content);
            return cursor.readInto(destination, destinationOffset, length);
          },
          "readIntoSync",
          canonicalPath,
        );
      },
      closeSync: (): void => {
        if (closed) return;
        cursor?.close();
        cursor = undefined;
        this.#write(
          (tx) => {
            tx.staging(this.storageLimits).releaseReadLease(
              selected.leaseId,
              selected.ownerId,
              selected.ownerNonce,
            );
          },
          "closeSync",
          canonicalPath,
        );
        closed = true;
      },
    });
  }
  #read<T>(
    callback: (tx: StorageTransactionPorts) => T,
    syscall = "nodeVfsRead",
    path?: string,
  ): T {
    try {
      return this.#port.transaction(
        "read",
        {
          maxRows: this.storageLimits.maxFinalTransactionRows,
          maxBytes: this.storageLimits.maxFinalTransactionBytes,
        },
        callback,
      );
    } catch (error) {
      mapStorageError(error, syscall, path);
    }
  }
  #write<T>(
    callback: (tx: StorageTransactionPorts) => T,
    syscall = "nodeVfsWrite",
    path?: string,
  ): T {
    try {
      return this.#port.transaction(
        "write",
        {
          maxRows: this.storageLimits.maxFinalTransactionRows,
          maxBytes: this.storageLimits.maxFinalTransactionBytes,
        },
        callback,
      );
    } catch (error) {
      mapStorageError(error, syscall, path);
    }
  }
  #validateMaterializedRange(
    position: number,
    length: number,
    syscall: string,
    path?: string,
  ): void {
    if (
      !Number.isSafeInteger(position) ||
      position < 0 ||
      !Number.isSafeInteger(length) ||
      length < 0
    )
      throw fsError("EINVAL", syscall, path, "invalid read position or length");
    if (length > this.filesystemLimits.maxMaterializedBytes)
      throw fsError("EFBIG", syscall, path, "read exceeds materialization limit");
  }
  #validateReadRange(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
    syscall: string,
    path?: string,
  ): void {
    this.#validateMaterializedRange(position, length, syscall, path);
    if (
      !Number.isSafeInteger(destinationOffset) ||
      destinationOffset < 0 ||
      destinationOffset + length > destination.byteLength
    )
      throw fsError("EINVAL", syscall, path, "invalid destination range");
  }
  #now(): number {
    const now = this.#clock();
    if (!Number.isSafeInteger(now) || now < 0) throw new Error("invalid clock");
    return now;
  }
}
export function createNodeVfsOperationsBridge(
  options: NodeVfsOperationsBridgeOptions,
): NodeVfsFilesystemBridge {
  return new Bridge(options);
}
export type { SynchronousContentSource } from "./streaming-prepare.js";

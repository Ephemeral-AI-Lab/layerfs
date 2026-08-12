import { buildManifest } from "./full-rebuild.js";
import { DEFAULT_FASTCDC } from "../cdc/fastcdc.js";
import {
  DEFAULT_BRANCH_CONFIGURATION,
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  AdmissionController,
  DURABLE_METADATA_ROW_BYTES,
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
import { canonicalizePath, type CanonicalPath } from "../namespace/paths.js";
import { readManifestInto, readManifestRange } from "../operations/manifest-io.js";
import type {
  DirectoryEntry,
  FileStat,
  StorageFormatOptions,
} from "../filesystem/types.js";
import { fsError } from "../filesystem/errors.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import { intrinsicByteLength } from "../cas/bytes.js";
import {
  ingestReservationBytes,
  metadataReservationBytes,
} from "./streaming-prepare.js";
import type {
  ClosureCertificate,
  InodeRow,
  NamespaceStore,
  OperationsStorage,
  StorageTransactionPorts,
} from "./storage-ports.js";

export interface SyncPreparedContent {
  readonly manifestHash: Uint8Array;
  readonly size: number;
  readonly certificate: ClosureCertificate;
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
  readIntoSync(
    path: string,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  readRangeSync(path: string, position: number, length: number): Uint8Array;
  readFileSync(path: string): Uint8Array;
  prepareContentSync(bytes: Uint8Array): SyncPreparedContent;
  readPreparedIntoSync(
    prepared: SyncPreparedContent,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  commitPreparedSync(
    path: string,
    prepared: SyncPreparedContent,
    options?: { create?: boolean; exclusive?: boolean; mode?: number },
  ): void;
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

class Bridge implements NodeVfsFilesystemBridge {
  readonly filesystemLimits: Readonly<FilesystemLimits>;
  readonly storageLimits: Readonly<StorageLimits>;
  readonly runtimeLimits: Readonly<RuntimeLimits>;
  readonly cowPageBytes: 4096 | 8192 | 16384;
  readonly #port: OperationsStorage;
  readonly #clock: () => number;
  readonly #admission: AdmissionController;
  readonly #cache: ContentCache;
  constructor(options: NodeVfsOperationsBridgeOptions) {
    this.#port = options.port;
    this.#clock = options.clock ?? Date.now;
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
    return this.#read((tx) => {
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
    });
  }
  readRangeSync(path: string, position: number, length: number): Uint8Array {
    const output = new Uint8Array(length);
    const read = this.readIntoSync(path, output, 0, position, length);
    return read === length ? output : output.slice(0, read);
  }
  readFileSync(path: string): Uint8Array {
    const size = this.statSync(path).size;
    if (size > this.runtimeLimits.maxManagedResidentBytes)
      throw fsError(
        "EFBIG",
        "readFileSync",
        path,
        "file exceeds synchronous materialization limit",
      );
    return this.readRangeSync(path, 0, size);
  }
  prepareContentSync(bytes: Uint8Array): SyncPreparedContent {
    const manifest = buildManifest(bytes, DEFAULT_FASTCDC);
    const leaseId = globalThis.crypto.randomUUID();
    const ownerId = globalThis.crypto.randomUUID();
    const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
    const now = this.#now();
    let begun = false;
    try {
      this.#write((tx) => {
        const staging = tx.staging(this.storageLimits, this.#cache);
        staging.begin({
          leaseId,
          ownerId,
          ownerNonce,
          now,
          expiresAt: now + this.storageLimits.stagingLeaseMs,
          ingestReservationBytes: ingestReservationBytes(
            intrinsicByteLength(bytes),
            this.storageLimits,
          ),
          metadataReservationBytes: metadataReservationBytes(
            intrinsicByteLength(bytes),
            this.storageLimits,
          ),
        });
        staging.bumpRoot(5, leaseId, false);
      });
      begun = true;
      for (const [hash, object] of manifest.objects) {
        const objectHash = BufferlessHex(hash);
        const objectBytes = intrinsicByteLength(object);
        this.#write((tx) => {
          const staging = tx.staging(this.storageLimits, this.#cache);
          staging.consumeIngestReservation(leaseId, ownerNonce, objectBytes);
          staging.consumeMetadataReservation(
            leaseId,
            ownerNonce,
            DURABLE_METADATA_ROW_BYTES,
          );
          tx.content(this.storageLimits, this.#cache).putObject(objectHash, object);
          staging.appendBatch(leaseId, ownerNonce, [
            { kind: "object", hash: objectHash, size: objectBytes },
          ]);
        });
      }
      for (const node of manifest.nodes.values()) {
        const nodeBytes = intrinsicByteLength(node.encoded);
        this.#write((tx) => {
          const staging = tx.staging(this.storageLimits, this.#cache);
          staging.consumeIngestReservation(leaseId, ownerNonce, nodeBytes);
          staging.consumeMetadataReservation(
            leaseId,
            ownerNonce,
            DURABLE_METADATA_ROW_BYTES,
          );
          tx.content(this.storageLimits, this.#cache).putManifestNode(
            node.hash,
            node.encoded,
          );
          staging.appendBatch(leaseId, ownerNonce, [
            { kind: "manifest-node", hash: node.hash, size: nodeBytes },
          ]);
        });
      }
      const rootBytes = intrinsicByteLength(manifest.root);
      const certificate = this.#write((tx) => {
        const staging = tx.staging(this.storageLimits, this.#cache);
        staging.consumeIngestReservation(leaseId, ownerNonce, rootBytes);
        staging.consumeMetadataReservation(
          leaseId,
          ownerNonce,
          DURABLE_METADATA_ROW_BYTES,
        );
        tx.content(this.storageLimits, this.#cache).putManifestRoot(
          manifest.rootHash,
          manifest.root,
        );
        staging.appendBatch(leaseId, ownerNonce, [
          {
            kind: "manifest-root",
            hash: manifest.rootHash,
            size: rootBytes,
          },
        ]);
        staging.beginReconciliation(leaseId, ownerNonce, manifest.rootHash);
        return Object.freeze({
          ...staging.snapshot(leaseId, ownerNonce),
          manifestHash: manifest.rootHash,
        });
      });
      let complete = false;
      while (!complete)
        complete = this.#write(
          (tx) =>
            tx
              .staging(this.storageLimits, this.#cache)
              .reconcileBatch(
                leaseId,
                ownerNonce,
                Math.max(
                  1,
                  Math.min(
                    this.storageLimits.maxQueryBatchSize,
                    Math.floor((this.storageLimits.maxFinalTransactionRows - 8) / 4),
                    Math.floor(
                      (this.storageLimits.maxFinalTransactionRows * 4 - 16) /
                        (this.storageLimits.maxManifestDepth * 2 + 12),
                    ),
                  ),
                ),
              ).complete,
        );
      this.#write((tx) =>
        tx.staging(this.storageLimits, this.#cache).seal(certificate),
      );
      return Object.freeze({
        manifestHash: manifest.rootHash,
        size: intrinsicByteLength(bytes),
        certificate,
      });
    } catch (error) {
      if (begun)
        try {
          this.#write((tx) =>
            tx
              .staging(this.storageLimits, this.#cache)
              .release(leaseId, ownerNonce, false),
          );
        } catch {}
      throw error;
    }
  }
  readPreparedIntoSync(
    prepared: SyncPreparedContent,
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    return this.#read((tx) =>
      readManifestInto(
        tx.content(this.storageLimits, this.#cache),
        prepared.manifestHash,
        position,
        destination,
        destinationOffset,
        length,
      ),
    );
  }
  commitPreparedSync(
    path: string,
    prepared: SyncPreparedContent,
    options: { create?: boolean; exclusive?: boolean; mode?: number } = {},
  ): void {
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
    try {
      this.#write((tx) => {
        tx.staging(this.storageLimits, this.#cache).validateSealed(
          prepared.certificate,
          this.#now(),
        );
        const ns = tx.namespace(
          this.filesystemLimits,
          this.storageLimits,
          "commitVisibleSync",
        );
        const existing = ns.resolveOptional(canonical, true);
        if (options.exclusive && existing)
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
        const revision = ns.nextRevision(now, existing ? 1 : 3, "node-vfs");
        if (existing) {
          ns.setFileContent(
            existing.inode.id,
            prepared.size,
            prepared.manifestHash,
            now,
            now,
            revision,
          );
          ns.recordInode(revision, existing.inode.id);
        } else {
          const parent = ns.resolveParent(canonical);
          const id = globalThis.crypto.randomUUID();
          ns.createInode({
            id,
            type: 0,
            mode: (options.mode ?? 0o666) & 0o7777,
            now,
            revision,
            size: prepared.size,
            manifestHash: prepared.manifestHash,
          });
          ns.putEntry(
            parent.parent.inode.id,
            parent.nameSort,
            parent.name,
            id,
            revision,
          );
          ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
          ns.recordInode(revision, id);
          this.#touch(tx, ns, parent.parent.inode, now, revision);
        }
        tx.staging(this.storageLimits, this.#cache).release(
          prepared.certificate.leaseId,
          prepared.certificate.ownerNonce,
          true,
        );
      });
    } catch (error) {
      try {
        this.#write((tx) =>
          tx
            .staging(this.storageLimits, this.#cache)
            .release(
              prepared.certificate.leaseId,
              prepared.certificate.ownerNonce,
              false,
            ),
        );
      } catch {}
      throw error;
    }
  }
  writeFileSync(
    path: string,
    bytes: Uint8Array,
    options?: { create?: boolean; exclusive?: boolean; mode?: number },
  ): void {
    this.commitPreparedSync(path, this.prepareContentSync(bytes), options);
  }
  mkdirSync(path: string, options: { recursive?: boolean; mode?: number } = {}): void {
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
          mode: (options.mode ?? 0o777) & 0o7777,
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
      const destination = canonicalizePath(newPath, this.filesystemLimits, "linkSync");
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
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "symlinkSync");
      const destination = canonicalizePath(path, this.filesystemLimits, "symlinkSync");
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
    if (sourcePath.value === destination.value) return;
    this.#write((tx) => {
      const ns = tx.namespace(this.filesystemLimits, this.storageLimits, "renameSync");
      const source = ns.resolve(sourcePath, false);
      const target = ns.resolveOptional(destination, false);
      if (target)
        this.#unlink(tx, ns, target.path, target.inode.type === 1, "renameSync");
      const parent = ns.resolveParent(destination);
      const now = this.#now();
      const revision = ns.nextRevision(now, 4, "node-vfs");
      ns.putEntry(source.parentInode!, source.nameSort!, null, null, revision);
      ns.putEntry(
        parent.parent.inode.id,
        parent.nameSort,
        parent.name,
        source.inode.id,
        revision,
      );
      ns.recordEntry(revision, source.parentInode!, source.nameSort!, true);
      ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
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
  #read<T>(callback: (tx: StorageTransactionPorts) => T): T {
    return this.#port.transaction(
      "read",
      {
        maxRows: this.storageLimits.maxFinalTransactionRows,
        maxBytes: this.storageLimits.maxFinalTransactionBytes,
      },
      callback,
    );
  }
  #write<T>(callback: (tx: StorageTransactionPorts) => T): T {
    return this.#port.transaction(
      "write",
      {
        maxRows: this.storageLimits.maxFinalTransactionRows,
        maxBytes: this.storageLimits.maxFinalTransactionBytes,
      },
      callback,
    );
  }
  #now(): number {
    const now = this.#clock();
    if (!Number.isSafeInteger(now) || now < 0) throw new Error("invalid clock");
    return now;
  }
}

function BufferlessHex(value: string): Uint8Array {
  const bytes = new Uint8Array(32);
  for (let index = 0; index < 32; index += 1)
    bytes[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return bytes;
}
export function createNodeVfsOperationsBridge(
  options: NodeVfsOperationsBridgeOptions,
): NodeVfsFilesystemBridge {
  return new Bridge(options);
}

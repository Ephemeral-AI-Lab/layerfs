import {
  DEFAULT_BRANCH_CONFIGURATION,
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  AdmissionController,
  RuntimeConcurrency,
  constrainStorageLimits,
  maxPersistedContentObjectBytes,
  persistedWriterProfile,
  resolveLimits,
  validateRuntimeLimits,
  type BranchConfiguration,
  type FilesystemLimits,
  type RuntimeLimits,
  type StorageLimits,
} from "../resources/limits.js";
import {
  canonicalizePath,
  compareUtf8,
  validateName,
  validateSymlinkTarget,
} from "../namespace/paths.js";
import {
  checkedAdd,
  checkedInteger,
  checkedMultiply,
} from "../resources/safe-integers.js";
import { encodeUtf8, utf8ByteLength } from "../namespace/utf8.js";
import {
  prepareContent,
  readManifestInto,
  readManifestRange,
} from "../operations/manifest-io.js";
import { copyBytes, intrinsicByteLength, intrinsicByteRange } from "../cas/bytes.js";
import {
  decodeManifestRoot,
  validateSupportedManifestParameters,
  type ManifestParameters,
} from "../manifests/codec.js";
import {
  durableEditReadSnapshotBudget,
  tryLoadBoundedManifestStateInTransaction,
  prepareDurableEditedContent,
  type DurableContentEdit,
  type DurableEditReadSnapshot,
  type DurableEditSource,
} from "./durable-edit-prepare.js";

import {
  abortError,
  FilesystemError,
  fsError,
  mapStorageError,
} from "../filesystem/errors.js";
import type {
  DirectoryEntry,
  EphemeralFilesystem,
  FileContent,
  FileStat,
  FileType,
  FilesystemCapabilities,
  FilesystemMaintenance,
  FilesystemObservation,
  FilesystemObserver,
  MkdirOptions,
  OpenFilesystemOptions,
  ReadRangeOptions,
  ReadStreamOptions,
  ReadTextOptions,
  ReaddirOptions,
  RmOptions,
  WriteFileOptions,
} from "../filesystem/types.js";
import { BranchManager } from "./branch-engine.js";
import type { Branches } from "../branches/types.js";
import { MaintenanceManager } from "./maintenance.js";
import { ContentCache } from "../cache/content-cache.js";
import { DEFAULT_LOCAL_REBUILD_LIMITS } from "./local-rebuild.js";
import type {
  AuthenticatedManifestCursor,
  ClosureCertificate,
  ContentStore,
  InodeRow,
  NamespaceStore,
  OperationsStorage,
  ResolvedPath,
  StorageTransactionMode,
  StorageTransactionPorts,
  ValidatedSealedLease,
} from "./storage-ports.js";

function inodeType(value: number): FileType {
  if (value === 0) return "file";
  if (value === 1) return "directory";
  if (value === 2) return "symlink";
  throw new Error("ECORRUPT: invalid inode type");
}
function predicates(type: FileType) {
  return {
    isFile: () => type === "file",
    isDirectory: () => type === "directory",
    isSymbolicLink: () => type === "symlink",
  };
}
function fileStat(inode: InodeRow, name: string): FileStat {
  const type = inodeType(inode.type);
  const size =
    type === "file"
      ? inode.size
      : type === "symlink"
        ? encodeUtf8(inode.symlink_target ?? "").byteLength
        : 0;
  if (typeof size !== "number") throw new Error("ECORRUPT: file size is missing");
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
    ...predicates(type),
  });
}
const editSourceInodes = new WeakMap<
  DurableEditSource,
  {
    readonly inode: InodeRow;
    readonly mainRevision?: number;
    readonly rootMutationGeneration?: number;
  }
>();

interface MutationSourceSelection {
  readonly manifestHash: Uint8Array;
  readonly root: Uint8Array;
  readonly size: number;
  readonly inodeSnapshot: InodeRow;
  readonly parameters: ManifestParameters;
  readonly token: number;
  readonly mainRevision?: number;
  readonly rootMutationGeneration?: number;
}

interface PreparedMutationSelection {
  readonly source: DurableEditSource;
  readonly token: number;
  readonly edit?: DurableContentEdit;
  readonly readSnapshot?: DurableEditReadSnapshot;
}
function directoryEntry(
  name: string,
  parentPath: string,
  type: FileType,
): DirectoryEntry {
  return Object.freeze({ name, parentPath, type, ...predicates(type) });
}
function validatedMode(
  mode: number | undefined,
  fallback: number,
  syscall: string,
  path: string,
): number {
  const value = mode ?? fallback;
  if (!Number.isSafeInteger(value) || value < 0)
    throw fsError("EINVAL", syscall, path, "mode must be a nonnegative safe integer");
  return value & 0o7777;
}

export class EphemeralFS implements EphemeralFilesystem {
  readonly capabilities: FilesystemCapabilities;
  readonly branches: Branches;
  readonly maintenance: FilesystemMaintenance;
  readonly #storagePort: OperationsStorage;
  readonly #clock: () => number;
  readonly #observer: FilesystemObserver | undefined;
  readonly #ownsDatabase: boolean;
  readonly #filesystemLimits: FilesystemLimits;
  readonly #storageLimits: StorageLimits;
  readonly #runtimeLimits: RuntimeLimits;
  readonly #branchLimits: BranchConfiguration;
  readonly #admission: AdmissionController;
  readonly #concurrency: RuntimeConcurrency;
  readonly #cache: ContentCache;
  readonly #readWindowBytes: number;
  readonly #pending = new Set<Promise<unknown>>();
  readonly #streams = new Map<
    string,
    { release: () => Promise<void>; error: () => void }
  >();
  #closing = false;
  #closed = false;
  #closePromise?: Promise<void>;

  private constructor(
    options: OpenFilesystemOptions,
    capabilities: FilesystemCapabilities,
    storagePort: OperationsStorage,
  ) {
    this.#storagePort = storagePort;
    this.#clock = options.clock ?? Date.now;
    this.#observer = options.observer;
    this.#ownsDatabase = options.ownsDatabase ?? false;
    this.capabilities = capabilities;
    this.#filesystemLimits = capabilities.filesystem;
    this.#storageLimits = capabilities.storage;
    this.#runtimeLimits = capabilities.runtime;
    this.#branchLimits = capabilities.branch;
    this.#admission = new AdmissionController(
      this.#runtimeLimits.maxManagedResidentBytes,
    );
    this.#concurrency = new RuntimeConcurrency(this.#runtimeLimits);
    this.#cache = new ContentCache(this.#runtimeLimits.maxCacheBytes, this.#admission);
    this.#readWindowBytes = Math.max(
      this.#filesystemLimits.preferredStreamChunkBytes,
      this.#runtimeLimits.maxQueryBatchBytes - 100 * 1024,
    );
    this.branches = new BranchManager(
      this.#storagePort,
      this.#filesystemLimits,
      this.#storageLimits,
      this.#runtimeLimits,
      this.#branchLimits,
      this.#clock,
      this.#admission,
      this.#concurrency,
      this.#cache,
      this.capabilities.format.cowPageBytes,
    );
    this.maintenance = new MaintenanceManager(
      this.#storagePort,
      this.#storageLimits,
      this.#runtimeLimits,
      this.#clock,
      this.#cache,
      this.capabilities.format.cowPageBytes,
      this.#branchLimits,
      this.#admission,
    );
  }

  static async open(
    options: OpenFilesystemOptions,
    storagePort: OperationsStorage,
  ): Promise<EphemeralFS> {
    const filesystem = resolveLimits(DEFAULT_FILESYSTEM_LIMITS, options.filesystem);
    const runtime = resolveLimits(DEFAULT_RUNTIME_LIMITS, options.runtime);
    if (
      filesystem.maxMaterializedBytes > DEFAULT_FILESYSTEM_LIMITS.maxMaterializedBytes
    )
      throw new RangeError(
        "the Node storage-prerequisite materialization profile is capped at 64 MiB; use ranges or streams for larger files",
      );
    const branch = resolveLimits(DEFAULT_BRANCH_CONFIGURATION, options.branch);
    const storage = constrainStorageLimits(options.storage, storagePort.capabilities);
    for (const [domain, values] of [
      ["filesystem", filesystem],
      ["runtime", runtime],
      ["branch", branch],
    ] as const)
      for (const [name, value] of Object.entries(values))
        if (!Number.isSafeInteger(value) || value <= 0)
          throw new RangeError(`${domain}.${name} must be a positive safe integer`);
    const minimumRetentionMs = 7 * 24 * 60 * 60 * 1000;
    if (branch.maxBranchIdBytes > 200 || branch.maxOperationIdBytes > 200)
      throw new RangeError(
        "branch and operation identifiers are capped at 200 UTF-8 bytes",
      );
    if (
      branch.terminalBranchRetentionMs < minimumRetentionMs ||
      branch.publicationResultRetentionMs < minimumRetentionMs
    )
      throw new RangeError("branch retention periods must be at least seven days");
    if (branch.maxConflictsPerPublication < branch.maxChangedPathsPerBranch)
      throw new RangeError(
        "maxConflictsPerPublication must cover maxChangedPathsPerBranch",
      );
    validateRuntimeLimits(
      filesystem,
      storage,
      runtime,
      options.format?.cowPageBytes ?? 16_384,
    );
    for (const name of ["maxPhysicalDatabaseBytes", "maxJournalBytes"] as const) {
      if (storage[name] !== storagePort.capabilities[name])
        throw new RangeError(
          `${name} must be configured on the SQLite adapter; a filesystem-only lower cap is not enforceable`,
        );
    }
    const metadata = storagePort.initialize({
      ...(options.format?.cowPageBytes === undefined
        ? {}
        : { cowPageBytes: options.format.cowPageBytes }),
      now: (options.clock ?? Date.now)(),
      maxManifestEntries: storage.maxManifestEntries,
      maxManifestDepth: storage.maxManifestDepth,
      maxFileBytes: storage.maxFileBytes,
      maxContentObjectBytes: maxPersistedContentObjectBytes(storage),
      writerProfile: persistedWriterProfile(filesystem, storage, branch),
    });
    validateRuntimeLimits(filesystem, storage, runtime, metadata.cowPageBytes);
    const format = Object.freeze({
      cowPageBytes: metadata.cowPageBytes,
      hashAlgorithm: "sha256" as const,
      chunkerAlgorithm: "fastcdc-v1" as const,
      manifestFormat: "efs-merkle-manifest-v1" as const,
    });
    const effectiveLimits = Object.freeze([
      ...Object.entries(filesystem).map(([name, value]) =>
        Object.freeze({
          domain: "filesystem" as const,
          name,
          value,
          scope: "persisted" as const,
          constrainedBy: "configuration" as const,
        }),
      ),
      ...Object.entries(storage).map(([name, value]) =>
        Object.freeze({
          domain: "storage" as const,
          name,
          value,
          scope: "persisted" as const,
          constrainedBy:
            name === "maxPhysicalDatabaseBytes" || name === "maxJournalBytes"
              ? ("adapter" as const)
              : ("configuration" as const),
        }),
      ),
      ...Object.entries(branch).map(([name, value]) =>
        Object.freeze({
          domain: "branch" as const,
          name,
          value,
          scope: "persisted" as const,
          constrainedBy: "configuration" as const,
        }),
      ),
      ...Object.entries(runtime).map(([name, value]) =>
        Object.freeze({
          domain: "runtime" as const,
          name,
          value,
          scope: "runtime" as const,
          constrainedBy: "configuration" as const,
        }),
      ),
    ]);
    return new EphemeralFS(
      options,
      Object.freeze({
        adapter: storagePort.capabilities,
        filesystem,
        storage,
        branch,
        runtime,
        format,
        effectiveLimits,
        readOnly: storagePort.readOnly,
      }),
      storagePort,
    );
  }

  readFile(path: string): Promise<Uint8Array>;
  readFile(path: string, options: ReadTextOptions): Promise<string>;
  readFile(path: string, options?: ReadTextOptions): Promise<Uint8Array | string> {
    return this.#operation("readFile", path, undefined, async () => {
      if (options !== undefined && options.encoding !== "utf8")
        throw fsError("EINVAL", "readFile", path, "unsupported encoding");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readFile");
      const selected = this.#transaction("read", (tx) => {
        const inode = this.#requireFile(
          tx
            .namespace(this.#filesystemLimits, this.#storageLimits, "readFile")
            .resolve(canonical, true),
          "readFile",
        );
        if (inode.size! > this.#filesystemLimits.maxMaterializedBytes)
          throw fsError(
            "EFBIG",
            "readFile",
            canonical.value,
            "file exceeds complete materialization limit",
          );
        return Object.freeze({
          size: inode.size!,
          manifestHash: copyBytes(inode.manifest_hash!),
        });
      });
      const capacity = options
        ? checkedMultiply(selected.size, 3, "UTF-8 read bytes and decoded string")
        : selected.size;
      this.#cache.makeRoom(capacity);
      const release = this.#admission.reserve(capacity);
      try {
        const bytes = new Uint8Array(selected.size);
        this.#readManifestMaterialized(selected.manifestHash, 0, bytes);
        return options
          ? new TextDecoder("utf-8", { fatal: false }).decode(bytes)
          : bytes;
      } finally {
        release();
      }
    });
  }

  readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array> {
    return this.#operation("readRange", path, undefined, async () => {
      checkedInteger(options?.offset, "offset");
      checkedInteger(
        options?.length,
        "length",
        this.#filesystemLimits.maxMaterializedBytes,
      );
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readRange");
      const selected = this.#transaction("read", (tx) => {
        const inode = this.#requireFile(
          tx
            .namespace(this.#filesystemLimits, this.#storageLimits, "readRange")
            .resolve(canonical, true),
          "readRange",
        );
        return Object.freeze({
          size: inode.size!,
          manifestHash: copyBytes(inode.manifest_hash!),
        });
      });
      const length = Math.max(
        0,
        Math.min(
          options.length,
          selected.size - Math.min(options.offset, selected.size),
        ),
      );
      this.#cache.makeRoom(length);
      const release = this.#admission.reserve(length);
      try {
        const output = new Uint8Array(length);
        this.#readManifestMaterialized(selected.manifestHash, options.offset, output);
        return output;
      } finally {
        release();
      }
    });
  }

  readStream(
    path: string,
    options: ReadStreamOptions = {},
  ): Promise<ReadableStream<Uint8Array>> {
    return this.#operation("readStream", path, options.signal, async () => {
      if (this.#storagePort.readOnly)
        throw fsError(
          "EROFS",
          "readStream",
          path,
          "durable stream leases require writable storage",
        );
      const offset = options.offset ?? 0;
      const requestedLength = options.length;
      checkedInteger(offset, "offset");
      if (requestedLength !== undefined) checkedInteger(requestedLength, "length");
      const releaseStreamAdmission = this.#concurrency.tryAcquireStream();
      if (!releaseStreamAdmission)
        throw fsError("EAGAIN", "readStream", path, "concurrent stream limit exceeded");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readStream");
      const leaseId = globalThis.crypto.randomUUID();
      const owner = globalThis.crypto.randomUUID();
      const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
      let selected: { manifestHash: Uint8Array; size: number };
      try {
        selected = this.#transaction("write", (tx) => {
          const inode = this.#requireFile(
            tx
              .namespace(this.#filesystemLimits, this.#storageLimits, "readStream")
              .resolve(canonical, true),
            "readStream",
          );
          const manifestHash = copyBytes(inode.manifest_hash!);
          const expires = this.#now() + this.#storageLimits.readLeaseMs;
          tx.staging(this.#storageLimits).acquireReadLease(
            leaseId,
            owner,
            ownerNonce,
            manifestHash,
            expires,
            undefined,
            undefined,
          );
          return { manifestHash, size: inode.size! };
        });
      } catch (error) {
        releaseStreamAdmission();
        throw error;
      }
      const end = Math.min(
        selected.size,
        requestedLength === undefined
          ? selected.size
          : checkedAdd(offset, requestedLength),
      );
      let position = Math.min(offset, selected.size);
      let released = false;
      let queuedRelease: (() => void) | undefined;
      let controllerReference: ReadableStreamDefaultController<Uint8Array> | undefined;
      let cursor: AuthenticatedManifestCursor | undefined;
      const startedAt = performance.now();
      const release = async (): Promise<void> => {
        if (released) return;
        released = true;
        cursor?.close();
        cursor = undefined;
        queuedRelease?.();
        queuedRelease = undefined;
        releaseStreamAdmission();
        this.#streams.delete(leaseId);
        const cache = this.#cache.metrics();
        this.#observe({
          type: "operation",
          operation: "readStream",
          outcome: "success",
          elapsedMs: performance.now() - startedAt,
          counters: Object.freeze({
            managedResidentBytes: this.#admission.usedBytes,
            peakManagedResidentBytes: this.#admission.peakBytes,
            cacheBytes: cache.bytes,
            cacheHits: cache.hits,
            cacheMisses: cache.misses,
            cacheEvictions: cache.evictions,
          }),
        });
        try {
          this.#transaction("write", (tx) => {
            tx.staging(this.#storageLimits).releaseReadLease(
              leaseId,
              owner,
              ownerNonce,
            );
          });
        } catch (error) {
          if (!this.#closing) throw error;
        }
      };
      const stream = new ReadableStream<Uint8Array>(
        {
          start(controller) {
            controllerReference = controller;
          },
          pull: async (controller) => {
            queuedRelease?.();
            queuedRelease = undefined;
            if (this.#closing || options.signal?.aborted) {
              await release();
              controller.error(
                options.signal?.aborted
                  ? abortError()
                  : fsError(
                      "EBADF",
                      "readStream",
                      canonical.value,
                      "filesystem is closing",
                    ),
              );
              return;
            }
            if (position >= end) {
              await release();
              controller.close();
              return;
            }
            const length = Math.min(this.#readWindowBytes, end - position);
            const free = this.#admission.reserve(length);
            try {
              const bytes = this.#transaction("read", (tx) => {
                const content = tx.content(this.#storageLimits, this.#cache);
                if (!cursor) {
                  cursor = content.openManifestCursor(selected.manifestHash, position);
                } else {
                  cursor.bindSource(content);
                }
                this.#cache.makeRoom(length);
                const output = new Uint8Array(length);
                const written = cursor.readInto(output, 0, output.byteLength);
                return output.subarray(0, written);
              });
              position += bytes.byteLength;
              let enqueued = 0;
              while (enqueued < bytes.byteLength) {
                const chunkEnd = Math.min(
                  bytes.byteLength,
                  enqueued + this.#filesystemLimits.preferredStreamChunkBytes,
                );
                controller.enqueue(bytes.subarray(enqueued, chunkEnd));
                enqueued = chunkEnd;
              }
              queuedRelease = free;
            } catch (error) {
              free();
              await release();
              controller.error(error);
            }
          },
          cancel: async () => {
            await release();
          },
        },
        { highWaterMark: 1, size: (chunk) => chunk.byteLength },
      );
      this.#streams.set(leaseId, {
        release,
        error: () => {
          cursor?.close();
          cursor = undefined;
          try {
            controllerReference?.error(
              fsError("EBADF", "readStream", canonical.value, "filesystem is closing"),
            );
          } catch {}
        },
      });
      return stream;
    });
  }

  writeFile(
    path: string,
    content: FileContent,
    options: WriteFileOptions = {},
  ): Promise<void> {
    return this.#operation("writeFile", path, options.signal, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, "writeFile");
      if (canonical.value === "/")
        throw fsError("EISDIR", "writeFile", canonical.value, "root is a directory");
      if (options.exclusive) {
        const exists = this.#transaction("read", (tx) =>
          tx
            .namespace(this.#filesystemLimits, this.#storageLimits, "writeFile")
            .resolveOptional(canonical, false),
        );
        if (exists)
          throw fsError("EEXIST", "writeFile", canonical.value, "destination exists");
      }
      let encodedString: Uint8Array | undefined;
      let frozen: Uint8Array | ReadableStream<Uint8Array>;
      if (typeof content === "string") {
        const encodedLength = utf8ByteLength(content);
        if (encodedLength > this.#storageLimits.maxWriteBytes)
          throw fsError(
            "EFBIG",
            "writeFile",
            path,
            "buffered write exceeds maxWriteBytes",
          );
        encodedString = new TextEncoder().encode(content);
        if (intrinsicByteLength(encodedString) !== encodedLength)
          throw new Error("UTF-8 length preflight disagrees with encoder output");
        frozen = encodedString;
      } else {
        frozen = content;
      }
      if (
        !(frozen instanceof Uint8Array) &&
        !(frozen && typeof frozen.getReader === "function")
      )
        throw fsError(
          "EINVAL",
          "writeFile",
          path,
          "content must be string, Uint8Array, or ReadableStream",
        );
      if (!(frozen instanceof Uint8Array)) {
        if (options.maxBytes === undefined)
          throw fsError(
            "EINVAL",
            "writeFile",
            path,
            "streamed writes require options.maxBytes",
          );
        checkedInteger(
          options.maxBytes,
          "streamed write maxBytes",
          this.#storageLimits.maxFileBytes,
        );
      }
      if (
        frozen instanceof Uint8Array &&
        intrinsicByteLength(frozen) > this.#storageLimits.maxWriteBytes
      )
        throw fsError(
          "EFBIG",
          "writeFile",
          path,
          "buffered write exceeds maxWriteBytes",
        );
      const prepared = await prepareContent(
        this.#storagePort,
        frozen,
        this.#storageLimits,
        this.#runtimeLimits,
        this.#admission,
        options.signal,
        this.#cache,
        this.#clock,
        options.maxBytes,
      );
      try {
        this.#transaction("write", (tx) => {
          const ns = tx.namespace(
            this.#filesystemLimits,
            this.#storageLimits,
            "writeFile",
          );
          const raw = ns.resolveOptional(canonical, false);
          let existing = raw;
          if (options.exclusive && raw)
            throw fsError("EEXIST", "writeFile", canonical.value, "destination exists");
          if (raw?.inode.type === 2 && !options.exclusive)
            existing = ns.resolve(canonical, true);
          if (existing?.inode.type === 1)
            throw fsError(
              "EISDIR",
              "writeFile",
              canonical.value,
              "destination is a directory",
            );
          if (existing && existing.inode.type !== 0)
            throw fsError(
              "ENOENT",
              "writeFile",
              canonical.value,
              "symbolic link target is not a regular file",
            );
          const now = this.#now();
          const revision = ns.nextRevision(now, existing ? 1 : 2);
          this.#validatePrepared(tx, prepared.certificate, now);
          if (existing) {
            const time = Math.max(
              now,
              existing.inode.mtime_ms,
              existing.inode.ctime_ms,
            );
            ns.setFileContent(
              existing.inode.id,
              prepared.size,
              prepared.hash,
              time,
              time,
              revision,
            );
            ns.recordInode(revision, existing.inode.id);
          } else {
            const { parent, name, nameSort } = ns.resolveParent(canonical);
            const inodeId = globalThis.crypto.randomUUID();
            const mode = validatedMode(
              options.mode,
              0o666,
              "writeFile",
              canonical.value,
            );
            ns.createInode({
              id: inodeId,
              type: 0,
              mode,
              now,
              revision,
              size: prepared.size,
              manifestHash: prepared.hash,
            });
            ns.putEntry(parent.inode.id, nameSort, name, inodeId, revision);
            ns.recordInode(revision, inodeId);
            ns.recordEntry(revision, parent.inode.id, nameSort);
            this.#touchParent(tx, ns, parent.inode, now, revision);
          }
          this.#releasePrepared(tx, prepared.certificate);
        });
      } catch (error) {
        this.#abandonPrepared(prepared.certificate);
        throw error;
      }
    });
  }

  writeRange(path: string, offset: number, content: Uint8Array): Promise<void> {
    const inputLength = intrinsicByteLength(content);
    if (inputLength > this.#storageLimits.maxWriteBytes)
      return Promise.reject(
        fsError("EFBIG", "writeRange", path, "write exceeds maxWriteBytes"),
      );
    return this.#operation("writeRange", path, undefined, async () => {
      checkedInteger(offset, "offset");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "writeRange");
      this.#cache.makeRoom(inputLength);
      const releaseInput = this.#admission.reserve(inputLength);
      try {
        const frozen = copyBytes(content);
        await this.#replaceExistingAtPath(canonical.value, "writeRange", (source) => {
          if (!frozen.byteLength) return undefined;
          const size = Math.max(source.size, checkedAdd(offset, frozen.byteLength));
          if (size > this.#storageLimits.maxFileBytes)
            throw fsError(
              "EFBIG",
              "writeRange",
              canonical.value,
              "result exceeds maxFileBytes",
            );
          const editOffset = Math.min(offset, source.size);
          const gap = Math.max(0, offset - source.size);
          const deleteLength = Math.min(frozen.byteLength, source.size - editOffset);
          return this.#bufferedInsertionEdit(editOffset, deleteLength, gap, frozen);
        });
      } finally {
        releaseInput();
      }
    });
  }

  replaceRange(
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): Promise<void> {
    const inputLength = intrinsicByteLength(insertBytes);
    if (inputLength > this.#storageLimits.maxWriteBytes)
      return Promise.reject(
        fsError("EFBIG", "replaceRange", path, "insertion exceeds maxWriteBytes"),
      );
    return this.#operation("replaceRange", path, undefined, async () => {
      checkedInteger(offset, "offset");
      checkedInteger(deleteLength, "deleteLength");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "replaceRange");
      this.#cache.makeRoom(inputLength);
      const releaseInput = this.#admission.reserve(inputLength);
      try {
        const frozen = copyBytes(insertBytes);
        await this.#replaceExistingAtPath(canonical.value, "replaceRange", (source) => {
          if (offset > source.size || deleteLength > source.size - offset)
            throw fsError(
              "EINVAL",
              "replaceRange",
              canonical.value,
              "replacement range is outside file",
            );
          if (!deleteLength && !frozen.byteLength) return undefined;
          const finalSize = source.size - deleteLength + frozen.byteLength;
          if (finalSize > this.#storageLimits.maxFileBytes)
            throw fsError(
              "EFBIG",
              "replaceRange",
              canonical.value,
              "result exceeds maxFileBytes",
            );
          return this.#bufferedInsertionEdit(offset, deleteLength, 0, frozen);
        });
      } finally {
        releaseInput();
      }
    });
  }

  truncate(path: string, size = 0): Promise<void> {
    return this.#operation("truncate", path, undefined, async () => {
      checkedInteger(size, "size", this.#storageLimits.maxFileBytes);
      const canonical = canonicalizePath(path, this.#filesystemLimits, "truncate");
      await this.#replaceExistingAtPath(canonical.value, "truncate", (source) => {
        if (size === source.size) return undefined;
        return size < source.size
          ? Object.freeze({
              offset: size,
              deleteLength: source.size - size,
              insertLength: 0,
              readInsert: (_offset: number, length: number) => new Uint8Array(length),
            })
          : Object.freeze({
              offset: source.size,
              deleteLength: 0,
              insertLength: size - source.size,
              readInsert: (_offset: number, length: number) => new Uint8Array(length),
            });
      });
    });
  }

  mkdir(path: string, options: MkdirOptions = {}): Promise<void> {
    return this.#operation("mkdir", path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, "mkdir");
      const mode = validatedMode(options.mode, 0o777, "mkdir", canonical.value);
      if (canonical.value === "/") {
        if (options.recursive) return;
        throw fsError("EEXIST", "mkdir", canonical.value, "root already exists");
      }
      this.#transaction("write", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, "mkdir");
        const existing = ns.resolveOptional(canonical, false);
        if (existing) {
          if (options.recursive && existing.inode.type === 1) return;
          throw fsError("EEXIST", "mkdir", canonical.value, "destination exists");
        }
        if (!options.recursive) {
          const parent = ns.resolveParent(canonical);
          const now = this.#now();
          const revision = ns.nextRevision(now, 2);
          this.#createDirectory(
            tx,
            ns,
            parent.parent.inode,
            parent.name,
            parent.nameSort,
            mode,
            now,
            revision,
          );
          return;
        }
        const missing: { parent: InodeRow; name: string; nameSort: Uint8Array }[] = [];
        let parent = ns.resolve("/").inode;
        for (let index = 0; index < canonical.segments.length; index += 1) {
          const name = canonical.segments[index]!;
          const nameSort = canonical.encodedSegments[index]!;
          const entry = ns.entry(parent.id, nameSort);
          if (entry?.inode_id) {
            const inode = ns.inode(entry.inode_id);
            if (!inode) throw new Error("ECORRUPT: missing inode");
            if (inode.type !== 1)
              throw fsError(
                index === canonical.segments.length - 1 ? "EEXIST" : "ENOTDIR",
                "mkdir",
                canonical.value,
                "path component is not a directory",
              );
            parent = inode;
          } else {
            missing.push({ parent, name, nameSort });
            const placeholder = {
              ...parent,
              id: globalThis.crypto.randomUUID(),
              type: 1,
              mode,
              birthtime_ms: 0,
              mtime_ms: 0,
              ctime_ms: 0,
              nlink: 1,
              size: null,
              manifest_hash: null,
              symlink_target: null,
              token: 0,
            } as InodeRow;
            parent = placeholder;
          }
        }
        if (missing.length > this.#filesystemLimits.maxAtomicTreeEntries)
          throw fsError(
            "EFBIG",
            "mkdir",
            canonical.value,
            "recursive create exceeds atomic tree limit",
          );
        const now = this.#now();
        const revision = ns.nextRevision(now, missing.length * 2);
        let actualParent = missing[0]!.parent;
        for (const item of missing) {
          const id =
            parent.id === item.parent.id
              ? globalThis.crypto.randomUUID()
              : item === missing.at(-1)
                ? parent.id
                : globalThis.crypto.randomUUID();
          const inode = this.#createDirectory(
            tx,
            ns,
            actualParent,
            item.name,
            item.nameSort,
            mode,
            now,
            revision,
            id,
          );
          actualParent = inode;
        }
      });
    });
  }

  readdir(path: string, options: ReaddirOptions = {}): Promise<DirectoryEntry[]> {
    return this.#operation("readdir", path, undefined, async () => {
      if (options.limit !== undefined)
        checkedInteger(
          options.limit,
          "limit",
          this.#filesystemLimits.maxReaddirEntries,
        );
      const start =
        options.startAfter === undefined
          ? undefined
          : validateName(options.startAfter, this.#filesystemLimits, "readdir");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readdir");
      return this.#transaction("read", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, "readdir");
        const selected = ns.resolve(canonical, true);
        if (selected.inode.type !== 1)
          throw fsError(
            "ENOTDIR",
            "readdir",
            canonical.value,
            "path is not a directory",
          );
        const limit = options.limit ?? this.#filesystemLimits.maxReaddirEntries;
        if (limit === 0) return [];
        const rows = ns.children(
          selected.inode.id,
          limit + 1,
          this.#runtimeLimits.maxQueryBatchBytes,
          start,
        );
        if (rows.length > limit)
          throw fsError(
            "EFBIG",
            "readdir",
            canonical.value,
            "directory listing exceeds configured limit",
          );
        return rows.map((row) =>
          directoryEntry(row.name, canonical.value, inodeType(row.type)),
        );
      });
    });
  }

  stat(path: string): Promise<FileStat> {
    return this.#stat(path, true, "stat");
  }
  lstat(path: string): Promise<FileStat> {
    return this.#stat(path, false, "lstat");
  }

  chmod(path: string, mode: number): Promise<void> {
    return this.#operation("chmod", path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, "chmod");
      const normalized = validatedMode(mode, 0, "chmod", canonical.value);
      this.#transaction("write", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, "chmod");
        const selected = ns.resolve(canonical, true);
        if (selected.inode.mode === normalized) return;
        const now = Math.max(this.#now(), selected.inode.ctime_ms);
        const revision = ns.nextRevision(now, 1);
        ns.setMode(selected.inode.id, normalized, now, revision);
        ns.recordInode(revision, selected.inode.id);
      });
    });
  }

  link(existingPath: string, newPath: string): Promise<void> {
    return this.#operation("link", existingPath, undefined, async () => {
      const sourcePath = canonicalizePath(existingPath, this.#filesystemLimits, "link");
      const destination = canonicalizePath(newPath, this.#filesystemLimits, "link");
      if (destination.value === "/")
        throw fsError(
          "EPERM",
          "link",
          destination.value,
          "root cannot be a hard-link destination",
        );
      this.#transaction("write", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, "link");
        const source = ns.resolve(sourcePath, true);
        if (source.inode.type !== 0)
          throw fsError(
            "EPERM",
            "link",
            sourcePath.value,
            "only regular files can be hard linked",
          );
        if (ns.resolveOptional(destination, false))
          throw fsError("EEXIST", "link", destination.value, "destination exists");
        const parent = ns.resolveParent(destination);
        const now = this.#now();
        const revision = ns.nextRevision(now, 3);
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
        this.#touchParent(tx, ns, parent.parent.inode, now, revision);
      });
    });
  }

  symlink(target: string, path: string): Promise<void> {
    return this.#operation("symlink", path, undefined, async () => {
      validateSymlinkTarget(target, this.#filesystemLimits, "symlink");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "symlink");
      if (canonical.value === "/")
        throw fsError(
          "EPERM",
          "symlink",
          canonical.value,
          "root cannot be a symlink destination",
        );
      this.#transaction("write", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, "symlink");
        if (ns.resolveOptional(canonical, false))
          throw fsError("EEXIST", "symlink", canonical.value, "destination exists");
        const parent = ns.resolveParent(canonical);
        const now = this.#now();
        const revision = ns.nextRevision(now, 3);
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
        ns.recordInode(revision, id);
        ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
        this.#touchParent(tx, ns, parent.parent.inode, now, revision);
      });
    });
  }

  readlink(path: string): Promise<string> {
    return this.#operation("readlink", path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readlink");
      return this.#transaction("read", (tx) => {
        const selected = tx
          .namespace(this.#filesystemLimits, this.#storageLimits, "readlink")
          .resolve(canonical, false);
        if (selected.inode.type !== 2 || selected.inode.symlink_target === null)
          throw fsError(
            "EINVAL",
            "readlink",
            canonical.value,
            "path is not a symbolic link",
          );
        return selected.inode.symlink_target;
      });
    });
  }

  rename(oldPath: string, newPath: string): Promise<void> {
    return this.#operation("rename", oldPath, undefined, async () => {
      const sourcePath = canonicalizePath(oldPath, this.#filesystemLimits, "rename");
      const destinationPath = canonicalizePath(
        newPath,
        this.#filesystemLimits,
        "rename",
      );
      if (sourcePath.value === "/" || destinationPath.value === "/")
        throw fsError(
          "EPERM",
          "rename",
          sourcePath.value,
          "root cannot be renamed or replaced",
        );
      if (sourcePath.value === destinationPath.value) return;
      if (destinationPath.value.startsWith(`${sourcePath.value}/`))
        throw fsError(
          "EINVAL",
          "rename",
          sourcePath.value,
          "directory cannot be moved into itself",
        );
      this.#transaction("write", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, "rename");
        const source = ns.resolve(sourcePath, false);
        const destination = ns.resolveOptional(destinationPath, false);
        const parent = ns.resolveParent(destinationPath);
        if (destination) {
          if (source.inode.type === 1 && destination.inode.type !== 1)
            throw fsError(
              "ENOTDIR",
              "rename",
              destinationPath.value,
              "cannot replace non-directory with directory",
            );
          if (source.inode.type !== 1 && destination.inode.type === 1)
            throw fsError(
              "EISDIR",
              "rename",
              destinationPath.value,
              "cannot replace directory with non-directory",
            );
          if (
            destination.inode.type === 1 &&
            this.#childCount(tx, destination.inode.id) > 0
          )
            throw fsError(
              "ENOTEMPTY",
              "rename",
              destinationPath.value,
              "destination directory is not empty",
            );
        }
        const now = this.#now();
        const revision = ns.nextRevision(now, 5);
        ns.putEntry(source.parentInode!, source.nameSort!, null, null, revision);
        ns.recordEntry(revision, source.parentInode!, source.nameSort!, true);
        if (destination?.inode.id === source.inode.id) {
          ns.decrementLinks(source.inode.id, now, revision);
          ns.recordInode(revision, source.inode.id);
        } else {
          if (destination) this.#removeDestination(tx, ns, destination, now, revision);
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
        if (sourceParent) this.#touchParent(tx, ns, sourceParent, now, revision);
        if (parent.parent.inode.id !== source.parentInode)
          this.#touchParent(tx, ns, parent.parent.inode, now, revision);
      });
    });
  }

  unlink(path: string): Promise<void> {
    return this.#remove(path, false, false, "unlink", true);
  }
  rm(path: string, options: RmOptions = {}): Promise<void> {
    return this.#remove(
      path,
      options.recursive ?? false,
      options.force ?? false,
      "rm",
      false,
    );
  }

  close(): Promise<void> {
    if (this.#closePromise) return this.#closePromise;
    this.#closing = true;
    this.#closePromise = (async () => {
      await (this.branches as BranchManager).close();
      for (const stream of this.#streams.values()) stream.error();
      await Promise.allSettled(
        [...this.#streams.values()].map((stream) => stream.release()),
      );
      await Promise.allSettled([...this.#pending]);
      try {
        this.#cache.clear();
        if (this.#ownsDatabase) await this.#storagePort.close();
      } finally {
        this.#closed = true;
      }
    })();
    return this.#closePromise;
  }

  async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }

  #stat(path: string, followFinal: boolean, syscall: string): Promise<FileStat> {
    return this.#operation(syscall, path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, syscall);
      return this.#transaction("read", (tx) => {
        const selected = tx
          .namespace(this.#filesystemLimits, this.#storageLimits, syscall)
          .resolve(canonical, followFinal);
        return fileStat(selected.inode, canonical.segments.at(-1) ?? "");
      });
    });
  }
  #readMutationSourceSelection(
    tx: StorageTransactionPorts,
    path: string,
    syscall: string,
  ): MutationSourceSelection {
    const selected = tx
      .namespace(this.#filesystemLimits, this.#storageLimits, syscall)
      .resolve(path, true);
    const inode = this.#requireFile(selected, syscall);
    const manifestHash = copyBytes(inode.manifest_hash!);
    const rootBytes = tx
      .content(this.#storageLimits, this.#cache)
      .withManifestRoot(manifestHash, (encoded) => copyBytes(encoded));
    if (!rootBytes) throw new Error("ECORRUPT: missing manifest root");
    const root = decodeManifestRoot(rootBytes, manifestHash);
    validateSupportedManifestParameters(root.parameters);
    if (root.fileSize !== inode.size)
      throw new Error("ECORRUPT: inode size disagrees with manifest root");
    return Object.freeze({
      manifestHash,
      root: rootBytes,
      size: inode.size!,
      inodeSnapshot: Object.freeze({
        ...inode,
        manifest_hash: inode.manifest_hash ? copyBytes(inode.manifest_hash) : null,
      }),
      parameters: root.parameters,
      token: inode.token,
      ...(selected.mainRevision === undefined
        ? {}
        : { mainRevision: selected.mainRevision }),
      ...(selected.rootMutationGeneration === undefined
        ? {}
        : { rootMutationGeneration: selected.rootMutationGeneration }),
    });
  }

  #createMutationSource(selected: MutationSourceSelection): {
    source: DurableEditSource;
    token: number;
  } {
    const maxReadWindowBytes = Math.max(
      1,
      Math.min(
        1024 * 1024,
        this.#runtimeLimits.maxQueryBatchBytes,
        this.#runtimeLimits.maxWriteSessionBytes,
      ),
    );
    let readTransactions = 0;
    let cachedWindow:
      | {
          readonly offset: number;
          readonly bytes: Uint8Array;
          readonly release: () => void;
        }
      | undefined;
    const releaseReadWindow = (): void => {
      cachedWindow?.release();
      cachedWindow = undefined;
    };
    const readSlice = (
      offset: number,
      length: number,
      content?: ContentStore,
      copyOutput = true,
    ): Uint8Array => {
      checkedInteger(offset, "manifest read offset");
      checkedInteger(length, "manifest read length");
      if (length === 0) return new Uint8Array(0);
      const end = checkedAdd(offset, length, "manifest read end");
      const cachedEnd = cachedWindow
        ? checkedAdd(cachedWindow.offset, cachedWindow.bytes.byteLength)
        : -1;
      if (!cachedWindow || offset < cachedWindow.offset || end > cachedEnd) {
        releaseReadWindow();
        const windowLength = Math.max(length, maxReadWindowBytes);
        // Center the first bounded window around the requested slice. Local
        // rebuilds commonly ask for the bytes immediately before and after
        // the edit; anchoring at the first request made those two reads
        // unnecessarily open separate SQLite read transactions.
        const maxOffset = Math.max(0, selected.size - windowLength);
        const centeredOffset = Math.max(
          0,
          offset - Math.floor((windowLength - length) / 2),
        );
        const windowOffset = Math.min(centeredOffset, maxOffset);
        const available = Math.min(windowLength, selected.size - windowOffset);
        const bytes = content
          ? readManifestRange(
              content,
              selected.manifestHash,
              windowOffset,
              available,
              this.#admission,
              this.#cache,
            )
          : this.#storagePort.transaction(
              "read",
              {
                maxRows: this.#storageLimits.maxFinalTransactionRows,
                maxBytes: this.#runtimeLimits.maxQueryBatchBytes,
              },
              (tx) =>
                readManifestRange(
                  tx.content(this.#storageLimits, this.#cache),
                  selected.manifestHash,
                  windowOffset,
                  available,
                  this.#admission,
                  this.#cache,
                ),
            );
        if (!content) readTransactions += 1;
        // `readManifestRange` releases its temporary output reservation before
        // returning. Keep the retained edit window accounted for until the
        // durable edit has finished, so source batching cannot bypass the
        // managed-resident admission limit.
        const release = this.#admission.reserve(bytes.byteLength);
        cachedWindow = Object.freeze({ offset: windowOffset, bytes, release });
      }
      const current = cachedWindow!;
      const relativeOffset = offset - current.offset;
      const range = intrinsicByteRange(
        current.bytes,
        relativeOffset,
        checkedAdd(relativeOffset, length, "manifest cached read end"),
      );
      return copyOutput ? copyBytes(range) : range;
    };
    const source: DurableEditSource = Object.freeze({
      manifestHash: copyBytes(selected.manifestHash),
      rootBytes: copyBytes(selected.root),
      ...(selected.rootMutationGeneration === undefined
        ? {}
        : { rootMutationGeneration: selected.rootMutationGeneration }),
      size: selected.size,
      parameters: selected.parameters,
      readStorageTransactions: 1,
      getReadStorageTransactions: () => readTransactions,
      // Public durable-edit routing remains M3, but this storage prerequisite keeps
      // fallback reads practical without pinning a long SQLite read transaction.
      maxReadWindowBytes,
      releaseReadWindow,
      read: (offset: number, length: number): Uint8Array => readSlice(offset, length),
      readInTransaction: (
        content: ContentStore,
        offset: number,
        length: number,
      ): Uint8Array => readSlice(offset, length, content, false),
    });
    editSourceInodes.set(source, {
      inode: selected.inodeSnapshot,
      ...(selected.mainRevision === undefined
        ? {}
        : { mainRevision: selected.mainRevision }),
      ...(selected.rootMutationGeneration === undefined
        ? {}
        : { rootMutationGeneration: selected.rootMutationGeneration }),
    });
    return Object.freeze({ source, token: selected.token });
  }

  #selectMutationSourceWithSnapshot(
    path: string,
    syscall: string,
    makeEdit: (source: DurableEditSource) => DurableContentEdit | undefined,
  ): PreparedMutationSelection {
    const maxReadWindowBytes = Math.max(
      1,
      Math.min(
        2 * 1024 * 1024,
        this.#runtimeLimits.maxQueryBatchBytes,
        this.#runtimeLimits.maxWriteSessionBytes,
      ),
    );
    let sourceForCleanup: DurableEditSource | undefined;
    try {
      return this.#storagePort.transaction(
        "read",
        durableEditReadSnapshotBudget(maxReadWindowBytes, this.#storageLimits),
        (tx) => {
          const sourceSelection = this.#readMutationSourceSelection(tx, path, syscall);
          const selected = this.#createMutationSource(sourceSelection);
          sourceForCleanup = selected.source;
          const edit = makeEdit(selected.source);
          if (!edit) return selected;
          const state = tryLoadBoundedManifestStateInTransaction(
            tx,
            selected.source,
            selected.source.manifestHash,
            { offset: edit.offset, deleteLength: edit.deleteLength },
            this.#storageLimits,
            DEFAULT_LOCAL_REBUILD_LIMITS,
            this.#cache,
            edit.insertLength === edit.deleteLength,
            sourceSelection.root,
          );
          return {
            ...selected,
            edit,
            ...(state ? { readSnapshot: Object.freeze({ state }) } : {}),
          };
        },
      );
    } catch (error) {
      sourceForCleanup?.releaseReadWindow?.();
      throw error;
    }
  }

  async #replaceExistingAtPath(
    path: string,
    syscall: string,
    makeEdit: (source: DurableEditSource) => DurableContentEdit | undefined,
  ): Promise<void> {
    const selected = this.#selectMutationSourceWithSnapshot(path, syscall, makeEdit);
    if (!selected.edit) return;
    await this.#replaceExisting(
      path,
      selected.source,
      selected.edit,
      selected.token,
      syscall,
      selected.readSnapshot,
    );
  }
  #bufferedInsertionEdit(
    offset: number,
    deleteLength: number,
    zeroPrefixLength: number,
    bytes: Uint8Array,
  ): DurableContentEdit {
    const insertLength = checkedAdd(zeroPrefixLength, bytes.byteLength);
    return Object.freeze({
      offset,
      deleteLength,
      insertLength,
      retainedBytes: bytes.byteLength,
      readInsert: (position: number, length: number): Uint8Array => {
        checkedInteger(position, "insertion offset", insertLength);
        checkedInteger(length, "insertion length", insertLength - position);
        const output = new Uint8Array(length);
        const dataStart = Math.max(position, zeroPrefixLength);
        const dataEnd = Math.min(position + length, insertLength);
        if (dataEnd > dataStart)
          output.set(
            intrinsicByteRange(
              bytes,
              dataStart - zeroPrefixLength,
              dataEnd - zeroPrefixLength,
            ),
            dataStart - position,
          );
        return output;
      },
    });
  }
  async #replaceExisting(
    path: string,
    source: DurableEditSource,
    edit: DurableContentEdit,
    expectedToken: number,
    syscall: string,
    readSnapshot?: DurableEditReadSnapshot,
  ): Promise<void> {
    let finalizedInPersistence = false;
    let inlineFinalizeMs = 0;
    const finalizePrepared = (
      tx: StorageTransactionPorts,
      certificate: ClosureCertificate,
      hash: Uint8Array,
      size: number,
      sealedLease?: ValidatedSealedLease & { readonly expiresAtMs?: number },
    ): void => {
      const started = performance.now();
      try {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, syscall);
        const inodeSnapshot = editSourceInodes.get(source);
        const inodeId = inodeSnapshot?.inode.id;
        const inode = inodeId
          ? undefined
          : this.#requireFile(ns.resolve(path, true), syscall);
        if (inode && inode.token !== expectedToken)
          throw fsError(
            "EAGAIN",
            syscall,
            path,
            "file changed while content was prepared",
          );
        const targetId = inodeId ?? inode!.id;
        const now = Math.max(
          this.#now(),
          inodeSnapshot?.inode.mtime_ms ?? inode?.mtime_ms ?? 0,
          inodeSnapshot?.inode.ctime_ms ?? inode?.ctime_ms ?? 0,
        );
        const validatedLease =
          sealedLease &&
          (sealedLease.expiresAtMs === undefined || sealedLease.expiresAtMs >= now)
            ? sealedLease
            : this.#validatePrepared(tx, certificate, now);
        const revision =
          inodeSnapshot?.mainRevision !== undefined &&
          inodeSnapshot.rootMutationGeneration !== undefined &&
          ns.nextRevisionFromSnapshot
            ? ns.nextRevisionFromSnapshot(
                now,
                1,
                inodeSnapshot.mainRevision,
                // The durable edit has already journaled its staging lease
                // once in the persistence transaction.
                inodeSnapshot.rootMutationGeneration + 1,
              )
            : ns.nextRevision(now, 1);
        if (
          ns.setFileContent(targetId, size, hash, now, now, revision, expectedToken) !==
          1
        )
          throw fsError(
            "EAGAIN",
            syscall,
            path,
            "file changed while content was prepared",
          );
        const updatedInode = inodeSnapshot
          ? Object.freeze({
              ...inodeSnapshot.inode,
              size,
              manifest_hash: copyBytes(hash),
              mtime_ms: now,
              ctime_ms: now,
              token: revision,
            })
          : undefined;
        if (updatedInode && ns.recordFileContentRevision)
          ns.recordFileContentRevision(revision, updatedInode);
        else ns.recordInode(revision, targetId);
        this.#releasePrepared(tx, certificate, validatedLease);
        finalizedInPersistence = true;
      } finally {
        inlineFinalizeMs += performance.now() - started;
      }
    };
    let prepared: Awaited<ReturnType<typeof prepareDurableEditedContent>> | undefined;
    try {
      prepared = await prepareDurableEditedContent(
        this.#storagePort,
        source,
        edit,
        this.#storageLimits,
        this.#runtimeLimits,
        this.#admission,
        this.#cache,
        this.#clock,
        true,
        finalizePrepared,
        readSnapshot,
      );
      if (inlineFinalizeMs > 0 && prepared.localRebuildMetrics?.phaseMs)
        prepared.localRebuildMetrics.phaseMs.finalizeMs += inlineFinalizeMs;
      const finalizeStarted = performance.now();
      try {
        const current = prepared;
        if (!current) throw new Error("durable edit preparation returned no result");
        if (!finalizedInPersistence)
          this.#transaction("write", (tx) =>
            finalizePrepared(tx, current.certificate, current.hash, current.size),
          );
      } finally {
        prepared.localRebuildMetrics?.phaseMs &&
          (prepared.localRebuildMetrics.phaseMs.finalizeMs +=
            performance.now() - finalizeStarted);
      }
    } catch (error) {
      if (prepared) this.#abandonPrepared(prepared.certificate);
      throw error;
    } finally {
      source.releaseReadWindow?.();
    }
  }
  #requireFile(selected: ResolvedPath, syscall: string): InodeRow {
    if (selected.inode.type === 1)
      throw fsError("EISDIR", syscall, selected.path.value, "path is a directory");
    if (
      selected.inode.type !== 0 ||
      selected.inode.manifest_hash === null ||
      selected.inode.size === null
    )
      throw fsError(
        "EINVAL",
        syscall,
        selected.path.value,
        "path is not a regular file",
      );
    return selected.inode;
  }
  #createDirectory(
    tx: StorageTransactionPorts,
    ns: NamespaceStore,
    parent: InodeRow,
    name: string,
    nameSort: Uint8Array,
    mode: number,
    now: number,
    revision: number,
    id: string = globalThis.crypto.randomUUID(),
  ): InodeRow {
    ns.createInode({ id, type: 1, mode, now, revision });
    ns.putEntry(parent.id, nameSort, name, id, revision);
    ns.recordInode(revision, id);
    ns.recordEntry(revision, parent.id, nameSort);
    this.#touchParent(tx, ns, parent, now, revision);
    return {
      id,
      type: 1,
      mode,
      birthtime_ms: now,
      mtime_ms: now,
      ctime_ms: now,
      nlink: 1,
      size: null,
      manifest_hash: null,
      symlink_target: null,
      token: revision,
    };
  }
  #touchParent(
    _tx: StorageTransactionPorts,
    ns: NamespaceStore,
    parent: InodeRow,
    now: number,
    revision: number,
  ): void {
    const time = Math.max(now, parent.mtime_ms, parent.ctime_ms);
    ns.touch(parent.id, time, time, revision);
    ns.recordInode(revision, parent.id);
  }
  #childCount(tx: StorageTransactionPorts, inodeId: string): number {
    return tx
      .namespace(this.#filesystemLimits, this.#storageLimits, "childCount")
      .childCount(inodeId);
  }
  #removeDestination(
    _tx: StorageTransactionPorts,
    ns: NamespaceStore,
    selected: ResolvedPath,
    now: number,
    revision: number,
  ): void {
    ns.putEntry(selected.parentInode!, selected.nameSort!, null, null, revision);
    ns.recordEntry(revision, selected.parentInode!, selected.nameSort!, true);
    if (selected.inode.type === 0 && selected.inode.nlink > 1) {
      ns.decrementLinks(selected.inode.id, now, revision);
      ns.recordInode(revision, selected.inode.id);
    } else {
      if (selected.inode.type === 1) ns.deleteEntriesUnder(selected.inode.id);
      ns.deleteInode(selected.inode.id);
      ns.recordInode(revision, selected.inode.id, true);
    }
  }
  #remove(
    path: string,
    recursive: boolean,
    force: boolean,
    syscall: string,
    filesOnly: boolean,
  ): Promise<void> {
    return this.#operation(syscall, path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, syscall);
      if (canonical.value === "/")
        throw fsError("EPERM", syscall, canonical.value, "root cannot be removed");
      this.#transaction("write", (tx) => {
        const ns = tx.namespace(this.#filesystemLimits, this.#storageLimits, syscall);
        const selected = ns.resolveOptional(canonical, false);
        if (!selected) {
          if (force) return;
          throw fsError("ENOENT", syscall, canonical.value, "path does not exist");
        }
        if (filesOnly && selected.inode.type === 1)
          throw fsError(
            "EISDIR",
            syscall,
            canonical.value,
            "unlink cannot remove a directory",
          );
        const children =
          selected.inode.type === 1 ? this.#collectTree(tx, ns, selected.inode.id) : [];
        if (children.length && !recursive)
          throw fsError(
            "ENOTEMPTY",
            syscall,
            canonical.value,
            "directory is not empty",
          );
        if (children.length + 1 > this.#filesystemLimits.maxAtomicTreeEntries)
          throw fsError(
            "EFBIG",
            syscall,
            canonical.value,
            "recursive removal exceeds atomic tree limit",
          );
        const now = this.#now();
        const revision = ns.nextRevision(now, children.length * 2 + 3);
        for (const child of children.reverse())
          this.#removeDestination(tx, ns, child, now, revision);
        this.#removeDestination(tx, ns, selected, now, revision);
        const parent = ns.inode(selected.parentInode!);
        if (parent) this.#touchParent(tx, ns, parent, now, revision);
      });
    });
  }
  #collectTree(
    _tx: StorageTransactionPorts,
    ns: NamespaceStore,
    rootId: string,
  ): ResolvedPath[] {
    const result: ResolvedPath[] = [];
    const stack = [rootId];
    while (stack.length) {
      const parentId = stack.pop()!;
      const rows = ns.children(
        parentId,
        this.#filesystemLimits.maxAtomicTreeEntries + 1,
        this.#runtimeLimits.maxQueryBatchBytes,
      );
      for (const row of rows) {
        const inode = ns.inode(row.inode_id);
        if (!inode) throw new Error("ECORRUPT: missing descendant inode");
        result.push({
          path: canonicalizePath(`/${row.name}`, this.#filesystemLimits, "rm"),
          inode,
          parentInode: parentId,
          name: row.name,
          nameSort: row.name_sort,
          entryToken: row.token,
        });
        if (inode.type === 1) stack.push(inode.id);
      }
      if (result.length > this.#filesystemLimits.maxAtomicTreeEntries) break;
    }
    return result;
  }
  #validatePrepared(
    tx: StorageTransactionPorts,
    certificate: ClosureCertificate,
    now: number,
  ) {
    return tx.staging(this.#storageLimits).validateSealed(certificate, now);
  }
  #releasePrepared(
    tx: StorageTransactionPorts,
    certificate: ClosureCertificate,
    validatedLease?: ValidatedSealedLease,
  ): void {
    if (
      !tx
        .staging(this.#storageLimits)
        .release(certificate.leaseId, certificate.ownerNonce, true, validatedLease)
    )
      throw new Error("ECORRUPT: staging lease could not be released");
  }
  #abandonPrepared(certificate: ClosureCertificate): void {
    try {
      this.#transaction("write", (tx) => {
        tx.staging(this.#storageLimits).release(
          certificate.leaseId,
          certificate.ownerNonce,
          false,
        );
      });
    } catch {}
  }
  #now(): number {
    const value = this.#clock();
    if (!Number.isSafeInteger(value) || value < 0)
      throw new Error("clock must return a nonnegative safe integer");
    return value;
  }
  #readManifestMaterialized(
    manifestHash: Uint8Array,
    offset: number,
    destination: Uint8Array,
  ): void {
    const rowBoundWindow = Math.max(
      1,
      Math.floor(
        (this.#storageLimits.maxFinalTransactionRows -
          this.#storageLimits.maxManifestDepth * 4 -
          16) /
          2,
      ),
    );
    const windowBytes = Math.max(
      1,
      Math.min(this.#runtimeLimits.maxQueryBatchBytes, rowBoundWindow),
    );
    let written = 0;
    while (written < destination.byteLength) {
      const length = Math.min(windowBytes, destination.byteLength - written);
      const count = this.#transaction("read", (tx) =>
        readManifestInto(
          tx.content(this.#storageLimits, this.#cache),
          manifestHash,
          offset + written,
          destination,
          written,
          length,
        ),
      );
      if (count !== length)
        throw new Error("ECORRUPT: authenticated manifest materialization ended early");
      written += count;
    }
  }
  #transaction<T>(
    mode: StorageTransactionMode,
    callback: (tx: StorageTransactionPorts) => T,
  ): T {
    return this.#storagePort.transaction(
      mode,
      {
        maxRows: this.#storageLimits.maxFinalTransactionRows,
        maxBytes: this.#storageLimits.maxFinalTransactionBytes,
      },
      callback,
    );
  }
  #operation<T>(
    operation: string,
    path: string | undefined,
    signal: AbortSignal | undefined,
    callback: () => Promise<T>,
  ): Promise<T> {
    if (this.#closing || this.#closed)
      return Promise.reject(
        fsError("EBADF", operation, path, "filesystem is closed or closing"),
      );
    if (signal?.aborted) return Promise.reject(abortError());
    const releaseOperation = this.#concurrency.tryAcquireOperation();
    if (!releaseOperation)
      return Promise.reject(
        fsError("EAGAIN", operation, path, "concurrent operation limit exceeded"),
      );
    const start = performance.now();
    const work = (async () => {
      try {
        const result = await callback();
        const cache = this.#cache.metrics();
        this.#observe({
          type: "operation",
          operation,
          outcome: "success",
          elapsedMs: performance.now() - start,
          counters: Object.freeze({
            managedResidentBytes: this.#admission.usedBytes,
            peakManagedResidentBytes: this.#admission.peakBytes,
            cacheBytes: cache.bytes,
            cacheHits: cache.hits,
            cacheMisses: cache.misses,
            cacheEvictions: cache.evictions,
          }),
        });
        return result;
      } catch (error) {
        const mapped =
          error instanceof FilesystemError ||
          (error instanceof DOMException && error.name === "AbortError")
            ? error
            : (() => {
                try {
                  mapStorageError(error, operation, path);
                } catch (value) {
                  return value;
                }
              })();
        const code = mapped instanceof FilesystemError ? mapped.code : undefined;
        const cache = this.#cache.metrics();
        this.#observe({
          type: "operation",
          operation,
          outcome: "error",
          elapsedMs: performance.now() - start,
          counters: Object.freeze({
            managedResidentBytes: this.#admission.usedBytes,
            peakManagedResidentBytes: this.#admission.peakBytes,
            cacheBytes: cache.bytes,
            cacheHits: cache.hits,
            cacheMisses: cache.misses,
            cacheEvictions: cache.evictions,
          }),
          ...(code === undefined ? {} : { errorCode: code }),
        });
        throw mapped;
      }
    })();
    this.#pending.add(work);
    void work
      .finally(() => {
        releaseOperation();
        this.#pending.delete(work);
      })
      .catch(() => {});
    return work;
  }
  #observe(event: FilesystemObservation): void {
    try {
      this.#observer?.(Object.freeze(event));
    } catch {}
  }
}

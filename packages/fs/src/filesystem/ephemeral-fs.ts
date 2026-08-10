import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SqliteRow, TransactionMode } from "../sqlite-driver.js";
import { DEFAULT_BRANCH_CONFIGURATION, DEFAULT_FILESYSTEM_LIMITS, DEFAULT_RUNTIME_LIMITS, AdmissionController, constrainStorageLimits, resolveLimits, type BranchConfiguration, type FilesystemLimits, type RuntimeLimits, type StorageLimits } from "../resources/limits.js";
import { initializeOrValidateSchema } from "../sqlite/schema.js";
import { runUnitOfWork } from "../sqlite/unit-of-work.js";
import { ContentRepository } from "../sqlite/content-repository.js";
import { NamespaceRepository, type InodeRow, type ResolvedPath } from "../namespace/repository.js";
import { canonicalizePath, compareUtf8, validateName, validateSymlinkTarget } from "../namespace/paths.js";
import { checkedAdd, checkedInteger, utf8 } from "../utils/bytes.js";
import { prepareContent, readManifestRange } from "../operations/manifest-io.js";
import { abortError, FilesystemError, fsError, mapStorageError } from "./errors.js";
import type { DirectoryEntry, EphemeralFilesystem, FileContent, FileStat, FileType, FilesystemCapabilities, FilesystemObservation, FilesystemObserver, MkdirOptions, OpenFilesystemOptions, ReadRangeOptions, ReadStreamOptions, ReadTextOptions, ReaddirOptions, RmOptions, WriteFileOptions } from "./types.js";
import { BranchManager } from "../branches/branch-engine.js";
import type { Branches } from "../branches/types.js";

interface ChildRow extends SqliteRow { name: string; name_sort: Uint8Array; type: number }
interface CountRow extends SqliteRow { count: number }

function inodeType(value: number): FileType { if (value === 0) return "file"; if (value === 1) return "directory"; if (value === 2) return "symlink"; throw new Error("ECORRUPT: invalid inode type"); }
function predicates(type: FileType) { return { isFile: () => type === "file", isDirectory: () => type === "directory", isSymbolicLink: () => type === "symlink" }; }
function fileStat(inode: InodeRow, name: string): FileStat {
  const type = inodeType(inode.type); const size = type === "file" ? inode.size : type === "symlink" ? utf8(inode.symlink_target ?? "").byteLength : 0;
  if (typeof size !== "number") throw new Error("ECORRUPT: file size is missing");
  return Object.freeze({ id: inode.id, name, type, mode: inode.mode, size, nlink: inode.nlink, mtimeMs: inode.mtime_ms, ctimeMs: inode.ctime_ms, birthtimeMs: inode.birthtime_ms, ...predicates(type) });
}
function directoryEntry(name: string, parentPath: string, type: FileType): DirectoryEntry { return Object.freeze({ name, parentPath, type, ...predicates(type) }); }
function validatedMode(mode: number | undefined, fallback: number, syscall: string, path: string): number { const value = mode ?? fallback; if (!Number.isSafeInteger(value) || value < 0) throw fsError("EINVAL", syscall, path, "mode must be a nonnegative safe integer"); return value & 0o7777; }

export class EphemeralFS implements EphemeralFilesystem {
  readonly capabilities: FilesystemCapabilities;
  readonly branches: Branches;
  readonly #database: FilesystemSQLiteDriver; readonly #clock: () => number; readonly #observer: FilesystemObserver | undefined; readonly #ownsDatabase: boolean;
  readonly #filesystemLimits: FilesystemLimits; readonly #storageLimits: StorageLimits; readonly #runtimeLimits: RuntimeLimits; readonly #branchLimits: BranchConfiguration;
  readonly #admission: AdmissionController; readonly #pending = new Set<Promise<unknown>>(); readonly #streams = new Map<string, { release: () => Promise<void>; error: () => void }>();
  #closing = false; #closed = false; #closePromise?: Promise<void>;

  private constructor(options: OpenFilesystemOptions, capabilities: FilesystemCapabilities) {
    this.#database = options.database; this.#clock = options.clock ?? Date.now; this.#observer = options.observer; this.#ownsDatabase = options.ownsDatabase ?? false;
    this.capabilities = capabilities; this.#filesystemLimits = capabilities.filesystem; this.#storageLimits = capabilities.storage; this.#runtimeLimits = capabilities.runtime; this.#branchLimits = capabilities.branch;
    this.#admission = new AdmissionController(this.#runtimeLimits.maxManagedResidentBytes);
    this.branches = new BranchManager(this.#database, this.#filesystemLimits, this.#storageLimits, this.#runtimeLimits, this.#branchLimits, this.#clock, this.#admission);
  }

  static async open(options: OpenFilesystemOptions): Promise<EphemeralFS> {
    const filesystem = resolveLimits(DEFAULT_FILESYSTEM_LIMITS, options.filesystem); const runtime = resolveLimits(DEFAULT_RUNTIME_LIMITS, options.runtime); const branch = resolveLimits(DEFAULT_BRANCH_CONFIGURATION, options.branch);
    const storage = constrainStorageLimits(options.storage, options.database.capabilities);
    const metadata = initializeOrValidateSchema(options.database, { ...(options.format?.cowPageBytes === undefined ? {} : { cowPageBytes: options.format.cowPageBytes }), now: (options.clock ?? Date.now)() });
    const format = Object.freeze({ cowPageBytes: metadata.cowPageBytes, hashAlgorithm: "sha256" as const, chunkerAlgorithm: "fastcdc-v1" as const, manifestFormat: "efs-merkle-manifest-v1" as const });
    const effectiveLimits = Object.freeze([
      ...Object.entries(filesystem).map(([name, value]) => Object.freeze({ domain: "filesystem" as const, name, value, scope: "persisted" as const, constrainedBy: "configuration" as const })),
      ...Object.entries(storage).map(([name, value]) => Object.freeze({ domain: "storage" as const, name, value, scope: "persisted" as const, constrainedBy: name === "maxPhysicalDatabaseBytes" || name === "maxJournalBytes" ? "adapter" as const : "configuration" as const })),
      ...Object.entries(runtime).map(([name, value]) => Object.freeze({ domain: "runtime" as const, name, value, scope: "runtime" as const, constrainedBy: "configuration" as const })),
    ]);
    return new EphemeralFS(options, Object.freeze({ adapter: options.database.capabilities, filesystem, storage, branch, runtime, format, effectiveLimits, readOnly: options.database.readOnly }));
  }

  readFile(path: string): Promise<Uint8Array>;
  readFile(path: string, options: ReadTextOptions): Promise<string>;
  readFile(path: string, options?: ReadTextOptions): Promise<Uint8Array | string> {
    return this.#operation("readFile", path, undefined, async () => {
      if (options !== undefined && options.encoding !== "utf8") throw fsError("EINVAL", "readFile", path, "unsupported encoding");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readFile");
      const bytes = this.#transaction("read", (tx) => {
        const inode = this.#requireFile(new NamespaceRepository(tx, this.#filesystemLimits, "readFile").resolve(canonical, true), "readFile");
        if (inode.size! > this.#filesystemLimits.maxMaterializedBytes) throw fsError("EFBIG", "readFile", canonical.value, "file exceeds complete materialization limit");
        return readManifestRange(new ContentRepository(tx, this.#storageLimits), inode.manifest_hash!, 0, inode.size!);
      });
      return options ? new TextDecoder("utf-8", { fatal: false }).decode(bytes) : bytes;
    });
  }

  readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array> {
    return this.#operation("readRange", path, undefined, async () => {
      checkedInteger(options?.offset, "offset"); checkedInteger(options?.length, "length", this.#filesystemLimits.maxMaterializedBytes);
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readRange");
      return this.#transaction("read", (tx) => { const inode = this.#requireFile(new NamespaceRepository(tx, this.#filesystemLimits, "readRange").resolve(canonical, true), "readRange"); return readManifestRange(new ContentRepository(tx, this.#storageLimits), inode.manifest_hash!, options.offset, options.length); });
    });
  }

  readStream(path: string, options: ReadStreamOptions = {}): Promise<ReadableStream<Uint8Array>> {
    return this.#operation("readStream", path, options.signal, async () => {
      if (this.#database.readOnly) throw fsError("EROFS", "readStream", path, "durable stream leases require writable storage");
      const offset = options.offset ?? 0; const requestedLength = options.length; checkedInteger(offset, "offset"); if (requestedLength !== undefined) checkedInteger(requestedLength, "length");
      if (this.#streams.size >= this.#runtimeLimits.maxConcurrentStreams) throw fsError("EAGAIN", "readStream", path, "concurrent stream limit exceeded");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readStream");
      const selected = this.#transaction("read", (tx) => { const inode = this.#requireFile(new NamespaceRepository(tx, this.#filesystemLimits, "readStream").resolve(canonical, true), "readStream"); return { manifestHash: inode.manifest_hash!.slice(), size: inode.size! }; });
      const end = Math.min(selected.size, requestedLength === undefined ? selected.size : checkedAdd(offset, requestedLength)); const leaseId = globalThis.crypto.randomUUID(); const owner = globalThis.crypto.randomUUID();
      this.#transaction("write", (tx) => {
        const expires = this.#now() + this.#storageLimits.readLeaseMs; tx.run("INSERT INTO efs_leases(id,kind,owner_id,expires_at_ms,state) VALUES(?,0,?,?,1)", [leaseId, owner, expires]); tx.run("INSERT INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)", [leaseId, selected.manifestHash]); this.#bumpRoot(tx, 2, leaseId);
      });
      let position = Math.min(offset, selected.size); let released = false; let queuedRelease: (() => void) | undefined; let controllerReference: ReadableStreamDefaultController<Uint8Array> | undefined;
      const release = async (): Promise<void> => {
        if (released) return; released = true; queuedRelease?.(); queuedRelease = undefined; this.#streams.delete(leaseId);
        try { this.#transaction("write", (tx) => { const result = tx.run("DELETE FROM efs_leases WHERE id=? AND owner_id=?", [leaseId, owner]); if (result.changes) this.#bumpRoot(tx, 3, leaseId); }); } catch (error) { if (!this.#closing) throw error; }
      };
      const stream = new ReadableStream<Uint8Array>({
        start(controller) { controllerReference = controller; },
        pull: async (controller) => {
          queuedRelease?.(); queuedRelease = undefined;
          if (this.#closing || options.signal?.aborted) { await release(); controller.error(options.signal?.aborted ? abortError() : fsError("EBADF", "readStream", canonical.value, "filesystem is closing")); return; }
          if (position >= end) { await release(); controller.close(); return; }
          const length = Math.min(this.#filesystemLimits.preferredStreamChunkBytes, end - position); const free = this.#admission.reserve(length);
          try { const bytes = this.#transaction("read", (tx) => readManifestRange(new ContentRepository(tx, this.#storageLimits), selected.manifestHash, position, length)); position += bytes.byteLength; controller.enqueue(bytes); queuedRelease = free; } catch (error) { free(); await release(); controller.error(error); }
        },
        cancel: async () => { await release(); },
      }, { highWaterMark: 1, size: (chunk) => chunk.byteLength });
      this.#streams.set(leaseId, { release, error: () => { try { controllerReference?.error(fsError("EBADF", "readStream", canonical.value, "filesystem is closing")); } catch {} } });
      return stream;
    });
  }

  writeFile(path: string, content: FileContent, options: WriteFileOptions = {}): Promise<void> {
    const frozen: Uint8Array | ReadableStream<Uint8Array> = typeof content === "string" ? new TextEncoder().encode(content) : content instanceof Uint8Array ? content.slice() : content;
    return this.#operation("writeFile", path, options.signal, async () => {
      if (!(frozen instanceof Uint8Array) && !(frozen && typeof frozen.getReader === "function")) throw fsError("EINVAL", "writeFile", path, "content must be string, Uint8Array, or ReadableStream");
      if (frozen instanceof Uint8Array && frozen.byteLength > this.#storageLimits.maxWriteBytes) throw fsError("EFBIG", "writeFile", path, "buffered write exceeds maxWriteBytes");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "writeFile"); if (canonical.value === "/") throw fsError("EISDIR", "writeFile", canonical.value, "root is a directory");
      if (options.exclusive) { const exists = this.#transaction("read", (tx) => new NamespaceRepository(tx, this.#filesystemLimits, "writeFile").resolveOptional(canonical, false)); if (exists) throw fsError("EEXIST", "writeFile", canonical.value, "destination exists"); }
      const prepared = await prepareContent(this.#database, frozen, this.#storageLimits, this.#runtimeLimits, this.#admission, options.signal);
      this.#transaction("write", (tx) => {
        const ns = new NamespaceRepository(tx, this.#filesystemLimits, "writeFile"); const raw = ns.resolveOptional(canonical, false); let existing = raw;
        if (options.exclusive && raw) throw fsError("EEXIST", "writeFile", canonical.value, "destination exists");
        if (raw?.inode.type === 2 && !options.exclusive) existing = ns.resolve(canonical, true);
        if (existing?.inode.type === 1) throw fsError("EISDIR", "writeFile", canonical.value, "destination is a directory");
        if (existing && existing.inode.type !== 0) throw fsError("ENOENT", "writeFile", canonical.value, "symbolic link target is not a regular file");
        const now = this.#now(); const revision = ns.nextRevision(now, existing ? 1 : 2);
        if (existing) {
          const time = Math.max(now, existing.inode.mtime_ms, existing.inode.ctime_ms);
          tx.run("UPDATE efs_inodes SET size=?,manifest_hash=?,mtime_ms=?,ctime_ms=?,token=? WHERE id=?", [prepared.size, prepared.hash, time, time, revision, existing.inode.id]); ns.recordInode(revision, existing.inode.id);
        } else {
          const { parent, name, nameSort } = ns.resolveParent(canonical); const inodeId = globalThis.crypto.randomUUID(); const mode = validatedMode(options.mode, 0o666, "writeFile", canonical.value);
          tx.run("INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,0,?,?,?,?,1,?,?,NULL,?)", [inodeId, mode, now, now, now, prepared.size, prepared.hash, revision]); ns.putEntry(parent.inode.id, nameSort, name, inodeId, revision); ns.recordInode(revision, inodeId); ns.recordEntry(revision, parent.inode.id, nameSort); this.#touchParent(tx, ns, parent.inode, now, revision);
        }
      });
    });
  }

  writeRange(path: string, offset: number, content: Uint8Array): Promise<void> {
    const frozen = content.slice();
    return this.#operation("writeRange", path, undefined, async () => {
      checkedInteger(offset, "offset"); if (frozen.byteLength > this.#storageLimits.maxWriteBytes) throw fsError("EFBIG", "writeRange", path, "write exceeds maxWriteBytes");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "writeRange"); const selected = this.#readCompleteForMutation(canonical.value, "writeRange"); if (!frozen.byteLength) return;
      const size = Math.max(selected.bytes.byteLength, checkedAdd(offset, frozen.byteLength)); if (size > this.#storageLimits.maxFileBytes || size > this.#runtimeLimits.maxManagedResidentBytes) throw fsError("EFBIG", "writeRange", canonical.value, "result is too large for bounded materialization");
      const bytes = new Uint8Array(size); bytes.set(selected.bytes); bytes.set(frozen, offset); await this.#replaceExisting(canonical.value, bytes, selected.token, "writeRange");
    });
  }

  replaceRange(path: string, offset: number, deleteLength: number, insertBytes: Uint8Array): Promise<void> {
    const frozen = insertBytes.slice();
    return this.#operation("replaceRange", path, undefined, async () => {
      checkedInteger(offset, "offset"); checkedInteger(deleteLength, "deleteLength"); const canonical = canonicalizePath(path, this.#filesystemLimits, "replaceRange"); const selected = this.#readCompleteForMutation(canonical.value, "replaceRange");
      if (offset > selected.bytes.byteLength || deleteLength > selected.bytes.byteLength - offset) throw fsError("EINVAL", "replaceRange", canonical.value, "replacement range is outside file"); if (!deleteLength && !frozen.byteLength) return;
      const finalSize = selected.bytes.byteLength - deleteLength + frozen.byteLength; if (finalSize > this.#storageLimits.maxFileBytes || finalSize > this.#runtimeLimits.maxManagedResidentBytes) throw fsError("EFBIG", "replaceRange", canonical.value, "result is too large for bounded materialization");
      const bytes = new Uint8Array(finalSize); bytes.set(selected.bytes.subarray(0, offset)); bytes.set(frozen, offset); bytes.set(selected.bytes.subarray(offset + deleteLength), offset + frozen.byteLength); await this.#replaceExisting(canonical.value, bytes, selected.token, "replaceRange");
    });
  }

  truncate(path: string, size = 0): Promise<void> {
    return this.#operation("truncate", path, undefined, async () => {
      checkedInteger(size, "size", this.#storageLimits.maxFileBytes); const canonical = canonicalizePath(path, this.#filesystemLimits, "truncate"); const selected = this.#readCompleteForMutation(canonical.value, "truncate"); if (size === selected.bytes.byteLength) return;
      if (size > this.#runtimeLimits.maxManagedResidentBytes) throw fsError("EFBIG", "truncate", canonical.value, "result is too large for bounded materialization"); const bytes = new Uint8Array(size); bytes.set(selected.bytes.subarray(0, size)); await this.#replaceExisting(canonical.value, bytes, selected.token, "truncate");
    });
  }

  mkdir(path: string, options: MkdirOptions = {}): Promise<void> {
    return this.#operation("mkdir", path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, "mkdir"); const mode = validatedMode(options.mode, 0o777, "mkdir", canonical.value);
      if (canonical.value === "/") { if (options.recursive) return; throw fsError("EEXIST", "mkdir", canonical.value, "root already exists"); }
      this.#transaction("write", (tx) => {
        const ns = new NamespaceRepository(tx, this.#filesystemLimits, "mkdir"); const existing = ns.resolveOptional(canonical, false);
        if (existing) { if (options.recursive && existing.inode.type === 1) return; throw fsError("EEXIST", "mkdir", canonical.value, "destination exists"); }
        if (!options.recursive) { const parent = ns.resolveParent(canonical); const now = this.#now(); const revision = ns.nextRevision(now, 2); this.#createDirectory(tx, ns, parent.parent.inode, parent.name, parent.nameSort, mode, now, revision); return; }
        const missing: { parent: InodeRow; name: string; nameSort: Uint8Array }[] = []; let parent = ns.resolve("/").inode;
        for (let index = 0; index < canonical.segments.length; index += 1) {
          const name = canonical.segments[index]!; const nameSort = canonical.encodedSegments[index]!; const entry = ns.entry(parent.id, nameSort);
          if (entry?.inode_id) { const inode = ns.inode(entry.inode_id); if (!inode) throw new Error("ECORRUPT: missing inode"); if (inode.type !== 1) throw fsError(index === canonical.segments.length - 1 ? "EEXIST" : "ENOTDIR", "mkdir", canonical.value, "path component is not a directory"); parent = inode; }
          else { missing.push({ parent, name, nameSort }); const placeholder = { ...parent, id: globalThis.crypto.randomUUID(), type: 1, mode, birthtime_ms: 0, mtime_ms: 0, ctime_ms: 0, nlink: 1, size: null, manifest_hash: null, symlink_target: null, token: 0 } as InodeRow; parent = placeholder; }
        }
        if (missing.length > this.#filesystemLimits.maxAtomicTreeEntries) throw fsError("EFBIG", "mkdir", canonical.value, "recursive create exceeds atomic tree limit");
        const now = this.#now(); const revision = ns.nextRevision(now, missing.length * 2); let actualParent = missing[0]!.parent;
        for (const item of missing) { const id = parent.id === item.parent.id ? globalThis.crypto.randomUUID() : (item === missing.at(-1) ? parent.id : globalThis.crypto.randomUUID()); const inode = this.#createDirectory(tx, ns, actualParent, item.name, item.nameSort, mode, now, revision, id); actualParent = inode; }
      });
    });
  }

  readdir(path: string, options: ReaddirOptions = {}): Promise<DirectoryEntry[]> {
    return this.#operation("readdir", path, undefined, async () => {
      if (options.limit !== undefined) checkedInteger(options.limit, "limit", this.#filesystemLimits.maxReaddirEntries); const start = options.startAfter === undefined ? undefined : validateName(options.startAfter, this.#filesystemLimits, "readdir");
      const canonical = canonicalizePath(path, this.#filesystemLimits, "readdir"); return this.#transaction("read", (tx) => {
        const selected = new NamespaceRepository(tx, this.#filesystemLimits, "readdir").resolve(canonical, true); if (selected.inode.type !== 1) throw fsError("ENOTDIR", "readdir", canonical.value, "path is not a directory");
        const limit = options.limit ?? this.#filesystemLimits.maxReaddirEntries; if (limit === 0) return [];
        const bindings = start ? [selected.inode.id, start, limit + 1] as const : [selected.inode.id, limit + 1] as const;
        const sql = start ? "SELECT e.name,e.name_sort,i.type FROM efs_entries e JOIN efs_inodes i ON i.id=e.inode_id WHERE e.parent_inode=? AND e.inode_id IS NOT NULL AND e.name_sort>? ORDER BY e.name_sort LIMIT ?" : "SELECT e.name,e.name_sort,i.type FROM efs_entries e JOIN efs_inodes i ON i.id=e.inode_id WHERE e.parent_inode=? AND e.inode_id IS NOT NULL ORDER BY e.name_sort LIMIT ?";
        const rows = tx.all<ChildRow>(sql, bindings, { maxRows: limit + 1 || 1, maxBytes: this.#runtimeLimits.maxQueryBatchBytes }); if (rows.length > limit) throw fsError("EFBIG", "readdir", canonical.value, "directory listing exceeds configured limit");
        return rows.map((row) => directoryEntry(row.name, canonical.value, inodeType(row.type)));
      });
    });
  }

  stat(path: string): Promise<FileStat> { return this.#stat(path, true, "stat"); }
  lstat(path: string): Promise<FileStat> { return this.#stat(path, false, "lstat"); }

  chmod(path: string, mode: number): Promise<void> {
    return this.#operation("chmod", path, undefined, async () => {
      const canonical = canonicalizePath(path, this.#filesystemLimits, "chmod"); const normalized = validatedMode(mode, 0, "chmod", canonical.value);
      this.#transaction("write", (tx) => { const ns = new NamespaceRepository(tx, this.#filesystemLimits, "chmod"); const selected = ns.resolve(canonical, true); if (selected.inode.mode === normalized) return; const now = Math.max(this.#now(), selected.inode.ctime_ms); const revision = ns.nextRevision(now, 1); tx.run("UPDATE efs_inodes SET mode=?,ctime_ms=?,token=? WHERE id=?", [normalized, now, revision, selected.inode.id]); ns.recordInode(revision, selected.inode.id); });
    });
  }

  link(existingPath: string, newPath: string): Promise<void> {
    return this.#operation("link", existingPath, undefined, async () => {
      const sourcePath = canonicalizePath(existingPath, this.#filesystemLimits, "link"); const destination = canonicalizePath(newPath, this.#filesystemLimits, "link"); if (destination.value === "/") throw fsError("EPERM", "link", destination.value, "root cannot be a hard-link destination");
      this.#transaction("write", (tx) => { const ns = new NamespaceRepository(tx, this.#filesystemLimits, "link"); const source = ns.resolve(sourcePath, true); if (source.inode.type !== 0) throw fsError("EPERM", "link", sourcePath.value, "only regular files can be hard linked"); if (ns.resolveOptional(destination, false)) throw fsError("EEXIST", "link", destination.value, "destination exists"); const parent = ns.resolveParent(destination); const now = this.#now(); const revision = ns.nextRevision(now, 3); ns.putEntry(parent.parent.inode.id, parent.nameSort, parent.name, source.inode.id, revision); tx.run("UPDATE efs_inodes SET nlink=nlink+1,ctime_ms=max(ctime_ms,?),token=? WHERE id=?", [now, revision, source.inode.id]); ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort); ns.recordInode(revision, source.inode.id); this.#touchParent(tx, ns, parent.parent.inode, now, revision); });
    });
  }

  symlink(target: string, path: string): Promise<void> {
    return this.#operation("symlink", path, undefined, async () => {
      validateSymlinkTarget(target, this.#filesystemLimits, "symlink"); const canonical = canonicalizePath(path, this.#filesystemLimits, "symlink"); if (canonical.value === "/") throw fsError("EPERM", "symlink", canonical.value, "root cannot be a symlink destination");
      this.#transaction("write", (tx) => { const ns = new NamespaceRepository(tx, this.#filesystemLimits, "symlink"); if (ns.resolveOptional(canonical, false)) throw fsError("EEXIST", "symlink", canonical.value, "destination exists"); const parent = ns.resolveParent(canonical); const now = this.#now(); const revision = ns.nextRevision(now, 3); const id = globalThis.crypto.randomUUID(); tx.run("INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,2,511,?,?,?,1,NULL,NULL,?,?)", [id, now, now, now, target, revision]); ns.putEntry(parent.parent.inode.id, parent.nameSort, parent.name, id, revision); ns.recordInode(revision, id); ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort); this.#touchParent(tx, ns, parent.parent.inode, now, revision); });
    });
  }

  readlink(path: string): Promise<string> {
    return this.#operation("readlink", path, undefined, async () => { const canonical = canonicalizePath(path, this.#filesystemLimits, "readlink"); return this.#transaction("read", (tx) => { const selected = new NamespaceRepository(tx, this.#filesystemLimits, "readlink").resolve(canonical, false); if (selected.inode.type !== 2 || selected.inode.symlink_target === null) throw fsError("EINVAL", "readlink", canonical.value, "path is not a symbolic link"); return selected.inode.symlink_target; }); });
  }

  rename(oldPath: string, newPath: string): Promise<void> {
    return this.#operation("rename", oldPath, undefined, async () => {
      const sourcePath = canonicalizePath(oldPath, this.#filesystemLimits, "rename"); const destinationPath = canonicalizePath(newPath, this.#filesystemLimits, "rename"); if (sourcePath.value === "/" || destinationPath.value === "/") throw fsError("EPERM", "rename", sourcePath.value, "root cannot be renamed or replaced"); if (sourcePath.value === destinationPath.value) return;
      if (destinationPath.value.startsWith(`${sourcePath.value}/`)) throw fsError("EINVAL", "rename", sourcePath.value, "directory cannot be moved into itself");
      this.#transaction("write", (tx) => { const ns = new NamespaceRepository(tx, this.#filesystemLimits, "rename"); const source = ns.resolve(sourcePath, false); const destination = ns.resolveOptional(destinationPath, false); const parent = ns.resolveParent(destinationPath);
        if (destination) { if (source.inode.type === 1 && destination.inode.type !== 1) throw fsError("ENOTDIR", "rename", destinationPath.value, "cannot replace non-directory with directory"); if (source.inode.type !== 1 && destination.inode.type === 1) throw fsError("EISDIR", "rename", destinationPath.value, "cannot replace directory with non-directory"); if (destination.inode.type === 1 && this.#childCount(tx, destination.inode.id) > 0) throw fsError("ENOTEMPTY", "rename", destinationPath.value, "destination directory is not empty"); }
        const now = this.#now(); const revision = ns.nextRevision(now, 5); ns.putEntry(source.parentInode!, source.nameSort!, null, null, revision); ns.recordEntry(revision, source.parentInode!, source.nameSort!, true);
        if (destination?.inode.id === source.inode.id) { tx.run("UPDATE efs_inodes SET nlink=nlink-1,ctime_ms=max(ctime_ms,?),token=? WHERE id=?", [now, revision, source.inode.id]); ns.recordInode(revision, source.inode.id); }
        else { if (destination) this.#removeDestination(tx, ns, destination, now, revision); ns.putEntry(parent.parent.inode.id, parent.nameSort, parent.name, source.inode.id, revision); ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort); }
        const sourceParent = ns.inode(source.parentInode!); if (sourceParent) this.#touchParent(tx, ns, sourceParent, now, revision); if (parent.parent.inode.id !== source.parentInode) this.#touchParent(tx, ns, parent.parent.inode, now, revision);
      });
    });
  }

  unlink(path: string): Promise<void> { return this.#remove(path, false, false, "unlink", true); }
  rm(path: string, options: RmOptions = {}): Promise<void> { return this.#remove(path, options.recursive ?? false, options.force ?? false, "rm", false); }

  close(): Promise<void> {
    if (this.#closePromise) return this.#closePromise; this.#closing = true;
    this.#closePromise = (async () => {
      for (const stream of this.#streams.values()) stream.error(); await Promise.allSettled([...this.#streams.values()].map((stream) => stream.release())); await Promise.allSettled([...this.#pending]);
      try { if (this.#ownsDatabase) await this.#database.close(); } finally { this.#closed = true; }
    })(); return this.#closePromise;
  }

  async [Symbol.asyncDispose](): Promise<void> { await this.close(); }

  #stat(path: string, followFinal: boolean, syscall: string): Promise<FileStat> { return this.#operation(syscall, path, undefined, async () => { const canonical = canonicalizePath(path, this.#filesystemLimits, syscall); return this.#transaction("read", (tx) => { const selected = new NamespaceRepository(tx, this.#filesystemLimits, syscall).resolve(canonical, followFinal); return fileStat(selected.inode, canonical.segments.at(-1) ?? ""); }); }); }
  #readCompleteForMutation(path: string, syscall: string): { bytes: Uint8Array; token: number } { return this.#transaction("read", (tx) => { const selected = new NamespaceRepository(tx, this.#filesystemLimits, syscall).resolve(path, true); const inode = this.#requireFile(selected, syscall); if (inode.size! > this.#runtimeLimits.maxManagedResidentBytes) throw fsError("EFBIG", syscall, path, "file exceeds bounded mutation memory"); return { bytes: readManifestRange(new ContentRepository(tx, this.#storageLimits), inode.manifest_hash!, 0, inode.size!), token: inode.token }; }); }
  async #replaceExisting(path: string, bytes: Uint8Array, expectedToken: number, syscall: string): Promise<void> { const prepared = await prepareContent(this.#database, bytes, this.#storageLimits, this.#runtimeLimits, this.#admission); this.#transaction("write", (tx) => { const ns = new NamespaceRepository(tx, this.#filesystemLimits, syscall); const selected = ns.resolve(path, true); const inode = this.#requireFile(selected, syscall); if (inode.token !== expectedToken) throw fsError("EAGAIN", syscall, path, "file changed while content was prepared"); const now = Math.max(this.#now(), inode.mtime_ms, inode.ctime_ms); const revision = ns.nextRevision(now, 1); tx.run("UPDATE efs_inodes SET size=?,manifest_hash=?,mtime_ms=?,ctime_ms=?,token=? WHERE id=? AND token=?", [prepared.size, prepared.hash, now, now, revision, inode.id, expectedToken]); ns.recordInode(revision, inode.id); }); }
  #requireFile(selected: ResolvedPath, syscall: string): InodeRow { if (selected.inode.type === 1) throw fsError("EISDIR", syscall, selected.path.value, "path is a directory"); if (selected.inode.type !== 0 || selected.inode.manifest_hash === null || selected.inode.size === null) throw fsError("EINVAL", syscall, selected.path.value, "path is not a regular file"); return selected.inode; }
  #createDirectory(tx: FilesystemSQLiteTransaction, ns: NamespaceRepository, parent: InodeRow, name: string, nameSort: Uint8Array, mode: number, now: number, revision: number, id: string = globalThis.crypto.randomUUID()): InodeRow { tx.run("INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,1,?,?,?,?,1,NULL,NULL,NULL,?)", [id, mode, now, now, now, revision]); ns.putEntry(parent.id, nameSort, name, id, revision); ns.recordInode(revision, id); ns.recordEntry(revision, parent.id, nameSort); this.#touchParent(tx, ns, parent, now, revision); return { id, type: 1, mode, birthtime_ms: now, mtime_ms: now, ctime_ms: now, nlink: 1, size: null, manifest_hash: null, symlink_target: null, token: revision }; }
  #touchParent(tx: FilesystemSQLiteTransaction, ns: NamespaceRepository, parent: InodeRow, now: number, revision: number): void { const time = Math.max(now, parent.mtime_ms, parent.ctime_ms); tx.run("UPDATE efs_inodes SET mtime_ms=?,ctime_ms=?,token=? WHERE id=?", [time, time, revision, parent.id]); ns.recordInode(revision, parent.id); }
  #childCount(tx: FilesystemSQLiteTransaction, inodeId: string): number { return tx.all<CountRow>("SELECT count(*) count FROM efs_entries WHERE parent_inode=? AND inode_id IS NOT NULL", [inodeId], { maxRows: 1, maxBytes: 1024 })[0]?.count ?? 0; }
  #removeDestination(tx: FilesystemSQLiteTransaction, ns: NamespaceRepository, selected: ResolvedPath, now: number, revision: number): void { ns.putEntry(selected.parentInode!, selected.nameSort!, null, null, revision); ns.recordEntry(revision, selected.parentInode!, selected.nameSort!, true); if (selected.inode.type === 0 && selected.inode.nlink > 1) { tx.run("UPDATE efs_inodes SET nlink=nlink-1,ctime_ms=max(ctime_ms,?),token=? WHERE id=?", [now, revision, selected.inode.id]); ns.recordInode(revision, selected.inode.id); } else { if (selected.inode.type === 1) tx.run("DELETE FROM efs_entries WHERE parent_inode=?", [selected.inode.id]); tx.run("DELETE FROM efs_inodes WHERE id=?", [selected.inode.id]); ns.recordInode(revision, selected.inode.id, true); } }
  #remove(path: string, recursive: boolean, force: boolean, syscall: string, filesOnly: boolean): Promise<void> { return this.#operation(syscall, path, undefined, async () => { const canonical = canonicalizePath(path, this.#filesystemLimits, syscall); if (canonical.value === "/") throw fsError("EPERM", syscall, canonical.value, "root cannot be removed"); this.#transaction("write", (tx) => { const ns = new NamespaceRepository(tx, this.#filesystemLimits, syscall); const selected = ns.resolveOptional(canonical, false); if (!selected) { if (force) return; throw fsError("ENOENT", syscall, canonical.value, "path does not exist"); } if (filesOnly && selected.inode.type === 1) throw fsError("EISDIR", syscall, canonical.value, "unlink cannot remove a directory"); const children = selected.inode.type === 1 ? this.#collectTree(tx, ns, selected.inode.id) : []; if (children.length && !recursive) throw fsError("ENOTEMPTY", syscall, canonical.value, "directory is not empty"); if (children.length + 1 > this.#filesystemLimits.maxAtomicTreeEntries) throw fsError("EFBIG", syscall, canonical.value, "recursive removal exceeds atomic tree limit"); const now = this.#now(); const revision = ns.nextRevision(now, children.length * 2 + 3); for (const child of children.reverse()) this.#removeDestination(tx, ns, child, now, revision); this.#removeDestination(tx, ns, selected, now, revision); const parent = ns.inode(selected.parentInode!); if (parent) this.#touchParent(tx, ns, parent, now, revision); }); }); }
  #collectTree(tx: FilesystemSQLiteTransaction, ns: NamespaceRepository, rootId: string): ResolvedPath[] { const result: ResolvedPath[] = []; const stack = [rootId]; while (stack.length) { const parentId = stack.pop()!; const rows = tx.all<ChildRow & { inode_id: string; token: number }>("SELECT e.name,e.name_sort,e.inode_id,e.token,i.type FROM efs_entries e JOIN efs_inodes i ON i.id=e.inode_id WHERE e.parent_inode=? AND e.inode_id IS NOT NULL ORDER BY e.name_sort", [parentId], { maxRows: this.#filesystemLimits.maxAtomicTreeEntries + 1, maxBytes: this.#runtimeLimits.maxQueryBatchBytes }); for (const row of rows) { const inode = ns.inode(row.inode_id); if (!inode) throw new Error("ECORRUPT: missing descendant inode"); result.push({ path: canonicalizePath(`/${row.name}`, this.#filesystemLimits, "rm"), inode, parentInode: parentId, name: row.name, nameSort: row.name_sort, entryToken: row.token }); if (inode.type === 1) stack.push(inode.id); } if (result.length > this.#filesystemLimits.maxAtomicTreeEntries) break; } return result; }
  #bumpRoot(tx: FilesystemSQLiteTransaction, kind: number, id: string): void { const ns = new NamespaceRepository(tx, this.#filesystemLimits, "maintenance"); const generation = ns.meta().root_mutation_generation + 1; tx.run("UPDATE efs_meta SET root_mutation_generation=? WHERE singleton=1", [generation]); tx.run("INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)", [generation, kind, utf8(id)]); }
  #now(): number { const value = this.#clock(); if (!Number.isSafeInteger(value) || value < 0) throw new Error("clock must return a nonnegative safe integer"); return value; }
  #transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T { return runUnitOfWork(this.#database, mode, { maxRows: this.#storageLimits.maxFinalTransactionRows, maxBytes: this.#storageLimits.maxFinalTransactionBytes }, callback); }
  #operation<T>(operation: string, path: string | undefined, signal: AbortSignal | undefined, callback: () => Promise<T>): Promise<T> {
    if (this.#closing || this.#closed) return Promise.reject(fsError("EBADF", operation, path, "filesystem is closed or closing")); if (signal?.aborted) return Promise.reject(abortError()); if (this.#pending.size >= this.#runtimeLimits.maxConcurrentOperations) return Promise.reject(fsError("EAGAIN", operation, path, "concurrent operation limit exceeded"));
    const start = performance.now(); const work = (async () => { try { const result = await callback(); this.#observe({ type: "operation", operation, outcome: "success", elapsedMs: performance.now() - start, counters: Object.freeze({ managedResidentBytes: this.#admission.usedBytes, peakManagedResidentBytes: this.#admission.peakBytes }) }); return result; } catch (error) { const mapped = error instanceof FilesystemError || (error instanceof DOMException && error.name === "AbortError") ? error : (() => { try { mapStorageError(error, operation, path); } catch (value) { return value; } })(); const code = mapped instanceof FilesystemError ? mapped.code : undefined; this.#observe({ type: "operation", operation, outcome: "error", elapsedMs: performance.now() - start, counters: Object.freeze({ managedResidentBytes: this.#admission.usedBytes, peakManagedResidentBytes: this.#admission.peakBytes }), ...(code === undefined ? {} : { errorCode: code }) }); throw mapped; } })(); this.#pending.add(work); void work.finally(() => this.#pending.delete(work)).catch(() => {}); return work;
  }
  #observe(event: FilesystemObservation): void { try { this.#observer?.(Object.freeze(event)); } catch {} }
}

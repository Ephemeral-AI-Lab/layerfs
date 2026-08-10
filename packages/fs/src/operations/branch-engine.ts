import type {
  BranchConfiguration,
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
import { AdmissionController } from "../resources/limits.js";
import {
  canonicalizePath,
  compareUtf8,
  validateName,
  validateSymlinkTarget,
  type CanonicalPath,
} from "../namespace/paths.js";
import { bytesToHex, hexToBytes } from "../cas/bytes.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import { prepareContent, readManifestRange } from "../operations/manifest-io.js";
import { fsError } from "../filesystem/errors.js";
import { ContentCache } from "../cache/content-cache.js";
import type {
  BranchChangeRow as ChangeRow,
  BranchRow,
  BranchStore,
  ClosureCertificate,
  EntryRow,
  InodeRow,
  NamespaceStore,
  OperationsStorage,
  StorageTransactionPorts,
} from "./storage-ports.js";
import type {
  DirectoryEntry,
  FileContent,
  FileStat,
  FileType,
  MkdirOptions,
  ReadRangeOptions,
  ReadStreamOptions,
  ReadTextOptions,
  ReaddirOptions,
  RmOptions,
  WriteFileOptions,
} from "../filesystem/types.js";
import {
  BranchError,
  type BranchInfo,
  type Branches,
  type CreateBranchOptions,
  type EphemeralBranch,
  type PublishOptions,
  type PublishResult,
} from "../branches/types.js";

interface DesiredNode {
  readonly inodeId: string;
  readonly type: 0 | 1 | 2;
  readonly mode: number;
  readonly birthtimeMs: number;
  readonly mtimeMs: number;
  readonly ctimeMs: number;
  readonly nlink: number;
  readonly size: number | null;
  readonly manifestHash: string | null;
  readonly symlinkTarget: string | null;
  readonly expectedInodeToken: number | null;
}
interface ViewNode {
  readonly path: CanonicalPath;
  readonly inode: InodeRow;
  readonly entryToken: number | null;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();
function encode(value: unknown): Uint8Array {
  return encoder.encode(JSON.stringify(value));
}
function decode<T>(value: Uint8Array): T {
  return JSON.parse(decoder.decode(value)) as T;
}
function typeName(type: number): FileType {
  return type === 0
    ? "file"
    : type === 1
      ? "directory"
      : type === 2
        ? "symlink"
        : (() => {
            throw new Error("ECORRUPT: invalid branch inode type");
          })();
}
function predicates(type: FileType) {
  return {
    isFile: () => type === "file",
    isDirectory: () => type === "directory",
    isSymbolicLink: () => type === "symlink",
  };
}
function stat(node: ViewNode): FileStat {
  const type = typeName(node.inode.type);
  const size =
    type === "file"
      ? node.inode.size!
      : type === "symlink"
        ? encodeUtf8(node.inode.symlink_target ?? "").byteLength
        : 0;
  return Object.freeze({
    id: node.inode.id,
    name: node.path.segments.at(-1) ?? "",
    type,
    mode: node.inode.mode,
    size,
    nlink: node.inode.nlink,
    mtimeMs: node.inode.mtime_ms,
    ctimeMs: node.inode.ctime_ms,
    birthtimeMs: node.inode.birthtime_ms,
    ...predicates(type),
  });
}
function desired(
  inode: InodeRow,
  expectedInodeToken: number | null = inode.token,
): DesiredNode {
  return Object.freeze({
    inodeId: inode.id,
    type: inode.type as 0 | 1 | 2,
    mode: inode.mode,
    birthtimeMs: inode.birthtime_ms,
    mtimeMs: inode.mtime_ms,
    ctimeMs: inode.ctime_ms,
    nlink: inode.nlink,
    size: inode.size,
    manifestHash: inode.manifest_hash ? bytesToHex(inode.manifest_hash) : null,
    symlinkTarget: inode.symlink_target,
    expectedInodeToken,
  });
}
function fromDesired(value: DesiredNode, token: number): InodeRow {
  return {
    id: value.inodeId,
    type: value.type,
    mode: value.mode,
    birthtime_ms: value.birthtimeMs,
    mtime_ms: value.mtimeMs,
    ctime_ms: value.ctimeMs,
    nlink: value.nlink,
    size: value.size,
    manifest_hash: value.manifestHash ? hexToBytes(value.manifestHash, 32) : null,
    symlink_target: value.symlinkTarget,
    token,
  };
}
function info(row: BranchRow): BranchInfo {
  return Object.freeze({
    id: row.id,
    baseRevision: String(row.base_revision),
    state: row.state === 0 ? "active" : row.state === 1 ? "merged" : "discarded",
    generation: row.generation,
    createdAt: row.created_at_ms,
    terminalAt: row.terminal_at_ms,
  });
}

class BranchView {
  readonly #repository: BranchStore;
  readonly #branch: BranchRow;
  readonly #filesystem: FilesystemLimits;
  constructor(
    tx: StorageTransactionPorts,
    branch: BranchRow,
    filesystem: FilesystemLimits,
    storage: StorageLimits,
  ) {
    this.#repository = tx.branches(storage);
    this.#branch = branch;
    this.#filesystem = filesystem;
  }
  resolve(
    input: string | CanonicalPath,
    followFinal = true,
    includeChanges = true,
    syscall = "branch",
  ): ViewNode {
    let path =
      typeof input === "string"
        ? canonicalizePath(input, this.#filesystem, syscall)
        : input;
    let traversals = 0;
    restart: while (true) {
      let inode = this.#historicInode(this.#repository.rootInodeId());
      if (!inode) throw new Error("ECORRUPT: branch base root is missing");
      let entryToken: number | null = null;
      if (!path.segments.length) return { path, inode, entryToken };
      let prefix = "";
      for (let index = 0; index < path.segments.length; index += 1) {
        const name = path.segments[index]!;
        prefix += `/${name}`;
        const exact = includeChanges ? this.#change(prefix) : undefined;
        if (exact) {
          if (exact.kind === 1 || !exact.encoded)
            throw fsError("ENOENT", syscall, path.value, "branch path does not exist");
          inode = fromDesired(
            decode<DesiredNode>(exact.encoded),
            this.#branch.generation,
          );
          entryToken = exact.expected_token;
        } else {
          const entry = this.#historicEntry(inode.id, path.encodedSegments[index]!);
          if (!entry?.inode_id || entry.name !== name)
            throw fsError("ENOENT", syscall, path.value, "branch path does not exist");
          const next = this.#historicInode(entry.inode_id);
          if (!next)
            throw new Error("ECORRUPT: branch history references missing inode");
          inode = next;
          entryToken = entry.token;
        }
        const final = index === path.segments.length - 1;
        if (inode.type === 2 && (!final || followFinal)) {
          traversals += 1;
          if (traversals > this.#filesystem.maxSymlinkTraversals)
            throw fsError("ELOOP", syscall, path.value, "too many symbolic links");
          const target = inode.symlink_target!;
          const base = path.segments.slice(0, index).join("/");
          const remaining = path.segments.slice(index + 1).join("/");
          path = canonicalizePath(
            target.startsWith("/")
              ? `${target}${remaining ? `/${remaining}` : ""}`
              : `/${base}${base ? "/" : ""}${target}${remaining ? `/${remaining}` : ""}`,
            this.#filesystem,
            syscall,
          );
          continue restart;
        }
        if (!final && inode.type !== 1)
          throw fsError(
            "ENOTDIR",
            syscall,
            path.value,
            "intermediate component is not a directory",
          );
      }
      return { path, inode, entryToken };
    }
  }
  optional(
    input: string | CanonicalPath,
    followFinal = true,
    includeChanges = true,
    syscall = "branch",
  ): ViewNode | undefined {
    try {
      return this.resolve(input, followFinal, includeChanges, syscall);
    } catch (error) {
      if (error instanceof Error && "code" in error && error.code === "ENOENT")
        return undefined;
      throw error;
    }
  }
  base(
    input: string | CanonicalPath,
    followFinal = false,
    syscall = "branch",
  ): ViewNode | undefined {
    return this.optional(input, followFinal, false, syscall);
  }
  change(path: string): ChangeRow | undefined {
    return this.#change(path);
  }
  children(path: CanonicalPath): ViewNode[] {
    const parent = this.resolve(path, true);
    if (parent.inode.type !== 1)
      throw fsError("ENOTDIR", "readdir", path.value, "path is not a directory");
    const slots = new Map<string, ViewNode>();
    const rows = this.#repository.historyEntries(
      parent.inode.id,
      this.#branch.base_revision,
    );
    for (const row of rows)
      if (!row.tombstone && row.encoded) {
        const entry = decode<EntryRow & { name_sort: string }>(row.encoded);
        if (entry.inode_id && entry.name) {
          const inode = this.#historicInode(entry.inode_id);
          if (inode) {
            const childPath = canonicalizePath(
              `${path.value === "/" ? "" : path.value}/${entry.name}`,
              this.#filesystem,
              "readdir",
            );
            slots.set(entry.name, { path: childPath, inode, entryToken: entry.token });
          }
        }
      }
    const prefix = path.value === "/" ? "/" : `${path.value}/`;
    for (const change of this.allChanges()) {
      const changePath = decoder.decode(change.path);
      if (!changePath.startsWith(prefix)) continue;
      const remainder = changePath.slice(prefix.length);
      if (!remainder || remainder.includes("/")) continue;
      if (change.kind === 1 || !change.encoded) slots.delete(remainder);
      else
        slots.set(remainder, {
          path: canonicalizePath(changePath, this.#filesystem, "readdir"),
          inode: fromDesired(
            decode<DesiredNode>(change.encoded),
            this.#branch.generation,
          ),
          entryToken: change.expected_token,
        });
    }
    return [...slots.values()].sort((a, b) =>
      compareUtf8(a.path.segments.at(-1)!, b.path.segments.at(-1)!),
    );
  }
  allChanges(): ChangeRow[] {
    return [...this.#repository.changes(this.#branch.id)];
  }
  #change(path: string): ChangeRow | undefined {
    return this.#repository.change(this.#branch.id, encodeUtf8(path));
  }
  #historicEntry(parent: string, nameSort: Uint8Array): EntryRow | undefined {
    const row = this.#repository.historicEntry(
      parent,
      nameSort,
      this.#branch.base_revision,
    );
    if (!row || row.tombstone || !row.encoded) return undefined;
    const value = decode<EntryRow & { name_sort: string }>(row.encoded);
    return { ...value, name_sort: hexToBytes(value.name_sort) };
  }
  #historicInode(id: string): InodeRow | undefined {
    const row = this.#repository.historicInode(id, this.#branch.base_revision);
    if (!row || row.tombstone || !row.encoded) return undefined;
    const value = decode<{
      id: string;
      type: number;
      mode: number;
      birthtime_ms: number;
      mtime_ms: number;
      ctime_ms: number;
      nlink: number;
      size: number | null;
      manifest_hash: string | null;
      symlink_target: string | null;
      token: number;
    }>(row.encoded);
    return {
      id: value.id,
      type: value.type,
      mode: value.mode,
      birthtime_ms: value.birthtime_ms,
      mtime_ms: value.mtime_ms,
      ctime_ms: value.ctime_ms,
      nlink: value.nlink,
      size: value.size,
      manifest_hash: value.manifest_hash ? hexToBytes(value.manifest_hash, 32) : null,
      symlink_target: value.symlink_target,
      token: value.token,
    };
  }
}

export class BranchManager implements Branches {
  readonly #port: OperationsStorage;
  readonly #filesystem: FilesystemLimits;
  readonly #storage: StorageLimits;
  readonly #runtime: RuntimeLimits;
  readonly #limits: BranchConfiguration;
  readonly #clock: () => number;
  readonly #admission: AdmissionController;
  readonly #cache: ContentCache;
  #handles = 0;
  constructor(
    port: OperationsStorage,
    filesystem: FilesystemLimits,
    storage: StorageLimits,
    runtime: RuntimeLimits,
    limits: BranchConfiguration,
    clock: () => number,
    admission: AdmissionController,
    cache: ContentCache,
  ) {
    this.#port = port;
    this.#filesystem = filesystem;
    this.#storage = storage;
    this.#runtime = runtime;
    this.#limits = limits;
    this.#clock = clock;
    this.#admission = admission;
    this.#cache = cache;
  }
  async create(input: string | CreateBranchOptions = {}): Promise<EphemeralBranch> {
    const options = typeof input === "string" ? { id: input } : input;
    const id = options.id ?? globalThis.crypto.randomUUID();
    this.#validateId(id, "branch");
    const row = this.#transaction("write", (tx) => {
      const repository = tx.branches(this.#storage);
      if (repository.activeCount() >= this.#limits.maxActiveBranches)
        throw new BranchError("LimitExceeded", "active branch limit exceeded", {
          limit: "maxActiveBranches",
        });
      const head = repository.headRevision();
      const base =
        options.baseRevision === undefined ? head : Number(options.baseRevision);
      if (!Number.isSafeInteger(base) || !repository.revisionExists(base))
        throw new BranchError("RevisionNotFound", "base revision does not exist", {
          branchId: id,
        });
      return repository.create(id, base, this.#now());
    });
    return this.#handle(row);
  }
  async open(id: string): Promise<EphemeralBranch> {
    this.#validateId(id, "branch");
    const row = this.#transaction("read", (tx) => this.#row(tx, id));
    if (!row)
      throw new BranchError("BranchNotFound", "branch does not exist", {
        branchId: id,
      });
    return this.#handle(row);
  }
  async get(id: string): Promise<BranchInfo> {
    this.#validateId(id, "branch");
    const row = this.#transaction("read", (tx) => this.#row(tx, id));
    if (!row)
      throw new BranchError("BranchNotFound", "branch does not exist", {
        branchId: id,
      });
    return info(row);
  }
  async replay(operationId: string, branchId?: string): Promise<PublishResult> {
    this.#validateId(operationId, "operation");
    return this.#transaction("read", (tx) => {
      const row = tx
        .branches(this.#storage)
        .operationResult(operationId, this.#limits.maxConflictResultBytes + 1024);
      if (!row || !row.encoded)
        throw new BranchError("OperationNotFound", "operation result does not exist", {
          operationId,
        });
      if (branchId !== undefined && row.branch_id !== branchId)
        throw new BranchError(
          "OperationBranchMismatch",
          "operation is bound to another branch",
          { branchId, operationId },
        );
      if (row.expires_at_ms === null || row.expires_at_ms < this.#now())
        throw new BranchError(
          "OperationResultExpired",
          "operation result has expired",
          { operationId },
        );
      return decode<PublishResult>(row.encoded);
    });
  }
  branchRow(id: string): BranchRow {
    const row = this.#transaction("read", (tx) => this.#row(tx, id));
    if (!row)
      throw new BranchError("BranchNotFound", "branch does not exist", {
        branchId: id,
      });
    return row;
  }
  view<T>(
    id: string,
    callback: (view: BranchView, tx: StorageTransactionPorts, branch: BranchRow) => T,
  ): T {
    return this.#transaction("read", (tx) => {
      const branch = this.#active(tx, id);
      return callback(
        new BranchView(tx, branch, this.#filesystem, this.#storage),
        tx,
        branch,
      );
    });
  }
  mutate(
    id: string,
    changes: readonly { path: string; node: DesiredNode | null }[],
    certificate?: ClosureCertificate,
  ): void {
    this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const repository = tx.branches(this.#storage);
      if (certificate)
        tx.staging(this.#storage).validateSealed(certificate, this.#now());
      for (const change of changes) {
        const canonical = canonicalizePath(change.path, this.#filesystem, "branch");
        const old = view.change(canonical.value);
        const base = view.base(canonical, false);
        const expectedEntry = old ? old.expected_token : (base?.entryToken ?? null);
        const expectedInode = old?.encoded
          ? decode<DesiredNode>(old.encoded).expectedInodeToken
          : (base?.inode.token ?? null);
        const value = change.node
          ? { ...change.node, expectedInodeToken: expectedInode }
          : null;
        const pathBytes = encodeUtf8(canonical.value);
        repository.putChange(
          id,
          pathBytes,
          expectedEntry,
          value ? 0 : 1,
          value ? encode(value) : null,
        );
        if (base?.inode.id)
          repository.putInodeExpectation(id, base.inode.id, expectedInode);
        repository.setManifestRoot(
          id,
          pathBytes,
          value?.manifestHash ? hexToBytes(value.manifestHash, 32) : undefined,
        );
      }
      const count = repository.changeCount(id);
      if (count > this.#limits.maxChangedPathsPerBranch)
        throw new BranchError("LimitExceeded", "changed-path limit exceeded", {
          branchId: id,
          limit: "maxChangedPathsPerBranch",
        });
      repository.incrementGeneration(id);
      if (certificate) this.#releasePrepared(tx, certificate);
    });
  }
  async publish(id: string, options: PublishOptions = {}): Promise<PublishResult> {
    const operationId = options.operationId ?? null;
    if (operationId) this.#validateId(operationId, "operation");
    return this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      const repository = tx.branches(this.#storage);
      if (operationId) {
        const prior = repository.operationResult(
          operationId,
          this.#limits.maxConflictResultBytes + 1024,
        );
        if (prior) {
          if (prior.branch_id !== id || prior.generation !== branch.generation)
            throw new BranchError(
              "OperationBranchMismatch",
              "operation is bound to another branch generation",
              { branchId: id, operationId },
            );
          if (prior.encoded) return decode<PublishResult>(prior.encoded);
          throw new BranchError(
            "BranchChanged",
            "operation reservation has no terminal result",
            { branchId: id, operationId },
          );
        }
        repository.reserveOperation(operationId, id, branch.generation, this.#now());
      }
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const changes = view.allChanges();
      const ns = tx.namespace(this.#filesystem, this.#storage, "publish");
      const head = ns.meta().main_revision;
      const conflicts = [];
      for (const change of changes) {
        const path = decoder.decode(change.path);
        const current = this.#currentTokens(ns, path);
        const value = change.encoded ? decode<DesiredNode>(change.encoded) : null;
        if (current.entry !== change.expected_token)
          conflicts.push({
            path,
            reason: "entry-changed" as const,
            expectedRevision:
              change.expected_token === null ? null : String(change.expected_token),
            actualRevision: current.entry === null ? null : String(current.entry),
          });
        else if (
          value?.expectedInodeToken !== null &&
          current.inode !== value?.expectedInodeToken
        )
          conflicts.push({
            path,
            reason: "node-changed" as const,
            expectedRevision: String(value!.expectedInodeToken),
            actualRevision: current.inode === null ? null : String(current.inode),
          });
      }
      if (conflicts.length) {
        const result: PublishResult = Object.freeze({
          outcome: "conflict",
          branchId: id,
          operationId,
          baseRevision: String(branch.base_revision),
          headRevision: String(head),
          revision: null,
          changedPaths: [] as [],
          conflicts: conflicts.slice(0, this.#limits.maxConflictsPerPublication),
        });
        this.#storeResult(tx, operationId, result);
        return result;
      }
      const live = changes
        .filter((change) => change.kind === 0)
        .sort(
          (a, b) =>
            decoder.decode(a.path).split("/").length -
            decoder.decode(b.path).split("/").length,
        );
      const deleted = changes
        .filter((change) => change.kind === 1)
        .sort(
          (a, b) =>
            decoder.decode(b.path).split("/").length -
            decoder.decode(a.path).split("/").length,
        );
      const now = this.#now();
      const revision = ns.nextRevision(now, changes.length * 3 + 1, `branch:${id}`);
      const touched = new Set<string>();
      for (const change of live)
        this.#applyLive(
          tx,
          ns,
          decoder.decode(change.path),
          decode<DesiredNode>(change.encoded!),
          revision,
          now,
          touched,
        );
      for (const change of deleted)
        this.#applyDelete(tx, ns, decoder.decode(change.path), revision, now, touched);
      for (const inodeId of touched) {
        const count = ns.linkCount(inodeId);
        if (count === 0) {
          ns.deleteInode(inodeId);
          ns.recordInode(revision, inodeId, true);
        } else {
          ns.setLinks(inodeId, count, now, revision);
          ns.recordInode(revision, inodeId);
        }
      }
      repository.finish(id, 1, now);
      const result: PublishResult = Object.freeze({
        outcome: "merged",
        branchId: id,
        operationId,
        baseRevision: String(branch.base_revision),
        parentRevision: String(head),
        revision: String(revision),
        changedPaths: changes.map((change) => decoder.decode(change.path)),
        conflicts: [] as [],
      });
      this.#storeResult(tx, operationId, result);
      return result;
    });
  }
  async discard(id: string): Promise<BranchInfo> {
    return this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      const now = this.#now();
      tx.branches(this.#storage).finish(id, 2, now);
      return info({ ...branch, state: 2, terminal_at_ms: now });
    });
  }
  prepare(content: Uint8Array | ReadableStream<Uint8Array>, signal?: AbortSignal) {
    return prepareContent(
      this.#port,
      content,
      this.#storage,
      this.#runtime,
      this.#admission,
      signal,
      this.#cache,
      this.#clock,
    );
  }
  abandonPrepared(certificate: ClosureCertificate): void {
    try {
      this.#transaction("write", (tx) => {
        tx.staging(this.#storage).release(
          certificate.leaseId,
          certificate.ownerNonce,
          false,
        );
      });
    } catch {}
  }
  readManifest(hash: Uint8Array, offset: number, length: number): Uint8Array {
    return this.#transaction("read", (tx) =>
      readManifestRange(tx.content(this.#storage, this.#cache), hash, offset, length),
    );
  }
  releaseHandle(): void {
    this.#handles = Math.max(0, this.#handles - 1);
  }
  #handle(row: BranchRow): EphemeralBranch {
    if (this.#handles >= this.#runtime.maxOpenBranchHandles)
      throw new BranchError("LimitExceeded", "open branch handle limit exceeded", {
        branchId: row.id,
        limit: "maxOpenBranchHandles",
      });
    this.#handles += 1;
    return new BranchHandle(this, row.id, this.#filesystem, this.#storage);
  }
  #row(tx: StorageTransactionPorts, id: string): BranchRow | undefined {
    return tx.branches(this.#storage).row(id);
  }
  #active(tx: StorageTransactionPorts, id: string): BranchRow {
    const row = this.#row(tx, id);
    if (!row)
      throw new BranchError("BranchNotFound", "branch does not exist", {
        branchId: id,
      });
    if (row.state !== 0)
      throw new BranchError("BranchNotActive", "branch is terminal", { branchId: id });
    return row;
  }
  #validateId(id: string, kind: "branch" | "operation"): void {
    const maximum =
      kind === "branch"
        ? this.#limits.maxBranchIdBytes
        : this.#limits.maxOperationIdBytes;
    if (!id || id.includes("\0") || encodeUtf8(id).byteLength > maximum)
      throw new BranchError(
        kind === "branch" ? "InvalidBranchId" : "InvalidOperationId",
        `invalid ${kind} identifier`,
        kind === "branch" ? { branchId: id } : { operationId: id },
      );
  }
  #transaction<T>(
    mode: "read" | "write",
    callback: (tx: StorageTransactionPorts) => T,
  ): T {
    return this.#port.transaction(
      mode,
      {
        maxRows: this.#storage.maxFinalTransactionRows,
        maxBytes: this.#storage.maxFinalTransactionBytes,
      },
      callback,
    );
  }
  #now(): number {
    const value = this.#clock();
    if (!Number.isSafeInteger(value) || value < 0)
      throw new Error("clock returned invalid time");
    return value;
  }
  #releasePrepared(tx: StorageTransactionPorts, certificate: ClosureCertificate): void {
    if (
      !tx
        .staging(this.#storage)
        .release(certificate.leaseId, certificate.ownerNonce, true)
    )
      throw new Error("ECORRUPT: staging lease could not be released");
  }
  #storeResult(
    tx: StorageTransactionPorts,
    operationId: string | null,
    result: PublishResult,
  ): void {
    if (!operationId) return;
    const bytes = encode(result);
    if (bytes.byteLength > this.#limits.maxConflictResultBytes)
      throw new BranchError("LimitExceeded", "publication result exceeds limit", {
        operationId,
        limit: "maxConflictResultBytes",
      });
    tx.branches(this.#storage).storeResult(
      operationId,
      result.outcome === "merged" ? 1 : 0,
      bytes,
      this.#now() + this.#limits.publicationResultRetentionMs,
    );
  }
  #currentTokens(
    ns: NamespaceStore,
    path: string,
  ): { entry: number | null; inode: number | null } {
    const canonical = canonicalizePath(path, this.#filesystem, "publish");
    if (!canonical.segments.length) {
      const root = ns.resolve("/");
      return { entry: null, inode: root.inode.token };
    }
    try {
      const parent = ns.resolveParent(canonical);
      const entry = ns.entry(parent.parent.inode.id, parent.nameSort);
      if (!entry?.inode_id) return { entry: entry?.token ?? null, inode: null };
      const inode = ns.inode(entry.inode_id);
      return { entry: entry.token, inode: inode?.token ?? null };
    } catch (error) {
      if (error instanceof Error && "code" in error && error.code === "ENOENT")
        return { entry: null, inode: null };
      throw error;
    }
  }
  #applyLive(
    tx: StorageTransactionPorts,
    ns: NamespaceStore,
    path: string,
    value: DesiredNode,
    revision: number,
    now: number,
    touched: Set<string>,
  ): void {
    const canonical = canonicalizePath(path, this.#filesystem, "publish");
    const parent = ns.resolveParent(canonical);
    const current = ns.resolveOptional(canonical, false);
    if (current && current.inode.id !== value.inodeId) {
      ns.putEntry(current.parentInode!, current.nameSort!, null, null, revision);
      touched.add(current.inode.id);
    }
    ns.upsertInode({
      id: value.inodeId,
      type: value.type,
      mode: value.mode,
      birthtimeMs: value.birthtimeMs,
      mtimeMs: Math.max(now, value.mtimeMs),
      ctimeMs: Math.max(now, value.ctimeMs),
      nlink: Math.max(1, value.nlink),
      size: value.size,
      manifestHash: value.manifestHash ? hexToBytes(value.manifestHash, 32) : null,
      symlinkTarget: value.symlinkTarget,
      token: revision,
    });
    ns.putEntry(
      parent.parent.inode.id,
      parent.nameSort,
      parent.name,
      value.inodeId,
      revision,
    );
    ns.recordEntry(revision, parent.parent.inode.id, parent.nameSort);
    ns.recordInode(revision, value.inodeId);
    touched.add(value.inodeId);
    this.#touch(tx, ns, parent.parent.inode, now, revision);
  }
  #applyDelete(
    tx: StorageTransactionPorts,
    ns: NamespaceStore,
    path: string,
    revision: number,
    now: number,
    touched: Set<string>,
  ): void {
    const current = ns.resolveOptional(path, false);
    if (!current || current.parentInode === null) return;
    ns.putEntry(current.parentInode, current.nameSort!, null, null, revision);
    ns.recordEntry(revision, current.parentInode, current.nameSort!, true);
    touched.add(current.inode.id);
    const parent = ns.inode(current.parentInode);
    if (parent) this.#touch(tx, ns, parent, now, revision);
    if (current.inode.type === 1) ns.deleteEntriesUnder(current.inode.id, true);
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
}

class BranchHandle implements EphemeralBranch {
  readonly id: string;
  readonly #manager: BranchManager;
  readonly #filesystem: FilesystemLimits;
  #closed = false;
  constructor(
    manager: BranchManager,
    id: string,
    filesystem: FilesystemLimits,
    storage: StorageLimits,
  ) {
    this.#manager = manager;
    this.id = id;
    this.#filesystem = filesystem;
  }
  async info(): Promise<BranchInfo> {
    this.#assertHandle();
    return info(this.#manager.branchRow(this.id));
  }
  async publish(options?: PublishOptions): Promise<PublishResult> {
    this.#assert();
    return this.#manager.publish(this.id, options);
  }
  async discard(): Promise<BranchInfo> {
    this.#assert();
    return this.#manager.discard(this.id);
  }
  readFile(path: string): Promise<Uint8Array>;
  readFile(path: string, options: ReadTextOptions): Promise<string>;
  async readFile(
    path: string,
    options?: ReadTextOptions,
  ): Promise<Uint8Array | string> {
    this.#assert();
    const value = this.#manager.view(this.id, (view) =>
      view.resolve(path, true, true, "readFile"),
    );
    if (value.inode.type !== 0)
      throw fsError(
        value.inode.type === 1 ? "EISDIR" : "EINVAL",
        "readFile",
        path,
        "path is not a file",
      );
    if (value.inode.size! > this.#filesystem.maxMaterializedBytes)
      throw fsError("EFBIG", "readFile", path, "file exceeds materialization limit");
    const bytes = this.#manager.readManifest(
      value.inode.manifest_hash!,
      0,
      value.inode.size!,
    );
    return options ? new TextDecoder().decode(bytes) : bytes;
  }
  async readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array> {
    this.#assert();
    const value = this.#manager.view(this.id, (view) =>
      view.resolve(path, true, true, "readRange"),
    );
    if (value.inode.type !== 0)
      throw fsError(
        value.inode.type === 1 ? "EISDIR" : "EINVAL",
        "readRange",
        path,
        "path is not a file",
      );
    return this.#manager.readManifest(
      value.inode.manifest_hash!,
      options.offset,
      options.length,
    );
  }
  async readStream(
    path: string,
    options: ReadStreamOptions = {},
  ): Promise<ReadableStream<Uint8Array>> {
    const value = (await this.readFile(path)) as Uint8Array;
    const offset = options.offset ?? 0;
    const end = Math.min(
      value.length,
      options.length === undefined ? value.length : offset + options.length,
    );
    let position = offset;
    return new ReadableStream({
      pull: (controller) => {
        if (options.signal?.aborted) {
          controller.error(new DOMException("aborted", "AbortError"));
          return;
        }
        if (position >= end) {
          controller.close();
          return;
        }
        const next = value.slice(
          position,
          Math.min(end, position + this.#filesystem.preferredStreamChunkBytes),
        );
        position += next.length;
        controller.enqueue(next);
      },
    });
  }
  async writeFile(
    path: string,
    content: FileContent,
    options: WriteFileOptions = {},
  ): Promise<void> {
    this.#assert();
    const canonical = canonicalizePath(path, this.#filesystem, "writeFile");
    const existing = this.#manager.view(this.id, (view) =>
      view.optional(canonical, false, true, "writeFile"),
    );
    if (options.exclusive && existing)
      throw fsError("EEXIST", "writeFile", canonical.value, "destination exists");
    if (existing?.inode.type === 1)
      throw fsError(
        "EISDIR",
        "writeFile",
        canonical.value,
        "destination is a directory",
      );
    this.#manager.view(this.id, (view) => {
      const parent =
        canonical.segments.length === 1
          ? view.resolve("/")
          : view.resolve(`/${canonical.segments.slice(0, -1).join("/")}`);
      if (parent.inode.type !== 1)
        throw fsError(
          "ENOTDIR",
          "writeFile",
          canonical.value,
          "parent is not a directory",
        );
    });
    const input =
      typeof content === "string"
        ? new TextEncoder().encode(content)
        : content instanceof Uint8Array
          ? content.slice()
          : content;
    const prepared = await this.#manager.prepare(input, options.signal);
    const now = Date.now();
    const node: DesiredNode = existing
      ? {
          ...desired(existing.inode),
          size: prepared.size,
          manifestHash: bytesToHex(prepared.hash),
          mtimeMs: now,
          ctimeMs: now,
        }
      : {
          inodeId: globalThis.crypto.randomUUID(),
          type: 0,
          mode: (options.mode ?? 0o666) & 0o7777,
          birthtimeMs: now,
          mtimeMs: now,
          ctimeMs: now,
          nlink: 1,
          size: prepared.size,
          manifestHash: bytesToHex(prepared.hash),
          symlinkTarget: null,
          expectedInodeToken: null,
        };
    try {
      this.#manager.mutate(
        this.id,
        [{ path: canonical.value, node }],
        prepared.certificate,
      );
    } catch (error) {
      this.#manager.abandonPrepared(prepared.certificate);
      throw error;
    }
  }
  async writeRange(path: string, offset: number, content: Uint8Array): Promise<void> {
    const old = (await this.readFile(path)) as Uint8Array;
    const bytes = new Uint8Array(Math.max(old.length, offset + content.length));
    bytes.set(old);
    bytes.set(content, offset);
    await this.writeFile(path, bytes);
  }
  async replaceRange(
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): Promise<void> {
    const old = (await this.readFile(path)) as Uint8Array;
    if (offset > old.length || deleteLength > old.length - offset)
      throw fsError("EINVAL", "replaceRange", path, "invalid range");
    const bytes = new Uint8Array(old.length - deleteLength + insertBytes.length);
    bytes.set(old.subarray(0, offset));
    bytes.set(insertBytes, offset);
    bytes.set(old.subarray(offset + deleteLength), offset + insertBytes.length);
    await this.writeFile(path, bytes);
  }
  async truncate(path: string, size = 0): Promise<void> {
    const old = (await this.readFile(path)) as Uint8Array;
    const bytes = new Uint8Array(size);
    bytes.set(old.subarray(0, size));
    await this.writeFile(path, bytes);
  }
  async mkdir(path: string, options: MkdirOptions = {}): Promise<void> {
    this.#assert();
    const canonical = canonicalizePath(path, this.#filesystem, "mkdir");
    const prefixes = options.recursive
      ? canonical.segments.map(
          (_, index) => `/${canonical.segments.slice(0, index + 1).join("/")}`,
        )
      : [canonical.value];
    const changes = [];
    for (const prefix of prefixes) {
      const existing = this.#manager.view(this.id, (view) =>
        view.optional(prefix, false),
      );
      if (existing) {
        if (existing.inode.type !== 1 || !options.recursive)
          throw fsError("EEXIST", "mkdir", prefix, "destination exists");
        continue;
      }
      const now = Date.now();
      changes.push({
        path: prefix,
        node: {
          inodeId: globalThis.crypto.randomUUID(),
          type: 1 as const,
          mode: (options.mode ?? 0o777) & 0o7777,
          birthtimeMs: now,
          mtimeMs: now,
          ctimeMs: now,
          nlink: 1,
          size: null,
          manifestHash: null,
          symlinkTarget: null,
          expectedInodeToken: null,
        },
      });
    }
    if (changes.length) this.#manager.mutate(this.id, changes);
  }
  async readdir(path: string, options: ReaddirOptions = {}): Promise<DirectoryEntry[]> {
    this.#assert();
    if (options.startAfter !== undefined)
      validateName(options.startAfter, this.#filesystem, "readdir");
    let nodes = this.#manager.view(this.id, (view) =>
      view.children(canonicalizePath(path, this.#filesystem, "readdir")),
    );
    if (options.startAfter !== undefined)
      nodes = nodes.filter(
        (node) => compareUtf8(node.path.segments.at(-1)!, options.startAfter!) > 0,
      );
    const limit = options.limit ?? this.#filesystem.maxReaddirEntries;
    if (nodes.length > limit)
      throw fsError("EFBIG", "readdir", path, "listing exceeds limit");
    return nodes.map((node) => {
      const name = node.path.segments.at(-1)!;
      const type = typeName(node.inode.type);
      return Object.freeze({
        name,
        parentPath: canonicalizePath(path, this.#filesystem, "readdir").value,
        type,
        ...predicates(type),
      });
    });
  }
  async stat(path: string): Promise<FileStat> {
    this.#assert();
    return stat(this.#manager.view(this.id, (view) => view.resolve(path, true)));
  }
  async lstat(path: string): Promise<FileStat> {
    this.#assert();
    return stat(this.#manager.view(this.id, (view) => view.resolve(path, false)));
  }
  async chmod(path: string, mode: number): Promise<void> {
    const node = this.#manager.view(this.id, (view) => view.resolve(path, true));
    this.#manager.mutate(this.id, [
      {
        path: node.path.value,
        node: { ...desired(node.inode), mode: mode & 0o7777, ctimeMs: Date.now() },
      },
    ]);
  }
  async link(existingPath: string, newPath: string): Promise<void> {
    const source = this.#manager.view(this.id, (view) =>
      view.resolve(existingPath, true),
    );
    if (source.inode.type !== 0)
      throw fsError("EPERM", "link", existingPath, "only files can be linked");
    this.#manager.mutate(this.id, [
      {
        path: canonicalizePath(newPath, this.#filesystem, "link").value,
        node: { ...desired(source.inode), nlink: source.inode.nlink + 1 },
      },
    ]);
  }
  async symlink(target: string, path: string): Promise<void> {
    validateSymlinkTarget(target, this.#filesystem, "symlink");
    const now = Date.now();
    this.#manager.mutate(this.id, [
      {
        path: canonicalizePath(path, this.#filesystem, "symlink").value,
        node: {
          inodeId: globalThis.crypto.randomUUID(),
          type: 2,
          mode: 0o777,
          birthtimeMs: now,
          mtimeMs: now,
          ctimeMs: now,
          nlink: 1,
          size: null,
          manifestHash: null,
          symlinkTarget: target,
          expectedInodeToken: null,
        },
      },
    ]);
  }
  async readlink(path: string): Promise<string> {
    const node = this.#manager.view(this.id, (view) => view.resolve(path, false));
    if (node.inode.type !== 2)
      throw fsError("EINVAL", "readlink", path, "not a symbolic link");
    return node.inode.symlink_target!;
  }
  async rename(oldPath: string, newPath: string): Promise<void> {
    const source = this.#manager.view(this.id, (view) => view.resolve(oldPath, false));
    this.#manager.mutate(this.id, [
      {
        path: canonicalizePath(newPath, this.#filesystem, "rename").value,
        node: desired(source.inode),
      },
      { path: canonicalizePath(oldPath, this.#filesystem, "rename").value, node: null },
    ]);
  }
  async unlink(path: string): Promise<void> {
    const source = this.#manager.view(this.id, (view) => view.resolve(path, false));
    if (source.inode.type === 1)
      throw fsError("EISDIR", "unlink", path, "path is a directory");
    this.#manager.mutate(this.id, [{ path: source.path.value, node: null }]);
  }
  async rm(path: string, options: RmOptions = {}): Promise<void> {
    const source = this.#manager.view(this.id, (view) => view.optional(path, false));
    if (!source) {
      if (options.force) return;
      throw fsError("ENOENT", "rm", path, "path does not exist");
    }
    const changes = [{ path: source.path.value, node: null }];
    if (source.inode.type === 1) {
      const children = this.#manager.view(this.id, (view) =>
        view.children(source.path),
      );
      if (children.length && !options.recursive)
        throw fsError("ENOTEMPTY", "rm", path, "directory is not empty");
      for (const child of children)
        changes.unshift({ path: child.path.value, node: null });
    }
    this.#manager.mutate(this.id, changes);
  }
  async close(): Promise<void> {
    if (!this.#closed) {
      this.#closed = true;
      this.#manager.releaseHandle();
    }
  }
  #assertHandle(): void {
    if (this.#closed)
      throw fsError("EBADF", "branch", this.id, "branch handle is closed");
  }
  #assert(): void {
    this.#assertHandle();
    const row = this.#manager.branchRow(this.id);
    if (row.state !== 0)
      throw new BranchError("BranchNotActive", "branch is terminal", {
        branchId: this.id,
      });
  }
}

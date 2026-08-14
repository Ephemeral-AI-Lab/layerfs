import type {
  BranchConfiguration,
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
import { AdmissionController, RuntimeConcurrency } from "../resources/limits.js";
import {
  canonicalizePath,
  compareUtf8,
  validateName,
  validateSymlinkTarget,
  type CanonicalPath,
} from "../namespace/paths.js";
import { bytesToHex, copyBytes, equalBytes, hexToBytes, intrinsicByteRange } from "../cas/bytes.js";
import {
  branchPatchInsertDigest,
  computeBranchGenerationDigest,
  type BranchGenerationExpectation,
  type BranchGenerationNode,
} from "./generation-digest.js";
import { encodeUtf8, utf8ByteLength } from "../namespace/utf8.js";
import {
  prepareContent,
  readManifestInto,
  readManifestRange,
  type PreparedManifest,
} from "../operations/manifest-io.js";
import {
  tryPrepareDurableEditedContentSync,
  type DurableContentEdit,
  type DurableEditSource,
} from "./durable-edit-prepare.js";
import type { SynchronousContentSource } from "./streaming-prepare.js";
import { checkedInteger, checkedAdd } from "../resources/safe-integers.js";
import type { CowPage, CowPageBytes } from "../cow/pages.js";
import {
  decodeManifestRoot,
  type ManifestParameters,
} from "../manifests/codec.js";
import { fsError } from "../filesystem/errors.js";
import { ContentCache } from "../cache/content-cache.js";
import type {
  AuthenticatedManifestCursor,
  BranchChangeRow as ChangeRow,
  BranchRow,
  BranchStore,
  ClosureCertificate,
  EntryRow,
  InodeRow,
  NamespaceStore,
  OperationsStorage,
  PersistedPatch,
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
import type {
  NodeVfsBranchOperations,
  NodeVfsCommitResult,
  NodeVfsOverwriteEdit,
  NodeVfsPinnedReadBridge,
  SyncPreparedContent,
} from "./node-vfs-bridge.js";
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
  readonly mutationTimeMs?: number;
  readonly ancestorTokens?: readonly AncestorToken[];
  readonly subtreeGuard?: boolean;
  readonly conflictRole?: "source" | "destination";
  readonly sourcePath?: string;
  readonly sourceInodeToken?: number | null;
  /** Namespace mutations touch their parent directory; pure content writes do not. */
  readonly touchesParent?: boolean;
  /** Branch generation at which page/patch overlays were last reset by materialization. */
  readonly overlayBaseGeneration?: number;
}
interface AncestorToken {
  readonly path: string;
  readonly inodeId: string | null;
  readonly entryToken: number | null;
}
interface BranchMutation {
  readonly path: string;
  readonly node: DesiredNode | null;
  readonly conflictRole?: "source" | "destination";
  readonly sourcePath?: string;
  readonly sourceInodeToken?: number | null;
  readonly subtreeGuard?: boolean;
  readonly touchesParent?: boolean;
  readonly mutationTimeMs?: number;
}
interface ViewNode {
  readonly path: CanonicalPath;
  readonly inode: InodeRow;
  readonly entryToken: number | null;
}
interface OverlayFileState {
  readonly branchId: string;
  readonly inodeId: string;
  readonly node: InodeRow;
  readonly entryToken: number | null;
  readonly size: number;
  readonly baseManifestHash: Uint8Array | null;
  readonly baseGeneration: number;
  readonly pages: boolean;
  readonly patches: boolean;
}
interface PublishCandidate {
  readonly hash: Uint8Array;
  readonly size: number;
  readonly certificate: ClosureCertificate;
}
interface BranchStreamSnapshot {
  readonly state: OverlayFileState;
  readonly leaseId: string;
  readonly ownerId: string;
  readonly ownerNonce: Uint8Array;
  expiresAt: number;
  readonly size: number;
  readonly generation: number;
  readonly releaseAdmission: () => void;
}
interface BranchContentStream {
  readonly stream: ReadableStream<Uint8Array>;
  readonly size: number;
  readonly generation: number;
}

interface BaseComposePiece {
  readonly kind: "base";
  readonly offset: number;
  readonly length: number;
}
interface BytesComposePiece {
  readonly kind: "bytes";
  readonly bytes: Uint8Array;
}
type ComposePiece = BaseComposePiece | BytesComposePiece;

class RetryableBranchMutation extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RetryableBranchMutation";
  }
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();
function reservationNonce(): Uint8Array {
  return globalThis.crypto.getRandomValues(new Uint8Array(16));
}
function encode(value: unknown): Uint8Array {
  return encoder.encode(JSON.stringify(value));
}
function decode<T>(value: Uint8Array): T {
  return JSON.parse(decoder.decode(value)) as T;
}

interface PublicationRequestBinding {
  readonly hasExpectation: boolean;
  readonly expectedGeneration: number | null;
  readonly expectedGenerationDigest: string | null;
}
interface StoredPublicationEnvelope {
  readonly kind: "efs-publication-result-v2";
  readonly request: PublicationRequestBinding;
  readonly result: PublishResult;
}
function publicationRequest(options: PublishOptions): PublicationRequestBinding {
  const hasGeneration = options.expectedGeneration !== undefined;
  const hasDigest = options.expectedGenerationDigest !== undefined;
  if (hasGeneration !== hasDigest)
    throw new BranchError(
      "InvalidPublicationExpectation",
      "expected generation and digest must be supplied together",
    );
  if (!hasGeneration)
    return Object.freeze({
      hasExpectation: false,
      expectedGeneration: null,
      expectedGenerationDigest: null,
    });
  if (
    !Number.isSafeInteger(options.expectedGeneration) ||
    options.expectedGeneration! < 0 ||
    !/^[0-9a-f]{64}$/u.test(options.expectedGenerationDigest!)
  )
    throw new BranchError(
      "InvalidPublicationExpectation",
      "publication expectation is malformed",
    );
  return Object.freeze({
    hasExpectation: true,
    expectedGeneration: options.expectedGeneration!,
    expectedGenerationDigest: options.expectedGenerationDigest!,
  });
}
function samePublicationRequest(
  left: PublicationRequestBinding,
  right: PublicationRequestBinding,
): boolean {
  return (
    left.hasExpectation === right.hasExpectation &&
    left.expectedGeneration === right.expectedGeneration &&
    left.expectedGenerationDigest === right.expectedGenerationDigest
  );
}
function compatiblePublicationRequest(
  stored: PublicationRequestBinding | undefined,
  requested: PublicationRequestBinding,
): boolean {
  // M7 terminal results did not persist a request envelope. They represent the
  // unguarded request only; never let a guarded M8 retry claim one.
  return stored ? samePublicationRequest(stored, requested) : !requested.hasExpectation;
}
function storedPublication(bytes: Uint8Array): {
  readonly request?: PublicationRequestBinding;
  readonly result: PublishResult;
} {
  const value = decode<PublishResult | StoredPublicationEnvelope>(bytes);
  if (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    value.kind === "efs-publication-result-v2"
  )
    return { request: value.request, result: value.result };
  return { result: value as PublishResult };
}

function createEditedStream(
  source: ReadableStream<Uint8Array>,
  sourceSize: number,
  offset: number,
  deleteLength: number,
  insertBytes: Uint8Array,
  windowBytes: number,
  zeroInsertLength = 0,
): ReadableStream<Uint8Array> {
  const reader = source.getReader();
  const prefixEnd = Math.min(sourceSize, offset);
  const skipEnd = Math.min(sourceSize, offset + deleteLength);
  const gapLength = Math.max(0, offset - sourceSize);
  let sourceChunk = new Uint8Array(0);
  let sourceChunkOffset = 0;
  let sourceDone = false;
  let sourcePosition = 0;
  let gapOffset = 0;
  let zeroInsertOffset = 0;
  let insertOffset = 0;
  let finished = false;

  const copySource = async (
    output: Uint8Array,
    outputOffset: number,
    length: number,
    discard: boolean,
  ): Promise<number> => {
    let copied = 0;
    while (copied < length) {
      if (sourceChunkOffset >= sourceChunk.byteLength) {
        if (sourceDone) throw new Error("ECORRUPT: branch source stream ended early");
        const next = await reader.read();
        if (next.done) {
          sourceDone = true;
          throw new Error("ECORRUPT: branch source stream ended early");
        }
        sourceChunk = Uint8Array.from(next.value);
        sourceChunkOffset = 0;
        if (!sourceChunk.byteLength) continue;
      }
      const take = Math.min(
        length - copied,
        sourceChunk.byteLength - sourceChunkOffset,
      );
      if (!discard)
        output.set(
          sourceChunk.subarray(sourceChunkOffset, sourceChunkOffset + take),
          outputOffset + copied,
        );
      sourceChunkOffset += take;
      sourcePosition += take;
      copied += take;
    }
    return copied;
  };

  return new ReadableStream<Uint8Array>({
    pull: async (controller) => {
      if (finished) {
        controller.close();
        return;
      }
      const output = new Uint8Array(windowBytes);
      let written = 0;
      while (written < output.byteLength) {
        if (sourcePosition < prefixEnd) {
          const take = Math.min(
            output.byteLength - written,
            prefixEnd - sourcePosition,
          );
          await copySource(output, written, take, false);
          written += take;
          continue;
        }
        if (sourcePosition < skipEnd) {
          const take = Math.min(output.byteLength - written, skipEnd - sourcePosition);
          await copySource(output, written, take, true);
          continue;
        }
        if (gapOffset < gapLength) {
          const take = Math.min(output.byteLength - written, gapLength - gapOffset);
          output.fill(0, written, written + take);
          gapOffset += take;
          written += take;
          continue;
        }
        if (zeroInsertOffset < zeroInsertLength) {
          const take = Math.min(
            output.byteLength - written,
            zeroInsertLength - zeroInsertOffset,
          );
          output.fill(0, written, written + take);
          zeroInsertOffset += take;
          written += take;
          continue;
        }
        if (insertOffset < insertBytes.byteLength) {
          const take = Math.min(
            output.byteLength - written,
            insertBytes.byteLength - insertOffset,
          );
          output.set(insertBytes.subarray(insertOffset, insertOffset + take), written);
          insertOffset += take;
          written += take;
          continue;
        }
        if (sourcePosition < sourceSize) {
          const take = Math.min(
            output.byteLength - written,
            sourceSize - sourcePosition,
          );
          await copySource(output, written, take, false);
          written += take;
          continue;
        }
        finished = true;
        break;
      }
      if (written) controller.enqueue(output.slice(0, written));
      else controller.close();
    },
    cancel: () => {
      void reader.cancel();
    },
  });
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
function info(row: BranchRow, generationDigest: string): BranchInfo {
  return Object.freeze({
    id: row.id,
    baseRevision: String(row.base_revision),
    state: row.state === 0 ? "active" : row.state === 1 ? "merged" : "discarded",
    generation: row.generation,
    generationDigest,
    createdAt: row.created_at_ms,
    terminalAt: row.terminal_at_ms,
    mergedRevision: row.merged_revision === null ? null : String(row.merged_revision),
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
          const desired = this.visibleDesired(decode<DesiredNode>(exact.encoded));
          inode = fromDesired(
            desired,
            desired.expectedInodeToken ?? this.#branch.generation,
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
  baseDescendants(path: CanonicalPath, limit: number): ViewNode[] {
    const result: ViewNode[] = [];
    const pending = [path];
    while (pending.length) {
      const parent = pending.pop()!;
      for (const child of this.#baseChildren(parent)) {
        result.push(child);
        if (result.length > limit)
          throw new BranchError("LimitExceeded", "changed-path limit exceeded");
        if (child.inode.type === 1) pending.push(child.path);
      }
    }
    return result;
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
      else {
        const desired = this.visibleDesired(decode<DesiredNode>(change.encoded));
        slots.set(remainder, {
          path: canonicalizePath(changePath, this.#filesystem, "readdir"),
          inode: fromDesired(
            desired,
            desired.expectedInodeToken ?? this.#branch.generation,
          ),
          entryToken: change.expected_token,
        });
      }
    }
    return [...slots.values()].sort((a, b) =>
      compareUtf8(a.path.segments.at(-1)!, b.path.segments.at(-1)!),
    );
  }
  #baseChildren(path: CanonicalPath): ViewNode[] {
    const parent = this.resolve(path, true, false, "readdir");
    if (parent.inode.type !== 1)
      throw fsError("ENOTDIR", "readdir", path.value, "path is not a directory");
    const result: ViewNode[] = [];
    for (const row of this.#repository.historyEntries(
      parent.inode.id,
      this.#branch.base_revision,
    )) {
      if (row.tombstone || !row.encoded) continue;
      const entry = decode<EntryRow & { name_sort: string }>(row.encoded);
      if (!entry.inode_id || !entry.name) continue;
      const inode = this.#historicInode(entry.inode_id);
      if (!inode) continue;
      result.push({
        path: canonicalizePath(
          `${path.value === "/" ? "" : path.value}/${entry.name}`,
          this.#filesystem,
          "readdir",
        ),
        inode,
        entryToken: entry.token,
      });
    }
    return result.sort((a, b) =>
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
    const encoded = decode<{
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
    const base: InodeRow = {
      id: encoded.id,
      type: encoded.type,
      mode: encoded.mode,
      birthtime_ms: encoded.birthtime_ms,
      mtime_ms: encoded.mtime_ms,
      ctime_ms: encoded.ctime_ms,
      nlink: encoded.nlink,
      size: encoded.size,
      manifest_hash: encoded.manifest_hash
        ? hexToBytes(encoded.manifest_hash, 32)
        : null,
      symlink_target: encoded.symlink_target,
      token: encoded.token,
    };
    const overlay = this.#repository.inodeOverlay(
      this.#branch.id,
      id,
      this.#filesystem.maxMaterializedBytes,
    );
    if (!overlay) return base;
    const desiredValue = decode<DesiredNode>(overlay);
    return fromDesired(
      desiredValue,
      desiredValue.expectedInodeToken ?? this.#branch.generation,
    );
  }
  visibleDesired(value: DesiredNode): DesiredNode {
    const overlay = this.#repository.inodeOverlay(
      this.#branch.id,
      value.inodeId,
      this.#filesystem.maxMaterializedBytes,
    );
    return overlay ? { ...value, ...decode<DesiredNode>(overlay) } : value;
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
  readonly #concurrency: RuntimeConcurrency;
  readonly #cache: ContentCache;
  readonly #pageBytes: number;
  #mainReadOnly = false;
  #handles = 0;
  #ownerClosed = false;
  readonly #branchHandles = new Set<BranchHandle>();
  readonly #management = new Set<Promise<unknown>>();
  constructor(
    port: OperationsStorage,
    filesystem: FilesystemLimits,
    storage: StorageLimits,
    runtime: RuntimeLimits,
    limits: BranchConfiguration,
    clock: () => number,
    admission: AdmissionController,
    concurrency: RuntimeConcurrency,
    cache: ContentCache,
    cowPageBytes: number,
  ) {
    this.#port = port;
    this.#filesystem = filesystem;
    this.#storage = storage;
    this.#runtime = runtime;
    this.#limits = limits;
    this.#clock = clock;
    this.#admission = admission;
    this.#concurrency = concurrency;
    this.#cache = cache;
    this.#pageBytes = cowPageBytes;
  }
  setMainReadOnly(value: boolean): void {
    this.#mainReadOnly = value;
  }
  async close(): Promise<void> {
    this.#ownerClosed = true;
    while (this.#branchHandles.size || this.#management.size) {
      const waits = [
        ...[...this.#branchHandles].map((handle) => handle.ownerClose()),
        ...this.#management,
      ];
      await Promise.allSettled(waits);
      for (const handle of [...this.#branchHandles]) {
        await handle.ownerClose();
        this.#branchHandles.delete(handle);
      }
    }
    this.#handles = 0;
  }
  ownerClosed(): boolean {
    return this.#ownerClosed;
  }
  acquireOperation(operation: string, path: string): () => void {
    this.#assertOwnerOpen();
    const release = this.#concurrency.tryAcquireOperation();
    if (!release)
      throw fsError("EAGAIN", operation, path, "concurrent operation limit exceeded");
    return release;
  }
  acquireStream(path: string): () => void {
    const release = this.#concurrency.tryAcquireStream();
    if (!release)
      throw fsError("EAGAIN", "readStream", path, "concurrent stream limit exceeded");
    return release;
  }
  reserveStreamChunk(bytes: number): () => void {
    if (!Number.isSafeInteger(bytes) || bytes < 0)
      throw new RangeError("invalid branch stream chunk size");
    try {
      return this.#admission.reserve(bytes);
    } catch {
      throw fsError(
        "EAGAIN",
        "readStream",
        "/",
        "branch stream admission limit exceeded",
      );
    }
  }
  #runManagement<T>(
    operation: string,
    path: string,
    callback: () => T | Promise<T>,
  ): Promise<T> {
    this.#assertOwnerOpen();
    const release = this.#concurrency.tryAcquireOperation();
    if (!release)
      throw fsError("EAGAIN", operation, path, "concurrent operation limit exceeded");
    let work: Promise<T>;
    work = Promise.resolve()
      .then(() => {
        this.#assertOwnerOpen();
        return callback();
      })
      .finally(() => {
        release();
        this.#management.delete(work);
      });
    this.#management.add(work);
    return work;
  }
  async create(input: string | CreateBranchOptions = {}): Promise<EphemeralBranch> {
    this.#assertOwnerOpen();
    const options = typeof input === "string" ? { id: input } : input;
    const id = options.id ?? globalThis.crypto.randomUUID();
    this.#validateId(id, "branch");
    return this.#runManagement("branches.create", id, () => {
      if (this.#handles >= this.#runtime.maxOpenBranchHandles)
        throw fsError(
          "EAGAIN",
          "branches.create",
          "/",
          "open branch handle limit exceeded",
        );
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
    });
  }
  async open(id: string): Promise<EphemeralBranch> {
    this.#assertOwnerOpen();
    this.#validateId(id, "branch");
    return this.#runManagement("branches.open", id, () => {
      const row = this.#transaction("read", (tx) => this.#row(tx, id));
      if (!row)
        throw new BranchError("BranchNotFound", "branch does not exist", {
          branchId: id,
        });
      return this.#handle(row);
    });
  }
  async get(id: string): Promise<BranchInfo> {
    this.#assertOwnerOpen();
    this.#validateId(id, "branch");
    return this.#runManagement("branches.get", id, () => {
      return this.branchInfo(id);
    });
  }
  async replay(operationId: string, branchId?: string): Promise<PublishResult> {
    this.#assertOwnerOpen();
    this.#validateId(operationId, "operation");
    if (branchId !== undefined) this.#validateId(branchId, "branch");
    return this.#runManagement("branches.replay", operationId, () => {
      return this.#transaction("read", (tx) => {
        const row = tx
          .branches(this.#storage)
          .operationResult(operationId, this.#limits.maxConflictResultBytes + 1024);
        if (!row)
          throw new BranchError(
            "OperationNotFound",
            "operation result does not exist",
            {
              operationId,
            },
          );
        if (branchId !== undefined && row.branch_id !== branchId)
          throw new BranchError(
            "OperationBranchMismatch",
            "operation is bound to another branch",
            { branchId, operationId },
          );
        if (row.outcome === -1) {
          if (row.expires_at_ms !== null && row.expires_at_ms <= this.#now())
            throw new BranchError(
              "OperationResultExpired",
              "operation result has expired",
              { operationId },
            );
          throw new BranchError(
            "OperationNotFound",
            "operation result is still being prepared",
            { operationId },
          );
        }
        if (!row.encoded)
          throw new BranchError(
            "OperationResultExpired",
            "operation result has expired",
            { operationId },
          );
        if (row.expires_at_ms === null || row.expires_at_ms <= this.#now())
          throw new BranchError(
            "OperationResultExpired",
            "operation result has expired",
            { operationId },
          );
        return storedPublication(row.encoded).result;
      });
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
  branchInfo(id: string): BranchInfo {
    return this.#transaction("read", (tx) => {
      const row = this.#row(tx, id);
      if (!row)
        throw new BranchError("BranchNotFound", "branch does not exist", {
          branchId: id,
        });
      return info(
        row,
        (row.state === 0
          ? undefined
          : tx.branches(this.#storage).terminalGenerationDigest(id, row.generation)) ??
          this.#generationDigest(tx, row),
      );
    });
  }
  generationDigest(id: string): string {
    return this.#transaction("read", (tx) => {
      const row = this.#row(tx, id);
      if (!row)
        throw new BranchError("BranchNotFound", "branch does not exist", {
          branchId: id,
        });
      return (
        (row.state === 0
          ? undefined
          : tx.branches(this.#storage).terminalGenerationDigest(id, row.generation)) ??
        this.#generationDigest(tx, row)
      );
    });
  }

  generationDigestInTransaction(tx: StorageTransactionPorts, id: string): string {
    const row = this.#row(tx, id);
    if (!row)
      throw new BranchError("BranchNotFound", "branch does not exist", {
        branchId: id,
      });
    return (
      (row.state === 0
        ? undefined
        : tx.branches(this.#storage).terminalGenerationDigest(id, row.generation)) ??
      this.#generationDigest(tx, row)
    );
  }

  #generationDigest(tx: StorageTransactionPorts, branch: BranchRow): string {
    const repository = tx.branches(this.#storage);
    const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
    const changes = view.allChanges();
    const nodes = new Map<string, BranchGenerationNode>();
    const expectations: BranchGenerationExpectation[] = [];
    const references = new Map<string, Uint8Array>();
    const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
    for (const change of changes) {
      const path = decoder.decode(change.path);
      const encoded = change.encoded ? decode<DesiredNode>(change.encoded) : undefined;
      const value = encoded ? view.visibleDesired(encoded) : undefined;
      expectations.push({
        reason:
          value?.conflictRole === "source"
            ? "source-changed"
            : value?.conflictRole === "destination"
              ? "destination-changed"
              : "entry-changed",
        path,
        expectedRevision: null,
        expectedToken:
          change.expected_token === null ? null : String(change.expected_token),
      });
      if (!value) continue;
      if (value.expectedInodeToken !== null)
        expectations.push({
          reason:
            value.conflictRole === "source"
              ? "source-changed"
              : value.conflictRole === "destination"
                ? "destination-changed"
                : "node-changed",
          path,
          expectedRevision: null,
          expectedToken: String(value.expectedInodeToken),
        });
      if (value.sourcePath !== undefined)
        expectations.push({
          reason: "source-changed",
          path: value.sourcePath,
          expectedRevision: null,
          expectedToken:
            value.sourceInodeToken === null || value.sourceInodeToken === undefined
              ? null
              : String(value.sourceInodeToken),
        });
      if (value.subtreeGuard)
        expectations.push({
          reason: "subtree-changed",
          path,
          expectedRevision: String(branch.base_revision),
          expectedToken: null,
        });
      for (const ancestor of value.ancestorTokens ?? [])
        expectations.push({
          reason: "ancestor-changed",
          path: ancestor.path,
          expectedRevision: null,
          expectedToken:
            ancestor.entryToken === null ? null : String(ancestor.entryToken),
        });
      if (value.inodeId.length === 0) continue;
      const manifestHash = value.manifestHash
        ? hexToBytes(value.manifestHash, 32)
        : null;
      if (manifestHash) references.set(bytesToHex(manifestHash), manifestHash);
      const logicalSize = value.size ?? 0;
      const pageCount = Math.ceil(logicalSize / this.#pageBytes);
      const pages: CowPage[] = [];
      if (value.type === 0)
        for (let first = 0; first < pageCount; first += this.#storage.maxQueryBatchSize)
          pages.push(
            ...overlay.headPages(
              branch.id,
              value.inodeId,
              first,
              Math.min(pageCount - 1, first + this.#storage.maxQueryBatchSize - 1),
            ),
          );
      const patches =
        value.type === 0
          ? overlay.patches(
              branch.id,
              value.inodeId,
              (value.overlayBaseGeneration ?? 0) - 1,
            )
          : [];
      const generationPatches = patches.map((patch) => ({
        order: patch.sequence,
        offset: patch.offset,
        deleteLength: patch.deleteLength,
        insertManifestDigest: branchPatchInsertDigest(patch.segments),
      }));
      for (const patch of generationPatches)
        if (patch.insertManifestDigest)
          references.set(
            bytesToHex(patch.insertManifestDigest),
            patch.insertManifestDigest,
          );
      nodes.set(value.inodeId, {
        inodeId: value.inodeId,
        kind: value.type === 0 ? "file" : value.type === 1 ? "directory" : "symlink",
        mode: value.mode,
        birthtimeMs: value.birthtimeMs,
        mtimeMs: value.mtimeMs,
        ctimeMs: value.ctimeMs,
        logicalSize,
        manifestHash,
        pages: pages.map((page) => ({ index: page.index, bytes: page.bytes })),
        patches: generationPatches,
        symlinkTarget: value.symlinkTarget,
      });
    }
    return computeBranchGenerationDigest({
      filesystemId: repository.filesystemId(),
      branchId: branch.id,
      baseRevision: String(branch.base_revision),
      generation: branch.generation,
      namespace: changes.map((change) => {
        const value = change.encoded ? decode<DesiredNode>(change.encoded) : undefined;
        return {
          path: decoder.decode(change.path),
          disposition:
            change.kind === 0 ? ("present" as const) : ("tombstone" as const),
          inodeId: change.kind === 0 && value ? value.inodeId : null,
        };
      }),
      nodes: [...nodes.values()],
      expectations,
      immutableReferences: [...references.values()].map((digest) => ({
        kind: "manifest" as const,
        digest,
      })),
    });
  }

  createNodeVfsOperations(id: string): NodeVfsBranchOperations {
    const opening = this.#transaction("read", (tx) => this.#row(tx, id));
    if (!opening) throw fsError("ENOENT", "openNodeVfs", id, "branch does not exist");
    if (opening.state !== 0)
      throw fsError("EROFS", "openNodeVfs", id, "branch is terminal");
    const assertParent = (path: CanonicalPath, syscall: string): void => {
      if (path.segments.length <= 1) return;
      this.view(id, (view) => {
        const parent = view.resolve(
          `/${path.segments.slice(0, -1).join("/")}`,
          true,
          true,
          syscall,
        );
        if (parent.inode.type !== 1)
          throw fsError("ENOTDIR", syscall, path.value, "parent is not a directory");
      });
    };
    const resolve = (path: string, followFinal: boolean) => {
      const selected = this.view(id, (view) =>
        view.resolve(path, followFinal, true, "nodeVfs"),
      );
      return Object.freeze({
        canonicalPath: selected.path.value,
        stat: stat(selected),
      });
    };
    const openPinnedRead = (path: string): NodeVfsPinnedReadBridge => {
      const selected = this.view(id, (view) =>
        view.resolve(path, true, true, "openFileSync"),
      );
      if (selected.inode.type !== 0)
        throw fsError(
          selected.inode.type === 1 ? "EISDIR" : "EINVAL",
          "openFileSync",
          path,
          "path is not a regular file",
        );
      const snapshot = this.openStreamSnapshot(
        id,
        selected.path.value,
        0,
        selected.inode.size!,
      );
      let closed = false;
      return Object.freeze({
        canonicalPath: selected.path.value,
        inodeId: selected.inode.id,
        stat: stat(selected),
        size: snapshot.size,
        generation: snapshot.generation,
        readIntoSync: (
          destination: Uint8Array,
          destinationOffset: number,
          position: number,
          length: number,
        ): number => {
          if (closed)
            throw fsError("EBADF", "readIntoSync", path, "pinned read is closed");
          return this.readStreamSnapshotInto(
            snapshot,
            destination,
            destinationOffset,
            position,
            length,
          );
        },
        closeSync: (): void => {
          if (closed) return;
          closed = true;
          this.releaseStreamSnapshot(snapshot);
          snapshot.releaseAdmission();
        },
      });
    };
    const commitPrepared = (
      path: string,
      prepared: SyncPreparedContent,
      options: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
        inodeId?: string;
        aliases?: readonly string[];
        expectedGeneration?: number;
      },
    ): NodeVfsCommitResult => {
      const canonical = canonicalizePath(path, this.#filesystem, "commitVisibleSync");
      const selected = this.view(id, (view, _tx, branch) => ({
        destination: view.optional(canonical, false, true, "commitVisibleSync"),
        existing: view.optional(canonical, true, true, "commitVisibleSync"),
        generation: branch.generation,
      }));
      const existing = selected.existing;
      if (existing?.inode.type === 1)
        throw fsError(
          "EISDIR",
          "commitVisibleSync",
          path,
          "destination is a directory",
        );
      if (
        options.inodeId !== undefined &&
        existing?.inode.id !== options.inodeId &&
        !(options.create && !existing)
      )
        throw fsError(
          "EBUSY",
          "commitVisibleSync",
          path,
          "open inode identity no longer matches the commit path",
        );
      const alreadyCommitted = Boolean(
        existing?.inode.type === 0 &&
        existing.inode.id === options.inodeId &&
        existing.inode.size === prepared.size &&
        existing.inode.manifest_hash !== null &&
        equalBytes(existing.inode.manifest_hash, prepared.manifestHash),
      );
      if (options.exclusive && selected.destination && !alreadyCommitted)
        throw fsError("EEXIST", "commitVisibleSync", path, "destination exists");
      if (!existing && options.create === false)
        throw fsError("ENOENT", "commitVisibleSync", path, "file does not exist");
      if (alreadyCommitted) {
        this.#transaction("write", (tx) => {
          this.#active(tx, id);
          this.#releasePrepared(tx, prepared.certificate);
        });
        return Object.freeze({ pinned: openPinnedRead(path) });
      }
      if (
        options.expectedGeneration !== undefined &&
        options.expectedGeneration !== selected.generation
      )
        throw fsError(
          "EAGAIN",
          "commitVisibleSync",
          path,
          "branch changed while the write session was dirty",
        );
      const targetPath = existing?.path ?? canonical;
      assertParent(targetPath, "commitVisibleSync");
      const now = this.#now();
      const inodeId =
        existing?.inode.id ?? options.inodeId ?? globalThis.crypto.randomUUID();
      const aliases = (options.aliases ?? [])
        .map((alias) => canonicalizePath(alias, this.#filesystem, "commitVisibleSync"))
        .filter((alias) => alias.value !== targetPath.value);
      for (const alias of aliases) {
        assertParent(alias, "commitVisibleSync");
        if (this.view(id, (view) => view.optional(alias, false)))
          throw fsError("EEXIST", "commitVisibleSync", alias.value, "alias exists");
      }
      const node: DesiredNode = existing
        ? {
            ...desired(existing.inode),
            size: prepared.size,
            manifestHash: bytesToHex(prepared.manifestHash),
            nlink: existing.inode.nlink + aliases.length,
            mtimeMs: now,
            ctimeMs: now,
          }
        : {
            inodeId,
            type: 0,
            mode: (options.mode ?? 0o644) & 0o7777,
            birthtimeMs: now,
            mtimeMs: now,
            ctimeMs: now,
            nlink: 1 + aliases.length,
            size: prepared.size,
            manifestHash: bytesToHex(prepared.manifestHash),
            symlinkTarget: null,
            expectedInodeToken: null,
          };
      this.mutate(
        id,
        [
          {
            path: targetPath.value,
            node,
            touchesParent: !existing,
            mutationTimeMs: now,
          },
          ...aliases.map((alias) => ({
            path: alias.value,
            node,
            touchesParent: true,
            mutationTimeMs: now,
          })),
        ],
        prepared.certificate,
        selected.generation,
      );
      return Object.freeze({ pinned: openPinnedRead(targetPath.value) });
    };
    const branchEditSource = (
      state: OverlayFileState,
      rootBytes: Uint8Array,
      parameters: ManifestParameters,
    ): DurableEditSource => {
      let cachedWindow:
        | {
            readonly offset: number;
            readonly bytes: Uint8Array;
            readonly release: () => void;
          }
        | undefined;
      const maxReadWindowBytes = Math.max(
        64 * 1024,
        Math.min(
          2 * 1024 * 1024,
          this.#storage.maxFinalTransactionBytes,
          this.#filesystem.maxMaterializedBytes,
        ),
      );
      let readTransactions = 0;
      const readSlice = (offset: number, length: number): Uint8Array => {
        checkedInteger(offset, "manifest read offset");
        checkedInteger(length, "manifest read length");
        if (length === 0) return new Uint8Array(0);
        const end = checkedAdd(offset, length, "manifest read end");
        const cachedEnd = cachedWindow
          ? checkedAdd(cachedWindow.offset, cachedWindow.bytes.byteLength)
          : -1;
        if (!cachedWindow || offset < cachedWindow.offset || end > cachedEnd) {
          cachedWindow?.release();
          cachedWindow = undefined;
          const windowLength = Math.max(length, maxReadWindowBytes);
          const maxOffset = Math.max(0, state.size - windowLength);
          const windowOffset = Math.min(offset, maxOffset);
          const available = Math.min(windowLength, state.size - windowOffset);
          const bytes = this.#transaction("read", (tx) =>
            this.#composeRangeBytes(tx, state, windowOffset, available),
          );
          readTransactions += 1;
          const release = this.#admission.reserve(bytes.byteLength);
          cachedWindow = Object.freeze({ offset: windowOffset, bytes, release });
        }
        const current = cachedWindow!;
        const relativeOffset = offset - current.offset;
        return intrinsicByteRange(
          current.bytes,
          relativeOffset,
          checkedAdd(relativeOffset, length, "manifest cached read end"),
        );
      };
      return Object.freeze({
        manifestHash: copyBytes(state.baseManifestHash!),
        rootBytes: copyBytes(rootBytes),
        size: state.size,
        parameters,
        readStorageTransactions: 1,
        getReadStorageTransactions: () => readTransactions,
        maxReadWindowBytes,
        read: readSlice,
        releaseReadWindow: () => {
          cachedWindow?.release();
          cachedWindow = undefined;
        },
      });
    };
    const plainManifestSource = (
      manifestHash: Uint8Array,
      rootBytes: Uint8Array,
      size: number,
      parameters: ManifestParameters,
    ): DurableEditSource => {
      let cachedWindow:
        | {
            readonly offset: number;
            readonly bytes: Uint8Array;
            readonly release: () => void;
          }
        | undefined;
      const maxReadWindowBytes = Math.max(
        64 * 1024,
        Math.min(
          2 * 1024 * 1024,
          this.#storage.maxFinalTransactionBytes,
          this.#filesystem.maxMaterializedBytes,
        ),
      );
      let readTransactions = 0;
      const readSlice = (offset: number, length: number): Uint8Array => {
        checkedInteger(offset, "manifest read offset");
        checkedInteger(length, "manifest read length");
        if (length === 0) return new Uint8Array(0);
        const end = checkedAdd(offset, length, "manifest read end");
        const cachedEnd = cachedWindow
          ? checkedAdd(cachedWindow.offset, cachedWindow.bytes.byteLength)
          : -1;
        if (!cachedWindow || offset < cachedWindow.offset || end > cachedEnd) {
          cachedWindow?.release();
          cachedWindow = undefined;
          const windowLength = Math.max(length, maxReadWindowBytes);
          const maxOffset = Math.max(0, size - windowLength);
          const windowOffset = Math.min(offset, maxOffset);
          const available = Math.min(windowLength, size - windowOffset);
          const bytes = this.#transaction("read", (tx) =>
            readManifestRange(
              tx.content(this.#storage, this.#cache),
              manifestHash,
              windowOffset,
              available,
              this.#admission,
              this.#cache,
            ),
          );
          readTransactions += 1;
          const release = this.#admission.reserve(bytes.byteLength);
          cachedWindow = Object.freeze({ offset: windowOffset, bytes, release });
        }
        const current = cachedWindow!;
        const relativeOffset = offset - current.offset;
        return intrinsicByteRange(
          current.bytes,
          relativeOffset,
          checkedAdd(relativeOffset, length, "manifest cached read end"),
        );
      };
      return Object.freeze({
        manifestHash: copyBytes(manifestHash),
        rootBytes: copyBytes(rootBytes),
        size,
        parameters,
        readStorageTransactions: 1,
        getReadStorageTransactions: () => readTransactions,
        maxReadWindowBytes,
        read: readSlice,
        releaseReadWindow: () => {
          cachedWindow?.release();
          cachedWindow = undefined;
        },
      });
    };
    const prepareOverwriteSync = (
      path: string,
      offset: number,
      insertion: SynchronousContentSource,
    ): SyncPreparedContent | undefined => {
      checkedInteger(offset, "offset");
      const canonical = canonicalizePath(path, this.#filesystem, "commitVisibleSync");
      let selected:
        | {
            readonly state: OverlayFileState;
            readonly rootBytes: Uint8Array;
            readonly parameters: ManifestParameters;
          }
        | undefined;
      this.#transaction("read", (tx) => {
        const branch = this.#active(tx, id);
        const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
        const state = this.#overlayFileState(
          tx,
          id,
          view,
          canonical,
          "commitVisibleSync",
        );
        if (
          insertion.size === 0 ||
          offset > state.size ||
          insertion.size > state.size - offset
        )
          return undefined;
        if (state.baseManifestHash === null)
          throw new Error("ECORRUPT: branch-visible base manifest is missing");
        const rootBytes = tx
          .content(this.#storage, this.#cache)
          .withManifestRoot(state.baseManifestHash, (encoded) => copyBytes(encoded));
        if (!rootBytes)
          throw new Error("ECORRUPT: branch-visible manifest root is missing");
        selected = Object.freeze({
          state,
          rootBytes,
          parameters: decodeManifestRoot(rootBytes, state.baseManifestHash).parameters,
        });
        return undefined;
      });
      if (!selected) return undefined;
      const edit: DurableContentEdit = Object.freeze({
        offset,
        deleteLength: insertion.size,
        insertLength: insertion.size,
        retainedBytes: insertion.size,
        readInsert: (position: number, length: number): Uint8Array => {
          const output = new Uint8Array(length);
          const read = insertion.readInto(output, 0, position, length);
          if (read !== length)
            throw new Error("Node VFS overwrite source returned an incomplete range");
          return output;
        },
      });
      let prepared;
      prepared = tryPrepareDurableEditedContentSync(
        this.#port,
        branchEditSource(selected.state, selected.rootBytes, selected.parameters),
        edit,
        this.#storage,
        this.#runtime,
        this.#admission,
        this.#cache,
        this.#clock,
      );
      if (!prepared) return undefined;
      if (prepared.mode === "streamed-fallback") {
        this.abandonPrepared(prepared.certificate);
        return undefined;
      }
      return Object.freeze({
        manifestHash: prepared.hash,
        size: prepared.size,
        certificate: prepared.certificate,
        preparationMode: prepared.mode === "durable-path-copy" ? "durable-path-copy" : "local-rebuild",
        sourceBytesRead:
          prepared.localRebuildMetrics?.sourceBytesRead ??
          prepared.pathCopyMetrics?.sourceBytesRead ??
          0,
      });
    };
    const prepareOverwritesSync = (
      path: string,
      edits: readonly NodeVfsOverwriteEdit[],
    ): SyncPreparedContent | undefined => {
      if (edits.length === 0) return undefined;
      if (edits.length === 1)
        return prepareOverwriteSync(path, edits[0]!.offset, edits[0]!.source);
      const canonical = canonicalizePath(path, this.#filesystem, "commitVisibleSync");
      let source:
        | {
            readonly state: OverlayFileState;
            readonly rootBytes: Uint8Array;
            readonly parameters: ManifestParameters;
          }
        | undefined;
      this.#transaction("read", (tx) => {
        const branch = this.#active(tx, id);
        const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
        const state = this.#overlayFileState(
          tx,
          id,
          view,
          canonical,
          "commitVisibleSync",
        );
        if (state.baseManifestHash === null)
          throw new Error("ECORRUPT: branch-visible base manifest is missing");
        const rootBytes = tx
          .content(this.#storage, this.#cache)
          .withManifestRoot(state.baseManifestHash, (encoded) => copyBytes(encoded));
        if (!rootBytes)
          throw new Error("ECORRUPT: branch-visible manifest root is missing");
        source = Object.freeze({
          state,
          rootBytes,
          parameters: decodeManifestRoot(rootBytes, state.baseManifestHash).parameters,
        });
        return undefined;
      });
      if (!source) return undefined;
      let current: SyncPreparedContent | undefined;
      let currentHash = copyBytes(source.state.baseManifestHash!);
      let currentRoot = copyBytes(source.rootBytes);
      let currentSize = source.state.size;
      let currentParameters = source.parameters;
      try {
        for (let index = 0; index < edits.length; index += 1) {
          const edit = edits[index]!;
          checkedInteger(edit.offset, "offset");
          if (
            edit.source.size === 0 ||
            edit.offset > currentSize ||
            edit.source.size > currentSize - edit.offset
          )
            return undefined;
          const prepared = tryPrepareDurableEditedContentSync(
            this.#port,
            index === 0
              ? branchEditSource(source.state, currentRoot, currentParameters)
              : plainManifestSource(
                  currentHash,
                  currentRoot,
                  currentSize,
                  currentParameters,
                ),
            Object.freeze({
              offset: edit.offset,
              deleteLength: edit.source.size,
              insertLength: edit.source.size,
              retainedBytes: edit.source.size,
              readInsert: (position: number, length: number): Uint8Array => {
                const output = new Uint8Array(length);
                const read = edit.source.readInto(output, 0, position, length);
                if (read !== length)
                  throw new Error(
                    "Node VFS overwrite source returned an incomplete range",
                  );
                return output;
              },
            }),
            this.#storage,
            this.#runtime,
            this.#admission,
            this.#cache,
            this.#clock,
          );
          if (!prepared || prepared.mode === "streamed-fallback") {
            if (prepared) this.abandonPrepared(prepared.certificate);
            if (current) this.abandonPrepared(current.certificate);
            return undefined;
          }
          if (current) this.abandonPrepared(current.certificate);
          current = Object.freeze({
            manifestHash: prepared.hash,
            size: prepared.size,
            certificate: prepared.certificate,
            preparationMode:
              prepared.mode === "durable-path-copy"
                ? "durable-path-copy"
                : "local-rebuild",
            sourceBytesRead:
              prepared.localRebuildMetrics?.sourceBytesRead ??
              prepared.pathCopyMetrics?.sourceBytesRead ??
              0,
          });
          if (index + 1 === edits.length) break;
          const rootBytes = this.#transaction("read", (tx) =>
            tx
              .content(this.#storage, this.#cache)
              .withManifestRoot(prepared.hash, (encoded) => copyBytes(encoded)),
          );
          if (!rootBytes) throw new Error("ECORRUPT: missing staged manifest root");
          currentHash = copyBytes(prepared.hash);
          currentRoot = copyBytes(rootBytes);
          currentSize = prepared.size;
          currentParameters = decodeManifestRoot(rootBytes, prepared.hash).parameters;
        }
        return current;
      } catch (error) {
        if (current) this.abandonPrepared(current.certificate);
        throw error;
      }
    };
    const unlink = (path: string, directory: boolean): void => {
      const source = this.view(id, (view) =>
        view.resolve(path, false, true, "nodeVfs"),
      );
      if (source.path.value === "/")
        throw fsError(
          directory ? "EBUSY" : "EPERM",
          directory ? "rmdirSync" : "unlinkSync",
          path,
          "root cannot be removed",
        );
      if (directory) {
        if (source.inode.type !== 1)
          throw fsError("ENOTDIR", "rmdirSync", path, "path is not a directory");
        if (this.view(id, (view) => view.children(source.path).length) !== 0)
          throw fsError("ENOTEMPTY", "rmdirSync", path, "directory is not empty");
      } else if (source.inode.type === 1)
        throw fsError("EISDIR", "unlinkSync", path, "path is a directory");
      this.mutate(id, [
        {
          path: source.path.value,
          node: null,
          touchesParent: true,
          mutationTimeMs: this.#now(),
        },
      ]);
    };
    const operations: NodeVfsBranchOperations = {
      version: (): number => {
        const row = this.#transaction("read", (tx) => this.#row(tx, id));
        if (!row) throw fsError("ENOENT", "nodeVfs", id, "branch is missing");
        if (row.state !== 0)
          throw fsError("EROFS", "nodeVfs", id, "branch is terminal");
        return row.generation;
      },
      resolve,
      openPinnedRead,
      readdir: (path: string): DirectoryEntry[] => {
        const canonical = canonicalizePath(path, this.#filesystem, "readdirSync");
        const parent = this.view(id, (view) => view.resolve(canonical, true));
        if (parent.inode.type !== 1)
          throw fsError("ENOTDIR", "readdirSync", path, "path is not a directory");
        const children = this.view(id, (view) => view.children(canonical));
        if (children.length > this.#filesystem.maxReaddirEntries)
          throw fsError("EFBIG", "readdirSync", path, "listing exceeds limit");
        return children.map((node) => {
          const type = typeName(node.inode.type);
          return Object.freeze({
            name: node.path.segments.at(-1)!,
            parentPath: canonical.value,
            type,
            ...predicates(type),
          });
        });
      },
      readlink: (path: string): string => {
        const node = this.view(id, (view) => view.resolve(path, false));
        if (node.inode.type !== 2)
          throw fsError("EINVAL", "readlinkSync", path, "not a symbolic link");
        return node.inode.symlink_target!;
      },
      readInto: (path, destination, destinationOffset, position, length): number => {
        return this.composeRangeForBranchInto(
          id,
          path,
          destination,
          destinationOffset,
          position,
          length,
        );
      },
      commitPrepared,
      prepareOverwriteSync,
      prepareOverwritesSync,
      mkdir: (path, options): void => {
        const canonical = canonicalizePath(path, this.#filesystem, "mkdirSync");
        if (canonical.value === "/") {
          if (options.recursive) return;
          throw fsError("EEXIST", "mkdirSync", path, "root exists");
        }
        const prefixes = options.recursive
          ? canonical.segments.map(
              (_, index) => `/${canonical.segments.slice(0, index + 1).join("/")}`,
            )
          : [canonical.value];
        if (!options.recursive) assertParent(canonical, "mkdirSync");
        const now = this.#now();
        const changes: BranchMutation[] = [];
        for (const prefix of prefixes) {
          const existing = this.view(id, (view) => view.optional(prefix, false));
          if (existing) {
            if (existing.inode.type !== 1 || !options.recursive)
              throw fsError("EEXIST", "mkdirSync", prefix, "destination exists");
            continue;
          }
          changes.push({
            path: prefix,
            node: {
              inodeId: globalThis.crypto.randomUUID(),
              type: 1,
              mode: (options.mode ?? 0o755) & 0o7777,
              birthtimeMs: now,
              mtimeMs: now,
              ctimeMs: now,
              nlink: 1,
              size: null,
              manifestHash: null,
              symlinkTarget: null,
              expectedInodeToken: null,
            },
            touchesParent: true,
            mutationTimeMs: now,
          });
        }
        if (changes.length) this.mutate(id, changes);
      },
      chmod: (path, mode): void => {
        const node = this.view(id, (view) => view.resolve(path, true));
        const nextMode = mode & 0o7777;
        if (node.inode.mode === nextMode) return;
        const now = this.#now();
        this.mutate(id, [
          {
            path: node.path.value,
            node: { ...desired(node.inode), mode: nextMode, ctimeMs: now },
            mutationTimeMs: now,
          },
        ]);
      },
      link: (existingPath, newPath): void => {
        const source = this.view(id, (view) => view.resolve(existingPath, true));
        if (source.inode.type !== 0)
          throw fsError("EPERM", "linkSync", existingPath, "only files can be linked");
        const destination = canonicalizePath(newPath, this.#filesystem, "linkSync");
        assertParent(destination, "linkSync");
        if (this.view(id, (view) => view.optional(destination, false)))
          throw fsError("EEXIST", "linkSync", newPath, "destination exists");
        const now = this.#now();
        const sourceInodeToken = this.view(
          id,
          (view) => view.base(source.path, false)?.inode.token ?? null,
        );
        this.mutate(id, [
          {
            path: destination.value,
            node: {
              ...desired(source.inode),
              nlink: source.inode.nlink + 1,
              ctimeMs: now,
            },
            touchesParent: true,
            mutationTimeMs: now,
            conflictRole: "destination",
            sourcePath: source.path.value,
            sourceInodeToken,
          },
        ]);
      },
      symlink: (target, path): void => {
        validateSymlinkTarget(target, this.#filesystem, "symlinkSync");
        const destination = canonicalizePath(path, this.#filesystem, "symlinkSync");
        assertParent(destination, "symlinkSync");
        if (this.view(id, (view) => view.optional(destination, false)))
          throw fsError("EEXIST", "symlinkSync", path, "destination exists");
        const now = this.#now();
        this.mutate(id, [
          {
            path: destination.value,
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
            touchesParent: true,
            mutationTimeMs: now,
          },
        ]);
      },
      rename: (oldPath, newPath): void => {
        const source = this.view(id, (view) => view.resolve(oldPath, false));
        const destination = canonicalizePath(newPath, this.#filesystem, "renameSync");
        if (source.path.value === destination.value) return;
        if (
          source.inode.type === 1 &&
          destination.value.startsWith(`${source.path.value}/`)
        )
          throw fsError(
            "EINVAL",
            "renameSync",
            oldPath,
            "directory cannot move into itself",
          );
        assertParent(destination, "renameSync");
        const existing = this.view(id, (view) => view.optional(destination, false));
        if (existing) {
          if (source.inode.type === 1 && existing.inode.type !== 1)
            throw fsError("ENOTDIR", "renameSync", newPath, "type mismatch");
          if (source.inode.type !== 1 && existing.inode.type === 1)
            throw fsError("EISDIR", "renameSync", newPath, "type mismatch");
          if (
            existing.inode.type === 1 &&
            this.view(id, (view) => view.children(existing.path).length)
          )
            throw fsError("ENOTEMPTY", "renameSync", newPath, "directory is not empty");
        }
        const now = this.#now();
        const changes: BranchMutation[] = [
          {
            path: destination.value,
            node: { ...desired(source.inode), ctimeMs: now },
            conflictRole: "destination",
            sourcePath: source.path.value,
            subtreeGuard: source.inode.type === 1,
            touchesParent: true,
            mutationTimeMs: now,
          },
          {
            path: source.path.value,
            node: null,
            conflictRole: "source",
            sourcePath: source.path.value,
            subtreeGuard: source.inode.type === 1,
            touchesParent: true,
            mutationTimeMs: now,
          },
        ];
        if (
          source.inode.type === 1 &&
          !this.view(id, (view) => view.base(source.path, false))
        ) {
          const descendants: ViewNode[] = [];
          const pending = [source.path];
          while (pending.length) {
            const parent = pending.pop()!;
            for (const child of this.view(id, (view) => view.children(parent))) {
              descendants.push(child);
              if (child.inode.type === 1) pending.push(child.path);
            }
          }
          for (const child of descendants.sort((a, b) =>
            compareUtf8(a.path.value, b.path.value),
          )) {
            const suffix = child.path.value.slice(source.path.value.length);
            changes.push({
              path: `${destination.value}${suffix}`,
              node: desired(child.inode),
              mutationTimeMs: now,
            });
            changes.push({ path: child.path.value, node: null, mutationTimeMs: now });
          }
        }
        this.mutate(id, changes);
      },
      unlink: (path): void => unlink(path, false),
      rmdir: (path): void => unlink(path, true),
    };
    return Object.freeze(operations);
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
    changes: readonly BranchMutation[],
    certificate?: ClosureCertificate,
    expectedGeneration?: number,
  ): void {
    this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      if (expectedGeneration !== undefined && branch.generation !== expectedGeneration)
        throw new BranchError(
          "BranchChanged",
          "branch generation changed during mutation preparation",
          { branchId: id },
        );
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const repository = tx.branches(this.#storage);
      const mutationTimeMs =
        changes.find((change) => change.mutationTimeMs)?.mutationTimeMs ?? this.#now();
      if (certificate)
        tx.staging(this.#storage).validateSealed(certificate, this.#now());
      for (const change of changes) {
        const canonical = canonicalizePath(change.path, this.#filesystem, "branch");
        const old = view.change(canonical.value);
        const base = view.base(canonical, false);
        const visible = view.optional(canonical, false, true, "branch");
        const linkedBase = change.node
          ? repository.historicInode(change.node.inodeId, branch.base_revision)
          : undefined;
        const expectedEntry = old ? old.expected_token : (base?.entryToken ?? null);
        const expectedInode = old?.encoded
          ? decode<DesiredNode>(old.encoded).expectedInodeToken
          : (base?.inode.token ??
            (change.conflictRole === "destination"
              ? null
              : linkedBase
                ? decode<{
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
                  }>(linkedBase.encoded!).token
                : null));
        const ancestors: AncestorToken[] = [];
        for (let index = 1; index < canonical.segments.length; index += 1) {
          const ancestorPath = `/${canonical.segments.slice(0, index).join("/")}`;
          const ancestor = view.base(ancestorPath, false);
          ancestors.push({
            path: ancestorPath,
            inodeId: ancestor?.inode.id ?? null,
            entryToken: ancestor?.entryToken ?? null,
          });
        }
        const value = change.node
          ? {
              ...change.node,
              expectedInodeToken: expectedInode,
              mutationTimeMs: change.mutationTimeMs ?? mutationTimeMs,
              ancestorTokens: ancestors,
              subtreeGuard: change.subtreeGuard ?? false,
              touchesParent: change.touchesParent ?? false,
              ...(change.conflictRole === undefined
                ? {}
                : { conflictRole: change.conflictRole }),
              ...(change.sourcePath === undefined
                ? {}
                : { sourcePath: change.sourcePath }),
              ...(change.sourceInodeToken === undefined
                ? {}
                : { sourceInodeToken: change.sourceInodeToken }),
              ...(change.node.manifestHash === null ||
              change.node.manifestHash === undefined
                ? {}
                : { overlayBaseGeneration: branch.generation + 1 }),
            }
          : ({
              inodeId: base?.inode.id ?? "",
              type: (base?.inode.type ?? 0) as 0 | 1 | 2,
              mode: base?.inode.mode ?? 0,
              birthtimeMs: base?.inode.birthtime_ms ?? 0,
              mtimeMs: base?.inode.mtime_ms ?? 0,
              ctimeMs: base?.inode.ctime_ms ?? 0,
              nlink: base?.inode.nlink ?? 0,
              size: base?.inode.size ?? null,
              manifestHash: base?.inode.manifest_hash
                ? bytesToHex(base.inode.manifest_hash)
                : null,
              symlinkTarget: base?.inode.symlink_target ?? null,
              expectedInodeToken: expectedInode,
              mutationTimeMs: change.mutationTimeMs ?? mutationTimeMs,
              ancestorTokens: ancestors,
              subtreeGuard: change.subtreeGuard ?? false,
              touchesParent: change.touchesParent ?? false,
              ...(change.conflictRole === undefined
                ? {}
                : { conflictRole: change.conflictRole }),
              ...(change.sourcePath === undefined
                ? {}
                : { sourcePath: change.sourcePath }),
              ...(change.sourceInodeToken === undefined
                ? {}
                : { sourceInodeToken: change.sourceInodeToken }),
            } satisfies DesiredNode);
        const pathBytes = encodeUtf8(canonical.value);
        if (
          visible &&
          (!change.node || change.node.inodeId !== visible.inode.id) &&
          !changes.some(
            (other) =>
              other !== change &&
              other.node?.inodeId === visible.inode.id &&
              other.path !== change.path,
          )
        ) {
          const linkAdjusted = {
            ...desired(visible.inode),
            ctimeMs: mutationTimeMs,
            mutationTimeMs,
            nlink: Math.max(0, visible.inode.nlink - 1),
          };
          repository.putInodeOverlay(
            id,
            visible.inode.id,
            visible.inode.token,
            encode(linkAdjusted),
          );
        }
        repository.putChange(
          id,
          pathBytes,
          expectedEntry,
          change.node ? 0 : 1,
          encode(value),
        );
        if (base?.inode.id || linkedBase) {
          repository.putInodeExpectation(
            id,
            base?.inode.id ?? change.node!.inodeId,
            expectedInode,
          );
        }
        if (change.node && value.manifestHash) {
          // A complete replacement becomes the new immutable content root.
          // Detach prior page/patch overlays so they cannot be applied again
          // over that replacement on a later branch read.
          const priorOverlay = repository.inodeOverlay(
            id,
            value.inodeId,
            this.#filesystem.maxMaterializedBytes,
          );
          if (
            priorOverlay ||
            (value.expectedInodeToken !== null &&
              repository.historicInode(value.inodeId, branch.base_revision))
          ) {
            const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
            overlay.clearPages(id, value.inodeId);
            repository.putInodeOverlay(
              id,
              value.inodeId,
              value.expectedInodeToken,
              encode(value),
            );
          }
        }
        if (change.node && change.sourceInodeToken !== undefined)
          repository.putInodeOverlay(
            id,
            value.inodeId,
            value.sourceInodeToken ?? null,
            encode(value),
          );
        repository.setManifestRoot(
          id,
          pathBytes,
          change.node && value.manifestHash
            ? hexToBytes(value.manifestHash, 32)
            : undefined,
        );
      }
      const changedPaths = this.#changedPaths(view, view.allChanges());
      if (changedPaths.length > this.#limits.maxChangedPathsPerBranch)
        throw new BranchError("LimitExceeded", "changed-path limit exceeded", {
          branchId: id,
          limit: "maxChangedPathsPerBranch",
        });
      const changedPathBytes = changedPaths.reduce(
        (total, path) => total + utf8ByteLength(path),
        0,
      );
      if (changedPathBytes > this.#limits.maxChangedPathBytes)
        throw new BranchError("LimitExceeded", "changed-path byte limit exceeded", {
          branchId: id,
          limit: "maxChangedPathBytes",
        });
      if (branch.generation >= Number.MAX_SAFE_INTEGER)
        throw new BranchError("LimitExceeded", "branch generation exhausted", {
          branchId: id,
          limit: "generation",
        });
      repository.incrementGeneration(id);
      if (certificate) this.#releasePrepared(tx, certificate);
    });
  }
  async publish(id: string, options: PublishOptions = {}): Promise<PublishResult> {
    if (this.#mainReadOnly)
      throw fsError(
        "EROFS",
        "publish",
        id,
        "replicas cannot publish into their read-only main view",
      );
    const request = publicationRequest(options);
    const operationId = options.operationId ?? null;
    if (options.operationId !== undefined)
      this.#validateId(options.operationId, "operation");
    if (operationId) {
      const prior = this.#transaction("read", (tx) => {
        const branches = tx.branches(this.#storage);
        const result = branches.operationResult(
          operationId,
          this.#limits.maxConflictResultBytes + 1024,
        );
        return result ? { result, branch: branches.row(id) } : undefined;
      });
      if (prior) {
        const result = prior.result;
        if (result.branch_id !== id)
          throw new BranchError(
            "OperationBranchMismatch",
            "operation is bound to another branch",
            { branchId: id, operationId },
          );
        if (result.outcome === -1) {
          if (
            !compatiblePublicationRequest(
              result.encoded ? decode(result.encoded) : undefined,
              request,
            )
          )
            throw new BranchError(
              "OperationRequestMismatch",
              "operation is bound to another guarded request",
              { branchId: id, operationId },
            );
          if (
            !prior.branch ||
            prior.branch.state !== 0 ||
            prior.branch.generation !== result.generation
          )
            throw new BranchError(
              "BranchChanged",
              "operation reservation is bound to another branch generation",
              { branchId: id, operationId },
            );
        } else {
          if (!result.encoded)
            throw new BranchError(
              "OperationResultExpired",
              "operation result has expired",
              { operationId },
            );
          if (result.expires_at_ms === null || result.expires_at_ms <= this.#now())
            throw new BranchError(
              "OperationResultExpired",
              "operation result has expired",
              { operationId },
            );
          const stored = storedPublication(result.encoded);
          if (!compatiblePublicationRequest(stored.request, request))
            throw new BranchError(
              "OperationRequestMismatch",
              "operation is bound to another guarded request",
              { branchId: id, operationId },
            );
          return stored.result;
        }
      }
    }
    const prepared = this.#transaction("read", (tx) => {
      const row = this.#row(tx, id);
      if (!row || row.state !== 0) return null;
      const view = new BranchView(tx, row, this.#filesystem, this.#storage);
      const byInode = new Map<string, { path: string; state: OverlayFileState }>();
      const changes = view.allChanges();
      for (const change of changes) {
        if (change.kind !== 0 || !change.encoded) continue;
        const value = decode<DesiredNode>(change.encoded);
        if (value.type !== 0 || byInode.has(value.inodeId)) continue;
        const state = this.#overlayFileState(
          tx,
          id,
          view,
          canonicalizePath(decoder.decode(change.path), this.#filesystem, "publish"),
          "publish",
        );
        if (state.pages || state.patches)
          byInode.set(value.inodeId, {
            path: decoder.decode(change.path),
            state,
          });
      }
      const generationDigest = this.#generationDigest(tx, row);
      if (
        request.hasExpectation &&
        (request.expectedGeneration !== row.generation ||
          request.expectedGenerationDigest !== generationDigest)
      )
        throw new BranchError(
          "BranchChanged",
          "branch does not match the guarded publication generation",
          { branchId: id, ...(operationId ? { operationId } : {}) },
        );
      return {
        generation: row.generation,
        generationDigest,
        byInode,
        changeCount: changes.length,
        changeBytes: changes.reduce(
          (total, change) =>
            total + change.path.byteLength + (change.encoded?.byteLength ?? 0),
          0,
        ),
        pathResolutionRows: changes.reduce(
          (total, change) =>
            total + Math.max(0, decoder.decode(change.path).split("/").length - 2) * 24,
          0,
        ),
        cleanupRows: tx.branches(this.#storage).terminalCleanupRows(id),
        changedPaths: this.#changedPaths(view, changes),
      };
    });
    const candidates = new Map<string, PublishCandidate>();
    if (prepared)
      this.#validatePublicationEnvelope(
        prepared.changeCount,
        prepared.changeBytes,
        prepared.byInode.size,
        prepared.pathResolutionRows,
        prepared.cleanupRows,
      );
    let reservedReplay: PublishResult | undefined;
    let waitForReservation = false;
    let reservationNonceForAttempt: Uint8Array | undefined;
    if (operationId && prepared) {
      this.#transaction("write", (tx) => {
        const repository = tx.branches(this.#storage);
        const branch = this.#row(tx, id);
        if (!branch || branch.state !== 0)
          throw new BranchError("BranchNotActive", "branch is terminal", {
            branchId: id,
          });
        if (branch.generation !== prepared.generation)
          throw new BranchError(
            "BranchChanged",
            "branch generation changed before operation reservation",
            { branchId: id, operationId },
          );
        const prior = repository.operationResult(
          operationId,
          this.#limits.maxConflictResultBytes + 1024,
        );
        if (prior) {
          if (prior.branch_id !== id)
            throw new BranchError(
              "OperationBranchMismatch",
              "operation is bound to another branch",
              { branchId: id, operationId },
            );
          if (prior.outcome !== -1) {
            if (!prior.encoded)
              throw new BranchError(
                "OperationResultExpired",
                "operation result has expired",
                { operationId },
              );
            if (prior.expires_at_ms !== null && prior.expires_at_ms <= this.#now())
              throw new BranchError(
                "OperationResultExpired",
                "operation result has expired",
                { operationId },
              );
            const stored = storedPublication(prior.encoded);
            if (!compatiblePublicationRequest(stored.request, request))
              throw new BranchError(
                "OperationRequestMismatch",
                "operation is bound to another guarded request",
                { branchId: id, operationId },
              );
            reservedReplay = stored.result;
            return;
          }
          if (
            !compatiblePublicationRequest(
              prior.encoded ? decode(prior.encoded) : undefined,
              request,
            )
          )
            throw new BranchError(
              "OperationRequestMismatch",
              "operation is bound to another guarded request",
              { branchId: id, operationId },
            );
          if (prior.generation !== branch.generation)
            throw new BranchError(
              "BranchChanged",
              "operation reservation is bound to another generation",
              { branchId: id, operationId },
            );
          if (prior.expires_at_ms !== null && prior.expires_at_ms <= this.#now()) {
            const nonce = reservationNonce();
            if (
              !repository.reclaimOperation(
                operationId,
                id,
                branch.generation,
                this.#now(),
                this.#now() +
                  Math.min(this.#limits.publicationResultRetentionMs, 5 * 60_000),
                nonce,
              )
            )
              throw new BranchError(
                "OperationResultExpired",
                "operation reservation could not be reclaimed",
                { operationId },
              );
            reservationNonceForAttempt = nonce;
            return;
          }
          waitForReservation = true;
          return;
        }
        const nonce = reservationNonce();
        repository.reserveOperation(
          operationId,
          id,
          branch.generation,
          this.#now(),
          this.#now() + Math.min(this.#limits.publicationResultRetentionMs, 5 * 60_000),
          nonce,
          encode(request),
        );
        reservationNonceForAttempt = nonce;
      });
      if (reservedReplay) return reservedReplay;
      if (waitForReservation)
        return this.#waitForOperationResult(
          id,
          operationId,
          prepared.generation,
          request,
        );
    }
    try {
      if (prepared) {
        for (const { path, state } of prepared.byInode.values()) {
          // Materialized composition returns both authenticated base values and
          // page/patch rows inside one bounded read transaction. Use it only
          // when two complete logical values plus fixed query headroom fit the
          // final-transaction byte envelope; otherwise stream bounded windows.
          const singleTransactionCompositionBytes = Math.max(
            0,
            Math.floor((this.#storage.maxFinalTransactionBytes - 16 * 1024) / 2),
          );
          const content =
            state.size <= this.#filesystem.maxMaterializedBytes &&
            state.size <= singleTransactionCompositionBytes
              ? this.composeFileForBranch(id, path, "publish")
              : this.#createComposeStream(id, state);
          const manifest = await this.prepare(content, undefined, state.size);
          candidates.set(state.inodeId, {
            hash: manifest.hash,
            size: manifest.size,
            certificate: manifest.certificate,
          });
        }
      }
    } catch (error) {
      for (const candidate of candidates.values())
        this.abandonPrepared(candidate.certificate);
      if (operationId)
        this.#releaseOperationReservation(operationId, reservationNonceForAttempt);
      throw error;
    }
    try {
      return this.#transaction("write", (tx) => {
        const repository = tx.branches(this.#storage);
        const row = this.#row(tx, id);
        if (!row)
          throw new BranchError("BranchNotFound", "branch does not exist", {
            branchId: id,
          });
        if (row.state !== 0) {
          if (operationId) {
            const terminalResult = repository.operationResult(
              operationId,
              this.#limits.maxConflictResultBytes + 1024,
            );
            if (terminalResult && terminalResult.branch_id !== id)
              throw new BranchError(
                "OperationBranchMismatch",
                "operation is bound to another branch",
                { branchId: id, operationId },
              );
            if (terminalResult && terminalResult.outcome !== -1) {
              if (!terminalResult.encoded)
                throw new BranchError(
                  "OperationResultExpired",
                  "operation result has expired",
                  { operationId },
                );
              if (
                terminalResult.expires_at_ms !== null &&
                terminalResult.expires_at_ms <= this.#now()
              )
                throw new BranchError(
                  "OperationResultExpired",
                  "operation result has expired",
                  { operationId },
                );
              const stored = storedPublication(terminalResult.encoded);
              if (!compatiblePublicationRequest(stored.request, request))
                throw new BranchError(
                  "OperationRequestMismatch",
                  "operation is bound to another guarded request",
                  { branchId: id, operationId },
                );
              return stored.result;
            }
            if (terminalResult && terminalResult.outcome !== -1)
              throw new BranchError(
                "OperationResultExpired",
                "operation result has expired",
                { operationId },
              );
          }
          throw new BranchError("BranchNotActive", "branch is terminal", {
            branchId: id,
          });
        }
        const branch = row;
        if (prepared !== null && branch.generation !== prepared.generation)
          throw new BranchError(
            "BranchChanged",
            "branch generation changed during publication preparation",
            { branchId: id },
          );
        const generationDigest = this.#generationDigest(tx, branch);
        if (prepared !== null && generationDigest !== prepared.generationDigest)
          throw new BranchError(
            "BranchChanged",
            "branch generation digest changed during publication preparation",
            { branchId: id },
          );
        if (
          request.hasExpectation &&
          (request.expectedGeneration !== branch.generation ||
            request.expectedGenerationDigest !== generationDigest)
        )
          throw new BranchError(
            "BranchChanged",
            "branch does not match the guarded publication generation",
            { branchId: id, ...(operationId ? { operationId } : {}) },
          );
        if (operationId) {
          const prior = repository.operationResult(
            operationId,
            this.#limits.maxConflictResultBytes + 1024,
          );
          if (prior) {
            if (prior.branch_id !== id)
              throw new BranchError(
                "OperationBranchMismatch",
                "operation is bound to another branch",
                { branchId: id, operationId },
              );
            if (prior.outcome !== -1) {
              if (!prior.encoded)
                throw new BranchError(
                  "OperationResultExpired",
                  "operation result has expired",
                  { operationId },
                );
              if (prior.expires_at_ms !== null && prior.expires_at_ms <= this.#now())
                throw new BranchError(
                  "OperationResultExpired",
                  "operation result has expired",
                  { operationId },
                );
              const stored = storedPublication(prior.encoded);
              if (!compatiblePublicationRequest(stored.request, request))
                throw new BranchError(
                  "OperationRequestMismatch",
                  "operation is bound to another guarded request",
                  { branchId: id, operationId },
                );
              return stored.result;
            }
            if (
              !compatiblePublicationRequest(
                prior.encoded ? decode(prior.encoded) : undefined,
                request,
              )
            )
              throw new BranchError(
                "OperationRequestMismatch",
                "operation is bound to another guarded request",
                { branchId: id, operationId },
              );
            if (prior.expires_at_ms !== null && prior.expires_at_ms <= this.#now())
              throw new BranchError(
                "OperationResultExpired",
                "operation reservation has expired",
                { operationId },
              );
            if (
              !reservationNonceForAttempt ||
              !equalBytes(prior.reservation_nonce, reservationNonceForAttempt)
            )
              throw new BranchError(
                "BranchChanged",
                "operation reservation nonce changed",
                { branchId: id, operationId },
              );
            if (prior.generation !== branch.generation)
              throw new BranchError(
                "BranchChanged",
                "operation reservation is bound to another generation",
                { branchId: id, operationId },
              );
          } else {
            throw new BranchError("BranchChanged", "operation reservation is missing", {
              branchId: id,
              operationId,
            });
          }
        }
        const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
        const changes = view.allChanges();
        const ns = tx.namespace(this.#filesystem, this.#storage, "publish");
        const head = ns.meta().main_revision;
        const conflicts: {
          path: string;
          reason:
            | "entry-changed"
            | "node-changed"
            | "source-changed"
            | "destination-changed"
            | "subtree-changed"
            | "ancestor-changed";
          expectedRevision: string | null;
          actualRevision: string | null;
          directPath: boolean;
        }[] = [];
        for (const change of changes) {
          const path = decoder.decode(change.path);
          const current = this.#currentTokens(ns, path);
          const value = change.encoded
            ? decode<DesiredNode>(change.encoded)
            : undefined;
          let reason:
            | "entry-changed"
            | "node-changed"
            | "source-changed"
            | "destination-changed"
            | "subtree-changed"
            | "ancestor-changed"
            | undefined;
          let expected: number | null = change.expected_token;
          let actual: number | null = current.entry;
          if (
            value?.subtreeGuard &&
            value.inodeId &&
            repository.historicInode(value.inodeId, branch.base_revision) &&
            repository.subtreeChanged(value.inodeId, branch.base_revision)
          ) {
            reason = "subtree-changed";
          } else if (
            value?.ancestorTokens?.some((ancestor) => {
              const token = this.#currentTokens(ns, ancestor.path);
              return (
                token.entry !== ancestor.entryToken ||
                token.inodeId !== ancestor.inodeId
              );
            })
          ) {
            reason = "ancestor-changed";
          } else if (
            value?.sourceInodeToken !== undefined &&
            value.sourcePath !== undefined &&
            (() => {
              const source = this.#currentTokens(ns, value.sourcePath);
              return (
                source.inode !== value.sourceInodeToken ||
                (value.sourceInodeToken === null && source.inodeId !== null)
              );
            })()
          ) {
            const source = this.#currentTokens(ns, value.sourcePath!);
            reason = "source-changed";
            expected = value.sourceInodeToken;
            actual = source.inode;
          } else if (
            value?.expectedInodeToken !== null &&
            value?.expectedInodeToken !== undefined &&
            current.inode !== value.expectedInodeToken
          ) {
            reason =
              value.conflictRole === "source"
                ? "source-changed"
                : value.conflictRole === "destination"
                  ? "destination-changed"
                  : "node-changed";
            expected = value.expectedInodeToken;
            actual = current.inode;
          } else if (current.entry !== change.expected_token) {
            reason =
              value?.conflictRole === "source"
                ? "source-changed"
                : value?.conflictRole === "destination"
                  ? "destination-changed"
                  : "entry-changed";
          }
          if (reason)
            conflicts.push({
              path:
                reason === "source-changed" && value?.sourcePath !== undefined
                  ? value.sourcePath
                  : path,
              directPath:
                !(reason === "source-changed" && value?.sourcePath !== undefined) ||
                path === value?.sourcePath,
              reason,
              expectedRevision: expected === null ? null : String(expected),
              actualRevision: actual === null ? null : String(actual),
            });
        }
        if (conflicts.length) {
          const conflictRank = (
            reason: (typeof conflicts)[number]["reason"],
          ): number =>
            reason === "subtree-changed"
              ? 0
              : reason === "ancestor-changed"
                ? 1
                : reason === "source-changed"
                  ? 2
                  : reason === "destination-changed"
                    ? 3
                    : reason === "node-changed"
                      ? 4
                      : 5;
          const uniqueConflicts = new Map<string, (typeof conflicts)[number]>();
          for (const conflict of conflicts) {
            const prior = uniqueConflicts.get(conflict.path);
            if (
              !prior ||
              conflictRank(conflict.reason) < conflictRank(prior.reason) ||
              (conflictRank(conflict.reason) === conflictRank(prior.reason) &&
                (conflict.reason === "source-changed" &&
                conflict.directPath !== prior.directPath
                  ? conflict.directPath
                  : (conflict.expectedRevision ?? "") < (prior.expectedRevision ?? "")))
            )
              uniqueConflicts.set(conflict.path, conflict);
          }
          const orderedConflicts = [...uniqueConflicts.values()].sort((left, right) =>
            compareUtf8(left.path, right.path),
          );
          if (orderedConflicts.length > this.#limits.maxConflictsPerPublication)
            throw new BranchError("LimitExceeded", "conflict limit exceeded", {
              branchId: id,
              limit: "maxConflictsPerPublication",
            });
          const result: PublishResult = Object.freeze({
            outcome: "conflict",
            branchId: id,
            operationId,
            branchGeneration: branch.generation,
            branchGenerationDigest: generationDigest,
            baseRevision: String(branch.base_revision),
            headRevision: String(head),
            revision: null,
            changedPaths: [] as [],
            conflicts: orderedConflicts.map(
              ({ path, reason, expectedRevision, actualRevision }) => ({
                path,
                reason,
                expectedRevision,
                actualRevision,
              }),
            ),
          });
          this.#releaseCandidates(tx, candidates);
          this.#storeResult(tx, operationId, request, result);
          return result;
        }
        const live = changes
          .filter((change) => change.kind === 0)
          .sort(
            (a, b) =>
              decoder.decode(a.path).split("/").length -
                decoder.decode(b.path).split("/").length ||
              compareUtf8(decoder.decode(a.path), decoder.decode(b.path)),
          );
        const deleted = changes
          .filter((change) => change.kind === 1)
          .sort(
            (a, b) =>
              decoder.decode(b.path).split("/").length -
                decoder.decode(a.path).split("/").length ||
              compareUtf8(decoder.decode(a.path), decoder.decode(b.path)),
          );
        const now = this.#now();
        const revision = ns.nextRevision(now, changes.length * 3 + 1, `branch:${id}`);
        const touched = new Set<string>();
        const effectTimes = new Map<string, number>();
        for (const change of changes) {
          if (!change.encoded) continue;
          const value = view.visibleDesired(decode<DesiredNode>(change.encoded));
          const time = value.mutationTimeMs;
          if (time !== undefined)
            effectTimes.set(
              value.inodeId,
              Math.max(effectTimes.get(value.inodeId) ?? 0, time),
            );
        }
        for (const change of live)
          this.#applyLive(
            tx,
            ns,
            decoder.decode(change.path),
            view.visibleDesired(decode<DesiredNode>(change.encoded!)),
            revision,
            now,
            touched,
            candidates.get(decode<DesiredNode>(change.encoded!).inodeId),
          );
        for (const change of deleted)
          this.#applyDelete(
            tx,
            ns,
            decoder.decode(change.path),
            revision,
            now,
            touched,
            change.encoded ? decode<DesiredNode>(change.encoded) : undefined,
          );
        for (const inodeId of touched) {
          const count = ns.linkCount(inodeId);
          if (count === 0) {
            ns.deleteInode(inodeId);
            ns.recordInode(revision, inodeId, true);
          } else {
            ns.setLinks(inodeId, count, effectTimes.get(inodeId) ?? now, revision);
            ns.recordInode(revision, inodeId);
          }
        }
        repository.putTerminalGenerationDigest(id, branch.generation, generationDigest);
        repository.finish(id, 1, now, revision);
        this.#releaseCandidates(tx, candidates);
        const result: PublishResult = Object.freeze({
          outcome: "merged",
          branchId: id,
          operationId,
          branchGeneration: branch.generation,
          branchGenerationDigest: generationDigest,
          baseRevision: String(branch.base_revision),
          parentRevision: String(head),
          revision: String(revision),
          changedPaths: prepared?.changedPaths ?? this.#changedPaths(view, changes),
          conflicts: [] as [],
        });
        this.#storeResult(tx, operationId, request, result);
        return result;
      });
    } catch (error) {
      for (const candidate of candidates.values())
        this.abandonPrepared(candidate.certificate);
      if (operationId) {
        this.#releaseOperationReservation(operationId, reservationNonceForAttempt);
      }
      throw error;
    }
  }
  async discard(id: string): Promise<BranchInfo> {
    this.#assertOwnerOpen();
    if (this.#mainReadOnly)
      throw fsError(
        "EROFS",
        "discard",
        id,
        "replicas cannot originate terminal branch state",
      );
    return this.#transaction("write", (tx) => {
      const branch = this.#row(tx, id);
      if (!branch)
        throw new BranchError("BranchNotFound", "branch does not exist", {
          branchId: id,
        });
      if (branch.state === 2)
        return info(
          branch,
          tx.branches(this.#storage).terminalGenerationDigest(id, branch.generation) ??
            this.#generationDigest(tx, branch),
        );
      if (branch.state !== 0)
        throw new BranchError("BranchNotActive", "branch is terminal", {
          branchId: id,
        });
      const now = this.#now();
      const generationDigest = this.#generationDigest(tx, branch);
      const repository = tx.branches(this.#storage);
      repository.putTerminalGenerationDigest(id, branch.generation, generationDigest);
      repository.finish(id, 2, now);
      return info(
        { ...branch, state: 2, terminal_at_ms: now, merged_revision: null },
        generationDigest,
      );
    });
  }
  prepare(
    content: Uint8Array | ReadableStream<Uint8Array>,
    signal?: AbortSignal,
    declaredMaxBytes?: number,
  ) {
    return prepareContent(
      this.#port,
      content,
      this.#storage,
      this.#runtime,
      this.#admission,
      signal,
      this.#cache,
      this.#clock,
      declaredMaxBytes,
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
      readManifestRange(
        tx.content(this.#storage, this.#cache),
        hash,
        offset,
        length,
        this.#admission,
        this.#cache,
      ),
    );
  }
  releaseHandle(handle?: BranchHandle): void {
    this.#handles = Math.max(0, this.#handles - 1);
    if (handle) this.#branchHandles.delete(handle);
  }
  #handle(row: BranchRow): EphemeralBranch {
    if (this.#handles >= this.#runtime.maxOpenBranchHandles)
      throw fsError(
        "EAGAIN",
        "branches.open",
        row.id,
        "open branch handle limit exceeded",
      );
    this.#handles += 1;
    const handle = new BranchHandle(this, row.id, this.#filesystem, this.#storage);
    this.#branchHandles.add(handle);
    return handle;
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
    if (
      typeof id !== "string" ||
      id.length === 0 ||
      id.includes("\0") ||
      utf8ByteLength(id) > maximum
    )
      throw new BranchError(
        kind === "branch" ? "InvalidBranchId" : "InvalidOperationId",
        `invalid ${kind} identifier`,
        kind === "branch" ? { branchId: id } : { operationId: id },
      );
  }
  #assertOwnerOpen(): void {
    if (this.#ownerClosed)
      throw fsError("EBADF", "branch", "/", "owning filesystem is closed");
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
  now(): number {
    return this.#now();
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
    request: PublicationRequestBinding,
    result: PublishResult,
  ): void {
    if (!operationId) return;
    const bytes = encode({
      kind: "efs-publication-result-v2",
      request,
      result,
    } satisfies StoredPublicationEnvelope);
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
      result.outcome === "merged" ? Number(result.revision) : null,
    );
  }
  #releaseOperationReservation(
    operationId: string,
    reservationNonce?: Uint8Array,
  ): void {
    try {
      this.#transaction("write", (tx) =>
        tx.branches(this.#storage).releaseOperation(operationId, reservationNonce),
      );
    } catch {}
  }
  async #waitForOperationResult(
    branchId: string,
    operationId: string,
    generation: number,
    request: PublicationRequestBinding,
  ): Promise<PublishResult> {
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      const state = this.#transaction("read", (tx) => ({
        result: tx
          .branches(this.#storage)
          .operationResult(operationId, this.#limits.maxConflictResultBytes + 1024),
        branch: this.#row(tx, branchId),
      }));
      if (!state.result || state.result.branch_id !== branchId)
        throw new BranchError(
          "OperationBranchMismatch",
          "operation reservation disappeared or changed branch",
          { branchId, operationId },
        );
      if (state.result.outcome !== -1) {
        if (!state.result.encoded)
          throw new BranchError(
            "OperationResultExpired",
            "operation result has expired",
            { operationId },
          );
        if (
          state.result.expires_at_ms !== null &&
          state.result.expires_at_ms <= this.#now()
        )
          throw new BranchError(
            "OperationResultExpired",
            "operation result has expired",
            {
              operationId,
            },
          );
        const stored = storedPublication(state.result.encoded);
        if (!compatiblePublicationRequest(stored.request, request))
          throw new BranchError(
            "OperationRequestMismatch",
            "operation is bound to another guarded request",
            { branchId, operationId },
          );
        return stored.result;
      }
      if (
        !compatiblePublicationRequest(
          state.result.encoded ? decode(state.result.encoded) : undefined,
          request,
        )
      )
        throw new BranchError(
          "OperationRequestMismatch",
          "operation is bound to another guarded request",
          { branchId, operationId },
        );
      if (
        state.result.expires_at_ms !== null &&
        state.result.expires_at_ms <= this.#now()
      )
        throw new BranchError(
          "OperationResultExpired",
          "operation result has expired",
          {
            operationId,
          },
        );
      if (
        !state.branch ||
        state.branch.state !== 0 ||
        state.branch.generation !== generation
      )
        throw new BranchError(
          "BranchChanged",
          "operation reservation is bound to another branch generation",
          { branchId, operationId },
        );
      await new Promise<void>((resolve) => setTimeout(resolve, 1));
    }
    throw new BranchError(
      "BranchChanged",
      "operation reservation did not produce a terminal result",
      { branchId, operationId },
    );
  }
  #currentTokens(
    ns: NamespaceStore,
    path: string,
  ): { entry: number | null; inode: number | null; inodeId: string | null } {
    const canonical = canonicalizePath(path, this.#filesystem, "publish");
    if (!canonical.segments.length) {
      const root = ns.resolve("/");
      return { entry: null, inode: root.inode.token, inodeId: root.inode.id };
    }
    try {
      const parent = ns.resolveParent(canonical);
      const entry = ns.entry(parent.parent.inode.id, parent.nameSort);
      if (!entry?.inode_id)
        return { entry: entry?.token ?? null, inode: null, inodeId: null };
      const inode = ns.inode(entry.inode_id);
      return {
        entry: entry.token,
        inode: inode?.token ?? null,
        inodeId: inode?.id ?? null,
      };
    } catch (error) {
      if (
        error instanceof Error &&
        "code" in error &&
        (error.code === "ENOENT" || error.code === "ENOTDIR")
      )
        return { entry: null, inode: null, inodeId: null };
      throw error;
    }
  }
  #changedPaths(view: BranchView, changes: readonly ChangeRow[]): string[] {
    const paths = new Set(changes.map((change) => decoder.decode(change.path)));
    for (const change of changes) {
      if (!change.encoded) continue;
      const value = view.visibleDesired(decode<DesiredNode>(change.encoded));
      if (
        value.type !== 1 ||
        value.sourcePath === undefined ||
        value.sourcePath === decoder.decode(change.path)
      )
        continue;
      const source = canonicalizePath(value.sourcePath, this.#filesystem, "publish");
      const destination = canonicalizePath(
        decoder.decode(change.path),
        this.#filesystem,
        "publish",
      );
      const baseSource = view.base(source, false);
      if (baseSource) {
        const descendants = view.baseDescendants(
          source,
          this.#limits.maxChangedPathsPerBranch,
        );
        for (const descendant of descendants) {
          const suffix = descendant.path.value.slice(source.value.length);
          paths.add(descendant.path.value);
          paths.add(`${destination.value}${suffix}`);
        }
      }
      for (const moved of changes) {
        const movedPath = decoder.decode(moved.path);
        const prefix = `${source.value}/`;
        if (movedPath.startsWith(prefix)) {
          const suffix = movedPath.slice(source.value.length);
          paths.add(movedPath);
          paths.add(`${destination.value}${suffix}`);
        }
      }
    }
    return [...paths].sort(compareUtf8);
  }
  #overlayFileState(
    tx: StorageTransactionPorts,
    id: string,
    view: BranchView,
    path: CanonicalPath,
    syscall: string,
  ): OverlayFileState {
    const node = view.resolve(path, false, true, syscall);
    if (node.inode.type !== 0)
      throw fsError(
        node.inode.type === 1 ? "EISDIR" : "EINVAL",
        syscall,
        path.value,
        "path is not a file",
      );
    const repository = tx.branches(this.#storage);
    const overlay = repository.inodeOverlay(
      id,
      node.inode.id,
      this.#filesystem.maxMaterializedBytes,
    );
    const baseGeneration = overlay
      ? (decode<DesiredNode>(overlay).overlayBaseGeneration ?? 0)
      : 0;
    const ov = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
    return {
      branchId: id,
      inodeId: node.inode.id,
      node: node.inode,
      entryToken: node.entryToken,
      size: node.inode.size!,
      baseManifestHash: node.inode.manifest_hash,
      baseGeneration,
      pages: ov.hasPages(id, node.inode.id),
      patches: ov.hasPatchesAfter(id, node.inode.id, baseGeneration),
    };
  }
  composeRangeForBranch(
    id: string,
    path: string,
    offset: number,
    length: number,
  ): Uint8Array {
    checkedInteger(offset, "offset");
    checkedInteger(length, "length");
    const canonical = canonicalizePath(path, this.#filesystem, "readRange");
    return this.#transaction("read", (tx) => {
      const branch = this.#active(tx, id);
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, "readRange");
      if (offset >= state.size) return new Uint8Array(0);
      if (Math.min(length, state.size - offset) > this.#filesystem.maxMaterializedBytes)
        throw fsError(
          "EFBIG",
          "readRange",
          canonical.value,
          "range exceeds materialization limit",
        );
      return this.#composeRangeBytes(
        tx,
        state,
        offset,
        Math.min(length, state.size - offset),
      );
    });
  }
  composeRangeForBranchInto(
    id: string,
    path: string,
    destination: Uint8Array,
    destinationOffset: number,
    offset: number,
    length: number,
  ): number {
    checkedInteger(offset, "offset");
    checkedInteger(length, "length");
    checkedInteger(destinationOffset, "destinationOffset");
    if (checkedAdd(destinationOffset, length) > destination.byteLength)
      throw new RangeError("invalid branch direct-read destination range");
    const canonical = canonicalizePath(path, this.#filesystem, "readIntoSync");
    return this.#transaction("read", (tx) => {
      const branch = this.#active(tx, id);
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, "readIntoSync");
      const available =
        offset >= state.size ? 0 : Math.min(length, state.size - offset);
      return this.#composeRangeInto(
        tx,
        state,
        offset,
        destination,
        destinationOffset,
        available,
      );
    });
  }
  composeFileForBranch(id: string, path: string, syscall = "readFile"): Uint8Array {
    const canonical = canonicalizePath(path, this.#filesystem, syscall);
    return this.#transaction("read", (tx) => {
      const branch = this.#active(tx, id);
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, syscall);
      if (state.size > this.#filesystem.maxMaterializedBytes)
        throw fsError(
          "EFBIG",
          syscall,
          canonical.value,
          "file exceeds materialization limit",
        );
      return this.#composeRangeBytes(tx, state, 0, state.size);
    });
  }
  composeStreamForBranch(
    id: string,
    path: string,
    syscall = "readStream",
  ): BranchContentStream {
    const canonical = canonicalizePath(path, this.#filesystem, syscall);
    const selected = this.#transaction("read", (tx) => {
      const branch = this.#active(tx, id);
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, syscall);
      return { state, generation: branch.generation };
    });
    return {
      stream: this.#createComposeStream(id, selected.state),
      size: selected.state.size,
      generation: selected.generation,
    };
  }
  openStreamSnapshot(
    id: string,
    path: string,
    offset: number,
    length: number,
  ): BranchStreamSnapshot {
    if (this.#port.readOnly)
      throw fsError(
        "EROFS",
        "readStream",
        path,
        "durable branch stream leases require writable storage",
      );
    const canonical = canonicalizePath(path, this.#filesystem, "readStream");
    const leaseId = globalThis.crypto.randomUUID();
    const ownerId = `branch-stream:${globalThis.crypto.randomUUID()}`;
    const ownerNonce = globalThis.crypto.getRandomValues(new Uint8Array(16));
    const releaseAdmission = this.acquireStream(canonical.value);
    try {
      return this.#transaction("write", (tx) => {
        const branch = this.#active(tx, id);
        const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
        const node = view.resolve(canonical, true, true, "readStream");
        if (node.inode.type !== 0)
          throw fsError(
            node.inode.type === 1 ? "EISDIR" : "EINVAL",
            "readStream",
            canonical.value,
            "path is not a file",
          );
        const state = this.#overlayFileState(tx, id, view, canonical, "readStream");
        if (!state.baseManifestHash)
          throw new Error("ECORRUPT: branch stream base manifest is missing");
        const expiresAt = this.#now() + this.#storage.readLeaseMs;
        tx.staging(this.#storage).acquireReadLease(
          leaseId,
          ownerId,
          ownerNonce,
          state.baseManifestHash,
          expiresAt,
          id,
          branch.generation,
        );
        const selectedOffset = Math.min(offset, state.size);
        const selectedLength = Math.min(length, state.size - selectedOffset);
        try {
          const firstPage = selectedLength
            ? Math.floor(selectedOffset / this.#pageBytes)
            : 0;
          const lastPage = selectedLength
            ? Math.floor((selectedOffset + selectedLength - 1) / this.#pageBytes)
            : -1;
          const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
          const includePages = state.pages && selectedLength > 0;
          const includePatches = state.patches && selectedLength > 0;
          const pinMembership =
            (!includePages && !includePatches) ||
            overlay.leaseMembershipFits(
              id,
              state.inodeId,
              firstPage,
              lastPage,
              state.baseGeneration,
              includePages,
              includePatches,
            );
          if (pinMembership && includePages) {
            overlay.pinHeads(
              leaseId,
              id,
              state.inodeId,
              firstPage,
              lastPage,
              ownerNonce,
            );
          }
          if (pinMembership && includePatches)
            overlay.pinPatches(
              leaseId,
              id,
              state.inodeId,
              ownerNonce,
              state.baseGeneration,
            );
        } catch (error) {
          if (
            error instanceof RangeError &&
            error.message ===
              "branch stream lease exceeds the final transaction envelope"
          )
            throw fsError(
              "EFBIG",
              "readStream",
              canonical.value,
              "branch stream lease exceeds the configured transaction envelope",
            );
          throw error;
        }
        return {
          state,
          leaseId,
          ownerId,
          ownerNonce,
          expiresAt,
          size: state.size,
          generation: branch.generation,
          releaseAdmission,
        };
      });
    } catch (error) {
      releaseAdmission();
      throw error;
    }
  }
  readStreamSnapshot(
    snapshot: BranchStreamSnapshot,
    offset: number,
    length: number,
  ): Uint8Array {
    const available =
      offset >= snapshot.size ? 0 : Math.min(length, snapshot.size - offset);
    const output = new Uint8Array(available);
    const written = this.readStreamSnapshotInto(snapshot, output, 0, offset, available);
    if (written !== output.byteLength)
      throw new Error("ECORRUPT: branch stream direct read ended early");
    return output;
  }
  readStreamSnapshotInto(
    snapshot: BranchStreamSnapshot,
    destination: Uint8Array,
    destinationOffset: number,
    offset: number,
    length: number,
  ): number {
    checkedInteger(offset, "offset");
    checkedInteger(length, "length");
    checkedInteger(destinationOffset, "destinationOffset");
    if (checkedAdd(destinationOffset, length) > destination.byteLength)
      throw new RangeError("invalid branch snapshot direct-read destination range");
    const now = this.#now();
    if (now > snapshot.expiresAt)
      throw fsError("EIO", "readStream", "/", "branch stream lease expired");
    const nextExpiresAt = checkedAdd(
      snapshot.expiresAt,
      this.#storage.readLeaseMs,
      "branch stream lease expiry",
    );
    const renewed = this.#transaction("write", (tx) =>
      tx
        .staging(this.#storage)
        .renewReadLease(
          snapshot.leaseId,
          snapshot.ownerId,
          snapshot.ownerNonce,
          snapshot.expiresAt,
          now,
          nextExpiresAt,
        ),
    );
    if (!renewed)
      throw fsError("EIO", "readStream", "/", "branch stream lease renewal failed");
    snapshot.expiresAt = nextExpiresAt;
    const available =
      offset >= snapshot.size ? 0 : Math.min(length, snapshot.size - offset);
    return this.#transaction("read", (tx) =>
      this.#composeRangeInto(
        tx,
        snapshot.state,
        offset,
        destination,
        destinationOffset,
        available,
        snapshot.leaseId,
        snapshot.ownerNonce,
      ),
    );
  }
  releaseStreamSnapshot(snapshot: BranchStreamSnapshot): void {
    try {
      this.#transaction("write", (tx) => {
        tx.staging(this.#storage).releaseReadLease(
          snapshot.leaseId,
          snapshot.ownerId,
          snapshot.ownerNonce,
        );
      });
    } catch {
      // Closing an owning filesystem may already have closed its database.
    }
  }
  pageWrite(id: string, path: string, offset: number, bytes: Uint8Array): void {
    checkedInteger(offset, "offset");
    if (!Number.isSafeInteger(bytes.byteLength) || bytes.byteLength < 0)
      throw new RangeError("invalid page write length");
    const canonical = canonicalizePath(path, this.#filesystem, "writeRange");
    const now = this.#now();
    this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      if (branch.generation >= Number.MAX_SAFE_INTEGER)
        throw new BranchError("LimitExceeded", "branch generation exhausted", {
          branchId: id,
          limit: "generation",
        });
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, "writeRange");
      if (offset + bytes.byteLength > state.size)
        throw fsError(
          "EINVAL",
          "writeRange",
          canonical.value,
          "range is beyond end of file",
        );
      if (bytes.byteLength === 0) return;
      if (state.patches)
        throw new RetryableBranchMutation("page write requires patch materialization");
      if (state.baseManifestHash === null)
        throw new Error("ECORRUPT: branch file content lacks a base manifest");
      const firstPage = Math.floor(offset / this.#pageBytes);
      const lastPage = Math.floor((offset + bytes.byteLength - 1) / this.#pageBytes);
      const pageCount = lastPage - firstPage + 1;
      const maxPages = Math.min(
        this.#storage.maxQueryBatchSize,
        Math.floor((this.#storage.maxFinalTransactionRows - 3) / 4),
      );
      if (
        pageCount > maxPages ||
        pageCount * 4096 + pageCount * this.#pageBytes >
          this.#storage.maxFinalTransactionBytes
      )
        throw new RetryableBranchMutation(
          "page write span exceeds the bounded overlay batch",
        );
      const current = this.#composeRangeBytes(
        tx,
        state,
        firstPage * this.#pageBytes,
        Math.min(state.size, (lastPage + 1) * this.#pageBytes) -
          firstPage * this.#pageBytes,
      );
      const next = new Uint8Array(current);
      next.set(bytes, offset - firstPage * this.#pageBytes);
      const pages: CowPage[] = [];
      for (let page = firstPage; page <= lastPage; page += 1) {
        const pageOffset = (page - firstPage) * this.#pageBytes;
        const expected = Math.min(this.#pageBytes, state.size - page * this.#pageBytes);
        pages.push(
          Object.freeze({
            index: page,
            bytes: next.slice(pageOffset, pageOffset + expected),
          }),
        );
      }
      tx.overlay(this.#storage, this.#pageBytes as CowPageBytes).writePages(
        id,
        state.inodeId,
        state.size,
        pages,
        now,
      );
      this.#updateOverlayNode(tx, id, state, {
        mtimeMs: now,
        ctimeMs: now,
      });
      this.#overlayChangeRow(
        tx,
        id,
        canonical,
        state,
        {
          mtimeMs: now,
          ctimeMs: now,
        },
        view,
        now,
      );
    });
  }
  patchWrite(
    id: string,
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): void {
    checkedInteger(offset, "offset");
    checkedInteger(deleteLength, "deleteLength");
    const canonical = canonicalizePath(path, this.#filesystem, "replaceRange");
    const now = this.#now();
    const segments = this.#patchSegments(insertBytes);
    this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      if (branch.generation >= Number.MAX_SAFE_INTEGER)
        throw new BranchError("LimitExceeded", "branch generation exhausted", {
          branchId: id,
          limit: "generation",
        });
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, "replaceRange");
      if (offset > state.size || deleteLength > state.size - offset)
        throw fsError("EINVAL", "replaceRange", canonical.value, "invalid range");
      const insertLength = insertBytes.byteLength;
      const nextSize = checkedAdd(
        state.size - deleteLength,
        insertLength,
        "branch file size",
      );
      if (nextSize > this.#storage.maxFileBytes)
        throw fsError(
          "EFBIG",
          "replaceRange",
          canonical.value,
          "file exceeds size limit",
        );
      if (state.pages)
        throw new RetryableBranchMutation("patch write requires page materialization");
      if (nextSize > this.#filesystem.maxMaterializedBytes)
        throw new RetryableBranchMutation(
          "structural patches require bounded materialization",
        );
      const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
      let generation: number;
      try {
        generation = overlay.appendPatch(
          id,
          state.inodeId,
          state.size,
          offset,
          deleteLength,
          segments,
        );
      } catch (error) {
        if (error instanceof RangeError)
          throw new RetryableBranchMutation(
            "structural patch limits require materialization",
          );
        throw error;
      }
      void generation;
      this.#updateOverlayNode(tx, id, state, {
        mtimeMs: now,
        ctimeMs: now,
        size: nextSize,
      });
      this.#overlayChangeRow(
        tx,
        id,
        canonical,
        state,
        {
          mtimeMs: now,
          ctimeMs: now,
          size: nextSize,
        },
        view,
        now,
      );
    });
  }
  materializeFile(
    id: string,
    path: string,
    prepared: PreparedManifest,
    expectedGeneration: number,
  ): void {
    if (!Number.isSafeInteger(expectedGeneration) || expectedGeneration < 0)
      throw new RangeError("invalid materialization generation");
    const canonical = canonicalizePath(path, this.#filesystem, "replaceRange");
    const now = this.#now();
    this.#transaction("write", (tx) => {
      const branch = this.#active(tx, id);
      if (branch.generation !== expectedGeneration)
        throw new RetryableBranchMutation("branch changed during materialization");
      if (branch.generation >= Number.MAX_SAFE_INTEGER)
        throw new BranchError("LimitExceeded", "branch generation exhausted", {
          branchId: id,
          limit: "generation",
        });
      const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
      const state = this.#overlayFileState(tx, id, view, canonical, "replaceRange");
      tx.staging(this.#storage).validateSealed(prepared.certificate, this.#now());
      const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
      overlay.clearPages(id, state.inodeId);
      const resetGeneration = branch.generation + 1;
      this.#updateOverlayNode(tx, id, state, {
        mtimeMs: now,
        ctimeMs: now,
        size: prepared.size,
        manifestHash: bytesToHex(prepared.hash),
        overlayBaseGeneration: resetGeneration,
      });
      this.#overlayChangeRow(tx, id, canonical, state, {
        mtimeMs: now,
        ctimeMs: now,
        size: prepared.size,
        manifestHash: bytesToHex(prepared.hash),
      });
      this.#overlayLimits(tx, id, branch);
      const repository = tx.branches(this.#storage);
      repository.setManifestRoot(id, encodeUtf8(canonical.value), prepared.hash);
      repository.incrementGeneration(id);
      this.#releasePrepared(tx, prepared.certificate);
    });
  }
  releaseBranchLease(leaseId: string, owner: string, ownerNonce: Uint8Array): void {
    try {
      this.#transaction("write", (tx) => {
        tx.staging(this.#storage).releaseReadLease(leaseId, owner, ownerNonce);
      });
    } catch {}
  }
  #updateOverlayNode(
    tx: StorageTransactionPorts,
    id: string,
    state: OverlayFileState,
    updates: Partial<DesiredNode>,
  ): void {
    const repository = tx.branches(this.#storage);
    const prior = repository.inodeOverlay(
      id,
      state.inodeId,
      this.#filesystem.maxMaterializedBytes,
    );
    const value: DesiredNode = Object.freeze({
      ...desired(state.node),
      size: state.size,
      overlayBaseGeneration: state.baseGeneration,
      ...(prior ? decode<DesiredNode>(prior) : {}),
      ...updates,
    });
    repository.putInodeOverlay(
      id,
      state.inodeId,
      value.expectedInodeToken,
      encode(value),
    );
  }
  #overlayChangeRow(
    tx: StorageTransactionPorts,
    id: string,
    path: CanonicalPath,
    state: OverlayFileState,
    updates: Partial<DesiredNode>,
    view?: BranchView,
    mutationTimeMs?: number,
  ): void {
    const repository = tx.branches(this.#storage);
    const row = repository.change(id, encodeUtf8(path.value));
    if ((row && (row.kind !== 0 || !row.encoded)) || (!row && !view))
      throw new Error("ECORRUPT: branch change row is missing for overlay content");
    const ancestors: AncestorToken[] = [];
    if (!row && view) {
      for (let index = 1; index < path.segments.length; index += 1) {
        const ancestorPath = `/${path.segments.slice(0, index).join("/")}`;
        const ancestor = view.base(ancestorPath, false);
        ancestors.push({
          path: ancestorPath,
          inodeId: ancestor?.inode.id ?? null,
          entryToken: ancestor?.entryToken ?? null,
        });
      }
    }
    const value: DesiredNode = Object.freeze({
      ...(row?.encoded ? decode<DesiredNode>(row.encoded) : desired(state.node)),
      ...(row
        ? {}
        : {
            expectedInodeToken: state.node.token,
            ancestorTokens: ancestors,
            touchesParent: false,
          }),
      ...(mutationTimeMs === undefined ? {} : { mutationTimeMs }),
      ...updates,
    });
    repository.putChange(
      id,
      encodeUtf8(path.value),
      row?.expected_token ?? state.entryToken,
      0,
      encode(value),
    );
    if (!row) repository.putInodeExpectation(id, state.inodeId, state.node.token);
  }
  #overlayLimits(tx: StorageTransactionPorts, id: string, branch: BranchRow): void {
    const view = new BranchView(tx, branch, this.#filesystem, this.#storage);
    const changedPaths = this.#changedPaths(view, view.allChanges());
    if (changedPaths.length > this.#limits.maxChangedPathsPerBranch)
      throw new BranchError("LimitExceeded", "changed-path limit exceeded", {
        branchId: id,
        limit: "maxChangedPathsPerBranch",
      });
    const changedPathBytes = changedPaths.reduce(
      (total, path) => total + utf8ByteLength(path),
      0,
    );
    if (changedPathBytes > this.#limits.maxChangedPathBytes)
      throw new BranchError("LimitExceeded", "changed-path byte limit exceeded", {
        branchId: id,
        limit: "maxChangedPathBytes",
      });
    if (branch.generation >= Number.MAX_SAFE_INTEGER)
      throw new BranchError("LimitExceeded", "branch generation exhausted", {
        branchId: id,
        limit: "generation",
      });
  }
  #validatePublicationEnvelope(
    changeCount: number,
    changeBytes: number,
    candidateCount: number,
    pathResolutionRows: number,
    cleanupRows: number,
  ): void {
    // The final transaction revalidates one bounded row per namespace token,
    // writes a fixed revision envelope, and releases each sealed candidate.
    // Preparation has already expanded arbitrary branch content outside this
    // transaction; the final path is intentionally a small constant envelope.
    const rows =
      25 + changeCount * 8 + candidateCount * 8 + pathResolutionRows + cleanupRows;
    const bytes =
      16 * 1024 +
      changeBytes * 2 +
      candidateCount * (this.#storage.maxManifestNodeBytes + 4096);
    if (
      !Number.isSafeInteger(rows) ||
      !Number.isSafeInteger(bytes) ||
      rows > this.#storage.maxFinalTransactionRows ||
      bytes > this.#storage.maxFinalTransactionBytes
    )
      throw new BranchError(
        "LimitExceeded",
        "publication write set exceeds the bounded final transaction envelope",
        {
          limit:
            rows > this.#storage.maxFinalTransactionRows
              ? "maxFinalTransactionRows"
              : "maxFinalTransactionBytes",
        },
      );
  }
  #composeRangeBytes(
    tx: StorageTransactionPorts,
    state: OverlayFileState,
    offset: number,
    length: number,
    leaseId?: string,
    ownerNonce?: Uint8Array,
  ): Uint8Array {
    const output = new Uint8Array(length);
    const written = this.#composeRangeInto(
      tx,
      state,
      offset,
      output,
      0,
      length,
      leaseId,
      ownerNonce,
    );
    if (written !== output.byteLength)
      throw new Error("ECORRUPT: composed branch range ended early");
    return output;
  }
  #composeRangeInto(
    tx: StorageTransactionPorts,
    state: OverlayFileState,
    offset: number,
    destination: Uint8Array,
    destinationOffset: number,
    length: number,
    leaseId?: string,
    ownerNonce?: Uint8Array,
  ): number {
    if (length === 0) return 0;
    if (offset < 0 || length < 0 || offset + length > state.size)
      throw new RangeError("composed range is outside the branch file");
    if (state.baseManifestHash === null)
      throw new Error("ECORRUPT: branch file content lacks a base manifest");
    if (!state.pages && !state.patches)
      return readManifestInto(
        tx.content(this.#storage, this.#cache),
        state.baseManifestHash,
        offset,
        destination,
        destinationOffset,
        length,
      );
    if (state.patches) {
      return this.#composePatchedRangeInto(
        tx,
        state,
        offset,
        destination,
        destinationOffset,
        length,
        leaseId,
        ownerNonce,
      );
    }
    const content = tx.content(this.#storage, this.#cache);
    const chunkBytes = this.#pageBytes * (this.#storage.maxQueryBatchSize - 1);
    let done = 0;
    while (done < length) {
      const chunkLength = Math.min(length - done, chunkBytes);
      const written = readManifestInto(
        content,
        state.baseManifestHash,
        offset + done,
        destination,
        destinationOffset + done,
        chunkLength,
      );
      if (written !== chunkLength)
        throw new Error("ECORRUPT: branch base manifest range ended early");
      const chunk = destination.subarray(
        destinationOffset + done,
        destinationOffset + done + chunkLength,
      );
      this.#applyPageOverrides(
        chunk,
        offset + done,
        this.#pageOverrides(tx, state, offset + done, chunkLength, leaseId, ownerNonce),
      );
      done += chunkLength;
    }
    return length;
  }
  #composePatchedRangeInto(
    tx: StorageTransactionPorts,
    state: OverlayFileState,
    offset: number,
    destination: Uint8Array,
    destinationOffset: number,
    length: number,
    leaseId?: string,
    ownerNonce?: Uint8Array,
  ): number {
    const pieces = this.#composePieces(tx, state, leaseId, ownerNonce);
    const content = tx.content(this.#storage, this.#cache);
    const end = offset + length;
    let logical = 0;
    for (const piece of pieces) {
      const pieceLength = this.#pieceLength(piece);
      const overlapStart = Math.max(offset, logical);
      const overlapEnd = Math.min(end, logical + pieceLength);
      if (overlapEnd > overlapStart) {
        const pieceOffset = overlapStart - logical;
        const targetOffset = destinationOffset + overlapStart - offset;
        if (piece.kind === "bytes")
          destination.set(
            piece.bytes.subarray(pieceOffset, pieceOffset + overlapEnd - overlapStart),
            targetOffset,
          );
        else {
          const written = readManifestInto(
            content,
            state.baseManifestHash!,
            piece.offset + pieceOffset,
            destination,
            targetOffset,
            overlapEnd - overlapStart,
          );
          if (written !== overlapEnd - overlapStart)
            throw new Error("ECORRUPT: patched branch manifest range ended early");
        }
      }
      logical += pieceLength;
      if (logical >= end) break;
    }
    return length;
  }
  #composePieces(
    tx: StorageTransactionPorts,
    state: OverlayFileState,
    leaseId?: string,
    ownerNonce?: Uint8Array,
  ): readonly ComposePiece[] {
    if (!state.baseManifestHash)
      throw new Error("ECORRUPT: branch stream base manifest is missing");
    const baseSize = this.#manifestFileSize(tx, state.baseManifestHash);
    const mutable: ComposePiece[] =
      baseSize === 0 ? [] : [{ kind: "base", offset: 0, length: baseSize }];
    let currentSize = baseSize;
    const split = (at: number): number => {
      if (at < 0 || at > currentSize)
        throw new Error("ECORRUPT: structural patch offset is outside the file");
      let cursorOffset = 0;
      for (let index = 0; index < mutable.length; index += 1) {
        const piece = mutable[index]!;
        const end = cursorOffset + this.#pieceLength(piece);
        if (at === cursorOffset) return index;
        if (at === end) {
          cursorOffset = end;
          continue;
        }
        if (at < end) {
          const leftLength = at - cursorOffset;
          const rightLength = end - at;
          const replacement: ComposePiece[] = [];
          if (leftLength) replacement.push(this.#slicePiece(piece, 0, leftLength));
          if (rightLength)
            replacement.push(this.#slicePiece(piece, leftLength, rightLength));
          mutable.splice(index, 1, ...replacement);
          return index + (leftLength ? 1 : 0);
        }
        cursorOffset = end;
      }
      return mutable.length;
    };
    for (const patch of this.#patchRows(tx, state, leaseId, ownerNonce)) {
      if (patch.offset > currentSize || patch.deleteLength > currentSize - patch.offset)
        throw new Error("ECORRUPT: structural patch replay range is invalid");
      const start = split(patch.offset);
      const end = split(patch.offset + patch.deleteLength);
      mutable.splice(
        start,
        end - start,
        ...patch.segments.map((bytes) => Object.freeze({ kind: "bytes", bytes })),
      );
      currentSize = currentSize - patch.deleteLength + patch.insertLength;
    }
    if (currentSize !== state.size)
      throw new Error("ECORRUPT: structural patch replay produced the wrong size");
    return mutable.map((piece) => Object.freeze(piece));
  }
  #patchRows(
    tx: StorageTransactionPorts,
    state: OverlayFileState,
    leaseId?: string,
    ownerNonce?: Uint8Array,
  ): readonly PersistedPatch[] {
    const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
    return leaseId
      ? overlay.leasedPatches(
          leaseId,
          state.branchId,
          state.inodeId,
          ownerNonce,
          state.baseGeneration,
        )
      : overlay.patches(state.branchId, state.inodeId, state.baseGeneration);
  }
  #pageOverrides(
    tx: StorageTransactionPorts,
    state: OverlayFileState,
    offset: number,
    length: number,
    leaseId?: string,
    ownerNonce?: Uint8Array,
  ): readonly CowPage[] {
    if (length === 0) return [];
    const firstPage = Math.floor(offset / this.#pageBytes);
    const lastPage = Math.floor((offset + length - 1) / this.#pageBytes);
    const overlay = tx.overlay(this.#storage, this.#pageBytes as CowPageBytes);
    return leaseId
      ? overlay.leasedPages(
          leaseId,
          state.branchId,
          state.inodeId,
          firstPage,
          lastPage,
          state.baseGeneration,
          ownerNonce,
        )
      : overlay.headPages(state.branchId, state.inodeId, firstPage, lastPage);
  }
  #applyPageOverrides(
    target: Uint8Array,
    targetOffset: number,
    overrides: readonly CowPage[],
  ): void {
    for (const page of overrides) {
      const pageStart = page.index * this.#pageBytes;
      const overlapStart = Math.max(pageStart, targetOffset);
      const overlapEnd = Math.min(
        pageStart + page.bytes.byteLength,
        targetOffset + target.byteLength,
      );
      if (overlapEnd <= overlapStart) continue;
      target.set(
        page.bytes.subarray(
          overlapStart - pageStart,
          overlapStart - pageStart + (overlapEnd - overlapStart),
        ),
        overlapStart - targetOffset,
      );
    }
  }
  #manifestFileSize(tx: StorageTransactionPorts, hash: Uint8Array): number {
    const size = tx
      .content(this.#storage, this.#cache)
      .withManifestRoot(hash, (encoded) => decodeManifestRoot(encoded, hash).fileSize);
    if (size === undefined)
      throw new Error("ECORRUPT: branch base manifest root is missing");
    return size;
  }
  #patchSegments(bytes: Uint8Array): Uint8Array[] {
    if (!bytes.byteLength) return [];
    const segments: Uint8Array[] = [];
    for (let offset = 0; offset < bytes.byteLength; offset += 524_288)
      segments.push(bytes.slice(offset, Math.min(bytes.byteLength, offset + 524_288)));
    return segments;
  }
  #createComposeStream(
    _id: string,
    state: OverlayFileState,
  ): ReadableStream<Uint8Array> {
    const windowBytes = this.#pageBytes * (this.#storage.maxQueryBatchSize - 1);
    let position = 0;
    let cursor: AuthenticatedManifestCursor | undefined;
    let pieces: readonly ComposePiece[] | undefined;
    let pieceIndex = 0;
    let pieceOffset = 0;
    const buildPieces = (tx: StorageTransactionPorts): readonly ComposePiece[] => {
      if (!state.patches) {
        return [
          Object.freeze({
            kind: "base",
            offset: 0,
            length: state.size,
          }),
        ];
      }
      if (!state.baseManifestHash)
        throw new Error("ECORRUPT: branch stream base manifest is missing");
      const baseSize = this.#manifestFileSize(tx, state.baseManifestHash);
      const mutable: ComposePiece[] =
        baseSize === 0
          ? []
          : [
              {
                kind: "base",
                offset: 0,
                length: baseSize,
              },
            ];
      let currentSize = baseSize;
      const split = (at: number): number => {
        if (at < 0 || at > currentSize)
          throw new Error("ECORRUPT: structural patch offset is outside the file");
        let cursorOffset = 0;
        for (let index = 0; index < mutable.length; index += 1) {
          const piece = mutable[index]!;
          const end = cursorOffset + this.#pieceLength(piece);
          if (at === cursorOffset) return index;
          if (at === end) {
            cursorOffset = end;
            continue;
          }
          if (at < end) {
            const leftLength = at - cursorOffset;
            const rightLength = end - at;
            const replacement: ComposePiece[] = [];
            if (leftLength) replacement.push(this.#slicePiece(piece, 0, leftLength));
            if (rightLength)
              replacement.push(this.#slicePiece(piece, leftLength, rightLength));
            mutable.splice(index, 1, ...replacement);
            return index + (leftLength ? 1 : 0);
          }
          cursorOffset = end;
        }
        return mutable.length;
      };
      for (const patch of this.#patchRows(tx, state)) {
        if (
          patch.offset > currentSize ||
          patch.deleteLength > currentSize - patch.offset
        )
          throw new Error("ECORRUPT: structural patch replay range is invalid");
        const start = split(patch.offset);
        const end = split(patch.offset + patch.deleteLength);
        const inserted = patch.segments.map((bytes) =>
          Object.freeze({ kind: "bytes", bytes }),
        );
        mutable.splice(start, end - start, ...inserted);
        currentSize = currentSize - patch.deleteLength + patch.insertLength;
      }
      if (currentSize !== state.size)
        throw new Error("ECORRUPT: structural patch replay produced the wrong size");
      return mutable.map((piece) => Object.freeze(piece));
    };
    const readPieces = (tx: StorageTransactionPorts, length: number): Uint8Array => {
      const output = new Uint8Array(length);
      const content = tx.content(this.#storage, this.#cache);
      let written = 0;
      while (written < length) {
        const piece = pieces?.[pieceIndex];
        if (!piece) throw new Error("ECORRUPT: composed stream ended early");
        const available = this.#pieceLength(piece) - pieceOffset;
        const take = Math.min(available, length - written);
        if (piece.kind === "bytes") {
          output.set(piece.bytes.subarray(pieceOffset, pieceOffset + take), written);
        } else {
          const bytes = readManifestRange(
            content,
            state.baseManifestHash!,
            piece.offset + pieceOffset,
            take,
            this.#admission,
            this.#cache,
          );
          output.set(bytes, written);
        }
        written += take;
        pieceOffset += take;
        if (pieceOffset === this.#pieceLength(piece)) {
          pieceIndex += 1;
          pieceOffset = 0;
        }
      }
      return output;
    };
    return new ReadableStream<Uint8Array>({
      pull: (controller) => {
        if (position >= state.size) {
          cursor?.close();
          cursor = undefined;
          controller.close();
          return;
        }
        const length = Math.min(windowBytes, state.size - position);
        this.#transaction("read", (tx) => {
          if (!pieces) pieces = buildPieces(tx);
          let output: Uint8Array;
          if (state.patches) output = readPieces(tx, length);
          else {
            const content = tx.content(this.#storage, this.#cache);
            if (!cursor)
              cursor = content.openManifestCursor(state.baseManifestHash!, position);
            else cursor.bindSource(content);
            output = new Uint8Array(length);
            const written = cursor.readInto(output, 0, output.byteLength);
            if (written !== output.byteLength)
              throw new Error("ECORRUPT: branch base manifest range ended early");
            this.#applyPageOverrides(
              output,
              position,
              this.#pageOverrides(tx, state, position, length),
            );
          }
          controller.enqueue(output);
        });
        position += length;
      },
      cancel: () => {
        cursor?.close();
        cursor = undefined;
      },
    });
  }
  #pieceLength(piece: ComposePiece): number {
    return piece.kind === "bytes" ? piece.bytes.byteLength : piece.length;
  }
  #slicePiece(piece: ComposePiece, offset: number, length: number): ComposePiece {
    if (piece.kind === "bytes")
      return Object.freeze({
        kind: "bytes",
        bytes: piece.bytes.slice(offset, offset + length),
      });
    return Object.freeze({
      kind: "base",
      offset: piece.offset + offset,
      length,
    });
  }
  #applyLive(
    tx: StorageTransactionPorts,
    ns: NamespaceStore,
    path: string,
    value: DesiredNode,
    revision: number,
    now: number,
    touched: Set<string>,
    candidate?: PublishCandidate,
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
      mtimeMs: value.mtimeMs,
      ctimeMs: value.ctimeMs,
      nlink: Math.max(1, value.nlink),
      size: candidate ? candidate.size : value.size,
      manifestHash: candidate
        ? candidate.hash
        : value.manifestHash
          ? hexToBytes(value.manifestHash, 32)
          : null,
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
    if (value.touchesParent)
      this.#touch(tx, ns, parent.parent.inode, value.mutationTimeMs ?? now, revision);
  }
  #applyDelete(
    tx: StorageTransactionPorts,
    ns: NamespaceStore,
    path: string,
    revision: number,
    now: number,
    touched: Set<string>,
    value?: DesiredNode,
  ): void {
    const current = ns.resolveOptional(path, false);
    if (!current || current.parentInode === null) return;
    ns.putEntry(current.parentInode, current.nameSort!, null, null, revision);
    ns.recordEntry(revision, current.parentInode, current.nameSort!, true);
    touched.add(current.inode.id);
    const parent = ns.inode(current.parentInode);
    if (parent) this.#touch(tx, ns, parent, value?.mutationTimeMs ?? now, revision);
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
  #releaseCandidates(
    tx: StorageTransactionPorts,
    candidates: ReadonlyMap<string, PublishCandidate>,
  ): void {
    for (const candidate of candidates.values()) {
      tx.staging(this.#storage).validateSealed(candidate.certificate, this.#now());
      this.#releasePrepared(tx, candidate.certificate);
    }
  }
}

class BranchHandle implements EphemeralBranch {
  readonly id: string;
  readonly #manager: BranchManager;
  readonly #filesystem: FilesystemLimits;
  readonly #streamControllers = new Set<ReadableStreamDefaultController<Uint8Array>>();
  readonly #streamReleases = new Set<() => void>();
  readonly #pending = new Set<Promise<unknown>>();
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
    return this.#run("info", async () => this.#manager.branchInfo(this.id));
  }
  async publish(options?: PublishOptions): Promise<PublishResult> {
    return this.#run("publish", () => this.#manager.publish(this.id, options), true);
  }
  async discard(): Promise<BranchInfo> {
    return this.#run("discard", () => this.#manager.discard(this.id), true);
  }
  readFile(path: string): Promise<Uint8Array>;
  readFile(path: string, options: ReadTextOptions): Promise<string>;
  async readFile(
    path: string,
    options?: ReadTextOptions,
  ): Promise<Uint8Array | string> {
    return this.#run("readFile", async () => {
      this.#assertActive();
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
      const bytes = this.#manager.composeFileForBranch(
        this.id,
        value.path.value,
        "readFile",
      );
      return options ? new TextDecoder().decode(bytes) : bytes;
    });
  }
  async readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array> {
    return this.#run("readRange", async () => {
      this.#assertActive();
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
      return this.#manager.composeRangeForBranch(
        this.id,
        value.path.value,
        options.offset,
        options.length,
      );
    });
  }
  async readStream(
    path: string,
    options: ReadStreamOptions = {},
  ): Promise<ReadableStream<Uint8Array>> {
    return this.#run("readStream", async () => {
      this.#assertActive();
      const offset = options.offset ?? 0;
      const requestedLength = options.length ?? Number.MAX_SAFE_INTEGER;
      if (!Number.isSafeInteger(offset) || offset < 0)
        throw new RangeError("offset must be a nonnegative safe integer");
      if (!Number.isSafeInteger(requestedLength) || requestedLength < 0)
        throw new RangeError("length must be a nonnegative safe integer");
      const snapshot = this.#manager.openStreamSnapshot(
        this.id,
        path,
        offset,
        requestedLength,
      );
      const end = Math.min(snapshot.size, offset + requestedLength);
      let position = offset;
      let released = false;
      let queuedRelease: (() => void) | undefined;
      let controllerReference: ReadableStreamDefaultController<Uint8Array> | undefined;
      const release = (): void => {
        if (released) return;
        released = true;
        queuedRelease?.();
        queuedRelease = undefined;
        this.#streamReleases.delete(release);
        this.#manager.releaseStreamSnapshot(snapshot);
        snapshot.releaseAdmission();
      };
      this.#streamReleases.add(release);
      return new ReadableStream<Uint8Array>({
        start: (controller) => {
          controllerReference = controller;
          this.#streamControllers.add(controller);
        },
        pull: (controller) => {
          queuedRelease?.();
          queuedRelease = undefined;
          if (this.#closed) {
            this.#streamControllers.delete(controller);
            release();
            controller.error(
              fsError("EBADF", "readStream", path, "branch handle is closed"),
            );
            return;
          }
          if (options.signal?.aborted) {
            this.#streamControllers.delete(controller);
            release();
            controller.error(new DOMException("aborted", "AbortError"));
            return;
          }
          if (position >= end) {
            this.#streamControllers.delete(controller);
            controllerReference = undefined;
            release();
            controller.close();
            return;
          }
          const length = Math.min(
            end - position,
            this.#filesystem.preferredStreamChunkBytes,
          );
          let next: Uint8Array;
          let retain: (() => void) | undefined;
          try {
            retain = this.#manager.reserveStreamChunk(length);
            next = this.#manager.readStreamSnapshot(snapshot, position, length);
          } catch (error) {
            retain?.();
            this.#streamControllers.delete(controller);
            controllerReference = undefined;
            release();
            controller.error(error);
            return;
          }
          position += next.length;
          queuedRelease = retain;
          controller.enqueue(next);
        },
        cancel: () => {
          if (controllerReference) this.#streamControllers.delete(controllerReference);
          controllerReference = undefined;
          release();
        },
      });
    });
  }
  async writeFile(
    path: string,
    content: FileContent,
    options: WriteFileOptions = {},
  ): Promise<void> {
    return this.#run("writeFile", () => this.#writeFile(path, content, options), true);
  }
  async #writeFile(
    path: string,
    content: FileContent,
    options: WriteFileOptions = {},
    expectedGeneration?: number,
  ): Promise<void> {
    return (async () => {
      this.#assertActive();
      const canonical = canonicalizePath(path, this.#filesystem, "writeFile");
      const destination = this.#manager.view(this.id, (view) =>
        view.optional(canonical, false, true, "writeFile"),
      );
      if (options.exclusive && destination)
        throw fsError("EEXIST", "writeFile", canonical.value, "destination exists");
      const existing = this.#manager.view(this.id, (view) =>
        view.optional(canonical, true, true, "writeFile"),
      );
      if (!existing && destination?.inode.type === 2)
        throw fsError("ENOENT", "writeFile", canonical.value, "dangling symbolic link");
      if (existing?.inode.type === 1)
        throw fsError(
          "EISDIR",
          "writeFile",
          canonical.value,
          "destination is a directory",
        );
      this.#manager.view(this.id, (view) => {
        const targetPath = existing?.path ?? canonical;
        const parent =
          targetPath.segments.length === 1
            ? view.resolve("/")
            : view.resolve(`/${targetPath.segments.slice(0, -1).join("/")}`);
        if (parent.inode.type !== 1)
          throw fsError(
            "ENOTDIR",
            "writeFile",
            canonical.value,
            "parent is not a directory",
          );
      });
      const mutationGeneration =
        expectedGeneration ??
        this.#manager.view(this.id, (_view, _tx, branch) => branch.generation);
      const input =
        typeof content === "string"
          ? new TextEncoder().encode(content)
          : content instanceof Uint8Array
            ? content.slice()
            : content;
      const prepared = await this.#manager.prepare(
        input,
        options.signal,
        options.maxBytes,
      );
      const now = this.#manager.now();
      const targetPath = existing?.path.value ?? canonical.value;
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
          [
            {
              path: targetPath,
              node,
              touchesParent: !existing,
              mutationTimeMs: now,
            },
          ],
          prepared.certificate,
          mutationGeneration,
        );
      } catch (error) {
        this.#manager.abandonPrepared(prepared.certificate);
        throw error;
      }
    })();
  }
  async writeRange(path: string, offset: number, content: Uint8Array): Promise<void> {
    return this.#run("writeRange", () => this.#writeRange(path, offset, content), true);
  }
  async #writeRange(path: string, offset: number, content: Uint8Array): Promise<void> {
    this.#assertActive();
    if (!Number.isSafeInteger(offset) || offset < 0)
      throw new RangeError("offset must be a nonnegative safe integer");
    const selected = this.#manager.view(this.id, (view, _tx, branch) => ({
      node: view.resolve(path, true, true, "writeRange"),
      generation: branch.generation,
    }));
    const node = selected.node;
    if (node.inode.type !== 0)
      throw fsError(
        node.inode.type === 1 ? "EISDIR" : "EINVAL",
        "writeRange",
        path,
        "path is not a file",
      );
    if (content.byteLength === 0) return;
    if (offset <= node.inode.size! && content.byteLength <= node.inode.size! - offset) {
      try {
        this.#manager.pageWrite(this.id, node.path.value, offset, content.slice());
        return;
      } catch (error) {
        if (!(error instanceof RetryableBranchMutation)) throw error;
      }
    }
    const source = this.#manager.composeStreamForBranch(
      this.id,
      node.path.value,
      "writeRange",
    );
    const nextSize = Math.max(source.size, offset + content.length);
    const edited = createEditedStream(
      source.stream,
      source.size,
      offset,
      0,
      content.slice(),
      this.#filesystem.preferredStreamChunkBytes,
    );
    await this.#writeFile(path, edited, { maxBytes: nextSize }, selected.generation);
  }
  async replaceRange(
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): Promise<void> {
    return this.#run(
      "replaceRange",
      () => this.#replaceRange(path, offset, deleteLength, insertBytes),
      true,
    );
  }
  async #replaceRange(
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): Promise<void> {
    this.#assertActive();
    if (
      !Number.isSafeInteger(offset) ||
      offset < 0 ||
      !Number.isSafeInteger(deleteLength) ||
      deleteLength < 0
    )
      throw new RangeError("range must contain nonnegative safe integers");
    const selected = this.#manager.view(this.id, (view, _tx, branch) => ({
      node: view.resolve(path, true, true, "replaceRange"),
      generation: branch.generation,
    }));
    const node = selected.node;
    if (node.inode.type !== 0)
      throw fsError(
        node.inode.type === 1 ? "EISDIR" : "EINVAL",
        "replaceRange",
        path,
        "path is not a file",
      );
    if (offset > node.inode.size! || deleteLength > node.inode.size! - offset)
      throw fsError("EINVAL", "replaceRange", path, "invalid range");
    if (deleteLength === 0 && insertBytes.byteLength === 0) return;
    if (
      offset <= node.inode.size! &&
      deleteLength <= node.inode.size! - offset &&
      deleteLength === insertBytes.byteLength
    ) {
      try {
        this.#manager.pageWrite(this.id, node.path.value, offset, insertBytes.slice());
        return;
      } catch (error) {
        if (!(error instanceof RetryableBranchMutation)) throw error;
      }
    }
    try {
      this.#manager.patchWrite(
        this.id,
        node.path.value,
        offset,
        deleteLength,
        insertBytes.slice(),
      );
      return;
    } catch (error) {
      if (!(error instanceof RetryableBranchMutation)) throw error;
    }
    const source = this.#manager.composeStreamForBranch(
      this.id,
      node.path.value,
      "replaceRange",
    );
    if (offset > source.size || deleteLength > source.size - offset)
      throw fsError("EINVAL", "replaceRange", path, "invalid range");
    const nextSize = source.size - deleteLength + insertBytes.length;
    const edited = createEditedStream(
      source.stream,
      source.size,
      offset,
      deleteLength,
      insertBytes.slice(),
      this.#filesystem.preferredStreamChunkBytes,
    );
    await this.#writeFile(path, edited, { maxBytes: nextSize }, selected.generation);
  }
  async truncate(path: string, size = 0): Promise<void> {
    return this.#run("truncate", () => this.#truncate(path, size), true);
  }
  async #truncate(path: string, size = 0): Promise<void> {
    this.#assertActive();
    if (!Number.isSafeInteger(size) || size < 0)
      throw new RangeError("size must be a nonnegative safe integer");
    const selected = this.#manager.view(this.id, (view, _tx, branch) => ({
      node: view.resolve(path, true, true, "truncate"),
      generation: branch.generation,
    }));
    const node = selected.node;
    if (node.inode.type !== 0)
      throw fsError(
        node.inode.type === 1 ? "EISDIR" : "EINVAL",
        "truncate",
        path,
        "path is not a file",
      );
    if (size === node.inode.size!) return;
    try {
      this.#manager.patchWrite(
        this.id,
        node.path.value,
        Math.min(size, node.inode.size!),
        Math.max(0, node.inode.size! - size),
        size > node.inode.size!
          ? new Uint8Array(size - node.inode.size!)
          : new Uint8Array(0),
      );
      return;
    } catch (error) {
      if (!(error instanceof RetryableBranchMutation)) throw error;
    }
    const source = this.#manager.composeStreamForBranch(
      this.id,
      node.path.value,
      "truncate",
    );
    const edited = createEditedStream(
      source.stream,
      source.size,
      Math.min(size, source.size),
      Math.max(0, source.size - size),
      new Uint8Array(0),
      this.#filesystem.preferredStreamChunkBytes,
      Math.max(0, size - source.size),
    );
    await this.#writeFile(path, edited, { maxBytes: size }, selected.generation);
  }
  async mkdir(path: string, options: MkdirOptions = {}): Promise<void> {
    return this.#run("mkdir", () => this.#mkdir(path, options), true);
  }
  async #mkdir(path: string, options: MkdirOptions = {}): Promise<void> {
    this.#assertActive();
    const canonical = canonicalizePath(path, this.#filesystem, "mkdir");
    const mutationTimeMs = this.#manager.now();
    const prefixes = options.recursive
      ? canonical.segments.map(
          (_, index) => `/${canonical.segments.slice(0, index + 1).join("/")}`,
        )
      : [canonical.value];
    if (!options.recursive && canonical.segments.length > 1)
      this.#assertParent(canonical, "mkdir");
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
      changes.push({
        path: prefix,
        node: {
          inodeId: globalThis.crypto.randomUUID(),
          type: 1 as const,
          mode: (options.mode ?? 0o777) & 0o7777,
          birthtimeMs: mutationTimeMs,
          mtimeMs: mutationTimeMs,
          ctimeMs: mutationTimeMs,
          nlink: 1,
          size: null,
          manifestHash: null,
          symlinkTarget: null,
          expectedInodeToken: null,
        },
        mutationTimeMs,
      });
    }
    if (changes.length)
      this.#manager.mutate(
        this.id,
        changes.map((change) => ({ ...change, touchesParent: true })),
      );
  }
  async readdir(path: string, options: ReaddirOptions = {}): Promise<DirectoryEntry[]> {
    return this.#run("readdir", () => this.#readdir(path, options));
  }
  async #readdir(
    path: string,
    options: ReaddirOptions = {},
  ): Promise<DirectoryEntry[]> {
    this.#assertActive();
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
    if (options.limit === undefined && nodes.length > limit)
      throw fsError("EFBIG", "readdir", path, "listing exceeds limit");
    return nodes.slice(0, limit).map((node) => {
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
    return this.#run("stat", () => this.#stat(path));
  }
  async #stat(path: string): Promise<FileStat> {
    this.#assertActive();
    return stat(this.#manager.view(this.id, (view) => view.resolve(path, true)));
  }
  async lstat(path: string): Promise<FileStat> {
    return this.#run("lstat", () => this.#lstat(path));
  }
  async #lstat(path: string): Promise<FileStat> {
    this.#assertActive();
    return stat(this.#manager.view(this.id, (view) => view.resolve(path, false)));
  }
  async chmod(path: string, mode: number): Promise<void> {
    return this.#run("chmod", () => this.#chmod(path, mode), true);
  }
  async #chmod(path: string, mode: number): Promise<void> {
    this.#assertActive();
    const node = this.#manager.view(this.id, (view) => view.resolve(path, true));
    const nextMode = mode & 0o7777;
    if (node.inode.mode === nextMode) return;
    const now = this.#manager.now();
    this.#manager.mutate(this.id, [
      {
        path: node.path.value,
        node: {
          ...desired(node.inode),
          mode: nextMode,
          ctimeMs: now,
        },
        mutationTimeMs: now,
      },
    ]);
  }
  async link(existingPath: string, newPath: string): Promise<void> {
    return this.#run("link", () => this.#link(existingPath, newPath), true);
  }
  async #link(existingPath: string, newPath: string): Promise<void> {
    this.#assertActive();
    const source = this.#manager.view(this.id, (view) =>
      view.resolve(existingPath, true),
    );
    if (source.inode.type !== 0)
      throw fsError("EPERM", "link", existingPath, "only files can be linked");
    const destination = canonicalizePath(newPath, this.#filesystem, "link");
    this.#assertParent(destination, "link");
    if (this.#manager.view(this.id, (view) => view.optional(destination, false)))
      throw fsError("EEXIST", "link", destination.value, "destination exists");
    const now = this.#manager.now();
    const sourceInodeToken = this.#manager.view(
      this.id,
      (view) => view.base(source.path, false)?.inode.token ?? null,
    );
    this.#manager.mutate(this.id, [
      {
        path: destination.value,
        node: {
          ...desired(source.inode),
          nlink: source.inode.nlink + 1,
          ctimeMs: now,
        },
        touchesParent: true,
        mutationTimeMs: now,
        conflictRole: "destination",
        sourcePath: source.path.value,
        sourceInodeToken,
      },
    ]);
  }
  async symlink(target: string, path: string): Promise<void> {
    return this.#run("symlink", () => this.#symlink(target, path), true);
  }
  async #symlink(target: string, path: string): Promise<void> {
    this.#assertActive();
    validateSymlinkTarget(target, this.#filesystem, "symlink");
    const destination = canonicalizePath(path, this.#filesystem, "symlink");
    this.#assertParent(destination, "symlink");
    if (this.#manager.view(this.id, (view) => view.optional(destination, false)))
      throw fsError("EEXIST", "symlink", destination.value, "destination exists");
    const now = this.#manager.now();
    this.#manager.mutate(this.id, [
      {
        path: destination.value,
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
        touchesParent: true,
        mutationTimeMs: now,
      },
    ]);
  }
  async readlink(path: string): Promise<string> {
    return this.#run("readlink", () => this.#readlink(path));
  }
  async #readlink(path: string): Promise<string> {
    this.#assertActive();
    const node = this.#manager.view(this.id, (view) => view.resolve(path, false));
    if (node.inode.type !== 2)
      throw fsError("EINVAL", "readlink", path, "not a symbolic link");
    return node.inode.symlink_target!;
  }
  async rename(oldPath: string, newPath: string): Promise<void> {
    return this.#run("rename", () => this.#rename(oldPath, newPath), true);
  }
  async #rename(oldPath: string, newPath: string): Promise<void> {
    this.#assertActive();
    const source = this.#manager.view(this.id, (view) => view.resolve(oldPath, false));
    const destination = canonicalizePath(newPath, this.#filesystem, "rename");
    if (source.path.value === destination.value) return;
    if (
      source.inode.type === 1 &&
      destination.value.startsWith(`${source.path.value}/`)
    )
      throw fsError(
        "EINVAL",
        "rename",
        source.path.value,
        "directory cannot be moved into itself",
      );
    this.#assertParent(destination, "rename");
    const existing = this.#manager.view(this.id, (view) =>
      view.optional(destination, false, true, "rename"),
    );
    if (existing) {
      if (source.inode.type === 1 && existing.inode.type !== 1)
        throw fsError(
          "ENOTDIR",
          "rename",
          destination.value,
          "cannot replace non-directory with directory",
        );
      if (source.inode.type !== 1 && existing.inode.type === 1)
        throw fsError(
          "EISDIR",
          "rename",
          destination.value,
          "cannot replace directory with non-directory",
        );
      if (
        existing.inode.type === 1 &&
        this.#manager.view(this.id, (view) => view.children(existing.path).length)
      )
        throw fsError(
          "ENOTEMPTY",
          "rename",
          destination.value,
          "destination directory is not empty",
        );
    }
    const now = this.#manager.now();
    const changes: BranchMutation[] = [
      {
        path: destination.value,
        node: { ...desired(source.inode), ctimeMs: now },
        conflictRole: "destination",
        sourcePath: source.path.value,
        subtreeGuard: source.inode.type === 1,
        touchesParent: true,
        mutationTimeMs: now,
      },
      {
        path: canonicalizePath(oldPath, this.#filesystem, "rename").value,
        node: null,
        conflictRole: "source",
        sourcePath: source.path.value,
        subtreeGuard: source.inode.type === 1,
        touchesParent: true,
        mutationTimeMs: now,
      },
    ];
    const sourceIsBase = this.#manager.view(this.id, (view) =>
      view.base(source.path, false),
    );
    if (source.inode.type === 1 && !sourceIsBase) {
      const descendants: ViewNode[] = [];
      const pending = [source.path];
      while (pending.length) {
        const parent = pending.pop()!;
        const children = this.#manager.view(this.id, (view) => view.children(parent));
        for (const child of children) {
          descendants.push(child);
          if (child.inode.type === 1) pending.push(child.path);
        }
      }
      descendants.sort((left, right) => compareUtf8(left.path.value, right.path.value));
      for (const descendant of descendants) {
        const suffix = descendant.path.value.slice(source.path.value.length);
        changes.push({
          path: `${destination.value}${suffix}`,
          node: desired(descendant.inode),
          touchesParent: false,
          mutationTimeMs: now,
        });
        changes.push({
          path: descendant.path.value,
          node: null,
          touchesParent: false,
          mutationTimeMs: now,
        });
      }
    }
    this.#manager.mutate(this.id, changes);
  }
  async unlink(path: string): Promise<void> {
    return this.#run("unlink", () => this.#unlink(path), true);
  }
  async #unlink(path: string): Promise<void> {
    this.#assertActive();
    const source = this.#manager.view(this.id, (view) => view.resolve(path, false));
    if (source.inode.type === 1)
      throw fsError("EISDIR", "unlink", path, "path is a directory");
    const now = this.#manager.now();
    this.#manager.mutate(this.id, [
      { path: source.path.value, node: null, touchesParent: true, mutationTimeMs: now },
    ]);
  }
  async rm(path: string, options: RmOptions = {}): Promise<void> {
    return this.#run("rm", () => this.#rm(path, options), true);
  }
  async #rm(path: string, options: RmOptions = {}): Promise<void> {
    this.#assertActive();
    const source = this.#manager.view(this.id, (view) => view.optional(path, false));
    if (!source) {
      if (options.force) return;
      throw fsError("ENOENT", "rm", path, "path does not exist");
    }
    const mutationTimeMs = this.#manager.now();
    const changes: BranchMutation[] = [
      {
        path: source.path.value,
        node: null,
        subtreeGuard: source.inode.type === 1 && (options.recursive ?? false),
        touchesParent: true,
        mutationTimeMs,
      },
    ];
    if (source.inode.type === 1) {
      const children: ViewNode[] = [];
      const pending = [source.path];
      while (pending.length) {
        const parent = pending.pop()!;
        const direct = this.#manager.view(this.id, (view) => view.children(parent));
        for (const child of direct) {
          children.push(child);
          if (child.inode.type === 1) pending.push(child.path);
        }
      }
      if (children.length && !options.recursive)
        throw fsError("ENOTEMPTY", "rm", path, "directory is not empty");
      for (const child of children)
        changes.unshift({
          path: child.path.value,
          node: null,
          subtreeGuard: child.inode.type === 1,
          touchesParent: true,
          mutationTimeMs,
        });
    }
    this.#manager.mutate(this.id, changes);
  }
  async close(): Promise<void> {
    if (this.#closed) {
      await Promise.allSettled([...this.#pending]);
      return;
    }
    this.#closed = true;
    this.#errorStreams();
    for (const release of [...this.#streamReleases]) release();
    await Promise.allSettled([...this.#pending]);
    this.#manager.releaseHandle(this);
  }
  ownerClose(): Promise<void> {
    if (this.#closed)
      return Promise.allSettled([...this.#pending]).then(() => undefined);
    this.#closed = true;
    this.#errorStreams();
    for (const release of [...this.#streamReleases]) release();
    return Promise.allSettled([...this.#pending]).then(() => undefined);
  }
  #errorStreams(): void {
    for (const controller of this.#streamControllers)
      controller.error(
        fsError("EBADF", "readStream", this.id, "branch handle is closed"),
      );
    this.#streamControllers.clear();
    for (const release of [...this.#streamReleases]) release();
  }
  #assertParent(path: CanonicalPath, syscall: string): void {
    if (path.segments.length < 1) return;
    if (path.segments.length === 1) return;
    this.#manager.view(this.id, (view) => {
      const parent = view.resolve(
        `/${path.segments.slice(0, -1).join("/")}`,
        true,
        true,
        syscall,
      );
      if (parent.inode.type !== 1)
        throw fsError("ENOTDIR", syscall, path.value, "parent is not a directory");
    });
  }
  #assertHandle(): void {
    if (this.#closed || this.#manager.ownerClosed())
      throw fsError("EBADF", "branch", this.id, "branch handle is closed");
  }
  #assertActive(): void {
    const row = this.#manager.branchRow(this.id);
    if (row.state !== 0)
      throw new BranchError("BranchNotActive", "branch is terminal", {
        branchId: this.id,
      });
  }
  #run<T>(
    operation: string,
    callback: () => Promise<T>,
    drainOnClose = false,
  ): Promise<T> {
    this.#assertHandle();
    const release = this.#manager.acquireOperation(operation, this.id);
    const work = Promise.resolve().then(() => {
      if (!drainOnClose) this.#assertHandle();
      return callback();
    });
    this.#pending.add(work);
    void work
      .finally(() => {
        release();
        this.#pending.delete(work);
      })
      .catch(() => {});
    return work;
  }
}

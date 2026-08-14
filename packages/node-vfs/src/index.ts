import {
  FilesystemError,
  type EphemeralFS,
  type EphemeralFilesystem,
  type FileStat,
  type RuntimeLimits,
} from "@ephemeralai/fs";
import {
  openNodeVfsBridge,
  type NodeVfsFilesystemBridge,
  type NodeVfsManagedSlab,
  type NodeVfsPreparedContent,
  type NodeVfsPinnedReadBridge,
} from "@ephemeralai/fs/integrations/node-vfs";
import type { NodeSQLiteDriver } from "@ephemeralai/fs-sqlite-node";

export type CowPageBytes = 4096 | 8192 | 16384;
export interface OpenNodeVfsOptions {
  readonly database: NodeSQLiteDriver;
  readonly branchId?: string;
  readonly runtime?: Partial<RuntimeLimits>;
  readonly observer?: NodeVfsObserver;
  readonly ownsDatabase?: boolean;
}
export interface NodeVfsCapabilities {
  readonly cowPageBytes: CowPageBytes;
  readonly runtime: Readonly<RuntimeLimits>;
  readonly preferredReadBytes: number;
  readonly supportsDirectRangeIo: true;
  readonly supportsWriteSessions: true;
  readonly supportsDataSync: boolean;
}
export interface OpenFileOptions {
  readonly writable?: boolean;
  readonly create?: boolean;
  readonly exclusive?: boolean;
  readonly truncate?: boolean;
  readonly mode?: number;
}
export interface FlushOptions {
  readonly dataOnly?: boolean;
}
export interface NodeFileSession {
  readonly id: string;
  readonly path: string;
  readonly writable: boolean;
  readIntoSync(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  readRangeSync(position: number, length: number): Uint8Array;
  writeSync(content: Uint8Array, position: number): number;
  truncateSync(size: number): void;
  statSync(): FileStat;
  stagePrefixSync(): void;
  commitVisibleSync(options?: FlushOptions): void;
  flushSync(options?: FlushOptions): void;
  closeSync(): void;
  abortSync(): void;
}
export interface NodeVfsProvider {
  readonly capabilities: NodeVfsCapabilities;
  readonly metrics: NodeVfsMetrics;
  existsSync(path: string): boolean;
  statSync(path: string): FileStat;
  lstatSync(path: string): FileStat;
  readdirSync(path: string): string[];
  readlinkSync(path: string): string;
  readRangeSync(path: string, position: number, length: number): Uint8Array;
  openFileSync(path: string, options?: OpenFileOptions): NodeFileSession;
  mkdirSync(path: string, options?: { recursive?: boolean; mode?: number }): void;
  chmodSync(path: string, mode: number): void;
  linkSync(existingPath: string, newPath: string): void;
  symlinkSync(target: string, path: string): void;
  renameSync(oldPath: string, newPath: string): void;
  unlinkSync(path: string): void;
  rmdirSync(path: string): void;
  syncSync(): void;
  closeSync(): void;
}
export interface NodeVfsHandle {
  readonly filesystem: EphemeralFilesystem;
  /** Owning core runtime; differs from `filesystem` for a branch-scoped handle. */
  readonly runtime: EphemeralFS;
  readonly provider: NodeVfsProvider;
  close(): Promise<void>;
}

export {
  createNodeVfsSynchronousFileSystem,
  type NodeVfsSynchronousFileSystem,
} from "./synchronous-adapter.js";
export interface NodeVfsMetricsSnapshot {
  readonly openSessions: number;
  readonly peakOpenSessions: number;
  readonly dirtySessions: number;
  readonly residentWriteBytes: number;
  readonly peakResidentWriteBytes: number;
  readonly residentControlBytes: number;
  readonly peakManagedResidentBytes: number;
  readonly stagedLogicalBytes: number;
  readonly admittedWriteBytes: number;
  readonly flushedWriteBytes: number;
  readonly flushCount: number;
  readonly forcedFlushCount: number;
  readonly failedFlushCount: number;
  readonly rejectedWriteCount: number;
  readonly directReadBytes: number;
  readonly coreBatchCount: number;
  readonly cowEditCount: number;
  readonly cowEditSourceBytes: number;
  readonly callbackSizeDistribution: Readonly<{
    upTo4KiB: number;
    upTo64KiB: number;
    upTo1MiB: number;
    over1MiB: number;
  }>;
  readonly contiguousRunBytes: number;
  readonly peakContiguousRunBytes: number;
  readonly flushReasonCounts: Readonly<{
    explicitCommit: number;
    flush: number;
    close: number;
    providerSync: number;
  }>;
}
export interface NodeVfsMetrics {
  snapshot(): NodeVfsMetricsSnapshot;
}
export type NodeVfsObservation =
  | { readonly kind: "session-open"; readonly sessionId: string }
  | { readonly kind: "session-close"; readonly sessionId: string }
  | { readonly kind: "forced-flush"; readonly bytes: number }
  | { readonly kind: "flush-failed"; readonly code: string }
  | { readonly kind: "memory-rejected"; readonly bytes: number };
export type NodeVfsObserver = (event: NodeVfsObservation) => void;

const SESSION_CONTROL_BYTES = 512;
const COORDINATOR_CONTROL_BYTES = 512;
const EDIT_CONTROL_BYTES = 192;
const PATH_CONTROL_BYTES = 96;

interface MutableMetrics {
  openSessions: number;
  peakOpenSessions: number;
  dirtySessions: number;
  residentWriteBytes: number;
  peakResidentWriteBytes: number;
  residentControlBytes: number;
  stagedLogicalBytes: number;
  admittedWriteBytes: number;
  flushedWriteBytes: number;
  flushCount: number;
  forcedFlushCount: number;
  failedFlushCount: number;
  rejectedWriteCount: number;
  directReadBytes: number;
  coreBatchCount: number;
  cowEditCount: number;
  cowEditSourceBytes: number;
  callbackSizeDistribution: {
    upTo4KiB: number;
    upTo64KiB: number;
    upTo1MiB: number;
    over1MiB: number;
  };
  contiguousRunBytes: number;
  peakContiguousRunBytes: number;
  flushReasonCounts: {
    explicitCommit: number;
    flush: number;
    close: number;
    providerSync: number;
  };
}
type FlushReason = "explicitCommit" | "flush" | "close" | "providerSync";

function fail(
  code: ConstructorParameters<typeof FilesystemError>[0],
  message: string,
  syscall?: string,
  path?: string,
): never {
  throw new FilesystemError(code, message, {
    ...(syscall === undefined ? {} : { syscall }),
    ...(path === undefined ? {} : { path }),
  });
}

function checkedInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value < 0)
    fail("EINVAL", `${name} must be a nonnegative safe integer`);
}

function validatedMode(mode: number | undefined, fallback: number): number {
  const value = mode ?? fallback;
  checkedInteger(value, "mode");
  return value & 0o7777;
}

function validateDestination(
  destination: Uint8Array,
  destinationOffset: number,
  position: number,
  length: number,
  maximum: number,
): void {
  if (!(destination instanceof Uint8Array))
    fail("EINVAL", "read destination must be a Uint8Array", "readIntoSync");
  checkedInteger(destinationOffset, "destinationOffset");
  checkedInteger(position, "position");
  checkedInteger(length, "length");
  if (length > maximum) fail("EFBIG", "read exceeds materialization limit");
  if (destinationOffset + length > destination.byteLength)
    fail("EINVAL", "read destination range is outside the supplied array");
}

abstract class Payload {
  #references = 1;
  readonly length: number;
  abstract readonly residentBytes: number;
  protected constructor(length: number) {
    this.length = length;
  }
  retain(): this {
    if (this.#references <= 0) throw new Error("released Node VFS payload");
    this.#references += 1;
    return this;
  }
  release(): void {
    if (this.#references <= 0) return;
    if (this.#references > 1) {
      this.#references -= 1;
      return;
    }
    this.releaseOwned();
    this.#references = 0;
  }
  abstract readInto(
    destination: Uint8Array,
    destinationOffset: number,
    sourceOffset: number,
    length: number,
  ): number;
  protected abstract releaseOwned(): void;
}

class ResidentPayload extends Payload {
  readonly residentBytes: number;
  readonly #slab: NodeVfsManagedSlab;
  readonly #provider: Provider;
  constructor(provider: Provider, slab: NodeVfsManagedSlab) {
    super(slab.bytes.byteLength);
    this.#provider = provider;
    this.#slab = slab;
    this.residentBytes = slab.bytes.byteLength;
  }
  readInto(
    destination: Uint8Array,
    destinationOffset: number,
    sourceOffset: number,
    length: number,
  ): number {
    destination.set(
      this.#slab.bytes.subarray(sourceOffset, sourceOffset + length),
      destinationOffset,
    );
    return length;
  }
  protected releaseOwned(): void {
    this.#slab.release();
    this.#provider.releaseResident(this.residentBytes);
  }
}

class PreparedPayload extends Payload {
  readonly residentBytes = 0;
  readonly #provider: Provider;
  readonly #bridge: NodeVfsFilesystemBridge;
  readonly #prepared: NodeVfsPreparedContent;
  #active = true;
  constructor(
    provider: Provider,
    bridge: NodeVfsFilesystemBridge,
    prepared: NodeVfsPreparedContent,
  ) {
    super(prepared.size);
    this.#provider = provider;
    this.#bridge = bridge;
    this.#prepared = prepared;
    this.#provider.addStaged(prepared.size);
  }
  readInto(
    destination: Uint8Array,
    destinationOffset: number,
    sourceOffset: number,
    length: number,
  ): number {
    return this.#bridge.readPreparedIntoSync(
      this.#prepared,
      destination,
      destinationOffset,
      sourceOffset,
      length,
    );
  }
  protected releaseOwned(): void {
    if (!this.#active) return;
    try {
      this.#bridge.abortPreparedSync(this.#prepared);
    } catch {
      // Cleanup faults cannot reverse a previously visible commit. Recovery
      // reclaims the sealed staging lease if this best-effort release failed.
    }
    this.#active = false;
    this.#provider.releaseStaged(this.length);
  }
}

class PinnedBase {
  #references = 1;
  readonly pinned: NodeVfsPinnedReadBridge;
  constructor(pinned: NodeVfsPinnedReadBridge) {
    this.pinned = pinned;
  }
  retain(): this {
    if (this.#references <= 0) throw new Error("released Node VFS pinned base");
    this.#references += 1;
    return this;
  }
  release(): void {
    if (this.#references <= 0) return;
    this.#references -= 1;
    if (this.#references === 0)
      try {
        this.pinned.closeSync();
      } catch {
        // A read-lease cleanup fault is recoverable and must never turn an
        // already-visible content commit into a reported failure.
      }
  }
}

interface WriteAdmission {
  readonly kind: "write";
  readonly sequence: number;
  readonly owner: Session;
  readonly position: number;
  readonly length: number;
  payloadOffset: number;
  payload: Payload;
  beforeSize: number;
  afterSize: number;
  releaseControl(): void;
}
interface TruncateAdmission {
  readonly kind: "truncate";
  readonly sequence: number;
  readonly owner: Session;
  readonly size: number;
  beforeSize: number;
  afterSize: number;
  releaseControl(): void;
}
type Admission = WriteAdmission | TruncateAdmission;

function sizeAfter(baseSize: number, admissions: readonly Admission[]): number {
  let size = baseSize;
  for (const admission of admissions) {
    admission.beforeSize = size;
    size =
      admission.kind === "truncate"
        ? admission.size
        : Math.max(size, admission.position + admission.length);
    admission.afterSize = size;
  }
  return size;
}

function logicalSize(baseSize: number, admissions: readonly Admission[]): number {
  let size = baseSize;
  for (const admission of admissions)
    size =
      admission.kind === "truncate"
        ? admission.size
        : Math.max(size, admission.position + admission.length);
  return size;
}

function zeroIntersection(
  destination: Uint8Array,
  destinationOffset: number,
  requestPosition: number,
  requestLength: number,
  start: number,
  end: number,
): void {
  const from = Math.max(requestPosition, start);
  const to = Math.min(requestPosition + requestLength, end);
  if (to > from)
    destination.fill(
      0,
      destinationOffset + from - requestPosition,
      destinationOffset + to - requestPosition,
    );
}

function readLogical(
  base: PinnedBase | undefined,
  baseSize: number,
  admissions: readonly Admission[],
  destination: Uint8Array,
  destinationOffset: number,
  position: number,
  length: number,
): number {
  const size = logicalSize(baseSize, admissions);
  const available = Math.max(0, Math.min(length, size - Math.min(position, size)));
  if (available === 0) return 0;
  destination.fill(0, destinationOffset, destinationOffset + available);
  if (base && position < baseSize) {
    const take = Math.min(available, baseSize - position);
    const read = base.pinned.readIntoSync(
      destination,
      destinationOffset,
      position,
      take,
    );
    if (read !== take) throw new Error("pinned Node VFS base ended early");
  }
  for (const admission of admissions) {
    if (admission.kind === "truncate") {
      zeroIntersection(
        destination,
        destinationOffset,
        position,
        available,
        Math.min(admission.beforeSize, admission.afterSize),
        Math.max(admission.beforeSize, admission.afterSize),
      );
      continue;
    }
    if (admission.position > admission.beforeSize)
      zeroIntersection(
        destination,
        destinationOffset,
        position,
        available,
        admission.beforeSize,
        admission.position,
      );
    const from = Math.max(position, admission.position);
    const to = Math.min(position + available, admission.position + admission.length);
    if (to <= from) continue;
    const copied = admission.payload.readInto(
      destination,
      destinationOffset + from - position,
      admission.payloadOffset + from - admission.position,
      to - from,
    );
    if (copied !== to - from) throw new Error("staged Node VFS payload ended early");
  }
  return available;
}

class ReadSnapshot {
  readonly base: PinnedBase | undefined;
  readonly baseSize: number;
  readonly admissions: readonly Admission[];
  readonly size: number;
  readonly inodeId: string;
  readonly mode: number;
  readonly nlink: number;
  readonly mtimeMs: number;
  readonly ctimeMs: number;
  readonly birthtimeMs: number;
  #closed = false;
  constructor(coordinator: InodeCoordinator) {
    this.base = coordinator.base?.retain();
    this.baseSize = coordinator.baseSize;
    this.admissions = Object.freeze(
      coordinator.admissions.map((admission) => {
        if (admission.kind === "write")
          return Object.freeze({
            ...admission,
            payload: admission.payload.retain(),
          });
        return Object.freeze({ ...admission });
      }),
    );
    this.size = logicalSize(this.baseSize, this.admissions);
    this.inodeId = coordinator.inodeId;
    this.mode = coordinator.mode;
    this.nlink = coordinator.nlink;
    this.mtimeMs = coordinator.mtimeMs;
    this.ctimeMs = coordinator.ctimeMs;
    this.birthtimeMs = coordinator.birthtimeMs;
  }
  readInto(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    if (this.#closed) fail("EBADF", "Node VFS read snapshot is closed");
    return readLogical(
      this.base,
      this.baseSize,
      this.admissions,
      destination,
      destinationOffset,
      position,
      length,
    );
  }
  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.base?.release();
    for (const admission of this.admissions)
      if (admission.kind === "write") admission.payload.release();
  }
}

class InodeCoordinator {
  inodeId: string;
  pendingCreate: boolean;
  readonly paths = new Set<string>();
  readonly pathReleases = new Map<string, () => void>();
  readonly admissions: Admission[] = [];
  readonly sessions = new Set<Session>();
  base: PinnedBase | undefined;
  baseSize: number;
  primaryPath: string;
  mode: number;
  nlink: number;
  mtimeMs: number;
  ctimeMs: number;
  birthtimeMs: number;
  readonly exclusive: boolean;
  readonly releaseControl: () => void;
  constructor(options: {
    inodeId: string;
    pendingCreate: boolean;
    path: string;
    base?: PinnedBase;
    baseSize: number;
    mode: number;
    exclusive: boolean;
    releaseControl: () => void;
  }) {
    this.inodeId = options.inodeId;
    this.pendingCreate = options.pendingCreate;
    this.primaryPath = options.path;
    this.paths.add(options.path);
    this.base = options.base;
    this.baseSize = options.baseSize;
    this.mode = options.mode;
    const initial = options.base?.pinned.stat;
    const now = Date.now();
    this.nlink = initial?.nlink ?? 1;
    this.mtimeMs = initial?.mtimeMs ?? now;
    this.ctimeMs = initial?.ctimeMs ?? now;
    this.birthtimeMs = initial?.birthtimeMs ?? now;
    this.exclusive = options.exclusive;
    this.releaseControl = options.releaseControl;
  }
  get size(): number {
    return sizeAfter(this.baseSize, this.admissions);
  }
  readInto(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    return readLogical(
      this.base,
      this.baseSize,
      this.admissions,
      destination,
      destinationOffset,
      position,
      length,
    );
  }
  touch(): void {
    const now = Date.now();
    this.mtimeMs = Math.max(this.mtimeMs, now);
    this.ctimeMs = Math.max(this.ctimeMs, now);
  }
  touchMetadata(): void {
    this.ctimeMs = Math.max(this.ctimeMs, Date.now());
  }
}

class Provider implements NodeVfsProvider {
  readonly capabilities: NodeVfsCapabilities;
  readonly metrics: NodeVfsMetrics;
  readonly #bridge: NodeVfsFilesystemBridge;
  readonly #observer: NodeVfsObserver | undefined;
  readonly #sessions = new Map<string, Session>();
  readonly #coordinators = new Map<string, InodeCoordinator>();
  readonly #paths = new Map<string, InodeCoordinator>();
  readonly #values: MutableMetrics = {
    openSessions: 0,
    peakOpenSessions: 0,
    dirtySessions: 0,
    residentWriteBytes: 0,
    peakResidentWriteBytes: 0,
    residentControlBytes: 0,
    stagedLogicalBytes: 0,
    admittedWriteBytes: 0,
    flushedWriteBytes: 0,
    flushCount: 0,
    forcedFlushCount: 0,
    failedFlushCount: 0,
    rejectedWriteCount: 0,
    directReadBytes: 0,
    coreBatchCount: 0,
    cowEditCount: 0,
    cowEditSourceBytes: 0,
    callbackSizeDistribution: {
      upTo4KiB: 0,
      upTo64KiB: 0,
      upTo1MiB: 0,
      over1MiB: 0,
    },
    contiguousRunBytes: 0,
    peakContiguousRunBytes: 0,
    flushReasonCounts: {
      explicitCommit: 0,
      flush: 0,
      close: 0,
      providerSync: 0,
    },
  };
  #sequence = 0;
  #sessionOrder = 0;
  #activationVersion: number;
  #closed = false;
  constructor(bridge: NodeVfsFilesystemBridge, observer?: NodeVfsObserver) {
    this.#bridge = bridge;
    this.#observer = observer;
    this.#activationVersion = bridge.activationVersionSync();
    this.capabilities = Object.freeze({
      cowPageBytes: bridge.cowPageBytes,
      runtime: bridge.runtimeLimits,
      preferredReadBytes: bridge.filesystemLimits.preferredStreamChunkBytes,
      supportsDirectRangeIo: true,
      supportsWriteSessions: true,
      supportsDataSync: false,
    });
    this.metrics = Object.freeze({ snapshot: () => this.snapshotMetrics() });
  }
  existsSync(path: string): boolean {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "existsSync");
    if (this.resolveOverlayCoordinator(canonical)) return true;
    return this.#bridge.existsSync(canonical);
  }
  statSync(path: string): FileStat {
    return this.#stat(path, true);
  }
  lstatSync(path: string): FileStat {
    return this.#stat(path, false);
  }
  #stat(path: string, followFinal: boolean): FileStat {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "statSync");
    const pending = followFinal
      ? this.resolveOverlayCoordinator(canonical)
      : this.#paths.get(canonical);
    if (pending?.pendingCreate) return this.statCoordinator(pending, canonical);
    const resolved = this.#bridge.resolvePathSync(canonical, followFinal);
    const coordinator = this.#coordinators.get(resolved.stat.id);
    return coordinator ? this.statCoordinator(coordinator, canonical) : resolved.stat;
  }
  readdirSync(path: string): string[] {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "readdirSync");
    const names = new Set(this.#bridge.readdirSync(canonical).map(({ name }) => name));
    const prefix = canonical === "/" ? "/" : `${canonical}/`;
    for (const candidate of this.#paths.keys()) {
      if (!candidate.startsWith(prefix)) continue;
      const suffix = candidate.slice(prefix.length);
      if (suffix && !suffix.includes("/")) names.add(suffix);
    }
    return [...names].sort((left, right) => {
      const a = new TextEncoder().encode(left);
      const b = new TextEncoder().encode(right);
      const length = Math.min(a.length, b.length);
      for (let index = 0; index < length; index += 1) {
        const difference = a[index]! - b[index]!;
        if (difference !== 0) return difference;
      }
      return a.length - b.length;
    });
  }
  readlinkSync(path: string): string {
    this.#assertOpen();
    return this.#bridge.readlinkSync(path);
  }
  readRangeSync(path: string, position: number, length: number): Uint8Array {
    this.#assertOpen();
    checkedInteger(position, "position");
    checkedInteger(length, "length");
    if (length > this.#bridge.filesystemLimits.maxMaterializedBytes)
      fail("EFBIG", "read exceeds materialization limit", "readRangeSync", path);
    const canonical = this.#bridge.canonicalPathSync(path, "readRangeSync");
    const coordinator = this.resolveCoordinator(canonical);
    if (!coordinator) {
      const value = this.#bridge.readRangeSync(canonical, position, length);
      this.direct(value.byteLength);
      return value;
    }
    const output = new Uint8Array(length);
    const read = coordinator.readInto(output, 0, position, length);
    this.direct(read);
    return read === output.byteLength ? output : output.slice(0, read);
  }
  openFileSync(path: string, options: OpenFileOptions = {}): NodeFileSession {
    this.#assertOpen();
    if (this.#sessions.size >= this.capabilities.runtime.maxOpenNodeVfsSessions)
      fail("EAGAIN", "Node VFS session count limit exceeded", "openFileSync", path);
    const canonical = this.#bridge.canonicalPathSync(path, "openFileSync");
    const requestedMode = validatedMode(options.mode, 0o644);
    const writable = options.writable ?? options.create ?? false;
    if ((options.create || options.exclusive || options.truncate) && !writable)
      fail("EINVAL", "create, exclusive, and truncate require a writable session");
    if (writable && this.#bridge.mainReadOnly)
      fail("EROFS", "replica main is read-only", "openFileSync", canonical);
    let coordinator = this.resolveOverlayCoordinator(canonical);
    let pinned: NodeVfsPinnedReadBridge | undefined;
    if (!coordinator) {
      try {
        pinned = this.#bridge.openPinnedReadSync(canonical);
        coordinator = this.#coordinators.get(pinned.inodeId);
        if (options.exclusive) {
          pinned.closeSync();
          fail("EEXIST", `file exists: ${canonical}`, "openFileSync", canonical);
        }
      } catch (error) {
        if (!(error instanceof FilesystemError) || error.code !== "ENOENT") throw error;
        if (!options.create)
          fail(
            "ENOENT",
            `file does not exist: ${canonical}`,
            "openFileSync",
            canonical,
          );
      }
    } else if (options.exclusive) {
      fail("EEXIST", `file exists: ${canonical}`, "openFileSync", canonical);
    }
    if (!coordinator && writable) {
      if (pinned) {
        coordinator = this.createCoordinator({
          inodeId: pinned.inodeId,
          pendingCreate: false,
          path: canonical,
          base: new PinnedBase(pinned),
          baseSize: pinned.size,
          mode: pinned.stat.mode,
          exclusive: false,
        });
        pinned = undefined;
      } else {
        coordinator = this.createCoordinator({
          inodeId: globalThis.crypto.randomUUID(),
          pendingCreate: true,
          path: canonical,
          baseSize: 0,
          mode: requestedMode,
          exclusive: options.exclusive ?? false,
        });
      }
    }
    if (coordinator && writable && pinned) {
      pinned.closeSync();
      pinned = undefined;
    }
    let releaseSession: () => void;
    try {
      releaseSession = this.reserveControl(SESSION_CONTROL_BYTES, "openFileSync");
    } catch (error) {
      pinned?.closeSync();
      if (coordinator) this.disposeCoordinator(coordinator);
      throw error;
    }
    let readSnapshot: ReadSnapshot | undefined;
    if (!writable) {
      if (coordinator) {
        pinned?.closeSync();
        pinned = undefined;
        readSnapshot = new ReadSnapshot(coordinator);
      }
    }
    const session = new Session(
      this,
      this.#bridge,
      ++this.#sessionOrder,
      canonical,
      writable,
      coordinator,
      pinned,
      readSnapshot,
      releaseSession,
    );
    coordinator?.sessions.add(session);
    if (writable && coordinator?.pendingCreate && coordinator.sessions.size === 1)
      session.markCreationDirty();
    this.#sessions.set(session.id, session);
    this.#values.openSessions += 1;
    this.#values.peakOpenSessions = Math.max(
      this.#values.peakOpenSessions,
      this.#values.openSessions,
    );
    this.#emit({ kind: "session-open", sessionId: session.id });
    if (options.truncate)
      try {
        session.truncateSync(0);
      } catch (error) {
        session.abortSync();
        throw error;
      }
    return session;
  }
  mkdirSync(path: string, options?: { recursive?: boolean; mode?: number }): void {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "mkdirSync");
    if (this.entryExists(canonical)) {
      if (options?.recursive) return;
      fail("EEXIST", "destination exists", "mkdirSync", canonical);
    }
    this.assertNoPendingAncestor(canonical);
    const mode = validatedMode(options?.mode, 0o755);
    this.#bridge.mkdirSync(canonical, { ...options, mode });
  }
  chmodSync(path: string, mode: number): void {
    this.#assertOpen();
    mode = validatedMode(mode, 0);
    const canonical = this.#bridge.canonicalPathSync(path, "chmodSync");
    const pending = this.resolveOverlayCoordinator(canonical);
    if (pending?.pendingCreate) {
      pending.mode = mode;
      pending.touchMetadata();
      return;
    }
    this.#bridge.chmodSync(canonical, mode);
  }
  linkSync(existingPath: string, newPath: string): void {
    this.#assertOpen();
    const source = this.#bridge.canonicalPathSync(existingPath, "linkSync");
    const destination = this.#bridge.canonicalPathSync(newPath, "linkSync");
    this.assertNoPendingAncestor(destination);
    if (this.entryExists(destination))
      fail("EEXIST", `file exists: ${destination}`, "linkSync", destination);
    const pending = this.resolveOverlayCoordinator(source);
    if (pending?.pendingCreate) {
      const releasePath = this.reserveControl(
        PATH_CONTROL_BYTES + destination.length,
        "linkSync",
      );
      pending.paths.add(destination);
      pending.nlink += 1;
      pending.touchMetadata();
      pending.pathReleases.set(destination, releasePath);
      this.#paths.set(destination, pending);
      return;
    }
    const coordinator = this.resolveCoordinator(source);
    const releasePath = coordinator
      ? this.reserveControl(PATH_CONTROL_BYTES + destination.length, "linkSync")
      : undefined;
    try {
      this.#bridge.linkSync(source, destination);
    } catch (error) {
      releasePath?.();
      throw error;
    }
    if (coordinator) {
      coordinator.paths.add(destination);
      coordinator.nlink += 1;
      coordinator.touchMetadata();
      coordinator.pathReleases.set(destination, releasePath!);
      this.#paths.set(destination, coordinator);
    }
  }
  symlinkSync(target: string, path: string): void {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "symlinkSync");
    if (canonical === "/")
      fail("EPERM", "root cannot be replaced", "symlinkSync", canonical);
    if (this.entryExists(canonical))
      fail("EEXIST", "destination exists", "symlinkSync", canonical);
    this.assertNoPendingAncestor(canonical);
    this.#bridge.symlinkSync(target, canonical);
  }
  renameSync(oldPath: string, newPath: string): void {
    this.#assertOpen();
    const source = this.#bridge.canonicalPathSync(oldPath, "renameSync");
    const destination = this.#bridge.canonicalPathSync(newPath, "renameSync");
    this.assertNoPendingAncestor(destination);
    if (source === destination) return;
    const pending = this.#paths.get(source);
    if (pending?.pendingCreate) {
      if (this.entryExists(destination))
        fail("EEXIST", `destination exists: ${destination}`, "renameSync", source);
      const nextRelease = this.reserveControl(
        PATH_CONTROL_BYTES + destination.length,
        "renameSync",
      );
      pending.paths.delete(source);
      pending.paths.add(destination);
      pending.pathReleases.get(source)?.();
      pending.pathReleases.delete(source);
      pending.pathReleases.set(destination, nextRelease);
      this.#paths.delete(source);
      this.#paths.set(destination, pending);
      if (pending.primaryPath === source) pending.primaryPath = destination;
      for (const session of pending.sessions) session.renamePath(source, destination);
      return;
    }
    const target = this.resolveCoordinator(destination, true);
    if (target && (target.admissions.length || target.sessions.size))
      fail("EBUSY", "rename destination has open sessions", "renameSync", source);
    const moving = [...this.#paths.entries()].filter(
      ([candidate]) => candidate === source || candidate.startsWith(`${source}/`),
    );
    const remaps = moving.map(([candidate, coordinator]) =>
      Object.freeze({
        source: candidate,
        destination: `${destination}${candidate.slice(source.length)}`,
        coordinator,
      }),
    );
    for (const remap of remaps) {
      const collision = this.#paths.get(remap.destination);
      if (
        collision &&
        !remaps.some((candidate) => candidate.source === remap.destination)
      )
        fail(
          "EBUSY",
          "rename destination contains open Node VFS state",
          "renameSync",
          destination,
        );
    }
    const releases = new Map<string, () => void>();
    try {
      for (const remap of remaps)
        releases.set(
          remap.source,
          this.reserveControl(
            PATH_CONTROL_BYTES + remap.destination.length,
            "renameSync",
          ),
        );
    } catch (error) {
      for (const release of releases.values()) release();
      throw error;
    }
    try {
      this.#bridge.renameSync(source, destination);
    } catch (error) {
      for (const release of releases.values()) release();
      throw error;
    }
    for (const remap of remaps) this.#paths.delete(remap.source);
    for (const remap of remaps) {
      const coordinator = remap.coordinator;
      coordinator.paths.delete(remap.source);
      coordinator.paths.add(remap.destination);
      coordinator.pathReleases.get(remap.source)?.();
      coordinator.pathReleases.delete(remap.source);
      coordinator.pathReleases.set(remap.destination, releases.get(remap.source)!);
      this.#paths.set(remap.destination, coordinator);
      if (coordinator.primaryPath === remap.source)
        coordinator.primaryPath = remap.destination;
      for (const session of coordinator.sessions)
        session.renamePathPrefix(source, destination);
    }
  }
  unlinkSync(path: string): void {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "unlinkSync");
    const direct = this.#paths.get(canonical);
    let coordinator = direct;
    if (!coordinator) {
      const resolved = this.#bridge.resolvePathSync(canonical, false);
      coordinator = this.#coordinators.get(resolved.stat.id);
    }
    if (coordinator?.pendingCreate) {
      if (coordinator.admissions.length || coordinator.sessions.size)
        fail("EBUSY", "pending create is open or dirty", "unlinkSync", canonical);
      coordinator.paths.delete(canonical);
      this.#paths.delete(canonical);
      this.disposeCoordinator(coordinator);
      return;
    }
    if (
      coordinator &&
      [...coordinator.sessions].some((session) => session.writable || session.dirty)
    )
      fail("EBUSY", "inode has an open writable session", "unlinkSync", canonical);
    this.#bridge.unlinkSync(canonical);
    if (coordinator) {
      coordinator.paths.delete(canonical);
      coordinator.nlink = Math.max(0, coordinator.nlink - 1);
      coordinator.touchMetadata();
    }
    coordinator?.pathReleases.get(canonical)?.();
    coordinator?.pathReleases.delete(canonical);
    this.#paths.delete(canonical);
  }
  rmdirSync(path: string): void {
    this.#assertOpen();
    const canonical = this.#bridge.canonicalPathSync(path, "rmdirSync");
    const prefix = canonical === "/" ? "/" : `${canonical}/`;
    if ([...this.#paths.keys()].some((candidate) => candidate.startsWith(prefix)))
      fail(
        "ENOTEMPTY",
        "directory contains pending Node VFS entries",
        "rmdirSync",
        path,
      );
    this.#bridge.rmdirSync(canonical);
  }
  syncSync(): void {
    this.#assertOpen();
    const dirty = [...this.#sessions.values()]
      .filter((session) => session.dirty)
      .sort((left, right) => left.order - right.order);
    for (const session of dirty) this.commitSession(session, "providerSync");
  }
  closeSync(): void {
    if (this.#closed) return;
    if ([...this.#sessions.values()].some((session) => session.dirty))
      fail("EBUSY", "Node VFS provider has dirty sessions", "closeSync");
    for (const session of [...this.#sessions.values()]) session.abortSync();
    this.#closed = true;
  }
  closeAllSync(): void {
    if (this.#closed) return;
    const sessions = [...this.#sessions.values()].sort(
      (left, right) => left.order - right.order,
    );
    for (const session of sessions) session.closeSync();
    this.closeSync();
  }
  allocateResident(content: Uint8Array, offset: number, length: number): Payload {
    if (length > this.capabilities.runtime.maxWriteSessionBytes)
      fail("EFBIG", "one admitted slab exceeds an empty session budget");
    this.relievePressure(length);
    let slab = this.#bridge.acquireSlabSync(content, offset, length);
    if (!slab) {
      this.forceStageLargest();
      slab = this.#bridge.acquireSlabSync(content, offset, length);
    }
    if (!slab) {
      this.reject(length);
      fail("EAGAIN", "aggregate managed-memory pressure could not be relieved");
    }
    this.#values.residentWriteBytes += slab.bytes.byteLength;
    this.#values.admittedWriteBytes += length;
    this.updateResidentPeak();
    return new ResidentPayload(this, slab);
  }
  prepareCallerPayload(content: Uint8Array): Payload {
    const prepared = this.#bridge.prepareContentSourceSync({
      size: content.byteLength,
      readInto: (destination, destinationOffset, position, length) => {
        destination.set(
          content.subarray(position, position + length),
          destinationOffset,
        );
        return length;
      },
    });
    this.#values.admittedWriteBytes += content.byteLength;
    this.#values.coreBatchCount += 1;
    return new PreparedPayload(this, this.#bridge, prepared);
  }
  addWrite(
    session: Session,
    coordinator: InodeCoordinator,
    position: number,
    payload: Payload,
  ): WriteAdmission {
    const releaseControl = this.reserveControl(EDIT_CONTROL_BYTES, "writeSync");
    const admission: WriteAdmission = {
      kind: "write",
      sequence: ++this.#sequence,
      owner: session,
      position,
      length: payload.length,
      payload,
      payloadOffset: 0,
      beforeSize: 0,
      afterSize: 0,
      releaseControl,
    };
    coordinator.admissions.push(admission);
    coordinator.touch();
    sizeAfter(coordinator.baseSize, coordinator.admissions);
    session.addAdmission(admission);
    return admission;
  }
  addTruncate(
    session: Session,
    coordinator: InodeCoordinator,
    size: number,
  ): TruncateAdmission {
    const releaseControl = this.reserveControl(EDIT_CONTROL_BYTES, "truncateSync");
    const admission: TruncateAdmission = {
      kind: "truncate",
      sequence: ++this.#sequence,
      owner: session,
      size,
      beforeSize: 0,
      afterSize: 0,
      releaseControl,
    };
    coordinator.admissions.push(admission);
    coordinator.touch();
    sizeAfter(coordinator.baseSize, coordinator.admissions);
    session.addAdmission(admission);
    return admission;
  }
  stageSession(session: Session, forced: boolean): void {
    const writes = session.admissions
      .filter(
        (admission): admission is WriteAdmission =>
          admission.kind === "write" && admission.payload instanceof ResidentPayload,
      )
      .slice(0, this.#bridge.storageLimits.maxQueryBatchSize);
    const staged = writes.reduce((sum, admission) => sum + admission.length, 0);
    if (!staged) return;
    const prepared = this.#bridge.prepareContentSourceSync({
      size: staged,
      readInto: (destination, destinationOffset, position, length) => {
        let remaining = length;
        let sourcePosition = position;
        let outputPosition = destinationOffset;
        let logicalOffset = 0;
        for (const admission of writes) {
          const end = logicalOffset + admission.length;
          if (sourcePosition >= end) {
            logicalOffset = end;
            continue;
          }
          const relative = Math.max(0, sourcePosition - logicalOffset);
          const take = Math.min(remaining, admission.length - relative);
          const read = admission.payload.readInto(
            destination,
            outputPosition,
            admission.payloadOffset + relative,
            take,
          );
          if (read !== take) throw new Error("resident staging source ended early");
          remaining -= take;
          sourcePosition += take;
          outputPosition += take;
          logicalOffset = end;
          if (remaining === 0) break;
        }
        return length - remaining;
      },
    });
    const shared = new PreparedPayload(this, this.#bridge, prepared);
    let payloadOffset = 0;
    for (const [index, admission] of writes.entries()) {
      const old = admission.payload;
      admission.payload = index === 0 ? shared : shared.retain();
      admission.payloadOffset = payloadOffset;
      payloadOffset += admission.length;
      old.release();
    }
    this.#values.coreBatchCount += 1;
    if (forced) {
      this.#values.forcedFlushCount += 1;
      this.#emit({ kind: "forced-flush", bytes: staged });
    }
  }
  commitSession(session: Session, reason: FlushReason): void {
    this.#assertOpen();
    const coordinator = session.coordinator;
    if (!coordinator) fail("EBADF", "session has no writable inode coordinator");
    const cutoff = session.requiredSequence ?? 0;
    if (cutoff === 0 && !session.creationDirty) return;
    const selected = coordinator.admissions.filter(
      (admission) => admission.sequence <= cutoff,
    );
    const logicalBytes = selected.reduce(
      (sum, admission) => sum + (admission.kind === "write" ? admission.length : 0),
      0,
    );
    const primary = coordinator.primaryPath;
    let prepared = session.retryPrepared(cutoff);
    try {
      if (!prepared) {
        const single = selected.length === 1 ? selected[0] : undefined;
        if (
          !coordinator.pendingCreate &&
          single?.kind === "write" &&
          single.beforeSize === coordinator.baseSize &&
          single.afterSize === coordinator.baseSize
        )
          prepared = this.#bridge.prepareOverwriteSync(primary, single.position, {
            size: single.length,
            readInto: (destination, destinationOffset, position, length) =>
              single.payload.readInto(
                destination,
                destinationOffset,
                single.payloadOffset + position,
                length,
              ),
          });
        if (
          !prepared &&
          !coordinator.pendingCreate &&
          selected.length > 1 &&
          selected.every(
            (admission) =>
              admission.kind === "write" &&
              admission.beforeSize === coordinator.baseSize &&
              admission.afterSize === coordinator.baseSize,
          )
        )
          prepared = this.#bridge.prepareOverwritesSync(
            primary,
            selected.map((admission) => {
              if (admission.kind !== "write") throw new Error("unreachable");
              return Object.freeze({
                offset: admission.position,
                source: Object.freeze({
                  size: admission.length,
                  readInto: (
                    destination: Uint8Array,
                    destinationOffset: number,
                    position: number,
                    length: number,
                  ) =>
                    admission.payload.readInto(
                      destination,
                      destinationOffset,
                      admission.payloadOffset + position,
                      length,
                    ),
                }),
              });
            }),
          );
        if (!prepared) {
          const size = sizeAfter(coordinator.baseSize, selected);
          prepared = this.#bridge.prepareContentSourceSync({
            size,
            readInto: (destination, destinationOffset, position, length) =>
              readLogical(
                coordinator.base,
                coordinator.baseSize,
                selected,
                destination,
                destinationOffset,
                position,
                length,
              ),
          });
        }
        if (prepared.editSourceBytes !== undefined) {
          this.#values.cowEditCount += 1;
          this.#values.cowEditSourceBytes += prepared.editSourceBytes;
        }
        session.setRetryPrepared(cutoff, prepared);
        this.#values.coreBatchCount += 1;
      }
      const paths = [...coordinator.paths];
      const committed = this.#bridge.commitPreparedSync(primary, prepared, {
        create: coordinator.pendingCreate,
        exclusive: coordinator.pendingCreate ? coordinator.exclusive : false,
        mode: coordinator.mode,
        inodeId: coordinator.inodeId,
        ...(coordinator.base?.pinned.generation === undefined
          ? {}
          : { expectedGeneration: coordinator.base.pinned.generation }),
        aliases: coordinator.pendingCreate
          ? paths.filter((candidate) => candidate !== primary)
          : [],
      });
      session.consumeRetryPrepared(cutoff);
      const oldId = coordinator.inodeId;
      const oldBase = coordinator.base;
      const next = committed.pinned;
      coordinator.inodeId = next.inodeId;
      coordinator.pendingCreate = false;
      coordinator.base = new PinnedBase(next);
      coordinator.baseSize = next.size;
      coordinator.mode = next.stat.mode;
      coordinator.nlink = next.stat.nlink;
      coordinator.mtimeMs = next.stat.mtimeMs;
      coordinator.ctimeMs = next.stat.ctimeMs;
      coordinator.birthtimeMs = next.stat.birthtimeMs;
      oldBase?.release();
      if (oldId !== coordinator.inodeId) {
        this.#coordinators.delete(oldId);
        this.#coordinators.set(coordinator.inodeId, coordinator);
      }
      for (const admission of selected) {
        if (admission.kind === "write") admission.payload.release();
        const index = coordinator.admissions.indexOf(admission);
        if (index >= 0) coordinator.admissions.splice(index, 1);
        admission.owner.committed(admission);
        admission.releaseControl();
      }
      session.creationCommitted();
      sizeAfter(coordinator.baseSize, coordinator.admissions);
      for (const candidate of coordinator.sessions)
        candidate.invalidateCommittedRetries(cutoff);
      this.#values.flushedWriteBytes += logicalBytes;
      this.#values.flushCount += 1;
      this.#values.flushReasonCounts[reason] += 1;
      this.#values.coreBatchCount += 1;
    } catch (error) {
      this.#values.failedFlushCount += 1;
      this.#emit({
        kind: "flush-failed",
        code: error instanceof FilesystemError ? error.code : "EIO",
      });
      throw error;
    }
  }
  abortSession(session: Session): void {
    const coordinator = session.coordinator;
    if (!coordinator) return;
    for (const admission of [...session.admissions]) {
      if (admission.kind === "write") admission.payload.release();
      const index = coordinator.admissions.indexOf(admission);
      if (index >= 0) coordinator.admissions.splice(index, 1);
      session.aborted(admission);
      admission.releaseControl();
    }
    session.creationCommitted();
    sizeAfter(coordinator.baseSize, coordinator.admissions);
  }
  removeSession(session: Session): void {
    if (!this.#sessions.delete(session.id)) return;
    session.coordinator?.sessions.delete(session);
    this.#values.openSessions -= 1;
    this.#emit({ kind: "session-close", sessionId: session.id });
    const coordinator = session.coordinator;
    if (coordinator && !coordinator.sessions.size && !coordinator.admissions.length)
      this.disposeCoordinator(coordinator);
  }
  releaseResident(bytes: number): void {
    this.#values.residentWriteBytes -= bytes;
    if (this.#values.residentWriteBytes < 0)
      throw new Error("Node VFS resident write accounting underflow");
  }
  addStaged(bytes: number): void {
    this.#values.stagedLogicalBytes += bytes;
  }
  releaseStaged(bytes: number): void {
    this.#values.stagedLogicalBytes -= bytes;
    if (this.#values.stagedLogicalBytes < 0)
      throw new Error("Node VFS staged-logical accounting underflow");
  }
  dirty(delta: 1 | -1): void {
    this.#values.dirtySessions += delta;
  }
  direct(bytes: number): void {
    this.#values.directReadBytes += bytes;
    this.#values.coreBatchCount += 1;
  }
  recordWriteCallback(position: number, bytes: number, contiguousBytes: number): void {
    void position;
    const distribution = this.#values.callbackSizeDistribution;
    if (bytes <= 4 * 1024) distribution.upTo4KiB += 1;
    else if (bytes <= 64 * 1024) distribution.upTo64KiB += 1;
    else if (bytes <= 1024 * 1024) distribution.upTo1MiB += 1;
    else distribution.over1MiB += 1;
    this.#values.contiguousRunBytes = contiguousBytes;
    this.#values.peakContiguousRunBytes = Math.max(
      this.#values.peakContiguousRunBytes,
      contiguousBytes,
    );
  }
  statCoordinator(coordinator: InodeCoordinator, path: string): FileStat {
    const size = coordinator.size;
    const name = path.split("/").at(-1) ?? "";
    return Object.freeze({
      id: coordinator.inodeId,
      name,
      type: "file" as const,
      mode: coordinator.mode,
      size,
      nlink: coordinator.nlink,
      mtimeMs: coordinator.mtimeMs,
      ctimeMs: coordinator.ctimeMs,
      birthtimeMs: coordinator.birthtimeMs,
      isFile: () => true,
      isDirectory: () => false,
      isSymbolicLink: () => false,
    });
  }
  snapshotMetrics(): NodeVfsMetricsSnapshot {
    const managed = this.#bridge.managedMemorySync();
    return Object.freeze({
      ...this.#values,
      callbackSizeDistribution: Object.freeze({
        ...this.#values.callbackSizeDistribution,
      }),
      flushReasonCounts: Object.freeze({ ...this.#values.flushReasonCounts }),
      peakManagedResidentBytes: managed.peakBytes,
    });
  }
  private createCoordinator(options: {
    inodeId: string;
    pendingCreate: boolean;
    path: string;
    base?: PinnedBase;
    baseSize: number;
    mode: number;
    exclusive: boolean;
  }): InodeCoordinator {
    const releaseControl = this.reserveControl(
      COORDINATOR_CONTROL_BYTES,
      "openFileSync",
    );
    let releasePath: () => void;
    try {
      releasePath = this.reserveControl(
        PATH_CONTROL_BYTES + options.path.length,
        "openFileSync",
      );
    } catch (error) {
      releaseControl();
      throw error;
    }
    const coordinator = new InodeCoordinator({ ...options, releaseControl });
    coordinator.pathReleases.set(options.path, releasePath);
    this.#coordinators.set(coordinator.inodeId, coordinator);
    this.#paths.set(options.path, coordinator);
    return coordinator;
  }
  private disposeCoordinator(coordinator: InodeCoordinator): void {
    if (coordinator.sessions.size || coordinator.admissions.length) return;
    this.#coordinators.delete(coordinator.inodeId);
    for (const path of coordinator.paths) this.#paths.delete(path);
    for (const release of coordinator.pathReleases.values()) release();
    coordinator.pathReleases.clear();
    coordinator.base?.release();
    coordinator.base = undefined;
    coordinator.releaseControl();
  }
  private resolveCoordinator(
    path: string,
    missingIsUndefined = false,
  ): InodeCoordinator | undefined {
    const canonical = this.#bridge.canonicalPathSync(path, "resolvePathSync");
    const direct = this.resolveOverlayCoordinator(canonical);
    if (direct) return direct;
    try {
      const resolved = this.#bridge.resolvePathSync(canonical, true);
      return this.#coordinators.get(resolved.stat.id);
    } catch (error) {
      if (
        missingIsUndefined &&
        error instanceof FilesystemError &&
        error.code === "ENOENT"
      )
        return undefined;
      throw error;
    }
  }
  private resolveOverlayCoordinator(
    path: string,
    depth = 0,
  ): InodeCoordinator | undefined {
    if (depth > 40) fail("ELOOP", "too many symbolic links", "resolvePathSync", path);
    const canonical = this.#bridge.canonicalPathSync(path, "resolvePathSync");
    const direct = this.#paths.get(canonical);
    if (direct) return direct;
    const segments = canonical === "/" ? [] : canonical.slice(1).split("/");
    for (let index = 0; index < segments.length; index += 1) {
      const prefix = `/${segments.slice(0, index + 1).join("/")}`;
      if (this.#paths.has(prefix))
        fail("ENOTDIR", "pending file is not a directory", "resolvePathSync", prefix);
      let value;
      try {
        value = this.#bridge.resolvePathSync(prefix, false);
      } catch (error) {
        if (error instanceof FilesystemError && error.code === "ENOENT")
          return undefined;
        throw error;
      }
      if (!value.stat.isSymbolicLink()) continue;
      const target = this.#bridge.readlinkSync(prefix);
      const parent = index === 0 ? "/" : `/${segments.slice(0, index).join("/")}`;
      const suffix = segments.slice(index + 1).join("/");
      const expanded = `${target.startsWith("/") ? target : `${parent}/${target}`}${suffix ? `/${suffix}` : ""}`;
      return this.resolveOverlayCoordinator(expanded, depth + 1);
    }
    return undefined;
  }
  private entryExists(canonical: string): boolean {
    if (this.#paths.has(canonical)) return true;
    try {
      this.#bridge.resolvePathSync(canonical, false);
      return true;
    } catch (error) {
      if (error instanceof FilesystemError && error.code === "ENOENT") return false;
      throw error;
    }
  }
  private assertNoPendingAncestor(canonical: string): void {
    const segments = canonical === "/" ? [] : canonical.slice(1).split("/");
    for (let index = 1; index < segments.length; index += 1) {
      const prefix = `/${segments.slice(0, index).join("/")}`;
      if (this.#paths.has(prefix))
        fail("ENOTDIR", "pending file is not a directory", "nodeVfs", prefix);
    }
  }
  private reserveControl(bytes: number, syscall: string): () => void {
    const release = this.#bridge.reserveControlSync(bytes);
    if (!release) {
      this.reject(bytes);
      fail("EAGAIN", "Node VFS control-state pressure could not be relieved", syscall);
    }
    this.#values.residentControlBytes += bytes;
    this.updateResidentPeak();
    let active = true;
    return () => {
      if (!active) return;
      active = false;
      this.#values.residentControlBytes -= bytes;
      release();
    };
  }
  private relievePressure(bytes: number): void {
    if (bytes > this.capabilities.runtime.maxPendingWriteBytes)
      fail("EFBIG", "one write cannot fit an empty pending-write budget");
    while (
      this.#values.residentWriteBytes + bytes >
      this.capabilities.runtime.maxPendingWriteBytes
    ) {
      if (!this.forceStageLargest()) {
        this.reject(bytes);
        fail("EAGAIN", "pending-write pressure could not be relieved");
      }
    }
  }
  private forceStageLargest(): boolean {
    const candidate = [...this.#sessions.values()]
      .filter((session) => session.residentBytes > 0)
      .sort(
        (left, right) =>
          right.residentBytes - left.residentBytes || left.order - right.order,
      )[0];
    if (!candidate) return false;
    this.stageSession(candidate, true);
    return true;
  }
  private reject(bytes: number): void {
    this.#values.rejectedWriteCount += 1;
    this.#emit({ kind: "memory-rejected", bytes });
  }
  private updateResidentPeak(): void {
    this.#values.peakResidentWriteBytes = Math.max(
      this.#values.peakResidentWriteBytes,
      this.#values.residentWriteBytes,
    );
  }
  #assertOpen(): void {
    if (this.#closed) fail("EBADF", "Node VFS provider is closed");
    this.refreshActivation();
  }
  private refreshActivation(): void {
    const nextVersion = this.#bridge.activationVersionSync();
    if (nextVersion === this.#activationVersion) return;
    for (const coordinator of [...this.#coordinators.values()]) {
      if (coordinator.pendingCreate || coordinator.admissions.length) continue;
      let pinned: NodeVfsPinnedReadBridge | undefined;
      for (const path of [...coordinator.paths]) {
        let candidate: NodeVfsPinnedReadBridge | undefined;
        try {
          candidate = this.#bridge.openPinnedReadSync(path);
        } catch (error) {
          if (
            !(error instanceof FilesystemError) ||
            (error.code !== "ENOENT" && error.code !== "EISDIR")
          )
            throw error;
        }
        if (!candidate || candidate.inodeId !== coordinator.inodeId) {
          candidate?.closeSync();
          if (this.#paths.get(path) === coordinator) this.#paths.delete(path);
          coordinator.paths.delete(path);
          coordinator.pathReleases.get(path)?.();
          coordinator.pathReleases.delete(path);
          continue;
        }
        if (!pinned) {
          pinned = candidate;
          coordinator.primaryPath = path;
        } else {
          candidate.closeSync();
        }
      }
      if (!pinned) {
        if (coordinator.sessions.size === 0) this.disposeCoordinator(coordinator);
        continue;
      }
      const oldBase = coordinator.base;
      coordinator.base = new PinnedBase(pinned);
      coordinator.baseSize = pinned.size;
      coordinator.mode = pinned.stat.mode;
      coordinator.nlink = pinned.stat.nlink;
      coordinator.mtimeMs = pinned.stat.mtimeMs;
      coordinator.ctimeMs = pinned.stat.ctimeMs;
      coordinator.birthtimeMs = pinned.stat.birthtimeMs;
      oldBase?.release();
    }
    this.#activationVersion = nextVersion;
  }
  assertWritableView(coordinator: InodeCoordinator | undefined): void {
    this.#assertOpen();
    if (coordinator && !coordinator.pendingCreate && coordinator.paths.size === 0)
      fail("EAGAIN", "the open inode no longer has an active branch path", "writeSync");
  }
  #emit(event: NodeVfsObservation): void {
    try {
      this.#observer?.(event);
    } catch {}
  }
}

/** Create a provider from a bridge owned by an already-open shared core runtime. */
export function createNodeVfsProvider(
  bridge: NodeVfsFilesystemBridge,
  observer?: NodeVfsObserver,
): NodeVfsProvider {
  return new Provider(bridge, observer);
}

class Session implements NodeFileSession {
  readonly id = globalThis.crypto.randomUUID();
  readonly writable: boolean;
  readonly order: number;
  readonly coordinator: InodeCoordinator | undefined;
  readonly admissions: Admission[] = [];
  readonly #provider: Provider;
  readonly #bridge: NodeVfsFilesystemBridge;
  readonly #pinned: NodeVfsPinnedReadBridge | undefined;
  readonly #snapshot: ReadSnapshot | undefined;
  readonly #releaseSession: () => void;
  #path: string;
  #closed = false;
  #dirty = false;
  #dirtyAccounted = false;
  #creationDirty = false;
  #contiguousEnd: number | undefined;
  #contiguousBytes = 0;
  #retry: { cutoff: number; prepared: NodeVfsPreparedContent } | undefined;
  constructor(
    provider: Provider,
    bridge: NodeVfsFilesystemBridge,
    order: number,
    path: string,
    writable: boolean,
    coordinator: InodeCoordinator | undefined,
    pinned: NodeVfsPinnedReadBridge | undefined,
    snapshot: ReadSnapshot | undefined,
    releaseSession: () => void,
  ) {
    this.#provider = provider;
    this.#bridge = bridge;
    this.order = order;
    this.#path = path;
    this.writable = writable;
    this.coordinator = coordinator;
    this.#pinned = pinned;
    this.#snapshot = snapshot;
    this.#releaseSession = releaseSession;
  }
  get path(): string {
    return this.#path;
  }
  get dirty(): boolean {
    return this.#dirty;
  }
  get creationDirty(): boolean {
    return this.#creationDirty;
  }
  get residentBytes(): number {
    return this.admissions.reduce(
      (sum, admission) =>
        sum + (admission.kind === "write" ? admission.payload.residentBytes : 0),
      0,
    );
  }
  get requiredSequence(): number | undefined {
    return this.admissions.reduce<number | undefined>(
      (maximum, admission) =>
        maximum === undefined
          ? admission.sequence
          : Math.max(maximum, admission.sequence),
      undefined,
    );
  }
  readIntoSync(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    this.#assertOpen();
    validateDestination(
      destination,
      destinationOffset,
      position,
      length,
      this.#bridge.filesystemLimits.maxMaterializedBytes,
    );
    let read: number;
    if (this.writable) {
      if (!this.coordinator) throw new Error("writable session lacks coordinator");
      read = this.coordinator.readInto(
        destination,
        destinationOffset,
        position,
        length,
      );
    } else if (this.#snapshot) {
      read = this.#snapshot.readInto(destination, destinationOffset, position, length);
    } else {
      read = this.#pinned!.readIntoSync(
        destination,
        destinationOffset,
        position,
        length,
      );
    }
    this.#provider.direct(read);
    return read;
  }
  readRangeSync(position: number, length: number): Uint8Array {
    checkedInteger(position, "position");
    checkedInteger(length, "length");
    if (length > this.#bridge.filesystemLimits.maxMaterializedBytes)
      fail("EFBIG", "read exceeds materialization limit");
    const output = new Uint8Array(length);
    const read = this.readIntoSync(output, 0, position, length);
    return read === output.byteLength ? output : output.slice(0, read);
  }
  writeSync(content: Uint8Array, position: number): number {
    this.#assertWritable();
    if (!(content instanceof Uint8Array))
      fail("EINVAL", "write content must be a Uint8Array", "writeSync", this.#path);
    checkedInteger(position, "position");
    if (position + content.byteLength > this.#bridge.storageLimits.maxFileBytes)
      fail("EFBIG", "write exceeds maxFileBytes", "writeSync", this.#path);
    if (content.byteLength === 0) return 0;
    this.#provider.recordWriteCallback(
      position,
      content.byteLength,
      this.#contiguousEnd === position
        ? this.#contiguousBytes + content.byteLength
        : content.byteLength,
    );
    this.#contiguousBytes =
      this.#contiguousEnd === position
        ? this.#contiguousBytes + content.byteLength
        : content.byteLength;
    this.#contiguousEnd = position + content.byteLength;
    if (!this.coordinator) throw new Error("writable session lacks coordinator");
    this.discardRetry();
    if (content.byteLength > this.#bridge.runtimeLimits.maxWriteSessionBytes) {
      const payload = this.#provider.prepareCallerPayload(content);
      try {
        this.#provider.addWrite(this, this.coordinator, position, payload);
      } catch (error) {
        payload.release();
        throw error;
      }
      return content.byteLength;
    }
    while (
      this.residentBytes + content.byteLength >
      this.#bridge.runtimeLimits.maxWriteSessionBytes
    ) {
      const before = this.residentBytes;
      this.#provider.stageSession(this, true);
      if (this.residentBytes >= before)
        fail(
          "EAGAIN",
          "session pressure could not be relieved",
          "writeSync",
          this.#path,
        );
    }
    const payload = this.#provider.allocateResident(content, 0, content.byteLength);
    try {
      this.#provider.addWrite(this, this.coordinator, position, payload);
    } catch (error) {
      payload.release();
      throw error;
    }
    return content.byteLength;
  }
  truncateSync(size: number): void {
    this.#assertWritable();
    checkedInteger(size, "size");
    if (size > this.#bridge.storageLimits.maxFileBytes)
      fail("EFBIG", "truncate exceeds maxFileBytes", "truncateSync", this.#path);
    if (!this.coordinator) throw new Error("writable session lacks coordinator");
    if (size === this.coordinator.size) return;
    this.discardRetry();
    this.#provider.addTruncate(this, this.coordinator, size);
  }
  statSync(): FileStat {
    this.#assertOpen();
    if (this.writable)
      return this.#provider.statCoordinator(this.coordinator!, this.#path);
    if (this.#snapshot) {
      return Object.freeze({
        id: this.#snapshot.inodeId,
        name: this.#path.split("/").at(-1) ?? "",
        type: "file" as const,
        mode: this.#snapshot.mode,
        size: this.#snapshot.size,
        nlink: this.#snapshot.nlink,
        mtimeMs: this.#snapshot.mtimeMs,
        ctimeMs: this.#snapshot.ctimeMs,
        birthtimeMs: this.#snapshot.birthtimeMs,
        isFile: () => true,
        isDirectory: () => false,
        isSymbolicLink: () => false,
      });
    }
    return this.#pinned!.stat;
  }
  stagePrefixSync(): void {
    this.#assertWritable();
    this.#provider.stageSession(this, false);
  }
  commitVisibleSync(_options: FlushOptions = {}): void {
    this.#assertWritable();
    if (!this.#dirty) return;
    this.#provider.commitSession(this, "explicitCommit");
  }
  flushSync(options?: FlushOptions): void {
    this.#assertWritable();
    if (this.#dirty) this.#provider.commitSession(this, "flush");
  }
  closeSync(): void {
    if (this.#closed) return;
    if (this.writable && this.#dirty) this.#provider.commitSession(this, "close");
    this.finishClose();
  }
  abortSync(): void {
    if (this.#closed) return;
    if (this.writable) this.#provider.abortSession(this);
    this.discardRetry();
    this.finishClose();
  }
  addAdmission(admission: Admission): void {
    this.admissions.push(admission);
    this.#dirty = true;
    if (!this.#dirtyAccounted) {
      this.#provider.dirty(1);
      this.#dirtyAccounted = true;
    }
  }
  markCreationDirty(): void {
    if (this.#creationDirty) return;
    this.#creationDirty = true;
    this.#dirty = true;
  }
  creationCommitted(): void {
    this.#creationDirty = false;
    this.updateDirty();
  }
  committed(admission: Admission): void {
    const index = this.admissions.indexOf(admission);
    if (index >= 0) this.admissions.splice(index, 1);
    this.updateDirty();
  }
  aborted(admission: Admission): void {
    this.committed(admission);
  }
  retryPrepared(cutoff: number): NodeVfsPreparedContent | undefined {
    return this.#retry?.cutoff === cutoff ? this.#retry.prepared : undefined;
  }
  setRetryPrepared(cutoff: number, prepared: NodeVfsPreparedContent): void {
    this.discardRetry();
    this.#retry = { cutoff, prepared };
    this.#provider.addStaged(prepared.size);
  }
  consumeRetryPrepared(cutoff: number): void {
    if (this.#retry?.cutoff === cutoff) {
      this.#provider.releaseStaged(this.#retry.prepared.size);
      this.#retry = undefined;
    }
  }
  invalidateCommittedRetries(cutoff: number): void {
    if (this.#retry && this.#retry.cutoff <= cutoff) this.discardRetry();
  }
  renamePath(source: string, destination: string): void {
    if (this.#path === source) this.#path = destination;
  }
  renamePathPrefix(source: string, destination: string): void {
    if (this.#path === source || this.#path.startsWith(`${source}/`))
      this.#path = `${destination}${this.#path.slice(source.length)}`;
  }
  private discardRetry(): void {
    if (!this.#retry) return;
    const prepared = this.#retry.prepared;
    this.#bridge.abortPreparedSync(prepared);
    this.#provider.releaseStaged(prepared.size);
    this.#retry = undefined;
  }
  private updateDirty(): void {
    if (this.#dirty && this.admissions.length === 0 && !this.#creationDirty) {
      this.#dirty = false;
      if (this.#dirtyAccounted) {
        this.#provider.dirty(-1);
        this.#dirtyAccounted = false;
      }
    }
  }
  private finishClose(): void {
    this.#pinned?.closeSync();
    this.#snapshot?.close();
    this.#closed = true;
    this.#releaseSession();
    this.#provider.removeSession(this);
  }
  #assertOpen(): void {
    if (this.#closed) fail("EBADF", "Node file session is closed");
  }
  #assertWritable(): void {
    this.#assertOpen();
    if (!this.writable) fail("EBADF", "Node file session is not writable");
    this.#provider.assertWritableView(this.coordinator);
  }
}

export async function openNodeVfs(options: OpenNodeVfsOptions): Promise<NodeVfsHandle> {
  const opened = await openNodeVfsBridge({
    database: options.database,
    ...(options.branchId === undefined ? {} : { branchId: options.branchId }),
    ...(options.runtime === undefined ? {} : { runtime: options.runtime }),
    ownsDatabase: false,
  });
  let provider: Provider;
  try {
    provider = new Provider(opened.bridge, options.observer);
  } catch (error) {
    await opened.runtime.close();
    throw error;
  }
  let closed = false;
  return Object.freeze({
    filesystem: opened.filesystem,
    runtime: opened.runtime,
    provider,
    async close() {
      if (closed) return;
      provider.closeAllSync();
      await opened.runtime.close();
      if (options.ownsDatabase) await options.database.close();
      closed = true;
    },
  });
}

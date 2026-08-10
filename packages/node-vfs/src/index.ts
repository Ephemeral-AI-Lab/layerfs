import {
  EphemeralFS,
  FilesystemError,
  type FileStat,
  type RuntimeLimits,
} from "@ephemeralai/fs";
import {
  createNodeVfsBridge,
  type NodeVfsFilesystemBridge,
  type SyncPreparedContent,
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
  readonly filesystem: EphemeralFS;
  readonly provider: NodeVfsProvider;
  close(): Promise<void>;
}
export interface NodeVfsMetricsSnapshot {
  readonly openSessions: number;
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

interface MutableMetrics {
  openSessions: number;
  dirtySessions: number;
  residentWriteBytes: number;
  peakResidentWriteBytes: number;
  residentControlBytes: number;
  peakManagedResidentBytes: number;
  stagedLogicalBytes: number;
  admittedWriteBytes: number;
  flushedWriteBytes: number;
  flushCount: number;
  forcedFlushCount: number;
  failedFlushCount: number;
  rejectedWriteCount: number;
  directReadBytes: number;
  coreBatchCount: number;
}
type Edit =
  | { readonly kind: "write"; readonly position: number; readonly bytes: Uint8Array }
  | { readonly kind: "truncate"; readonly size: number };

class Provider implements NodeVfsProvider {
  readonly capabilities: NodeVfsCapabilities;
  readonly metrics: NodeVfsMetrics;
  readonly #bridge: NodeVfsFilesystemBridge;
  readonly #observer: NodeVfsObserver | undefined;
  readonly #sessions = new Map<string, Session>();
  readonly #values: MutableMetrics = {
    openSessions: 0,
    dirtySessions: 0,
    residentWriteBytes: 0,
    peakResidentWriteBytes: 0,
    residentControlBytes: 0,
    peakManagedResidentBytes: 0,
    stagedLogicalBytes: 0,
    admittedWriteBytes: 0,
    flushedWriteBytes: 0,
    flushCount: 0,
    forcedFlushCount: 0,
    failedFlushCount: 0,
    rejectedWriteCount: 0,
    directReadBytes: 0,
    coreBatchCount: 0,
  };
  #closed = false;
  constructor(bridge: NodeVfsFilesystemBridge, observer?: NodeVfsObserver) {
    this.#bridge = bridge;
    this.#observer = observer;
    this.capabilities = Object.freeze({
      cowPageBytes: bridge.cowPageBytes,
      runtime: bridge.runtimeLimits,
      preferredReadBytes: bridge.filesystemLimits.preferredStreamChunkBytes,
      supportsDirectRangeIo: true,
      supportsWriteSessions: true,
      supportsDataSync: true,
    });
    this.metrics = Object.freeze({
      snapshot: () => Object.freeze({ ...this.#values }),
    });
  }
  existsSync(path: string): boolean {
    this.#assert();
    return this.#bridge.existsSync(path);
  }
  statSync(path: string): FileStat {
    this.#assert();
    return this.#bridge.statSync(path, true);
  }
  lstatSync(path: string): FileStat {
    this.#assert();
    return this.#bridge.statSync(path, false);
  }
  readdirSync(path: string): string[] {
    this.#assert();
    return this.#bridge.readdirSync(path).map(({ name }) => name);
  }
  readlinkSync(path: string): string {
    this.#assert();
    return this.#bridge.readlinkSync(path);
  }
  readRangeSync(path: string, position: number, length: number): Uint8Array {
    this.#assert();
    const value = this.#bridge.readRangeSync(path, position, length);
    this.#values.directReadBytes += value.length;
    this.#values.coreBatchCount += 1;
    return value;
  }
  openFileSync(path: string, options: OpenFileOptions = {}): NodeFileSession {
    this.#assert();
    if (this.#sessions.size >= this.capabilities.runtime.maxOpenNodeVfsSessions)
      throw new FilesystemError("EAGAIN", "Node VFS session limit exceeded");
    const exists = this.#bridge.existsSync(path);
    if (!exists && !options.create)
      throw new FilesystemError("ENOENT", `file does not exist: ${path}`);
    if (exists && options.exclusive)
      throw new FilesystemError("EEXIST", `file exists: ${path}`);
    if (exists && this.#bridge.statSync(path).type !== "file")
      throw new FilesystemError("EISDIR", `not a regular file: ${path}`);
    const session = new Session(
      this,
      this.#bridge,
      path,
      { ...options, writable: options.writable ?? options.create ?? false },
      exists,
    );
    this.#sessions.set(session.id, session);
    this.#values.openSessions += 1;
    this.#values.residentControlBytes += 512;
    this.#updatePeak();
    this.#emit({ kind: "session-open", sessionId: session.id });
    return session;
  }
  mkdirSync(path: string, options?: { recursive?: boolean; mode?: number }): void {
    this.#assert();
    this.#bridge.mkdirSync(path, options);
  }
  chmodSync(path: string, mode: number): void {
    this.#assert();
    this.#bridge.chmodSync(path, mode);
  }
  linkSync(existingPath: string, newPath: string): void {
    this.#assert();
    this.#bridge.linkSync(existingPath, newPath);
  }
  symlinkSync(target: string, path: string): void {
    this.#assert();
    this.#bridge.symlinkSync(target, path);
  }
  renameSync(oldPath: string, newPath: string): void {
    this.#assert();
    this.#bridge.renameSync(oldPath, newPath);
  }
  unlinkSync(path: string): void {
    this.#assert();
    this.#bridge.unlinkSync(path);
  }
  rmdirSync(path: string): void {
    this.#assert();
    this.#bridge.rmdirSync(path);
  }
  syncSync(): void {
    this.#assert();
    for (const session of [...this.#sessions.values()])
      if (session.dirty) session.commitVisibleSync();
  }
  closeSync(): void {
    if (this.#closed) return;
    for (const session of [...this.#sessions.values()]) session.abortSync();
    this.#closed = true;
  }
  admit(bytes: number): void {
    if (
      bytes > this.capabilities.runtime.maxWriteSessionBytes ||
      this.#values.residentWriteBytes + bytes >
        this.capabilities.runtime.maxPendingWriteBytes ||
      this.#values.residentWriteBytes + this.#values.residentControlBytes + bytes >
        this.capabilities.runtime.maxManagedResidentBytes
    ) {
      this.#values.rejectedWriteCount += 1;
      this.#emit({ kind: "memory-rejected", bytes });
      throw new FilesystemError("EAGAIN", "Node VFS write memory limit exceeded");
    }
    this.#values.residentWriteBytes += bytes;
    this.#values.admittedWriteBytes += bytes;
    this.#updatePeak();
  }
  release(bytes: number): void {
    this.#values.residentWriteBytes = Math.max(
      0,
      this.#values.residentWriteBytes - bytes,
    );
  }
  dirty(delta: 1 | -1): void {
    this.#values.dirtySessions += delta;
  }
  staged(bytes: number): void {
    this.#values.stagedLogicalBytes += bytes;
    this.#values.forcedFlushCount += 1;
    this.#emit({ kind: "forced-flush", bytes });
  }
  flushed(bytes: number): void {
    this.#values.flushedWriteBytes += bytes;
    this.#values.flushCount += 1;
    this.#values.coreBatchCount += 1;
  }
  failed(error: unknown): void {
    this.#values.failedFlushCount += 1;
    this.#emit({
      kind: "flush-failed",
      code: error instanceof FilesystemError ? error.code : "EIO",
    });
  }
  direct(bytes: number): void {
    this.#values.directReadBytes += bytes;
    this.#values.coreBatchCount += 1;
  }
  remove(session: Session): void {
    if (this.#sessions.delete(session.id)) {
      this.#values.openSessions -= 1;
      this.#values.residentControlBytes -= 512;
      this.#emit({ kind: "session-close", sessionId: session.id });
    }
  }
  #assert(): void {
    if (this.#closed) throw new FilesystemError("EBADF", "Node VFS provider is closed");
  }
  #emit(event: NodeVfsObservation): void {
    try {
      this.#observer?.(event);
    } catch {}
  }
  #updatePeak(): void {
    this.#values.peakResidentWriteBytes = Math.max(
      this.#values.peakResidentWriteBytes,
      this.#values.residentWriteBytes,
    );
    this.#values.peakManagedResidentBytes = Math.max(
      this.#values.peakManagedResidentBytes,
      this.#values.residentWriteBytes + this.#values.residentControlBytes,
    );
  }
}

class Session implements NodeFileSession {
  readonly id = globalThis.crypto.randomUUID();
  readonly path: string;
  readonly writable: boolean;
  readonly #provider: Provider;
  readonly #bridge: NodeVfsFilesystemBridge;
  readonly #options: OpenFileOptions;
  readonly #edits: Edit[] = [];
  #resident = 0;
  #staged: SyncPreparedContent | undefined;
  #closed = false;
  #visible: boolean;
  dirty = false;
  constructor(
    provider: Provider,
    bridge: NodeVfsFilesystemBridge,
    path: string,
    options: OpenFileOptions,
    existed: boolean,
  ) {
    this.#provider = provider;
    this.#bridge = bridge;
    this.path = path;
    this.writable = options.writable ?? false;
    this.#options = options;
    this.#visible = existed;
    if (options.truncate) this.truncateSync(0);
  }
  readIntoSync(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number {
    this.#assert();
    if (!this.dirty && !this.#staged && this.#visible) {
      const read = this.#bridge.readIntoSync(
        this.path,
        destination,
        destinationOffset,
        position,
        length,
      );
      this.#provider.direct(read);
      return read;
    }
    const value = this.#compose();
    const available = Math.max(0, Math.min(length, value.length - position));
    destination.set(value.subarray(position, position + available), destinationOffset);
    return available;
  }
  readRangeSync(position: number, length: number): Uint8Array {
    const output = new Uint8Array(length);
    const read = this.readIntoSync(output, 0, position, length);
    return read === length ? output : output.slice(0, read);
  }
  writeSync(content: Uint8Array, position: number): number {
    this.#assertWritable();
    if (!Number.isSafeInteger(position) || position < 0)
      throw new FilesystemError("EINVAL", "invalid write position");
    const copy = content.slice();
    this.#provider.admit(copy.byteLength);
    this.#resident += copy.byteLength;
    this.#edits.push({ kind: "write", position, bytes: copy });
    this.#markDirty();
    return copy.byteLength;
  }
  truncateSync(size: number): void {
    this.#assertWritable();
    if (!Number.isSafeInteger(size) || size < 0)
      throw new FilesystemError("EINVAL", "invalid truncate size");
    this.#edits.push({ kind: "truncate", size });
    this.#markDirty();
  }
  statSync(): FileStat {
    this.#assert();
    if (!this.dirty && !this.#staged && this.#visible)
      return this.#bridge.statSync(this.path);
    const base = this.#visible ? this.#bridge.statSync(this.path) : undefined;
    const size = this.#compose().length;
    const now = Date.now();
    return Object.freeze({
      id: base?.id ?? this.id,
      name: this.path.split("/").at(-1) ?? "",
      type: "file",
      mode: base?.mode ?? this.#options.mode ?? 0o666,
      size,
      nlink: base?.nlink ?? 1,
      mtimeMs: now,
      ctimeMs: now,
      birthtimeMs: base?.birthtimeMs ?? now,
      isFile: () => true,
      isDirectory: () => false,
      isSymbolicLink: () => false,
    });
  }
  stagePrefixSync(): void {
    this.#assertWritable();
    if (!this.dirty) return;
    const value = this.#compose();
    this.#staged = this.#bridge.prepareContentSync(value);
    this.#provider.staged(value.length);
  }
  commitVisibleSync(_options: FlushOptions = {}): void {
    this.#assertWritable();
    if (!this.dirty) return;
    try {
      const value = this.#compose();
      const prepared =
        this.#staged && !this.#edits.length
          ? this.#staged
          : this.#bridge.prepareContentSync(value);
      this.#bridge.commitPreparedSync(this.path, prepared, {
        create: this.#options.create ?? !this.#visible,
        ...(this.#options.exclusive === undefined || this.#visible
          ? {}
          : { exclusive: this.#options.exclusive }),
        ...(this.#options.mode === undefined ? {} : { mode: this.#options.mode }),
      });
      this.#visible = true;
      this.#provider.flushed(value.length);
      this.#clearDirty();
      this.#staged = undefined;
    } catch (error) {
      this.#provider.failed(error);
      throw error;
    }
  }
  flushSync(options?: FlushOptions): void {
    this.commitVisibleSync(options);
  }
  closeSync(): void {
    if (this.#closed) return;
    if (this.dirty) this.commitVisibleSync();
    this.#closed = true;
    this.#provider.remove(this);
  }
  abortSync(): void {
    if (this.#closed) return;
    this.#clearDirty();
    this.#staged = undefined;
    this.#closed = true;
    this.#provider.remove(this);
  }
  #compose(): Uint8Array {
    let value: Uint8Array;
    if (this.#staged) {
      value = new Uint8Array(this.#staged.size);
      this.#bridge.readPreparedIntoSync(this.#staged, value, 0, 0, value.length);
    } else if (this.#visible && this.#bridge.existsSync(this.path))
      value = this.#bridge.readFileSync(this.path);
    else value = new Uint8Array();
    for (const edit of this.#edits) {
      if (edit.kind === "truncate") {
        const resized = new Uint8Array(edit.size);
        resized.set(value.subarray(0, edit.size));
        value = resized;
      } else {
        const size = Math.max(value.length, edit.position + edit.bytes.length);
        if (size !== value.length) {
          const resized = new Uint8Array(size);
          resized.set(value);
          value = resized;
        }
        value.set(edit.bytes, edit.position);
      }
    }
    return value;
  }
  #markDirty(): void {
    if (!this.dirty) {
      this.dirty = true;
      this.#provider.dirty(1);
    }
  }
  #clearDirty(): void {
    if (this.dirty) {
      this.dirty = false;
      this.#provider.dirty(-1);
    }
    this.#provider.release(this.#resident);
    this.#resident = 0;
    this.#edits.length = 0;
  }
  #assert(): void {
    if (this.#closed) throw new FilesystemError("EBADF", "Node file session is closed");
  }
  #assertWritable(): void {
    this.#assert();
    if (!this.writable)
      throw new FilesystemError("EBADF", "Node file session is not writable");
  }
}

export async function openNodeVfs(options: OpenNodeVfsOptions): Promise<NodeVfsHandle> {
  if (options.branchId !== undefined)
    throw new FilesystemError(
      "EINVAL",
      "synchronous branch mounts are not enabled in version 0.1",
    );
  const filesystem = await EphemeralFS.open({
    database: options.database,
    ...(options.runtime === undefined ? {} : { runtime: options.runtime }),
    ownsDatabase: false,
  });
  const bridge = createNodeVfsBridge({
    database: options.database,
    ...(options.runtime === undefined ? {} : { runtime: options.runtime }),
  });
  const provider = new Provider(bridge, options.observer);
  let closed = false;
  return Object.freeze({
    filesystem,
    provider,
    async close() {
      if (closed) return;
      closed = true;
      provider.closeSync();
      await filesystem.close();
      if (options.ownsDatabase) options.database.close();
    },
  });
}

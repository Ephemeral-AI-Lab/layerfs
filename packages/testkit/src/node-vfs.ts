import type { EphemeralFS, FileStat, RuntimeLimits } from "@ephemeralai/fs";

export type NodeVfsConformanceCaseId =
  | "pinned-direct-reads"
  | "irregular-range-writes"
  | "three-session-orders"
  | "pending-namespace"
  | "hidden-staging"
  | "flush-close-abort"
  | "session-backpressure";

export interface NodeVfsConformanceMetrics {
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
  readonly cowEditCount: number;
  readonly cowEditSourceBytes: number;
}

export interface NodeVfsConformanceSession {
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
  commitVisibleSync(options?: { readonly dataOnly?: boolean }): void;
  flushSync(options?: { readonly dataOnly?: boolean }): void;
  closeSync(): void;
  abortSync(): void;
}

export interface NodeVfsConformanceProvider {
  readonly capabilities: {
    readonly cowPageBytes: 4096 | 8192 | 16384;
    readonly runtime: Readonly<RuntimeLimits>;
    readonly supportsDirectRangeIo: true;
    readonly supportsWriteSessions: true;
    readonly supportsDataSync: boolean;
  };
  readonly metrics: { snapshot(): NodeVfsConformanceMetrics };
  existsSync(path: string): boolean;
  statSync(path: string): FileStat;
  readRangeSync(path: string, position: number, length: number): Uint8Array;
  openFileSync(
    path: string,
    options?: {
      readonly writable?: boolean;
      readonly create?: boolean;
      readonly exclusive?: boolean;
      readonly truncate?: boolean;
      readonly mode?: number;
    },
  ): NodeVfsConformanceSession;
  linkSync(existingPath: string, newPath: string): void;
  symlinkSync(target: string, path: string): void;
  readlinkSync(path: string): string;
  renameSync(oldPath: string, newPath: string): void;
  unlinkSync(path: string): void;
  syncSync(): void;
  closeSync(): void;
}

export interface NodeVfsConformanceHandle {
  readonly provider: NodeVfsConformanceProvider;
  readonly filesystem: EphemeralFS;
  close(): Promise<void>;
}

export interface NodeVfsConformanceFactory {
  create(options?: {
    readonly runtime?: Partial<RuntimeLimits>;
    readonly cowPageBytes?: 4096 | 8192 | 16384;
  }): Promise<NodeVfsConformanceHandle>;
}

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`Node VFS conformance: ${message}`);
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  for (let index = 0; index < left.byteLength; index += 1)
    if (left[index] !== right[index]) return false;
  return true;
}

function text(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function decoded(value: Uint8Array): string {
  return new TextDecoder().decode(value);
}

function expectCode(operation: () => unknown, code: string): void {
  try {
    operation();
  } catch (error) {
    invariant(
      error !== null &&
        typeof error === "object" &&
        "code" in error &&
        error.code === code,
      `expected ${code}, received ${String(error)}`,
    );
    return;
  }
  throw new Error(`Node VFS conformance: expected ${code}`);
}

async function withHandle<T>(
  factory: NodeVfsConformanceFactory,
  callback: (handle: NodeVfsConformanceHandle) => Promise<T> | T,
  options?: Parameters<NodeVfsConformanceFactory["create"]>[0],
): Promise<T> {
  const handle = await factory.create(options);
  try {
    return await callback(handle);
  } finally {
    await handle.close();
  }
}

/** Run the host-neutral, synchronous Node VFS conformance scenario. */
export async function runNodeVfsConformance(
  factory: NodeVfsConformanceFactory,
): Promise<readonly NodeVfsConformanceCaseId[]> {
  const passed: NodeVfsConformanceCaseId[] = [];
  const threeSessionOrders = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
  ] as const;
  await withHandle(factory, async ({ provider, filesystem }) => {
    invariant(
      provider.capabilities.runtime.maxWriteSessionBytes === 16 * 1024 * 1024 &&
        provider.capabilities.runtime.maxPendingWriteBytes === 64 * 1024 * 1024 &&
        provider.capabilities.runtime.maxManagedResidentBytes === 128 * 1024 * 1024,
      "default Node VFS memory limits differ",
    );
    invariant(
      Object.isFrozen(provider.capabilities) &&
        Object.isFrozen(provider.capabilities.runtime),
      "capabilities are mutable",
    );
    const writer = provider.openFileSync("/pinned", { writable: true, create: true });
    writer.writeSync(text("0123456789abcdef"), 0);
    writer.closeSync();
    const pinned = provider.openFileSync("/pinned");
    invariant(
      decoded(provider.readRangeSync("/pinned", 0, 4)) === "0123" &&
        decoded(provider.readRangeSync("/pinned", 7, 4)) === "789a" &&
        decoded(provider.readRangeSync("/pinned", 14, 8)) === "ef" &&
        provider.readRangeSync("/pinned", 32, 4).byteLength === 0,
      "start, middle, end, or EOF range reads differ",
    );
    const destination = new Uint8Array(32).fill(0xa5);
    invariant(
      pinned.readIntoSync(destination, 7, 4, 6) === 6,
      "readIntoSync returned the wrong byte count",
    );
    invariant(
      destination.slice(0, 7).every((value) => value === 0xa5) &&
        destination.slice(13).every((value) => value === 0xa5) &&
        decoded(destination.slice(7, 13)) === "456789",
      "readIntoSync changed destination sentinels",
    );
    const replacement = provider.openFileSync("/pinned", { writable: true });
    replacement.writeSync(text("replacement"), 0);
    replacement.truncateSync(11);
    invariant(
      decoded(provider.readRangeSync("/pinned", 0, 11)) === "replacement",
      "provider did not expose admitted bytes before commit",
    );
    const admitted = provider.openFileSync("/pinned");
    invariant(
      decoded(admitted.readRangeSync(0, 11)) === "replacement",
      "second handle did not expose provider-admitted bytes",
    );
    admitted.closeSync();
    replacement.commitVisibleSync();
    let collection = await filesystem.maintenance.collectGarbage({
      runId: "node-vfs-pinned-lease",
      maxBatches: 1,
    });
    for (let batch = 0; batch < 10_000 && collection.state !== "complete"; batch += 1)
      collection = await filesystem.maintenance.collectGarbage({
        runId: "node-vfs-pinned-lease",
        maxBatches: 1,
      });
    invariant(
      collection.state === "complete",
      "pinned-read collection did not complete",
    );
    invariant(
      decoded(pinned.readRangeSync(0, 16)) === "0123456789abcdef",
      "pinned read selection changed after overwrite",
    );
    replacement.closeSync();
    pinned.closeSync();
    passed.push("pinned-direct-reads");
  });

  await withHandle(factory, ({ provider }) => {
    const session = provider.openFileSync("/ranges", { writable: true, create: true });
    for (const [position, value] of [
      [0, "abc"],
      [3, "defgh"],
      [8, "ij"],
    ] as const)
      session.writeSync(text(value), position);
    session.writeSync(text("XY"), 2);
    session.writeSync(Uint8Array.of(90), 14);
    invariant(
      equalBytes(
        session.readRangeSync(0, 15),
        Uint8Array.from([97, 98, 88, 89, 101, 102, 103, 104, 105, 106, 0, 0, 0, 0, 90]),
      ),
      "overlap or sparse write result differs",
    );
    session.truncateSync(18);
    invariant(
      session.statSync().size === 18 &&
        session.readRangeSync(15, 3).every((value) => value === 0),
      "truncate growth did not zero-fill",
    );
    session.truncateSync(6);
    invariant(
      decoded(session.readRangeSync(0, 16)) === "abXYef",
      "truncate shrink differs",
    );
    session.closeSync();
    const renamed = provider.openFileSync("/ranges", { writable: true });
    renamed.writeSync(Uint8Array.of(90), 5);
    provider.renameSync("/ranges", "/ranges-renamed");
    expectCode(() => provider.unlinkSync("/ranges-renamed"), "EBUSY");
    renamed.closeSync();
    invariant(
      decoded(provider.readRangeSync("/ranges-renamed", 0, 6)) === "abXYeZ",
      "rename did not retain dirty inode coordination",
    );
    const unlinked = provider.openFileSync("/ranges-renamed");
    provider.unlinkSync("/ranges-renamed");
    invariant(
      decoded(unlinked.readRangeSync(0, 6)) === "abXYeZ" &&
        !provider.existsSync("/ranges-renamed"),
      "pinned read did not survive unlink",
    );
    unlinked.closeSync();
    passed.push("irregular-range-writes");
  });

  for (const commitOrder of threeSessionOrders)
    for (const closeOrder of threeSessionOrders)
      await withHandle(factory, ({ provider }) => {
        const initial = provider.openFileSync("/ordered", {
          writable: true,
          create: true,
        });
        initial.writeSync(text("000"), 0);
        initial.closeSync();
        const sessions = [0, 1, 2].map(() =>
          provider.openFileSync("/ordered", { writable: true }),
        );
        sessions[0]!.writeSync(Uint8Array.of(65), 0);
        sessions[1]!.writeSync(Uint8Array.of(66), 1);
        sessions[2]!.writeSync(Uint8Array.of(67), 2);
        for (const index of commitOrder) sessions[index]!.commitVisibleSync();
        invariant(
          decoded(provider.readRangeSync("/ordered", 0, 3)) === "ABC",
          `three-session commit ${commitOrder.join(",")} close ${closeOrder.join(",")} lost an update`,
        );
        for (const index of closeOrder) sessions[index]!.closeSync();
      });
  passed.push("three-session-orders");

  await withHandle(factory, ({ provider }) => {
    const pending = provider.openFileSync("/pending", {
      writable: true,
      create: true,
      exclusive: true,
    });
    pending.writeSync(text("pending"), 0);
    invariant(
      provider.existsSync("/pending"),
      "pending create is not provider-visible",
    );
    expectCode(
      () =>
        provider.openFileSync("/pending", {
          writable: true,
          create: true,
          exclusive: true,
        }),
      "EEXIST",
    );
    provider.linkSync("/pending", "/pending-link");
    provider.renameSync("/pending-link", "/pending-renamed");
    invariant(
      decoded(provider.readRangeSync("/pending-renamed", 0, 7)) === "pending",
      "pending hard-link rename lost inode identity",
    );
    pending.commitVisibleSync();
    provider.symlinkSync("/pending", "/pending-symlink");
    invariant(
      provider.readlinkSync("/pending-symlink") === "/pending" &&
        decoded(provider.readRangeSync("/pending-symlink", 0, 7)) === "pending",
      "symlink behavior differs",
    );
    pending.closeSync();
    passed.push("pending-namespace");
  });

  await withHandle(factory, async ({ provider, filesystem }) => {
    const session = provider.openFileSync("/staged", { writable: true, create: true });
    session.writeSync(new Uint8Array(1024), 0);
    invariant(
      provider.metrics.snapshot().residentWriteBytes === 1024,
      "resident bytes differ",
    );
    session.stagePrefixSync();
    invariant(
      provider.metrics.snapshot().residentWriteBytes === 0,
      "hidden staging did not release resident payload capacity",
    );
    let visible = true;
    try {
      await filesystem.stat("/staged");
    } catch (error) {
      visible = !(
        error !== null &&
        typeof error === "object" &&
        "code" in error &&
        error.code === "ENOENT"
      );
    }
    invariant(!visible, "hidden staging advanced portable visible state");
    session.flushSync({ dataOnly: true });
    invariant((await filesystem.stat("/staged")).size === 1024, "flush did not commit");
    session.closeSync();
    passed.push("hidden-staging");
  });

  await withHandle(factory, ({ provider }) => {
    const first = provider.openFileSync("/sync-a", { writable: true, create: true });
    const second = provider.openFileSync("/sync-b", { writable: true, create: true });
    first.writeSync(text("a"), 0);
    second.writeSync(text("b"), 0);
    provider.syncSync();
    invariant(
      provider.metrics.snapshot().dirtySessions === 0,
      "provider sync left dirty sessions",
    );
    first.closeSync();
    second.closeSync();
    const aborted = provider.openFileSync("/aborted", { writable: true, create: true });
    aborted.writeSync(new Uint8Array(4096), 0);
    aborted.abortSync();
    invariant(
      !provider.existsSync("/aborted") &&
        provider.metrics.snapshot().residentWriteBytes === 0,
      "abort retained pending state or resident capacity",
    );
    passed.push("flush-close-abort");
  });

  for (const count of [1, 16, 64] as const)
    await withHandle(
      factory,
      ({ provider }) => {
        const bytesPerSession = (16 * 1024 * 1024) / count;
        const sessions = Array.from({ length: count }, (_, index) => {
          const session = provider.openFileSync(`/limit-${count}-${index}`, {
            writable: true,
            create: true,
          });
          session.writeSync(new Uint8Array(bytesPerSession), 0);
          return session;
        });
        expectCode(
          () =>
            provider.openFileSync(`/limit-${count}-overflow`, {
              writable: true,
              create: true,
            }),
          "EAGAIN",
        );
        sessions[0]!.writeSync(Uint8Array.of(1), bytesPerSession);
        invariant(
          provider.metrics.snapshot().forcedFlushCount >= 1,
          `${count}-session pending-write boundary did not force hidden staging`,
        );
        for (const session of sessions) session.abortSync();
        const metrics = provider.metrics.snapshot();
        invariant(
          metrics.openSessions === 0 &&
            metrics.residentWriteBytes === 0 &&
            metrics.stagedLogicalBytes === 0 &&
            metrics.residentControlBytes === 0 &&
            metrics.peakManagedResidentBytes <=
              provider.capabilities.runtime.maxManagedResidentBytes,
          `${count}-session backpressure or cleanup metrics differ`,
        );
      },
      {
        runtime: {
          maxOpenNodeVfsSessions: count,
          maxWriteSessionBytes: (16 * 1024 * 1024) / count,
          maxPendingWriteBytes: 16 * 1024 * 1024,
        },
      },
    );
  passed.push("session-backpressure");
  return Object.freeze(passed);
}

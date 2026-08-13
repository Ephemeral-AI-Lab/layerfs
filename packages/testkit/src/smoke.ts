import {
  EphemeralFS,
  type Branches,
  type EphemeralBranch,
  type EphemeralFilesystem,
  type FilesystemCapabilities,
  type FilesystemMaintenance,
  type FilesystemObservation,
} from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory, ConformanceDatabase } from "./index.js";

const MIB = 1024 * 1024;
export const PORTABLE_SMOKE_SEED = 0x5eed5eed;
export const PORTABLE_SMOKE_PAYLOAD_BYTES = 16 * MIB;
export const PORTABLE_SMOKE_COW_EDITS = 5_000;
export const PORTABLE_SMOKE_NAMESPACE_OPERATIONS = 2_000;
export const PORTABLE_SMOKE_ACTORS_PER_KIND = 16;
export const PORTABLE_SMOKE_OPERATIONS_PER_ACTOR = 64;
export const PORTABLE_SMOKE_DEADLINE_MS = 60_000;

export interface PortableSmokeOperationMetric {
  readonly name: string;
  readonly elapsedMs: number;
}

export interface PortableSmokeResult {
  readonly schema: "efs-portable-smoke-result-v1";
  readonly adapter: string;
  readonly seed: number;
  readonly fixtureDigest: string;
  readonly finalPayloadDigest: string;
  readonly namespaceDigest: string;
  readonly elapsedMs: number;
  readonly completedOperationCount: number;
  readonly namespaceOperationCount: number;
  readonly restarts: number;
  readonly peakManagedResidentBytes: number;
  readonly objectCount: number;
  readonly manifestCount: number;
  readonly slowestOperations: readonly PortableSmokeOperationMetric[];
}

type SmokeFilesystem = EphemeralFilesystem & {
  readonly capabilities: FilesystemCapabilities;
  readonly maintenance: FilesystemMaintenance;
  readonly branches: Branches;
};

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable smoke: ${message}`);
}

function deterministicBytes(length: number, seed: number): Uint8Array {
  let state = seed >>> 0;
  const bytes = new Uint8Array(length);
  for (let index = 0; index < length; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[index] = state & 0xff;
  }
  return bytes;
}

function streamBytes(bytes: Uint8Array): ReadableStream<Uint8Array> {
  let offset = 0;
  return new ReadableStream({
    pull(controller) {
      if (offset >= bytes.byteLength) {
        controller.close();
        return;
      }
      const end = Math.min(offset + 256 * 1024, bytes.byteLength);
      controller.enqueue(bytes.slice(offset, end));
      offset = end;
    },
  });
}

function hex(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += byte.toString(16).padStart(2, "0");
  return value;
}

async function digest(
  adapter: FilesystemSQLiteDriver,
  bytes: Uint8Array,
): Promise<string> {
  const value = adapter.hashBytes
    ? adapter.hashBytes(bytes)
    : await adapter.hashBytesAsync?.(bytes);
  invariant(
    value instanceof Uint8Array && value.byteLength === 32,
    "SHA-256 unavailable",
  );
  return hex(value);
}

async function fileDigest(
  filesystem: SmokeFilesystem,
  adapter: FilesystemSQLiteDriver,
  path: string,
): Promise<string> {
  return digest(adapter, await filesystem.readFile(path));
}

async function namespaceDescriptors(
  filesystem: SmokeFilesystem,
  adapter: FilesystemSQLiteDriver,
  currentPath = "/",
): Promise<string[]> {
  const descriptors = [`${currentPath}|directory`];
  const entries = await filesystem.readdir(currentPath);
  for (const entry of entries) {
    const childPath =
      currentPath === "/" ? `/${entry.name}` : `${currentPath}/${entry.name}`;
    if (entry.isDirectory()) {
      descriptors.push(...(await namespaceDescriptors(filesystem, adapter, childPath)));
    } else if (entry.isSymbolicLink()) {
      descriptors.push(`${childPath}|symlink|${await filesystem.readlink(childPath)}`);
    } else {
      const stat = await filesystem.lstat(childPath);
      descriptors.push(
        `${childPath}|file|${stat.size}|${stat.nlink}|${await fileDigest(
          filesystem,
          adapter,
          childPath,
        )}`,
      );
    }
  }
  return descriptors;
}

async function expectedNamespaceDescriptors(
  adapter: FilesystemSQLiteDriver,
  expectedPayload: Uint8Array,
): Promise<string[]> {
  const source = new TextEncoder().encode("source");
  const sourceDigest = await digest(adapter, source);
  const result = [
    "/|directory",
    "/concurrent|directory",
    "/namespace|directory",
    `/namespace/source|file|${source.length}|251|${sourceDigest}`,
    "/smoke|directory",
    `/smoke/payload|file|${expectedPayload.length}|1|${await digest(
      adapter,
      expectedPayload,
    )}`,
  ];
  for (let index = 0; index < 250; index += 1) {
    const suffix = index.toString().padStart(4, "0");
    const directoryPath = `/namespace/d-${suffix}`;
    result.push(`${directoryPath}|directory`);
    result.push(`${directoryPath}/hard|file|${source.length}|251|${sourceDigest}`);
    result.push(`${directoryPath}/symbolic|symlink|../source`);
  }
  for (let writer = 0; writer < PORTABLE_SMOKE_ACTORS_PER_KIND; writer += 1) {
    const bytes = new Uint8Array(PORTABLE_SMOKE_OPERATIONS_PER_ACTOR);
    for (
      let operation = 0;
      operation < PORTABLE_SMOKE_OPERATIONS_PER_ACTOR;
      operation += 1
    )
      bytes[operation] = (writer + operation) % 251;
    result.push(
      `/concurrent/w-${writer}|file|${bytes.length}|1|${await digest(adapter, bytes)}`,
    );
  }
  return result.sort();
}

function equalStrings(left: readonly string[], right: readonly string[]): boolean {
  return (
    left.length === right.length && left.every((value, index) => value === right[index])
  );
}

async function verifyAll(filesystem: SmokeFilesystem): Promise<void> {
  let cursor: string | undefined;
  for (let batch = 0; batch < 100_000; batch += 1) {
    const result = await filesystem.maintenance.verify({
      ...(cursor === undefined ? {} : { cursor }),
      maxEntities: 4,
    });
    cursor = result.nextCursor ?? undefined;
    if (result.complete) return;
  }
  throw new Error("portable smoke: bounded verification did not complete");
}

function activeDurableState(adapter: FilesystemSQLiteDriver): {
  readonly leases: number;
  readonly staging: number;
  readonly reservations: number;
} {
  return adapter.transaction(
    "read",
    (tx) =>
      tx.all<{
        readonly leases: number;
        readonly staging: number;
        readonly reservations: number;
      }>(
        "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) leases,(SELECT count(*) FROM efs_staging_certificates) staging,(SELECT count(*) FROM efs_operation_results WHERE outcome=-1 AND length(encoded)=0) reservations",
        [],
        { maxRows: 1, maxBytes: 512 },
      )[0]!,
  );
}

export type PortableSmokePhaseOutcome =
  | Readonly<{
      status: "restart";
      completedPhase: 0 | 1 | 2;
    }>
  | Readonly<{
      status: "complete";
      result: PortableSmokeResult;
    }>;

/**
 * Host-coordinated form of the exact smoke profile. The caller MUST perform a real
 * physical restart/eviction after every `restart` outcome, then call
 * `recordPhysicalRestart()` before entering the next adapter context.
 */
export class PortableSmokeSession {
  readonly #adapterName: string;
  readonly #started = performance.now();
  readonly #payload = deterministicBytes(
    PORTABLE_SMOKE_PAYLOAD_BYTES,
    PORTABLE_SMOKE_SEED,
  );
  readonly #expected = this.#payload.slice();
  readonly #slowestOperations: PortableSmokeOperationMetric[] = [];
  #phase: 0 | 1 | 2 | 3 = 0;
  #phaseName = "initialization";
  #restartPending = false;
  #completedOperationCount = 0;
  #namespaceOperationCount = 0;
  #restarts = 0;
  #peakManagedResidentBytes = 0;
  #initialDigest: string | undefined;

  constructor(adapterName: string) {
    invariant(adapterName.length > 0, "adapter name is empty");
    this.#adapterName = adapterName;
  }

  #recordMetric(name: string, elapsedMs: number): void {
    this.#slowestOperations.push(
      Object.freeze({
        name,
        elapsedMs: Math.round(elapsedMs * 1_000) / 1_000,
      }),
    );
    this.#slowestOperations.sort((left, right) => right.elapsedMs - left.elapsedMs);
    if (this.#slowestOperations.length > 10) this.#slowestOperations.length = 10;
  }

  async #measured<T>(
    name: string,
    callback: () => Promise<T>,
    options: { readonly namespace?: boolean } = {},
  ): Promise<T> {
    const operationStarted = performance.now();
    try {
      const value = await callback();
      this.#completedOperationCount += 1;
      if (options.namespace) this.#namespaceOperationCount += 1;
      return value;
    } finally {
      this.#recordMetric(name, performance.now() - operationStarted);
    }
  }

  recordPhysicalRestart(elapsedMs: number): void {
    invariant(this.#restartPending, "no smoke restart is pending");
    invariant(
      Number.isFinite(elapsedMs) && elapsedMs >= 0,
      "invalid physical restart duration",
    );
    const name =
      this.#phase === 1
        ? "restart-after-initial-write"
        : this.#phase === 2
          ? "restart-after-namespace"
          : "restart-during-collection";
    this.#recordMetric(name, elapsedMs);
    this.#completedOperationCount += 1;
    this.#restarts += 1;
    this.#restartPending = false;
  }

  async run(adapter: FilesystemSQLiteDriver): Promise<PortableSmokePhaseOutcome> {
    invariant(!this.#restartPending, "physical restart was not recorded");
    let filesystem: SmokeFilesystem | undefined;
    const observer = (event: FilesystemObservation): void => {
      this.#peakManagedResidentBytes = Math.max(
        this.#peakManagedResidentBytes,
        event.counters.peakManagedResidentBytes ?? 0,
      );
    };
    const open = async (): Promise<SmokeFilesystem> =>
      (await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: { maxGcBatchSize: 64, maxQueryBatchSize: 256 },
        observer,
      })) as SmokeFilesystem;
    const pause = (completedPhase: 0 | 1 | 2): PortableSmokePhaseOutcome => {
      this.#phase = (completedPhase + 1) as 1 | 2 | 3;
      this.#restartPending = true;
      return Object.freeze({ status: "restart", completedPhase });
    };
    try {
      filesystem = await open();
      if (this.#phase === 0) {
        this.#phaseName = "initial-write";
        this.#initialDigest = await digest(adapter, this.#expected);
        await this.#measured("mkdir-smoke", () =>
          filesystem!.mkdir("/smoke", { recursive: true }),
        );
        await this.#measured("write-16m-payload", () =>
          filesystem!.writeFile("/smoke/payload", streamBytes(this.#payload), {
            maxBytes: PORTABLE_SMOKE_PAYLOAD_BYTES,
          }),
        );
        return pause(0);
      }

      if (this.#phase === 1) {
        this.#phaseName = "cow-edits-and-namespace";
        invariant(
          (await this.#measured("digest-after-initial-reopen", () =>
            fileDigest(filesystem!, adapter, "/smoke/payload"),
          )) === this.#initialDigest,
          "initial payload digest changed across reopen",
        );
        const cowBranch = await filesystem.branches.create("smoke-cow");
        try {
          for (let index = 0; index < PORTABLE_SMOKE_COW_EDITS; index += 1) {
            const group = index % 3;
            const offset =
              group === 0
                ? index % 4096
                : group === 1
                  ? (index * 97) % (32 * 4096)
                  : (index * 7919) % PORTABLE_SMOKE_PAYLOAD_BYTES;
            const value = (index * 17) & 0xff;
            this.#expected[offset] = value;
            await this.#measured("cow-one-byte-edit", () =>
              cowBranch.writeRange("/smoke/payload", offset, Uint8Array.of(value)),
            );
          }
          invariant(
            (await cowBranch.publish({ operationId: "smoke-cow-publish" })).outcome ===
              "merged",
            "COW branch did not publish",
          );
        } finally {
          await cowBranch.close();
        }

        await filesystem.mkdir("/namespace", { recursive: true });
        await filesystem.writeFile("/namespace/source", "source");
        for (
          let index = 0;
          index < PORTABLE_SMOKE_NAMESPACE_OPERATIONS / 8;
          index += 1
        ) {
          const suffix = index.toString().padStart(4, "0");
          const directoryPath = `/namespace/d-${suffix}`;
          await this.#measured(
            "namespace-mkdir",
            () => filesystem!.mkdir(directoryPath),
            {
              namespace: true,
            },
          );
          await this.#measured(
            "namespace-create",
            () =>
              filesystem!.writeFile(`${directoryPath}/created`, `created-${suffix}`),
            { namespace: true },
          );
          await this.#measured(
            "namespace-stat-created",
            () => filesystem!.stat(`${directoryPath}/created`),
            { namespace: true },
          );
          await this.#measured(
            "namespace-rename",
            () =>
              filesystem!.rename(
                `${directoryPath}/created`,
                `${directoryPath}/renamed`,
              ),
            { namespace: true },
          );
          await this.#measured(
            "namespace-hard-link",
            () => filesystem!.link("/namespace/source", `${directoryPath}/hard`),
            { namespace: true },
          );
          await this.#measured(
            "namespace-stat-hard-link",
            () => filesystem!.stat(`${directoryPath}/hard`),
            { namespace: true },
          );
          await this.#measured(
            "namespace-unlink",
            () => filesystem!.unlink(`${directoryPath}/renamed`),
            { namespace: true },
          );
          await this.#measured(
            "namespace-symbolic-link",
            () => filesystem!.symlink("../source", `${directoryPath}/symbolic`),
            { namespace: true },
          );
        }
        invariant(
          this.#namespaceOperationCount === PORTABLE_SMOKE_NAMESPACE_OPERATIONS,
          "namespace operation count differs",
        );
        return pause(1);
      }

      if (this.#phase === 2) {
        this.#phaseName = "concurrent-actors-and-interrupted-collection";
        await filesystem.mkdir("/concurrent", { recursive: true });
        const writerBranches: EphemeralBranch[] = [];
        for (let writer = 0; writer < PORTABLE_SMOKE_ACTORS_PER_KIND; writer += 1) {
          await filesystem.writeFile(
            `/concurrent/w-${writer}`,
            new Uint8Array(PORTABLE_SMOKE_OPERATIONS_PER_ACTOR),
          );
          writerBranches.push(
            await filesystem.branches.create(`smoke-writer-${writer}`),
          );
        }
        try {
          await Promise.all([
            ...Array.from({ length: PORTABLE_SMOKE_ACTORS_PER_KIND }, (_, reader) =>
              (async () => {
                for (
                  let operation = 0;
                  operation < PORTABLE_SMOKE_OPERATIONS_PER_ACTOR;
                  operation += 1
                ) {
                  const value = await this.#measured("concurrent-reader", () =>
                    filesystem!.readRange("/namespace/source", {
                      offset: 0,
                      length: 6,
                    }),
                  );
                  invariant(
                    value.byteLength === 6,
                    `concurrent reader ${reader}:${operation} returned the wrong length`,
                  );
                }
              })(),
            ),
            ...Array.from({ length: PORTABLE_SMOKE_ACTORS_PER_KIND }, (_, writer) =>
              (async () => {
                for (
                  let operation = 0;
                  operation < PORTABLE_SMOKE_OPERATIONS_PER_ACTOR;
                  operation += 1
                )
                  await this.#measured("concurrent-writer", () =>
                    writerBranches[writer]!.writeRange(
                      `/concurrent/w-${writer}`,
                      operation,
                      Uint8Array.of((writer + operation) % 251),
                    ),
                  );
              })(),
            ),
          ]);
          for (let writer = 0; writer < writerBranches.length; writer += 1)
            invariant(
              (
                await writerBranches[writer]!.publish({
                  operationId: `smoke-writer-publish-${writer}`,
                })
              ).outcome === "merged",
              `writer ${writer} did not publish`,
            );
        } finally {
          await Promise.all(writerBranches.map((branch) => branch.close()));
        }
        await this.#measured("write-orphan", () =>
          filesystem!.writeFile("/orphan", "collect-me"),
        );
        await this.#measured("unlink-orphan", () => filesystem!.unlink("/orphan"));
        const collection = await filesystem.maintenance.collectGarbage({
          runId: "smoke-interrupted-collection",
          maxBatches: 1,
        });
        invariant(collection.state === "paused", "collection was not interrupted");
        return pause(2);
      }

      this.#phaseName = "resumed-collection-and-final-verification";
      let collection = await filesystem.maintenance.collectGarbage({
        runId: "smoke-interrupted-collection",
        maxBatches: 1,
      });
      for (let call = 0; call < 5_000 && collection.state !== "complete"; call += 1)
        collection = await filesystem.maintenance.collectGarbage({
          runId: "smoke-interrupted-collection",
          maxBatches: 1,
        });
      invariant(collection.state === "complete", "collection did not resume");
      invariant(this.#restarts === 3, "smoke did not perform exactly three restarts");

      const finalPayloadDigest = await fileDigest(
        filesystem,
        adapter,
        "/smoke/payload",
      );
      invariant(
        finalPayloadDigest === (await digest(adapter, this.#expected)),
        "final payload digest differs",
      );
      invariant(
        (await filesystem.readFile("/concurrent/w-15"))[63] === (15 + 63) % 251,
        "concurrent writer result differs",
      );
      invariant(
        (await filesystem.readFile("/namespace/d-0249/hard", { encoding: "utf8" })) ===
          "source",
        "hard-link result differs",
      );
      const actualNamespace = (await namespaceDescriptors(filesystem, adapter)).sort();
      const expectedNamespace = await expectedNamespaceDescriptors(
        adapter,
        this.#expected,
      );
      invariant(
        equalStrings(actualNamespace, expectedNamespace),
        "final namespace differs",
      );
      const namespaceDigest = await digest(
        adapter,
        new TextEncoder().encode(actualNamespace.join("\n")),
      );
      await verifyAll(filesystem);
      const snapshot = await filesystem.maintenance.snapshotStorage();
      invariant(snapshot.state === "complete", "storage snapshot did not complete");
      const active = activeDurableState(adapter);
      invariant(
        active.leases === 0 && active.staging === 0 && active.reservations === 0,
        `durable reservations leaked (${active.leases}/${active.staging}/${active.reservations})`,
      );
      const elapsedMs = performance.now() - this.#started;
      invariant(
        elapsedMs < PORTABLE_SMOKE_DEADLINE_MS,
        `profile exceeded 60 seconds (${elapsedMs.toFixed(1)} ms)`,
      );
      invariant(this.#initialDigest !== undefined, "initial digest is missing");
      return Object.freeze({
        status: "complete",
        result: Object.freeze({
          schema: "efs-portable-smoke-result-v1",
          adapter: this.#adapterName,
          seed: PORTABLE_SMOKE_SEED,
          fixtureDigest: this.#initialDigest,
          finalPayloadDigest,
          namespaceDigest,
          elapsedMs: Math.round(elapsedMs),
          completedOperationCount: this.#completedOperationCount,
          namespaceOperationCount: this.#namespaceOperationCount,
          restarts: this.#restarts,
          peakManagedResidentBytes: this.#peakManagedResidentBytes,
          objectCount: snapshot.objectCount,
          manifestCount: snapshot.manifestCount,
          slowestOperations: Object.freeze([...this.#slowestOperations]),
        }),
      });
    } catch (error) {
      throw new Error(
        `portable smoke failure ${JSON.stringify({
          seed: PORTABLE_SMOKE_SEED,
          phase: this.#phaseName,
          completedOperationCount: this.#completedOperationCount,
          namespaceOperationCount: this.#namespaceOperationCount,
          slowestOperations: this.#slowestOperations,
          error: String(error),
        })}`,
        { cause: error },
      );
    } finally {
      try {
        await filesystem?.close();
      } catch {}
    }
  }
}

/** Execute the exact finite 60-second profile against a real adapter factory. */
export async function runFilesystemSmoke(
  factory: ConformanceAdapterFactory,
): Promise<PortableSmokeResult> {
  const started = performance.now();
  const fixture = await factory.create({
    label: "portable-smoke",
    seed: PORTABLE_SMOKE_SEED,
  });
  let adapter = fixture.adapter;
  let filesystem: SmokeFilesystem | undefined;
  let restarts = 0;
  let phase = "initialization";
  let completedOperationCount = 0;
  let namespaceOperationCount = 0;
  let peakManagedResidentBytes = 0;
  const slowestOperations: PortableSmokeOperationMetric[] = [];
  const payload = deterministicBytes(PORTABLE_SMOKE_PAYLOAD_BYTES, PORTABLE_SMOKE_SEED);
  const expected = payload.slice();
  const initialDigest = await digest(adapter, expected);

  const observer = (event: FilesystemObservation): void => {
    peakManagedResidentBytes = Math.max(
      peakManagedResidentBytes,
      event.counters.peakManagedResidentBytes ?? 0,
    );
  };
  const measured = async <T>(
    name: string,
    callback: () => Promise<T>,
    options: { readonly namespace?: boolean } = {},
  ): Promise<T> => {
    const operationStarted = performance.now();
    try {
      const value = await callback();
      completedOperationCount += 1;
      if (options.namespace) namespaceOperationCount += 1;
      return value;
    } finally {
      slowestOperations.push(
        Object.freeze({
          name,
          elapsedMs: Math.round((performance.now() - operationStarted) * 1_000) / 1_000,
        }),
      );
      slowestOperations.sort((left, right) => right.elapsedMs - left.elapsedMs);
      if (slowestOperations.length > 10) slowestOperations.length = 10;
    }
  };
  const open = async (): Promise<void> => {
    filesystem = (await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      storage: { maxGcBatchSize: 64, maxQueryBatchSize: 256 },
      observer,
    })) as SmokeFilesystem;
  };
  const close = async (): Promise<void> => {
    await filesystem?.close();
    filesystem = undefined;
    adapter.close();
  };
  const restart = async (): Promise<void> => {
    await close();
    adapter = await fixture.reopen({ physical: true });
    await open();
    restarts += 1;
  };
  const collect = (
    database: ConformanceDatabase,
    target: SmokeFilesystem,
    options: { readonly runId: string; readonly maxBatches: number },
  ) =>
    database.collectGarbage
      ? database.collectGarbage(target, options)
      : target.maintenance.collectGarbage(options);

  try {
    phase = "initial-write-and-reopen";
    await open();
    await measured("mkdir-smoke", () =>
      filesystem!.mkdir("/smoke", { recursive: true }),
    );
    await measured("write-16m-payload", () =>
      filesystem!.writeFile("/smoke/payload", streamBytes(payload), {
        maxBytes: PORTABLE_SMOKE_PAYLOAD_BYTES,
      }),
    );
    await measured("restart-after-initial-write", restart);
    invariant(
      (await measured("digest-after-initial-reopen", () =>
        fileDigest(filesystem!, adapter, "/smoke/payload"),
      )) === initialDigest,
      "initial payload digest changed across reopen",
    );

    phase = "cow-edits";
    const cowBranch = await filesystem!.branches.create("smoke-cow");
    for (let index = 0; index < PORTABLE_SMOKE_COW_EDITS; index += 1) {
      const group = index % 3;
      const offset =
        group === 0
          ? index % 4096
          : group === 1
            ? (index * 97) % (32 * 4096)
            : (index * 7919) % PORTABLE_SMOKE_PAYLOAD_BYTES;
      const value = (index * 17) & 0xff;
      expected[offset] = value;
      await measured("cow-one-byte-edit", () =>
        cowBranch.writeRange("/smoke/payload", offset, Uint8Array.of(value)),
      );
    }
    invariant(
      (await cowBranch.publish({ operationId: "smoke-cow-publish" })).outcome ===
        "merged",
      "COW branch did not publish",
    );
    await cowBranch.close();

    phase = "namespace-operations";
    await filesystem!.mkdir("/namespace", { recursive: true });
    await filesystem!.writeFile("/namespace/source", "source");
    for (let index = 0; index < PORTABLE_SMOKE_NAMESPACE_OPERATIONS / 8; index += 1) {
      const suffix = index.toString().padStart(4, "0");
      const directoryPath = `/namespace/d-${suffix}`;
      await measured("namespace-mkdir", () => filesystem!.mkdir(directoryPath), {
        namespace: true,
      });
      await measured(
        "namespace-create",
        () => filesystem!.writeFile(`${directoryPath}/created`, `created-${suffix}`),
        { namespace: true },
      );
      await measured(
        "namespace-stat-created",
        () => filesystem!.stat(`${directoryPath}/created`),
        { namespace: true },
      );
      await measured(
        "namespace-rename",
        () =>
          filesystem!.rename(`${directoryPath}/created`, `${directoryPath}/renamed`),
        { namespace: true },
      );
      await measured(
        "namespace-hard-link",
        () => filesystem!.link("/namespace/source", `${directoryPath}/hard`),
        { namespace: true },
      );
      await measured(
        "namespace-stat-hard-link",
        () => filesystem!.stat(`${directoryPath}/hard`),
        { namespace: true },
      );
      await measured(
        "namespace-unlink",
        () => filesystem!.unlink(`${directoryPath}/renamed`),
        { namespace: true },
      );
      await measured(
        "namespace-symbolic-link",
        () => filesystem!.symlink("../source", `${directoryPath}/symbolic`),
        { namespace: true },
      );
    }
    invariant(
      namespaceOperationCount === PORTABLE_SMOKE_NAMESPACE_OPERATIONS,
      "namespace operation count differs",
    );
    await measured("restart-after-namespace", restart);

    phase = "concurrent-readers-and-writers";
    await filesystem!.mkdir("/concurrent", { recursive: true });
    const writerBranches: EphemeralBranch[] = [];
    for (let writer = 0; writer < PORTABLE_SMOKE_ACTORS_PER_KIND; writer += 1) {
      await filesystem!.writeFile(
        `/concurrent/w-${writer}`,
        new Uint8Array(PORTABLE_SMOKE_OPERATIONS_PER_ACTOR),
      );
      writerBranches.push(await filesystem!.branches.create(`smoke-writer-${writer}`));
    }
    await Promise.all([
      ...Array.from({ length: PORTABLE_SMOKE_ACTORS_PER_KIND }, (_, reader) =>
        (async () => {
          for (
            let operation = 0;
            operation < PORTABLE_SMOKE_OPERATIONS_PER_ACTOR;
            operation += 1
          ) {
            const value = await measured("concurrent-reader", () =>
              filesystem!.readRange("/namespace/source", { offset: 0, length: 6 }),
            );
            invariant(
              value.byteLength === 6,
              `concurrent reader ${reader}:${operation} returned the wrong length`,
            );
          }
        })(),
      ),
      ...Array.from({ length: PORTABLE_SMOKE_ACTORS_PER_KIND }, (_, writer) =>
        (async () => {
          for (
            let operation = 0;
            operation < PORTABLE_SMOKE_OPERATIONS_PER_ACTOR;
            operation += 1
          )
            await measured("concurrent-writer", () =>
              writerBranches[writer]!.writeRange(
                `/concurrent/w-${writer}`,
                operation,
                Uint8Array.of((writer + operation) % 251),
              ),
            );
        })(),
      ),
    ]);
    for (let writer = 0; writer < writerBranches.length; writer += 1)
      invariant(
        (
          await writerBranches[writer]!.publish({
            operationId: `smoke-writer-publish-${writer}`,
          })
        ).outcome === "merged",
        `writer ${writer} did not publish`,
      );
    await Promise.all(writerBranches.map((branch) => branch.close()));

    phase = "interrupted-collection";
    await measured("write-orphan", () =>
      filesystem!.writeFile("/orphan", "collect-me"),
    );
    await measured("unlink-orphan", () => filesystem!.unlink("/orphan"));
    let collection = await collect(fixture, filesystem!, {
      runId: "smoke-interrupted-collection",
      maxBatches: 1,
    });
    invariant(collection.state === "paused", "collection was not interrupted");
    await measured("restart-during-collection", restart);
    for (let call = 0; call < 5_000 && collection.state !== "complete"; call += 1)
      collection = await collect(fixture, filesystem!, {
        runId: "smoke-interrupted-collection",
        maxBatches: 1,
      });
    invariant(collection.state === "complete", "collection did not resume");
    invariant(restarts === 3, "smoke did not perform exactly three restarts");

    phase = "final-verification";
    const finalPayloadDigest = await fileDigest(filesystem!, adapter, "/smoke/payload");
    invariant(
      finalPayloadDigest === (await digest(adapter, expected)),
      "final payload digest differs",
    );
    invariant(
      (await filesystem!.readFile("/concurrent/w-15"))[63] === (15 + 63) % 251,
      "concurrent writer result differs",
    );
    invariant(
      (await filesystem!.readFile("/namespace/d-0249/hard", { encoding: "utf8" })) ===
        "source",
      "hard-link result differs",
    );
    const actualNamespace = (await namespaceDescriptors(filesystem!, adapter)).sort();
    const expectedNamespace = await expectedNamespaceDescriptors(adapter, expected);
    invariant(
      equalStrings(actualNamespace, expectedNamespace),
      "final namespace differs",
    );
    const namespaceDigest = await digest(
      adapter,
      new TextEncoder().encode(actualNamespace.join("\n")),
    );
    await verifyAll(filesystem!);
    const snapshot = await filesystem!.maintenance.snapshotStorage();
    invariant(snapshot.state === "complete", "storage snapshot did not complete");
    const active = activeDurableState(adapter);
    invariant(
      active.leases === 0 && active.staging === 0 && active.reservations === 0,
      `durable reservations leaked (${active.leases}/${active.staging}/${active.reservations})`,
    );
    const elapsedMs = performance.now() - started;
    invariant(
      elapsedMs < PORTABLE_SMOKE_DEADLINE_MS,
      `profile exceeded 60 seconds (${elapsedMs.toFixed(1)} ms)`,
    );
    return Object.freeze({
      schema: "efs-portable-smoke-result-v1",
      adapter: factory.name,
      seed: PORTABLE_SMOKE_SEED,
      fixtureDigest: initialDigest,
      finalPayloadDigest,
      namespaceDigest,
      elapsedMs: Math.round(elapsedMs),
      completedOperationCount,
      namespaceOperationCount,
      restarts,
      peakManagedResidentBytes,
      objectCount: snapshot.objectCount,
      manifestCount: snapshot.manifestCount,
      slowestOperations: Object.freeze([...slowestOperations]),
    });
  } catch (error) {
    throw new Error(
      `portable smoke failure ${JSON.stringify({
        seed: PORTABLE_SMOKE_SEED,
        phase,
        completedOperationCount,
        namespaceOperationCount,
        slowestOperations,
        error: String(error),
      })}`,
      { cause: error },
    );
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      adapter.close();
    } catch {}
    await fixture.dispose();
  }
}

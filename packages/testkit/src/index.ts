import {
  EphemeralFS,
  type Branches,
  type EphemeralFilesystem,
  type FilesystemCapabilities,
  type FilesystemMaintenance,
  type GarbageCollectionOptions,
  type GarbageCollectionResult,
} from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import { test } from "vitest";
import {
  recordPortableFixtureContext,
  type PortableFixtureContext,
} from "./fixture-context.js";

export * from "./smoke.js";
export * from "./fault.js";
export * from "./driver.js";
export * from "./branch.js";
export * from "./maintenance.js";
export * from "./scale.js";
export * from "./restart.js";
export * from "./schema.js";
export * from "./publication-fault.js";
export * from "./maintenance-fault.js";
export * from "./filesystem-fault-attempt.js";
export * from "./cow.js";
export * from "./storage.js";
export * from "./fixture-context.js";
export * from "./node-vfs.js";

export type ConformanceCapability =
  | "read-only-reopen"
  | "second-connection"
  | "schema-fixtures"
  | "fault-injection"
  | "garbage-collection"
  | "physical-reopen"
  | "crash-recovery"
  | "ownership";
export interface ConformanceFaultController {
  arm(point: string, occurrence?: number): void;
  clear(): void;
}
export interface ConformanceFixtureOptions {
  readonly label?: string;
  readonly seed?: number;
}
export interface ConformanceDatabase {
  readonly adapter: FilesystemSQLiteDriver;
  readonly capabilities: readonly ConformanceCapability[];
  readonly faults?: ConformanceFaultController;
  reopen(options?: {
    readOnly?: boolean;
    physical?: boolean;
  }): Promise<FilesystemSQLiteDriver>;
  openSecondConnection?(): Promise<FilesystemSQLiteDriver>;
  reopenFromFixture?(fixtureName: string): Promise<FilesystemSQLiteDriver>;
  collectGarbage?(
    filesystem: EphemeralFS,
    options?: GarbageCollectionOptions,
  ): Promise<GarbageCollectionResult>;
  crashAndReopen?(): Promise<FilesystemSQLiteDriver>;
  createOwnershipProbe?(): Promise<{
    readonly adapter: FilesystemSQLiteDriver;
    closeCallCount(): number;
  }>;
  dispose(): Promise<void>;
}
export interface ConformanceAdapterFactory {
  readonly name: string;
  recordFixtureContext?(context: PortableFixtureContext): void | Promise<void>;
  create(options?: ConformanceFixtureOptions): Promise<ConformanceDatabase>;
}
export interface CorrectnessResult {
  readonly schema: "efs-correctness-result-v1";
  readonly commit: string;
  readonly adapter: string;
  readonly driver: string;
  readonly capabilities: Readonly<Record<string, string | number | boolean | null>>;
  readonly limits: Readonly<Record<string, number>>;
  readonly schemaVersion: number;
  readonly formatVersion: string;
  readonly seed: number;
  readonly fixtureDigest: string;
  readonly faultPoint: string | null;
  readonly commands: readonly string[];
  readonly environment: Readonly<Record<string, string>>;
  readonly passed: number;
  readonly failed: number;
  readonly elapsedMs: number;
}
export interface BenchmarkResult {
  readonly schema: "efs-benchmark-result-v1";
  readonly benchmark: string;
  readonly commit: string;
  readonly engine: string;
  readonly driver: string;
  readonly fixture: Readonly<{ name: string; sha256: string }>;
  readonly configuration: Readonly<Record<string, unknown>>;
  readonly trials: number;
  readonly latencyMs: Readonly<{ p50: number; p95: number; p99: number }>;
  readonly counters: Readonly<Record<string, number>>;
  readonly pass: boolean;
}

export type PortableConformanceCaseId =
  | "storage-deduplication"
  | "filesystem-namespace"
  | "filesystem-path-errors"
  | "filesystem-range-edges"
  | "filesystem-link-semantics"
  | "filesystem-rename-removal"
  | "filesystem-metadata"
  | "filesystem-pagination-cap"
  | "filesystem-error-details"
  | "stream-snapshot"
  | "stream-abort-backpressure"
  | "lease-staging-lifecycle"
  | "read-side-effect-boundary"
  | "overlapping-operations"
  | "branch-publication"
  | "maintenance-cursors"
  | "resource-capabilities"
  | "durable-reopen"
  | "read-only-reopen"
  | "second-connection"
  | "close-lifecycle";

export const PORTABLE_CONFORMANCE_CASE_IDS = Object.freeze([
  "storage-deduplication",
  "filesystem-namespace",
  "filesystem-path-errors",
  "filesystem-range-edges",
  "filesystem-link-semantics",
  "filesystem-rename-removal",
  "filesystem-metadata",
  "filesystem-pagination-cap",
  "filesystem-error-details",
  "stream-snapshot",
  "stream-abort-backpressure",
  "lease-staging-lifecycle",
  "read-side-effect-boundary",
  "overlapping-operations",
  "branch-publication",
  "maintenance-cursors",
  "resource-capabilities",
  "durable-reopen",
  "read-only-reopen",
  "second-connection",
  "close-lifecycle",
] as const satisfies readonly PortableConformanceCaseId[]);

export interface PortableConformanceCaseResult {
  readonly id: PortableConformanceCaseId;
  readonly status: "passed" | "skipped";
  readonly reason?: string;
}

type PortableFilesystem = EphemeralFilesystem & {
  readonly capabilities: FilesystemCapabilities;
  readonly maintenance: FilesystemMaintenance;
  readonly branches: Branches;
};

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable conformance: ${message}`);
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  for (let index = 0; index < left.byteLength; index += 1)
    if (left[index] !== right[index]) return false;
  return true;
}

async function streamBytes(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const reader = stream.getReader();
  const parts: Uint8Array[] = [];
  let length = 0;
  while (true) {
    const result = await reader.read();
    if (result.done) break;
    parts.push(result.value);
    length += result.value.byteLength;
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.byteLength;
  }
  return bytes;
}

async function expectCode(operation: Promise<unknown>, code: string): Promise<void> {
  try {
    await operation;
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
  throw new Error(`portable conformance: expected ${code} rejection`);
}

async function expectDetailedError(
  operation: Promise<unknown>,
  expected: { readonly code: string; readonly syscall: string; readonly path: string },
): Promise<void> {
  try {
    await operation;
  } catch (error) {
    invariant(error !== null && typeof error === "object", "error is not an object");
    invariant(
      "code" in error &&
        error.code === expected.code &&
        "syscall" in error &&
        error.syscall === expected.syscall &&
        "path" in error &&
        error.path === expected.path,
      `error details differ for ${expected.syscall} ${expected.path}`,
    );
    return;
  }
  throw new Error(`portable conformance: expected ${expected.code} rejection`);
}

function scalar(adapter: FilesystemSQLiteDriver, sql: string, name = "value"): number {
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<Readonly<Record<string, number>>>(sql, [], {
        maxRows: 1,
        maxBytes: 512,
      })[0],
  );
  invariant(row !== undefined, `scalar query ${name} returned no row`);
  const value = row[name];
  invariant(
    typeof value === "number" && Number.isSafeInteger(value) && value >= 0,
    `scalar ${name} is invalid`,
  );
  return value;
}

function durableReadFootprint(adapter: FilesystemSQLiteDriver): string {
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<Readonly<Record<string, number>>>(
        "SELECT (SELECT count(*) FROM efs_entries) entries,(SELECT count(*) FROM efs_inodes) inodes,(SELECT count(*) FROM efs_cas_objects) objects,(SELECT count(*) FROM efs_manifest_roots) roots,(SELECT count(*) FROM efs_manifest_nodes) nodes,(SELECT count(*) FROM efs_branches) branches,(SELECT count(*) FROM efs_leases WHERE state IN (0,1)) active_leases,(SELECT count(*) FROM efs_staging_certificates) staging",
        [],
        { maxRows: 1, maxBytes: 1024 },
      )[0],
  );
  invariant(row !== undefined, "durable read footprint is missing");
  return JSON.stringify(row);
}

async function verifyMetadataUsage(
  filesystem: PortableFilesystem,
  label: string,
): Promise<void> {
  let cursor: string | undefined;
  try {
    for (let batch = 0; batch < 10_000; batch += 1) {
      const result = await filesystem.maintenance.verify({
        scopes: ["metadata"],
        ...(cursor === undefined ? {} : { cursor }),
        maxEntities: 4,
      });
      cursor = result.nextCursor ?? undefined;
      if (result.complete) return;
    }
  } catch (error) {
    throw new Error(`portable conformance: usage mismatch after ${label}`, {
      cause: error,
    });
  }
  throw new Error(`portable conformance: metadata verification stalled after ${label}`);
}

/**
 * Runs the same host-neutral milestone conformance scenario against a real adapter
 * factory. Runtime harnesses may invoke this inside their storage-owning isolate.
 */
export async function runFilesystemConformance(
  factory: ConformanceAdapterFactory,
): Promise<readonly PortableConformanceCaseResult[]> {
  const label = "portable-m6";
  const seed = 0x5eedc0de;
  const fixture = await factory.create({ label, seed });
  await recordPortableFixtureContext(factory, fixture.adapter, label, seed);
  const results: PortableConformanceCaseResult[] = [];
  let adapter = fixture.adapter;
  let filesystem: PortableFilesystem | undefined;
  const passed = (id: PortableConformanceCaseId): void => {
    results.push(Object.freeze({ id, status: "passed" }));
  };
  const skipped = (id: PortableConformanceCaseId, reason: string): void => {
    results.push(Object.freeze({ id, status: "skipped", reason }));
  };
  try {
    let now = 10_000;
    const portableFilesystemLimits = Object.freeze({
      maxReaddirEntries: 16,
      preferredStreamChunkBytes: 1024,
    });
    filesystem = (await EphemeralFS.open({
      database: adapter,
      clock: () => now++,
      ownsDatabase: false,
      filesystem: portableFilesystemLimits,
      storage: { maxGcBatchSize: 2, maxQueryBatchSize: 16 },
    })) as PortableFilesystem;

    const shared = Uint8Array.from({ length: 32 * 1024 }, (_, index) => index & 0xff);
    await filesystem.writeFile("/dedup-a", shared);
    const firstStorage = await filesystem.maintenance.snapshotStorage();
    await filesystem.writeFile("/dedup-b", shared);
    const secondStorage = await filesystem.maintenance.snapshotStorage();
    invariant(
      secondStorage.storedObjectPayloadBytes === firstStorage.storedObjectPayloadBytes,
      "identical content was stored more than once",
    );
    invariant(
      equalBytes(await filesystem.readFile("/dedup-b"), shared),
      "deduplicated content changed",
    );
    await verifyMetadataUsage(filesystem, "storage-deduplication");
    passed("storage-deduplication");

    await filesystem.mkdir("/tree//nested/../nested", { recursive: true, mode: 0o750 });
    await verifyMetadataUsage(filesystem, "filesystem-mkdir");
    await filesystem.writeFile("/tree/nested/file", "abcdef", { mode: 0o640 });
    await verifyMetadataUsage(filesystem, "filesystem-write-file");
    await filesystem.writeRange("/tree/nested/file", 8, Uint8Array.of(88));
    await verifyMetadataUsage(filesystem, "filesystem-write-range");
    await filesystem.replaceRange(
      "/tree/nested/file",
      1,
      3,
      new TextEncoder().encode("Q"),
    );
    await verifyMetadataUsage(filesystem, "filesystem-replace-range");
    await filesystem.truncate("/tree/nested/file", 3);
    await verifyMetadataUsage(filesystem, "filesystem-truncate");
    invariant(
      (await filesystem.readFile("/tree/nested/file", { encoding: "utf8" })) === "aQe",
      "range mutation result differs",
    );
    await filesystem.link("/tree/nested/file", "/tree/alias");
    await filesystem.symlink("../alias", "/tree/nested/link");
    invariant(
      (await filesystem.stat("/tree/alias")).id ===
        (await filesystem.stat("/tree/nested/file")).id,
      "hard-link inode identity differs",
    );
    invariant(
      (await filesystem.readFile("/tree/nested/link", { encoding: "utf8" })) === "aQe",
      "relative symbolic link did not resolve",
    );
    await verifyMetadataUsage(filesystem, "filesystem-links");
    for (const name of ["z", "é", "a", "Ω"])
      await filesystem.writeFile(`/tree/nested/${name}`, new Uint8Array());
    const encoder = new TextEncoder();
    const names = (await filesystem.readdir("/tree/nested")).map((entry) => entry.name);
    const sorted = [...names].sort((left, right) => {
      const leftBytes = encoder.encode(left);
      const rightBytes = encoder.encode(right);
      const length = Math.min(leftBytes.length, rightBytes.length);
      for (let index = 0; index < length; index += 1) {
        const difference = leftBytes[index]! - rightBytes[index]!;
        if (difference !== 0) return difference;
      }
      return leftBytes.length - rightBytes.length;
    });
    invariant(
      JSON.stringify(names) === JSON.stringify(sorted),
      "listing is not UTF-8 ordered",
    );
    await verifyMetadataUsage(filesystem, "filesystem-utf8-names");
    await expectCode(filesystem.stat("/../../escape"), "EINVAL");
    passed("filesystem-namespace");

    const root = await filesystem.stat("/");
    invariant(root.isDirectory() && root.name === "", "root metadata is invalid");
    await expectCode(filesystem.stat(""), "EINVAL");
    await expectCode(filesystem.stat("relative"), "EINVAL");
    await expectCode(filesystem.stat("/nul\0name"), "EINVAL");
    await expectCode(
      filesystem.stat(
        `/${"x".repeat(filesystem.capabilities.filesystem.maxNameBytes + 1)}`,
      ),
      "EINVAL",
    );
    await expectCode(filesystem.unlink("/"), "EPERM");
    await expectCode(
      filesystem.writeFile("/tree/nested/file", "replacement", { exclusive: true }),
      "EEXIST",
    );
    invariant(
      (await filesystem.readFile("/tree/nested/file", { encoding: "utf8" })) === "aQe",
      "exclusive write changed the existing file",
    );
    await expectCode(filesystem.mkdir("/missing/child"), "ENOENT");
    passed("filesystem-path-errors");

    await filesystem.writeFile("/portable-ranges", "abc");
    invariant(
      (await filesystem.readRange("/portable-ranges", { offset: 99, length: 5 }))
        .byteLength === 0,
      "read beyond EOF was not empty",
    );
    await filesystem.writeRange("/portable-ranges", 8, Uint8Array.of(88));
    invariant(
      equalBytes(
        await filesystem.readFile("/portable-ranges"),
        Uint8Array.of(97, 98, 99, 0, 0, 0, 0, 0, 88),
      ),
      "range growth did not zero-fill the gap",
    );
    await filesystem.replaceRange(
      "/portable-ranges",
      1,
      1,
      new TextEncoder().encode("ZZ"),
    );
    await filesystem.truncate("/portable-ranges", 12);
    const grownRange = await filesystem.readFile("/portable-ranges");
    invariant(
      grownRange.byteLength === 12 && grownRange.slice(10).every((byte) => byte === 0),
      "truncate growth did not append zeros",
    );
    await expectCode(
      filesystem.replaceRange("/portable-ranges", 99, 0, new Uint8Array()),
      "EINVAL",
    );
    passed("filesystem-range-edges");

    await filesystem.writeFile("/portable-link-source", "linked");
    await filesystem.link("/portable-link-source", "/portable-link-alias");
    const linkedSource = await filesystem.stat("/portable-link-source");
    const linkedAlias = await filesystem.stat("/portable-link-alias");
    invariant(
      linkedSource.id === linkedAlias.id && linkedSource.nlink === 2,
      "hard-link aliases did not share one two-link inode",
    );
    await filesystem.unlink("/portable-link-source");
    invariant(
      (await filesystem.stat("/portable-link-alias")).nlink === 1,
      "unlink did not decrement the surviving hard-link count",
    );
    await filesystem.symlink("missing-target", "/portable-dangling");
    invariant(
      (await filesystem.lstat("/portable-dangling")).isSymbolicLink() &&
        (await filesystem.readlink("/portable-dangling")) === "missing-target",
      "dangling symbolic-link metadata changed",
    );
    await expectCode(filesystem.stat("/portable-dangling"), "ENOENT");
    await filesystem.symlink("portable-loop-b", "/portable-loop-a");
    await filesystem.symlink("portable-loop-a", "/portable-loop-b");
    await expectCode(filesystem.stat("/portable-loop-a"), "ELOOP");
    await expectCode(filesystem.link("/tree", "/portable-directory-link"), "EPERM");
    passed("filesystem-link-semantics");

    await filesystem.mkdir("/portable-move/source/child", { recursive: true });
    await filesystem.writeFile("/portable-move/source/child/value", "moved");
    await expectCode(
      filesystem.rename("/portable-move/source", "/portable-move/source/child/inside"),
      "EINVAL",
    );
    await filesystem.rename("/portable-move/source", "/portable-move/destination");
    invariant(
      (await filesystem.readFile("/portable-move/destination/child/value", {
        encoding: "utf8",
      })) === "moved",
      "directory rename did not move descendants atomically",
    );
    await expectCode(filesystem.stat("/portable-move/source"), "ENOENT");
    await expectCode(filesystem.rm("/portable-move/destination"), "ENOTEMPTY");
    await filesystem.rm("/portable-move/destination", { recursive: true });
    await expectCode(filesystem.stat("/portable-move/destination"), "ENOENT");
    await filesystem.rm("/portable-already-absent", { force: true });
    passed("filesystem-rename-removal");

    const metadataBefore = await filesystem.stat("/tree/nested/file");
    await filesystem.chmod("/tree/nested/file", 0o600);
    const metadataAfter = await filesystem.stat("/tree/nested/file");
    invariant(
      metadataAfter.id === metadataBefore.id &&
        metadataAfter.birthtimeMs === metadataBefore.birthtimeMs &&
        metadataAfter.mode === 0o600 &&
        metadataAfter.ctimeMs >= metadataBefore.ctimeMs &&
        metadataAfter.isFile() &&
        !metadataAfter.isDirectory() &&
        !metadataAfter.isSymbolicLink(),
      "metadata or type predicates changed incorrectly",
    );
    passed("filesystem-metadata");

    await filesystem.mkdir("/portable-pages");
    for (let index = 0; index < 17; index += 1)
      await filesystem.writeFile(
        `/portable-pages/entry-${index.toString().padStart(2, "0")}`,
        new Uint8Array(),
      );
    await expectCode(filesystem.readdir("/portable-pages"), "EFBIG");
    const pagedNames: string[] = [];
    let startAfter: string | undefined;
    for (;;) {
      const page = await filesystem.readdir("/portable-pages", {
        limit: 6,
        ...(startAfter === undefined ? {} : { startAfter }),
      });
      pagedNames.push(...page.map((entry) => entry.name));
      if (page.length < 6) break;
      startAfter = page.at(-1)!.name;
    }
    invariant(
      JSON.stringify(pagedNames) ===
        JSON.stringify(
          Array.from(
            { length: 17 },
            (_, index) => `entry-${index.toString().padStart(2, "0")}`,
          ),
        ),
      "explicit readdir paging skipped, repeated, or reordered an entry",
    );
    invariant(
      (await filesystem.readdir("/portable-pages", { limit: 0 })).length === 0,
      "zero-limit readdir was not empty",
    );
    passed("filesystem-pagination-cap");

    await expectDetailedError(filesystem.stat("/portable-missing"), {
      code: "ENOENT",
      syscall: "stat",
      path: "/portable-missing",
    });
    await expectDetailedError(filesystem.readdir("/tree/nested/file"), {
      code: "ENOTDIR",
      syscall: "readdir",
      path: "/tree/nested/file",
    });
    await expectDetailedError(filesystem.writeFile("/", "invalid"), {
      code: "EISDIR",
      syscall: "writeFile",
      path: "/",
    });
    passed("filesystem-error-details");

    const original = Uint8Array.from({ length: 20_000 }, (_, index) => index & 0xff);
    await filesystem.writeFile("/snapshot", original);
    const selected = await filesystem.readStream("/snapshot");
    await filesystem.writeFile("/snapshot", "new");
    invariant(
      equalBytes(await streamBytes(selected), original),
      "stream snapshot changed",
    );
    invariant(
      (await filesystem.readFile("/snapshot", { encoding: "utf8" })) === "new",
      "new read did not observe replacement",
    );
    await verifyMetadataUsage(filesystem, "stream-snapshot");
    passed("stream-snapshot");

    const activeLeaseCount = (): number =>
      scalar(adapter, "SELECT count(*) value FROM efs_leases WHERE state IN (0,1)");
    const backpressureBytes = Uint8Array.from(
      { length: 32 * 1024 },
      (_, index) => (index * 13) & 0xff,
    );
    await filesystem.writeFile("/portable-backpressure", backpressureBytes);
    const backpressured = await filesystem.readStream("/portable-backpressure");
    invariant(activeLeaseCount() === 1, "stream did not acquire one read lease");
    const backpressureReader = backpressured.getReader();
    const firstChunk = await backpressureReader.read();
    invariant(
      !firstChunk.done &&
        firstChunk.value.byteLength > 0 &&
        firstChunk.value.byteLength <=
          filesystem.capabilities.filesystem.preferredStreamChunkBytes,
      "stream emitted an empty or over-limit chunk",
    );
    invariant(activeLeaseCount() === 1, "backpressure released the live lease early");
    await backpressureReader.cancel();
    invariant(activeLeaseCount() === 0, "stream cancellation retained its lease");
    const abortController = new AbortController();
    const aborted = await filesystem.readStream("/portable-backpressure", {
      signal: abortController.signal,
    });
    const abortReader = aborted.getReader();
    abortController.abort();
    let abortRejected = false;
    let bytesAfterAbort = 0;
    try {
      for (;;) {
        const result = await abortReader.read();
        if (result.done) break;
        bytesAfterAbort += result.value.byteLength;
      }
    } catch (error) {
      invariant(
        error instanceof DOMException && error.name === "AbortError",
        `aborted stream returned ${String(error)}`,
      );
      abortRejected = true;
    }
    invariant(abortRejected, "aborted stream completed without AbortError");
    invariant(
      bytesAfterAbort <= backpressureBytes.byteLength,
      "aborted stream emitted bytes outside its selected range",
    );
    invariant(activeLeaseCount() === 0, "aborted stream retained its lease");
    const preAbortedController = new AbortController();
    preAbortedController.abort();
    try {
      await filesystem.readStream("/portable-backpressure", {
        signal: preAbortedController.signal,
      });
      throw new Error("portable conformance: pre-aborted stream was admitted");
    } catch (error) {
      invariant(
        error instanceof DOMException && error.name === "AbortError",
        `pre-aborted stream returned ${String(error)}`,
      );
    }
    invariant(activeLeaseCount() === 0, "pre-aborted stream acquired a lease");
    passed("stream-abort-backpressure");

    const streamedFailure = new Error("portable producer failure");
    await expectCode(
      filesystem.writeFile(
        "/portable-stream-failure",
        new ReadableStream<Uint8Array>({
          start(controller) {
            controller.enqueue(Uint8Array.of(1, 2, 3));
            controller.error(streamedFailure);
          },
        }),
        { maxBytes: 1024 },
      ),
      "EIO",
    );
    await expectCode(filesystem.stat("/portable-stream-failure"), "ENOENT");
    invariant(
      activeLeaseCount() === 0,
      "failed streamed write retained an active lease",
    );
    invariant(
      scalar(
        adapter,
        "SELECT coalesce(sum(ingest_reservation_bytes+metadata_reservation_bytes),0) value FROM efs_staging_certificates",
      ) === 0,
      "failed streamed write retained a staging reservation",
    );
    const sourceBytes = Uint8Array.of(10, 20, 30, 40);
    const ownedWrite = filesystem.writeFile("/portable-owned-write", sourceBytes);
    sourceBytes.fill(0);
    await ownedWrite;
    invariant(
      equalBytes(
        await filesystem.readFile("/portable-owned-write"),
        Uint8Array.of(10, 20, 30, 40),
      ),
      "writeFile retained caller-owned input",
    );
    await filesystem.writeFile("/portable-exclusive", "first", { exclusive: true });
    await expectCode(
      filesystem.writeFile("/portable-exclusive", "second", { exclusive: true }),
      "EEXIST",
    );
    invariant(
      (await filesystem.readFile("/portable-exclusive", { encoding: "utf8" })) ===
        "first",
      "exclusive failure changed the visible file",
    );
    passed("lease-staging-lifecycle");

    const beforeReads = durableReadFootprint(adapter);
    await filesystem.stat("/portable-owned-write");
    await filesystem.lstat("/portable-owned-write");
    await filesystem.readFile("/portable-owned-write");
    await filesystem.readRange("/portable-owned-write", { offset: 1, length: 2 });
    await filesystem.readdir("/portable-pages", { limit: 6 });
    invariant(
      durableReadFootprint(adapter) === beforeReads,
      "ordinary reads created durable namespace, content, branch, staging, or lease state",
    );
    passed("read-side-effect-boundary");

    await filesystem.mkdir("/portable-overlap");
    await Promise.all(
      Array.from({ length: 8 }, (_, index) =>
        filesystem!.writeFile(
          `/portable-overlap/value-${index.toString().padStart(2, "0")}`,
          `overlap-${index}`,
          { exclusive: true },
        ),
      ),
    );
    const overlap = await filesystem.readdir("/portable-overlap", { limit: 8 });
    invariant(overlap.length === 8, "overlapping operations lost a commit");
    for (let index = 0; index < 8; index += 1)
      invariant(
        (await filesystem.readFile(
          `/portable-overlap/value-${index.toString().padStart(2, "0")}`,
          { encoding: "utf8" },
        )) === `overlap-${index}`,
        `overlapping operation ${index} returned the wrong value`,
      );
    passed("overlapping-operations");

    await filesystem.writeFile("/branch-base", "base");
    const merged = await filesystem.branches.create("portable-merged");
    await merged.writeFile("/published", "value");
    const mergedResult = await merged.publish({ operationId: "portable-merge-op" });
    invariant(mergedResult.outcome === "merged", "independent branch did not merge");
    invariant(
      (await filesystem.readFile("/published", { encoding: "utf8" })) === "value",
      "published branch value is missing",
    );
    invariant(
      JSON.stringify(
        await filesystem.branches.replay("portable-merge-op", "portable-merged"),
      ) === JSON.stringify(mergedResult),
      "publication replay differs",
    );
    await merged.close();
    const conflicted = await filesystem.branches.create("portable-conflict");
    await conflicted.writeFile("/branch-base", "branch");
    await filesystem.writeFile("/branch-base", "main");
    const conflict = await conflicted.publish({ operationId: "portable-conflict-op" });
    invariant(
      conflict.outcome === "conflict",
      "same-inode publication did not conflict",
    );
    invariant(
      (await filesystem.readFile("/branch-base", { encoding: "utf8" })) === "main",
      "conflict changed main",
    );
    await conflicted.close();
    await verifyMetadataUsage(filesystem, "branch-publication");
    passed("branch-publication");

    let cursor: string | undefined;
    let checked = 0;
    for (let batch = 0; batch < 10_000; batch += 1) {
      const verification = await filesystem.maintenance.verify({
        ...(cursor === undefined ? {} : { cursor }),
        maxEntities: 2,
      });
      checked += verification.checkedEntities;
      cursor = verification.nextCursor ?? undefined;
      if (verification.complete) break;
    }
    invariant(
      cursor === undefined && checked > 0,
      "verification did not finish boundedly",
    );
    let collectionState = "";
    for (let batch = 0; batch < 10_000; batch += 1) {
      const collection = await filesystem.maintenance.collectGarbage({
        runId: "portable-conformance-gc",
        maxBatches: 1,
      });
      collectionState = collection.state;
      if (collection.state === "complete") break;
    }
    invariant(
      collectionState === "complete",
      "garbage collection did not resume to completion",
    );
    invariant(
      (await filesystem.maintenance.snapshotStorage()).state === "complete",
      "storage snapshot did not complete",
    );
    passed("maintenance-cursors");

    const limits = adapter.capabilities;
    invariant(
      Number.isSafeInteger(limits.maxBlobBytes) &&
        limits.maxBlobBytes > 0 &&
        Number.isSafeInteger(limits.maxBindings) &&
        limits.maxBindings >= 8 &&
        Number.isSafeInteger(limits.maxPhysicalDatabaseBytes) &&
        limits.maxPhysicalDatabaseBytes > 0 &&
        Number.isSafeInteger(limits.maxJournalBytes) &&
        limits.maxJournalBytes > 0,
      "adapter resource capabilities are not finite positive integers",
    );
    passed("resource-capabilities");

    await filesystem.writeFile("/survives-reopen", "durable");
    await filesystem.close();
    filesystem = undefined;
    adapter.close();
    adapter = await fixture.reopen({ physical: false });
    filesystem = (await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      filesystem: portableFilesystemLimits,
      storage: { maxGcBatchSize: 2, maxQueryBatchSize: 16 },
    })) as PortableFilesystem;
    invariant(
      (await filesystem.readFile("/survives-reopen", { encoding: "utf8" })) ===
        "durable",
      "committed bytes did not survive reopen",
    );
    passed("durable-reopen");

    if (fixture.capabilities.includes("read-only-reopen")) {
      await filesystem.close();
      filesystem = undefined;
      adapter.close();
      adapter = await fixture.reopen({ readOnly: true, physical: true });
      filesystem = (await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        filesystem: portableFilesystemLimits,
        storage: { maxGcBatchSize: 2, maxQueryBatchSize: 16 },
      })) as PortableFilesystem;
      invariant(
        (await filesystem.readFile("/survives-reopen", { encoding: "utf8" })) ===
          "durable",
        "read-only reopen changed committed bytes",
      );
      await expectCode(filesystem.writeFile("/read-only-write", "forbidden"), "EROFS");
      await filesystem.close();
      filesystem = undefined;
      adapter.close();
      adapter = await fixture.reopen({ physical: true });
      filesystem = (await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        filesystem: portableFilesystemLimits,
        storage: { maxGcBatchSize: 2, maxQueryBatchSize: 16 },
      })) as PortableFilesystem;
      passed("read-only-reopen");
    } else {
      skipped("read-only-reopen", "adapter does not report read-only reopen");
    }

    if (fixture.capabilities.includes("second-connection")) {
      invariant(
        fixture.openSecondConnection !== undefined,
        "second-connection capability lacks its hook",
      );
      const secondAdapter = await fixture.openSecondConnection();
      const secondFilesystem = (await EphemeralFS.open({
        database: secondAdapter,
        ownsDatabase: false,
        filesystem: portableFilesystemLimits,
        storage: { maxGcBatchSize: 2, maxQueryBatchSize: 16 },
      })) as PortableFilesystem;
      try {
        invariant(
          (await secondFilesystem.readFile("/survives-reopen", {
            encoding: "utf8",
          })) === "durable",
          "second connection did not observe committed bytes",
        );
        await secondFilesystem.writeFile("/second-connection", "visible");
        invariant(
          (await filesystem.readFile("/second-connection", { encoding: "utf8" })) ===
            "visible",
          "first connection did not observe the second connection commit",
        );
        await Promise.all(
          Array.from({ length: 12 }, (_, index) =>
            (index % 2 === 0 ? filesystem! : secondFilesystem).writeFile(
              `/two-instance-${index.toString().padStart(2, "0")}`,
              `serializable-${index}`,
              { exclusive: true },
            ),
          ),
        );
        for (let index = 0; index < 12; index += 1) {
          const reader = index % 2 === 0 ? secondFilesystem : filesystem;
          invariant(
            (await reader.readFile(
              `/two-instance-${index.toString().padStart(2, "0")}`,
              { encoding: "utf8" },
            )) === `serializable-${index}`,
            `two-instance operation ${index} was not serializable`,
          );
        }
      } finally {
        await secondFilesystem.close();
        secondAdapter.close();
      }
      passed("second-connection");
    } else {
      skipped("second-connection", "adapter does not report a second connection");
    }

    await filesystem.writeFile("/portable-close-stream", "selected-before-close");
    const closingStream = await filesystem.readStream("/portable-close-stream");
    const closingReader = closingStream.getReader();
    const closingBranch = await filesystem.branches.create("portable-owner-close");
    await filesystem.close();
    await filesystem.close();
    await expectCode(filesystem.stat("/"), "EBADF");
    await expectCode(closingReader.read(), "EBADF");
    await expectCode(closingBranch.info(), "EBADF");
    closingReader.releaseLock();
    await closingBranch.close();
    filesystem = undefined;
    adapter.close();
    passed("close-lifecycle");
    return Object.freeze(results);
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

/** Registers the normative shared filesystem suite with Vitest. */
export function filesystemConformance(factory: ConformanceAdapterFactory): void {
  test(`${factory.name}: shared filesystem conformance`, async () => {
    await runFilesystemConformance(factory);
  });
}

export type RecordingEvent =
  | Readonly<{
      type: "create";
      factory: string;
      label: string | null;
      seed: number | null;
    }>
  | Readonly<{ type: "reopen"; readOnly: boolean; physical: boolean }>
  | Readonly<{ type: "second-connection" }>
  | Readonly<{ type: "dispose" }>;

/** Wraps a real test factory without weakening its restart or connection behavior. */
export function createRecordingFactory(
  factory: ConformanceAdapterFactory,
  events: RecordingEvent[],
): ConformanceAdapterFactory {
  return Object.freeze({
    name: `recording:${factory.name}`,
    async create(
      options: ConformanceFixtureOptions = {},
    ): Promise<ConformanceDatabase> {
      events.push(
        Object.freeze({
          type: "create",
          factory: factory.name,
          label: options.label ?? null,
          seed: options.seed ?? null,
        }),
      );
      const database = await factory.create(options);
      let disposed = false;
      return Object.freeze({
        adapter: database.adapter,
        capabilities: database.capabilities,
        ...(database.faults === undefined ? {} : { faults: database.faults }),
        async reopen(reopenOptions: { readOnly?: boolean; physical?: boolean } = {}) {
          events.push(
            Object.freeze({
              type: "reopen",
              readOnly: reopenOptions.readOnly ?? false,
              physical: reopenOptions.physical ?? false,
            }),
          );
          return database.reopen(reopenOptions);
        },
        ...(database.openSecondConnection === undefined
          ? {}
          : {
              async openSecondConnection() {
                events.push(Object.freeze({ type: "second-connection" }));
                return database.openSecondConnection!();
              },
            }),
        async dispose() {
          if (!disposed) {
            disposed = true;
            events.push(Object.freeze({ type: "dispose" }));
            await database.dispose();
          }
        },
      });
    },
  });
}

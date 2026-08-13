import { EphemeralFS } from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";

export const PORTABLE_STORAGE_CONFORMANCE_CASE_IDS = Object.freeze([
  "storage-staging-closure-100001",
  "storage-certificate-field-corruption",
  "storage-sealed-membership-immutability",
  "storage-concurrent-payload-quota",
  "storage-usage-recount",
  "storage-manifest-range-corruption",
] as const);

export const PORTABLE_STORAGE_CASE_IDS = Object.freeze([
  ...PORTABLE_STORAGE_CONFORMANCE_CASE_IDS,
  "storage-staging-batch-crash-recovery" as const,
]);

export type PortableStorageCaseId = (typeof PORTABLE_STORAGE_CASE_IDS)[number];

export interface PortableStorageCaseResult {
  readonly id: PortableStorageCaseId;
  readonly status: "passed";
}

export interface PortableStagingClosureEvidence {
  readonly schema: "efs-portable-staging-closure-v1";
  readonly manifestEntries: 100_001;
  readonly uniqueClosureMembers: number;
  readonly reconciliationStatements: number;
  readonly finalValidationStatements: 1;
  readonly certificateFieldsRejected: 10;
  readonly sealedMembershipMutationsRejected: 2;
}

export interface PortableStorageInternals {
  runStagingClosure(
    adapter: FilesystemSQLiteDriver,
  ): Promise<PortableStagingClosureEvidence>;
  stageCrashBatch(
    adapter: FilesystemSQLiteDriver,
    batch: number,
  ): Promise<{ readonly durableEntries: number }>;
  recoverStagingCrash(adapter: FilesystemSQLiteDriver): Promise<{
    readonly activeLeases: number;
    readonly stagingCertificates: number;
    readonly stagingEntries: number;
    readonly stagingBytes: number;
    readonly ingestReservationBytes: number;
  }>;
}

export interface PortableStagingCrashEvidence {
  readonly schema: "efs-portable-staging-crash-v1";
  readonly batches: 3;
  readonly physicalRestarts: 3;
  readonly recovered: true;
}

export type PortableStagingCrashOutcome =
  | { readonly status: "restart-required"; readonly batch: number }
  | { readonly status: "complete"; readonly result: PortableStagingCrashEvidence };

/** Host-coordinated staging crash scenario; adapters must physically restart between calls. */
export class PortableStagingCrashSession {
  #batch = 0;

  async run(
    adapter: FilesystemSQLiteDriver,
    internals: PortableStorageInternals,
  ): Promise<PortableStagingCrashOutcome> {
    if (this.#batch < 3) {
      const batch = this.#batch;
      const progress = await internals.stageCrashBatch(adapter, batch);
      invariant(
        progress.durableEntries === (batch + 1) * 3,
        `staging batch ${batch} did not commit exactly three new entries`,
      );
      this.#batch += 1;
      return Object.freeze({ status: "restart-required", batch });
    }
    const recovered = await internals.recoverStagingCrash(adapter);
    invariant(
      recovered.activeLeases === 0 &&
        recovered.stagingCertificates === 0 &&
        recovered.stagingEntries === 0 &&
        recovered.stagingBytes === 0 &&
        recovered.ingestReservationBytes === 0,
      `abandoned staging rows or charges remain: ${JSON.stringify(recovered)}`,
    );
    return Object.freeze({
      status: "complete",
      result: Object.freeze({
        schema: "efs-portable-staging-crash-v1",
        batches: 3,
        physicalRestarts: 3,
        recovered: true,
      }),
    });
  }
}

export const PORTABLE_STORAGE_STORAGE_LIMITS = Object.freeze({
  maxManagedPayloadBytes: 16 * 1024 * 1024,
  maxStagingPayloadBytes: 10 * 1024 * 1024,
  maxChargedMetadataBytes: 128 * 1024 * 1024,
  maxMaintenanceBytes: 2 * 1024 * 1024,
  maintenanceReserveBytes: 1024 * 1024,
  maxBranchOverlayBytes: 32 * 1024 * 1024,
  maxQueryBatchSize: 32,
  maxGcBatchSize: 32,
});

export const PORTABLE_STORAGE_RUNTIME_LIMITS = Object.freeze({
  maxManagedResidentBytes: 128 * 1024 * 1024,
  maxCacheBytes: 8 * 1024 * 1024,
  maxPendingWriteBytes: 20 * 1024 * 1024,
  maxWriteSessionBytes: 10 * 1024 * 1024,
  maxPrefetchBytes: 512 * 1024,
  maxQueryBatchBytes: 512 * 1024,
});

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable storage conformance: ${message}`);
}

function streamedFixture(length: number, seed: number): ReadableStream<Uint8Array> {
  let offset = 0;
  return new ReadableStream<Uint8Array>({
    pull(controller) {
      if (offset === length) {
        controller.close();
        return;
      }
      const size = Math.min(256 * 1024, length - offset);
      const bytes = new Uint8Array(size);
      for (let index = 0; index < size; index += 1)
        bytes[index] = ((offset + index) * 31 + seed) & 0xff;
      offset += size;
      controller.enqueue(bytes);
    },
  });
}

function expectedRange(offset: number, length: number, seed: number): Uint8Array {
  return Uint8Array.from(
    { length },
    (_, index) => ((offset + index) * 31 + seed) & 0xff,
  );
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  for (let index = 0; index < left.byteLength; index += 1)
    if (left[index] !== right[index]) return false;
  return true;
}

async function verifyAll(filesystem: EphemeralFS): Promise<number> {
  let cursor: string | undefined;
  let verified = 0;
  for (let call = 0; call < 100_000; call += 1) {
    const result = await filesystem.maintenance.verify({
      ...(cursor === undefined ? {} : { cursor }),
      maxEntities: 16,
    });
    verified += result.checkedEntities;
    cursor = result.nextCursor ?? undefined;
    if (result.complete) return verified;
  }
  throw new Error("portable storage conformance: verification did not finish");
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
  throw new Error(`portable storage conformance: expected ${code} rejection`);
}

/**
 * Runs the adapter-neutral M2 storage case whose implementation port remains private.
 * The injected port is shared by the Node and workerd harnesses; it may use private
 * repositories without widening the package export boundary.
 */
export async function runStorageConformance(
  factory: ConformanceAdapterFactory,
  internals: PortableStorageInternals,
): Promise<readonly PortableStorageCaseResult[]> {
  const fixture = await factory.create({
    label: "portable-storage",
    seed: 0x57a61e,
  });
  try {
    const evidence = await internals.runStagingClosure(fixture.adapter);
    invariant(evidence.manifestEntries === 100_001, "large closure was reduced");
    invariant(
      evidence.uniqueClosureMembers > 1,
      "large closure did not contain manifest nodes",
    );
    invariant(
      evidence.reconciliationStatements < evidence.manifestEntries * 8,
      "closure reconciliation used unbounded SQL amplification",
    );
    invariant(
      evidence.finalValidationStatements === 1,
      "final validation was not constant-row work",
    );
    invariant(
      evidence.certificateFieldsRejected === 10,
      "not every closure-certificate field was rejected when altered",
    );
    invariant(
      evidence.sealedMembershipMutationsRejected === 2,
      "sealed membership accepted a mutation",
    );

    const storage = PORTABLE_STORAGE_STORAGE_LIMITS;
    const runtime = PORTABLE_STORAGE_RUNTIME_LIMITS;
    const first = await EphemeralFS.open({
      database: fixture.adapter,
      ownsDatabase: false,
      storage,
      runtime,
    });
    const second = await EphemeralFS.open({
      database: fixture.adapter,
      ownsDatabase: false,
      storage,
      runtime,
    });
    const priorUsage = fixture.adapter.transaction(
      "read",
      (transaction) =>
        transaction.all<{ readonly used: number }>(
          "SELECT object_bytes+manifest_root_bytes+manifest_node_bytes+page_bytes+patch_bytes+staging_bytes+ingest_reservation_bytes+result_bytes used FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    invariant(priorUsage !== undefined, "pre-race usage authority is missing");
    const availablePayload =
      storage.maxManagedPayloadBytes -
      storage.maintenanceReserveBytes -
      priorUsage.used;
    const fileBytes = Math.min(8 * 1024 * 1024, Math.floor(availablePayload * 0.27));
    invariant(fileBytes >= 1024 * 1024, "fixture leaves no useful quota race envelope");
    const contenders = [
      { path: "/quota-left", seed: 17 },
      { path: "/quota-right", seed: 93 },
    ] as const;
    const outcomes = await Promise.allSettled([
      first.writeFile(contenders[0].path, streamedFixture(fileBytes, 17), {
        maxBytes: fileBytes,
      }),
      second.writeFile(contenders[1].path, streamedFixture(fileBytes, 93), {
        maxBytes: fileBytes,
      }),
    ]);
    const succeeded = outcomes
      .map((outcome, index) => ({ outcome, fixture: contenders[index]! }))
      .filter(({ outcome }) => outcome.status === "fulfilled");
    const failed = outcomes.filter((outcome) => outcome.status === "rejected");
    invariant(
      succeeded.length === 1 && failed.length === 1,
      `concurrent payload quota did not admit exactly one serialized writer: ${outcomes
        .map((outcome) =>
          outcome.status === "fulfilled"
            ? "fulfilled"
            : `rejected:${String(outcome.reason)}`,
        )
        .join(
          ",",
        )} used=${priorUsage.used} available=${availablePayload} each=${fileBytes}`,
    );
    const failure = failed[0]!;
    invariant(
      failure.status === "rejected" &&
        failure.reason !== null &&
        typeof failure.reason === "object" &&
        "code" in failure.reason &&
        failure.reason.code === "ENOSPC",
      `quota race returned ${String(failure.status === "rejected" ? failure.reason : failure)}`,
    );
    const selected = succeeded[0]!.fixture;
    const offsets = [0, Math.floor(fileBytes / 2), fileBytes - 257];
    for (const offset of offsets) {
      const actual = await first.readRange(selected.path, { offset, length: 257 });
      invariant(
        equalBytes(actual, expectedRange(offset, 257, selected.seed)),
        `manifest range at ${offset} changed`,
      );
    }
    invariant(
      (await first.readRange(selected.path, { offset: fileBytes, length: 1 }))
        .byteLength === 0,
      "manifest EOF range was not empty",
    );
    const snapshot = await first.maintenance.snapshotStorage();
    const usage = fixture.adapter.transaction(
      "read",
      (transaction) =>
        transaction.all<{
          readonly object_bytes: number;
          readonly manifest_bytes: number;
          readonly charged_object_bytes: number;
          readonly charged_manifest_bytes: number;
          readonly roots: number;
          readonly nodes: number;
        }>(
          "SELECT (SELECT coalesce(sum(length(bytes)),0) FROM efs_cas_objects) object_bytes,(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_roots)+(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_nodes) manifest_bytes,object_bytes charged_object_bytes,manifest_root_bytes+manifest_node_bytes charged_manifest_bytes,(SELECT count(*) FROM efs_manifest_roots) roots,(SELECT count(*) FROM efs_manifest_nodes) nodes FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    invariant(usage !== undefined, "usage authority is missing");
    invariant(
      usage.object_bytes === usage.charged_object_bytes &&
        usage.manifest_bytes === usage.charged_manifest_bytes &&
        snapshot.storedObjectPayloadBytes === usage.object_bytes &&
        snapshot.storedManifestPayloadBytes === usage.manifest_bytes,
      "usage authority differs from durable payload sums",
    );
    invariant(
      usage.roots >= 2 && usage.nodes > 0 && (await verifyAll(first)) > 0,
      "durable manifest or bounded verification evidence is missing",
    );
    await first.close();
    await second.close();

    const corrupt = fixture.adapter.transaction(
      "read",
      (transaction) =>
        transaction.all<{ readonly hash: Uint8Array; readonly size: number }>(
          "SELECT hash,length(bytes) size FROM efs_cas_objects WHERE length(bytes)>1024 ORDER BY hash LIMIT 1",
          [],
          { maxRows: 1, maxBytes: 1024 },
        )[0],
    );
    invariant(
      corrupt !== undefined,
      "manifest fixture has no content object to corrupt",
    );
    fixture.adapter.transaction("write", (transaction) =>
      transaction.run("UPDATE efs_cas_objects SET bytes=? WHERE hash=?", [
        new Uint8Array(corrupt.size),
        corrupt.hash,
      ]),
    );
    const reopened = await EphemeralFS.open({
      database: fixture.adapter,
      ownsDatabase: false,
      storage,
      runtime,
    });
    await expectCode(reopened.readFile(selected.path), "EIO");
    await reopened.close();

    return Object.freeze(
      PORTABLE_STORAGE_CONFORMANCE_CASE_IDS.map((id) =>
        Object.freeze({ id, status: "passed" as const }),
      ),
    );
  } finally {
    try {
      await fixture.adapter.close();
    } catch {}
    await fixture.dispose();
  }
}

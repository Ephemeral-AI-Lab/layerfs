import { EphemeralFS } from "@ephemeralai/fs";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  SqliteBindings,
  SqliteRow,
} from "@ephemeralai/fs/sqlite-driver";

export type PortableMaintenanceFaultVariant = "snapshot" | "collection" | "abandoned";
export type PortableMaintenanceFaultKind = "statement" | "batch";
export const PORTABLE_MAINTENANCE_FAULT_TOPOLOGY = Object.freeze({
  snapshot: Object.freeze({
    durableStatements: 110,
    committedBatches: 42,
    maxBatchStatements: 6,
  }),
  collection: Object.freeze({
    durableStatements: 259,
    committedBatches: 128,
    maxBatchStatements: 3,
  }),
  abandoned: Object.freeze({
    durableStatements: 61,
    committedBatches: 33,
    maxBatchStatements: 3,
  }),
} as const);

export interface PortableMaintenanceFaultMetrics {
  readonly durableStatements: number;
  readonly committedBatches: number;
  readonly maxBatchStatements: number;
}

export interface PortableMaintenanceFaultAttempt {
  readonly schema: "efs-portable-maintenance-fault-attempt-v1";
  readonly variant: PortableMaintenanceFaultVariant;
  readonly kind: PortableMaintenanceFaultKind;
  readonly ordinal: number;
  readonly injected: boolean;
  readonly metrics: PortableMaintenanceFaultMetrics;
  readonly resultCounters?: Readonly<Record<string, number>>;
}

const STORAGE = Object.freeze({
  maxGcBatchSize: 1,
  maxQueryBatchSize: 1,
  maxFinalTransactionRows: 64,
  maxMaintenanceBytes: 32 * 1024 * 1024,
  maintenanceReserveBytes: 32 * 1024 * 1024,
});

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable maintenance fault: ${message}`);
}

function maintenanceFaultDriver(
  adapter: FilesystemSQLiteDriver,
  selectedKind: PortableMaintenanceFaultKind,
  selectedOrdinal: number,
) {
  const state = {
    armed: false,
    durableStatements: 0,
    committedBatches: 0,
    maxBatchStatements: 0,
  };
  const fault = (kind: PortableMaintenanceFaultKind, ordinal: number): DOMException =>
    new DOMException(`portable maintenance ${kind} fault ${ordinal}`, "AbortError");
  const driver: FilesystemSQLiteDriver = Object.freeze({
    kind: adapter.kind,
    readOnly: adapter.readOnly,
    capabilities: adapter.capabilities,
    ...(adapter.hashBytes === undefined
      ? {}
      : { hashBytes: adapter.hashBytes.bind(adapter) }),
    ...(adapter.hashBytesAsync === undefined
      ? {}
      : { hashBytesAsync: adapter.hashBytesAsync.bind(adapter) }),
    transaction<T>(
      mode: "read" | "write" | "exclusive",
      callback: (tx: FilesystemSQLiteTransaction) => T,
    ): T {
      const observing = state.armed;
      const statements: string[] = [];
      const result = adapter.transaction(mode, (tx) =>
        callback(
          Object.freeze({
            scope: tx.scope,
            run(sql: string, bindings: SqliteBindings = []) {
              const value = tx.run(sql, bindings);
              if (observing) statements.push(sql);
              return value;
            },
            all<Row extends SqliteRow = SqliteRow>(
              sql: string,
              bindings: SqliteBindings,
              budget: { readonly maxRows: number; readonly maxBytes: number },
            ): readonly Row[] {
              return tx.all<Row>(sql, bindings, budget);
            },
          }),
        ),
      );
      if (!observing || mode === "read" || statements.length === 0) return result;
      state.committedBatches += 1;
      state.maxBatchStatements = Math.max(state.maxBatchStatements, statements.length);
      let matchingStatement = false;
      for (const _sql of statements) {
        state.durableStatements += 1;
        if (selectedKind === "statement" && state.durableStatements === selectedOrdinal)
          matchingStatement = true;
      }
      if (matchingStatement) {
        state.armed = false;
        throw fault("statement", selectedOrdinal);
      }
      if (selectedKind === "batch" && state.committedBatches === selectedOrdinal) {
        state.armed = false;
        throw fault("batch", selectedOrdinal);
      }
      return result;
    },
    physicalStorage: () => adapter.physicalStorage?.() ?? Object.freeze({}),
    ...(adapter.checkpoint === undefined
      ? {}
      : { checkpoint: adapter.checkpoint.bind(adapter) }),
    close: () => adapter.close(),
  });
  return Object.freeze({
    driver,
    arm() {
      state.armed = true;
    },
    disarm() {
      state.armed = false;
    },
    metrics(): PortableMaintenanceFaultMetrics {
      return Object.freeze({
        durableStatements: state.durableStatements,
        committedBatches: state.committedBatches,
        maxBatchStatements: state.maxBatchStatements,
      });
    },
  });
}

async function driveSnapshot(filesystem: EphemeralFS) {
  for (let call = 0; call < 2_000; call += 1) {
    const result = await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
    if (result.state === "complete") return result;
  }
  throw new Error("portable maintenance snapshot did not complete");
}

function collectionRunId(variant: PortableMaintenanceFaultVariant): string {
  return variant === "abandoned"
    ? "abandoned-fault"
    : "portable-maintenance-fault-collection";
}

async function driveCollection(
  filesystem: EphemeralFS,
  variant: PortableMaintenanceFaultVariant,
) {
  for (let call = 0; call < 4_000; call += 1) {
    const result = await filesystem.maintenance.collectGarbage({
      runId: collectionRunId(variant),
      maxBatches: 1,
    });
    if (result.state === "complete") return result;
  }
  throw new Error("portable maintenance collection did not complete");
}

function snapshotCounters(result: Awaited<ReturnType<typeof driveSnapshot>>) {
  return Object.freeze({
    mainLogicalBytes: result.mainLogicalBytes,
    storedObjectPayloadBytes: result.storedObjectPayloadBytes,
    storedManifestPayloadBytes: result.storedManifestPayloadBytes,
    reachableObjectPayloadBytes: result.reachableObjectPayloadBytes,
    reachableManifestPayloadBytes: result.reachableManifestPayloadBytes,
    reclaimablePayloadBytes: result.reclaimablePayloadBytes,
    objectCount: result.objectCount,
    manifestRootCount: result.manifestRootCount,
    manifestNodeCount: result.manifestNodeCount,
    chargedMetadataBytes: result.chargedMetadataBytes,
    revisionCount: result.revisionCount,
  });
}

function collectionCounters(result: Awaited<ReturnType<typeof driveCollection>>) {
  return Object.freeze({
    examinedManifestRootCount: result.examinedManifestRootCount,
    deletedManifestRootCount: result.deletedManifestRootCount,
    examinedManifestNodeCount: result.examinedManifestNodeCount,
    deletedManifestNodeCount: result.deletedManifestNodeCount,
    examinedObjectCount: result.examinedObjectCount,
    deletedObjectCount: result.deletedObjectCount,
    reclaimedObjectPayloadBytes: result.reclaimedObjectPayloadBytes,
    reclaimedManifestPayloadBytes: result.reclaimedManifestPayloadBytes,
    reclaimedBranchOverlayPayloadBytes: result.reclaimedBranchOverlayPayloadBytes,
  });
}

async function prepareFixture(
  adapter: FilesystemSQLiteDriver,
  variant: PortableMaintenanceFaultVariant,
): Promise<void> {
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    storage: STORAGE,
    clock: () => 10,
  });
  try {
    if (variant === "snapshot") {
      await filesystem.writeFile("/main", "main-value");
      const branch = await filesystem.branches.create("maintenance-fault-branch");
      await branch.writeFile("/branch", "branch-only-value");
      await branch.close();
      return;
    }
    await filesystem.writeFile(
      "/kept",
      variant === "abandoned" ? "abandoned-kept-value" : "reachable-value",
    );
    if (variant !== "abandoned") {
      // Leave one terminal prior run for bounded cleanup in the selected run.
      let prior = await filesystem.maintenance.collectGarbage({
        runId: "portable-maintenance-prior-terminal",
        maxBatches: 1,
      });
      for (let call = 0; call < 4_000 && prior.state !== "complete"; call += 1)
        prior = await filesystem.maintenance.collectGarbage({
          runId: "portable-maintenance-prior-terminal",
          maxBatches: 1,
        });
      invariant(prior.state === "complete", "prior collection did not complete");
      await filesystem.writeFile("/orphan", "unreachable-value");
      await filesystem.unlink("/orphan");
      await filesystem.writeFile("/root-journal-a", "a");
      await filesystem.writeFile("/root-journal-b", "b");
    }
  } finally {
    await filesystem.close();
  }
  if (variant === "abandoned") {
    const runId = collectionRunId(variant);
    const runCharge = 512 + new TextEncoder().encode(runId).byteLength * 2;
    const usageColumns = Object.freeze([
      "object_count",
      "object_bytes",
      "manifest_root_count",
      "manifest_root_bytes",
      "manifest_node_count",
      "manifest_node_bytes",
      "page_count",
      "page_bytes",
      "patch_count",
      "patch_bytes",
      "staging_bytes",
      "ingest_reservation_bytes",
      "result_bytes",
      "maintenance_bytes",
      "permanent_identifiers",
      "charged_metadata_bytes",
      "mutation_sequence",
    ]);
    adapter.transaction("write", (tx) => {
      const meta = tx.all<
        {
          next_allocation_sequence: number;
          root_mutation_generation: number;
        } & SqliteRow
      >(
        "SELECT next_allocation_sequence,root_mutation_generation FROM efs_meta WHERE singleton=1",
        [],
        { maxRows: 1, maxBytes: 512 },
      )[0];
      invariant(meta !== undefined, "missing metadata for abandoned run");
      tx.run(
        "UPDATE efs_usage SET maintenance_bytes=maintenance_bytes+?,mutation_sequence=mutation_sequence+1 WHERE singleton=1",
        [runCharge],
      );
      tx.run(
        `UPDATE efs_usage SET integrity_token=${usageColumns
          .map((column) => `CAST(${column} AS TEXT)`)
          .join("||':'||")} WHERE singleton=1`,
      );
      tx.run(
        "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES(?,0,?,?,0,NULL,?)",
        [runId, meta.next_allocation_sequence - 1, meta.root_mutation_generation, 1],
      );
      for (let index = 1; index <= 3; index += 1)
        tx.run("INSERT INTO efs_gc_marks(run_id,kind,hash,processed) VALUES(?,2,?,0)", [
          runId,
          new Uint8Array(32).fill(index),
        ]);
      tx.run("UPDATE efs_gc_runs SET state=8 WHERE id=? AND state<>7", [runId]);
    });
    const abandoned = adapter.transaction(
      "read",
      (tx) =>
        tx.all<{ state: number; mark_count: number } & SqliteRow>(
          "SELECT state,(SELECT count(*) FROM efs_gc_marks WHERE run_id=efs_gc_runs.id) mark_count FROM efs_gc_runs WHERE id=?",
          [collectionRunId(variant)],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    invariant(
      abandoned?.state === 8 && abandoned.mark_count === 3,
      "setup did not persist the exact abandoned marked run",
    );
  }
}

async function verifyUsage(filesystem: EphemeralFS): Promise<void> {
  let cursor: string | undefined;
  for (let call = 0; call < 100_000; call += 1) {
    const result = await filesystem.maintenance.verify({
      ...(cursor === undefined ? {} : { cursor }),
      maxEntities: 4,
    });
    cursor = result.nextCursor ?? undefined;
    if (result.complete) return;
  }
  throw new Error("portable maintenance usage verification did not complete");
}

/** Run one fresh post-commit maintenance fault attempt. */
export async function runPortableMaintenanceFaultAttempt(
  adapter: FilesystemSQLiteDriver,
  variant: PortableMaintenanceFaultVariant,
  kind: PortableMaintenanceFaultKind,
  ordinal: number,
): Promise<PortableMaintenanceFaultAttempt> {
  invariant(Number.isSafeInteger(ordinal) && ordinal > 0, "invalid fault ordinal");
  await prepareFixture(adapter, variant);
  const injector = maintenanceFaultDriver(adapter, kind, ordinal);
  const filesystem = await EphemeralFS.open({
    database: injector.driver,
    ownsDatabase: false,
    storage: STORAGE,
    clock: () => 10,
  });
  injector.arm();
  let injected = false;
  let resultCounters: Readonly<Record<string, number>> | undefined;
  try {
    resultCounters =
      variant === "snapshot"
        ? snapshotCounters(await driveSnapshot(filesystem))
        : collectionCounters(await driveCollection(filesystem, variant));
  } catch (error) {
    if (!(error instanceof DOMException && error.name === "AbortError")) throw error;
    injected = true;
  } finally {
    injector.disarm();
  }
  const metrics = injector.metrics();
  if (!injected) {
    const limit =
      kind === "statement" ? metrics.durableStatements : metrics.committedBatches;
    invariant(ordinal > limit, "maintenance completed before selected fault ordinal");
    await verifyUsage(filesystem);
    await filesystem.close();
  }
  return Object.freeze({
    schema: "efs-portable-maintenance-fault-attempt-v1",
    variant,
    kind,
    ordinal,
    injected,
    metrics,
    ...(resultCounters === undefined ? {} : { resultCounters }),
  });
}

/** Resume and verify one selected maintenance operation after host restart/eviction. */
export async function verifyPortableMaintenanceFaultRecovery(
  adapter: FilesystemSQLiteDriver,
  variant: PortableMaintenanceFaultVariant,
): Promise<Readonly<Record<string, number>>> {
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    storage: STORAGE,
    clock: () => 10,
  });
  try {
    const counters =
      variant === "snapshot"
        ? snapshotCounters(await driveSnapshot(filesystem))
        : collectionCounters(await driveCollection(filesystem, variant));
    await verifyUsage(filesystem);
    invariant(
      (await filesystem.readFile(variant === "snapshot" ? "/main" : "/kept", {
        encoding: "utf8",
      })) ===
        (variant === "snapshot"
          ? "main-value"
          : variant === "abandoned"
            ? "abandoned-kept-value"
            : "reachable-value"),
      "recovery changed reachable bytes",
    );
    return counters;
  } finally {
    await filesystem.close();
  }
}

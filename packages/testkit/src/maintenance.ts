import { EphemeralFS } from "@ephemeralai/fs";
import type {
  FilesystemSQLiteDriver,
  SqliteValue,
} from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";

export const PORTABLE_MAINTENANCE_CASE_IDS = Object.freeze([
  "maintenance-snapshot-restart",
  "maintenance-gc-root-reconciliation",
  "maintenance-corruption-no-sweep",
  "maintenance-quota-rollback",
  "maintenance-resource-envelopes",
] as const);
export type PortableMaintenanceCaseId = (typeof PORTABLE_MAINTENANCE_CASE_IDS)[number];
export interface PortableMaintenanceCaseResult {
  readonly id: PortableMaintenanceCaseId;
  readonly status: "passed";
}

const PORTABLE_MAINTENANCE_STORAGE = Object.freeze({
  maxGcBatchSize: 2,
  maxQueryBatchSize: 8,
  maxManagedPayloadBytes: 4 * 1024 * 1024,
  maxMaintenanceBytes: 1024 * 1024,
  maintenanceReserveBytes: 1024 * 1024,
});
const PORTABLE_MAINTENANCE_RUNTIME = Object.freeze({
  maxQueryBatchBytes: 256 * 1024,
});

type CountRow = Readonly<Record<string, SqliteValue>> & {
  readonly objects: number;
  readonly roots: number;
  readonly nodes: number;
};

type NodeRow = Readonly<Record<string, SqliteValue>> & {
  readonly hash: Uint8Array;
  readonly encoded: Uint8Array;
};

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable maintenance conformance: ${message}`);
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  for (let index = 0; index < left.byteLength; index += 1)
    if (left[index] !== right[index]) return false;
  return true;
}

function content(length: number, seed: number): Uint8Array {
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

function byteStream(bytes: Uint8Array): ReadableStream<Uint8Array> {
  let offset = 0;
  return new ReadableStream<Uint8Array>({
    pull(controller) {
      if (offset >= bytes.byteLength) {
        controller.close();
        return;
      }
      const end = Math.min(offset + 64 * 1024, bytes.byteLength);
      controller.enqueue(bytes.slice(offset, end));
      offset = end;
    },
  });
}

function counts(adapter: FilesystemSQLiteDriver): CountRow {
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<CountRow>(
        "SELECT (SELECT count(*) FROM efs_cas_objects) AS objects,(SELECT count(*) FROM efs_manifest_roots) AS roots,(SELECT count(*) FROM efs_manifest_nodes) AS nodes",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(row !== undefined, "content row counts are missing");
  return row;
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
  throw new Error(`portable maintenance conformance: expected ${code} rejection`);
}

async function finishVerification(filesystem: EphemeralFS): Promise<number> {
  let cursor: string | undefined;
  let checked = 0;
  for (let call = 0; call < 20_000; call += 1) {
    const result = await filesystem.maintenance.verify({
      maxEntities: 7,
      ...(cursor === undefined ? {} : { cursor }),
    });
    invariant(result.checkedEntities <= 7, "verification exceeded its entity bound");
    invariant(
      result.peakManagedResidentBytes <=
        filesystem.capabilities.runtime.maxManagedResidentBytes,
      "verification exceeded managed-memory admission",
    );
    checked += result.checkedEntities;
    cursor = result.nextCursor ?? undefined;
    if (result.complete) {
      invariant(cursor === undefined, "complete verification retained a cursor");
      return checked;
    }
  }
  throw new Error("portable maintenance conformance: verification did not finish");
}

/** Shared bounded maintenance, recovery, corruption, quota, and resource suite. */
export async function runMaintenanceConformance(
  factory: ConformanceAdapterFactory,
): Promise<readonly PortableMaintenanceCaseResult[]> {
  const results: PortableMaintenanceCaseResult[] = [];
  const passed = (id: PortableMaintenanceCaseId): void => {
    results.push(Object.freeze({ id, status: "passed" }));
  };

  {
    const fixture = await factory.create({
      label: "portable-maintenance-restart",
      seed: 0x6d61696e,
    });
    let adapter = fixture.adapter;
    let filesystem: EphemeralFS | undefined;
    try {
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      for (let index = 0; index < 96; index += 1)
        await filesystem.writeFile(
          `/retained-${index.toString().padStart(3, "0")}`,
          content(4096, index + 1),
        );
      for (let index = 0; index < 48; index += 1)
        await filesystem.writeFile(
          `/retained-${index.toString().padStart(3, "0")}`,
          content(4096, index + 1000),
        );
      const first = await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
      invariant(first.state === "paused", "snapshot did not expose bounded progress");
      invariant(first.committedBatches === 1, "snapshot ignored maxBatches=1");
      await filesystem.close();
      filesystem = undefined;
      adapter.close();
      adapter = await fixture.reopen({ physical: true });
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      let snapshot = first;
      for (let call = 0; snapshot.state !== "complete" && call < 20_000; call += 1)
        snapshot = await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
      invariant(snapshot.state === "complete", "snapshot did not resume after reopen");
      invariant(snapshot.objectCount > 0, "snapshot lost stored objects");
      invariant(
        snapshot.reclaimablePayloadBytes >= 0,
        "snapshot reported negative reclaimable payload",
      );
      invariant(
        (await finishVerification(filesystem)) > 96,
        "verification skipped rows",
      );
      passed("maintenance-snapshot-restart");

      const branch = await filesystem.branches.create("portable-maintenance-branch");
      await branch.writeFile("/branch-retained", "branch-value");
      await filesystem.writeFile("/remove-during-gc", "remove-me");
      let collection = await filesystem.maintenance.collectGarbage({
        runId: "portable-maintenance-gc",
        maxBatches: 1,
      });
      invariant(collection.state === "paused", "collection did not pause boundedly");
      await filesystem.writeFile("/added-during-gc", "new-root");
      await filesystem.unlink("/remove-during-gc");
      await filesystem.close();
      filesystem = undefined;
      adapter.close();
      adapter = await fixture.reopen({ physical: true });
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      for (let call = 0; collection.state !== "complete" && call < 20_000; call += 1)
        collection = await filesystem.maintenance.collectGarbage({
          runId: "portable-maintenance-gc",
          maxBatches: 1,
        });
      invariant(
        collection.state === "complete",
        "collection did not resume after reopen",
      );
      invariant(
        (await filesystem.readFile("/added-during-gc", { encoding: "utf8" })) ===
          "new-root",
        "root added during marking was swept",
      );
      const reopenedBranch = await filesystem.branches.open(
        "portable-maintenance-branch",
      );
      invariant(
        (await reopenedBranch.readFile("/branch-retained", { encoding: "utf8" })) ===
          "branch-value",
        "active branch root was swept",
      );
      await reopenedBranch.discard();
      await reopenedBranch.close();
      await expectCode(filesystem.stat("/remove-during-gc"), "ENOENT");
      invariant(
        (await finishVerification(filesystem)) > 0,
        "post-GC verification empty",
      );
      passed("maintenance-gc-root-reconciliation");
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

  {
    const fixture = await factory.create({
      label: "portable-maintenance-corruption",
      seed: 0xc011ec7,
    });
    let adapter = fixture.adapter;
    let filesystem: EphemeralFS | undefined;
    try {
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      await filesystem.writeFile("/corrupt", content(64 * 1024, 0xc0ffee));
      await filesystem.close();
      filesystem = undefined;
      const before = counts(adapter);
      const node = adapter.transaction(
        "read",
        (tx) =>
          tx.all<NodeRow>(
            "SELECT n.hash,n.encoded FROM efs_inodes i JOIN efs_manifest_roots r ON r.hash=i.manifest_hash JOIN efs_manifest_nodes n ON n.hash=r.root_node_hash WHERE i.id=(SELECT inode_id FROM efs_entries WHERE name='corrupt')",
            [],
            { maxRows: 1, maxBytes: 32 * 1024 },
          )[0],
      );
      invariant(node !== undefined, "corruption fixture node is missing");
      const corrupted = node.encoded.slice();
      corrupted[0] = corrupted[0]! ^ 0xff;
      adapter.transaction("write", (tx) => {
        tx.run("UPDATE efs_manifest_nodes SET encoded=? WHERE hash=?", [
          corrupted,
          node.hash,
        ]);
      });
      adapter.close();
      adapter = await fixture.reopen({ physical: true });
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      let rejected = false;
      try {
        for (let call = 0; call < 20_000; call += 1) {
          const result = await filesystem.maintenance.collectGarbage({
            runId: "portable-corruption-gc",
            maxBatches: 1,
          });
          if (result.state === "complete") break;
        }
      } catch {
        rejected = true;
      }
      invariant(rejected, "reachable manifest corruption did not stop collection");
      invariant(
        JSON.stringify(counts(adapter)) === JSON.stringify(before),
        "corruption failure swept content",
      );
      let readRejected = false;
      try {
        await filesystem.readFile("/corrupt");
      } catch {
        readRejected = true;
      }
      invariant(readRejected, "corrupt reachable content was returned");
      adapter.transaction("write", (tx) => {
        tx.run("UPDATE efs_manifest_nodes SET encoded=? WHERE hash=?", [
          node.encoded,
          node.hash,
        ]);
      });
      await filesystem.close();
      filesystem = undefined;
      adapter.close();
      adapter = await fixture.reopen({ physical: true });
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      let recovered = false;
      for (let call = 0; call < 20_000; call += 1) {
        const result = await filesystem.maintenance.collectGarbage({
          runId: "portable-corruption-gc",
          maxBatches: 1,
        });
        if (result.state === "complete") {
          recovered = true;
          break;
        }
      }
      invariant(recovered, "collection did not recover after corruption repair");
      passed("maintenance-corruption-no-sweep");
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

  {
    const fixture = await factory.create({
      label: "portable-maintenance-quota",
      seed: 0x71756f74,
    });
    let adapter = fixture.adapter;
    let filesystem: EphemeralFS | undefined;
    try {
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: PORTABLE_MAINTENANCE_STORAGE,
        runtime: PORTABLE_MAINTENANCE_RUNTIME,
      });
      const original = content(128 * 1024, 0x1111);
      await filesystem.writeFile("/quota", original);
      const before = await filesystem.maintenance.snapshotStorage();
      const rejectedBytes = content(5 * 1024 * 1024, 0x2222);
      await expectCode(
        filesystem.writeFile("/quota", byteStream(rejectedBytes), {
          maxBytes: rejectedBytes.byteLength,
        }),
        "ENOSPC",
      );
      invariant(
        equalBytes(await filesystem.readFile("/quota"), original),
        "quota rollback changed the old file",
      );
      const after = await filesystem.maintenance.snapshotStorage();
      invariant(
        after.objectCount === before.objectCount &&
          after.manifestRootCount === before.manifestRootCount &&
          after.manifestNodeCount === before.manifestNodeCount &&
          after.storedObjectPayloadBytes === before.storedObjectPayloadBytes &&
          after.storedManifestPayloadBytes === before.storedManifestPayloadBytes,
        "quota rollback changed durable usage",
      );
      invariant(
        (await finishVerification(filesystem)) > 0,
        "quota usage did not verify",
      );
      passed("maintenance-quota-rollback");

      const capabilities = filesystem.capabilities;
      invariant(
        capabilities.effectiveLimits.length > 0 &&
          new Set(capabilities.effectiveLimits.map((limit) => limit.domain)).size === 4,
        "effective limits omit a resource domain",
      );
      for (const limit of capabilities.effectiveLimits)
        invariant(
          Number.isSafeInteger(limit.value) && limit.value > 0,
          `resource limit ${limit.domain}.${limit.name} is not finite`,
        );
      const physical = adapter.physicalStorage?.();
      invariant(
        physical?.mainFileBytes !== undefined && physical.mainFileBytes > 0,
        "adapter omitted physical database usage",
      );
      invariant(
        physical.mainFileBytes <= capabilities.storage.maxPhysicalDatabaseBytes,
        "physical database exceeded its reported ceiling",
      );
      if (physical.walBytes !== undefined)
        invariant(
          physical.walBytes <= capabilities.storage.maxJournalBytes,
          "journal exceeded its reported ceiling",
        );
      const resourceSnapshot = await filesystem.maintenance.snapshotStorage();
      invariant(
        resourceSnapshot.peakManagedResidentBytes <=
          capabilities.runtime.maxManagedResidentBytes,
        "snapshot exceeded its managed-memory ceiling",
      );
      invariant(
        capabilities.adapter.pageMetricsMode === "runtime-size-only"
          ? resourceSnapshot.physical?.freelistBytes === undefined
          : resourceSnapshot.physical?.freelistBytes !== undefined,
        "page metrics were fabricated or omitted for the adapter mode",
      );
      passed("maintenance-resource-envelopes");
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

  return Object.freeze(results);
}

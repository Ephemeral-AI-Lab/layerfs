import { ContentCache } from "../../packages/fs/dist/cache/content-cache.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { buildManifestFromEntries } from "../../packages/fs/dist/manifests/builder.js";
import {
  AdmissionController,
  DEFAULT_BRANCH_CONFIGURATION,
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
  maxPersistedContentObjectBytes,
  persistedWriterProfile,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { CHARGED_ROW_BYTES } from "../../packages/fs/dist/sqlite/usage-repository.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  SqliteBindings,
  SqliteRow,
  TransactionMode,
} from "../../packages/fs/dist/sqlite/driver.js";
import {
  PORTABLE_STORAGE_RUNTIME_LIMITS,
  PORTABLE_STORAGE_STORAGE_LIMITS,
} from "../../packages/testkit/dist/index.js";
import type {
  PortableStagingClosureEvidence,
  PortableStorageInternals,
} from "../../packages/testkit/dist/index.js";

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable storage internals: ${message}`);
}

function mutatedBytes(bytes: Uint8Array): Uint8Array {
  const copy = bytes.slice();
  copy[0] = (copy[0] ?? 0) ^ 0xff;
  return copy;
}

function countedDriver(
  driver: FilesystemSQLiteDriver,
  counter: { value: number },
): FilesystemSQLiteDriver {
  return Object.freeze({
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    ...(driver.hashBytes === undefined
      ? {}
      : { hashBytes: driver.hashBytes.bind(driver) }),
    ...(driver.hashBytesAsync === undefined
      ? {}
      : { hashBytesAsync: driver.hashBytesAsync.bind(driver) }),
    transaction<T>(
      mode: TransactionMode,
      callback: (transaction: FilesystemSQLiteTransaction) => T,
    ): T {
      return driver.transaction(mode, (transaction) =>
        callback({
          scope: transaction.scope,
          run(sql: string, bindings?: SqliteBindings) {
            counter.value += 1;
            return transaction.run(sql, bindings);
          },
          all<Row extends SqliteRow = SqliteRow>(
            sql: string,
            bindings: SqliteBindings,
            budget: { readonly maxRows: number; readonly maxBytes: number },
          ) {
            counter.value += 1;
            return transaction.all<Row>(sql, bindings, budget);
          },
        }),
      );
    },
    close() {},
  });
}

async function runStagingClosure(
  driver: FilesystemSQLiteDriver,
): Promise<PortableStagingClosureEvidence> {
  const storage = constrainStorageLimits(
    PORTABLE_STORAGE_STORAGE_LIMITS,
    driver.capabilities,
  );
  initializeOrValidateSchema(driver, {
    maxManifestEntries: storage.maxManifestEntries,
    maxManifestDepth: storage.maxManifestDepth,
    maxFileBytes: storage.maxFileBytes,
    maxContentObjectBytes: maxPersistedContentObjectBytes(storage),
    writerProfile: persistedWriterProfile(
      DEFAULT_FILESYSTEM_LIMITS,
      storage,
      DEFAULT_BRANCH_CONFIGURATION,
    ),
  });
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const cache = new ContentCache(1, admission);
  const leaseId = "portable-large-stage";
  const nonce = Uint8Array.from({ length: 16 }, (_, index) => index + 1);
  const budget = {
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
  };
  runUnitOfWork(driver, "write", budget, (transaction) =>
    new StagingRepository(transaction, storage).begin({
      leaseId,
      ownerId: "portable-test",
      ownerNonce: nonce,
      now: 1,
      expiresAt: 1_000_000,
    }),
  );
  const total = 100_001 as const;
  const batchSize = 128;
  const sharedBytes = new Uint8Array(8).fill(23);
  const sharedHash = sha256(sharedBytes);
  runUnitOfWork(driver, "write", budget, (transaction) => {
    new ContentRepository(transaction, storage).putObject(sharedHash, sharedBytes);
    new StagingRepository(transaction, storage).appendBatch(leaseId, nonce, [
      { kind: "object", hash: sharedHash, size: sharedBytes.length },
    ]);
  });
  for (let start = 0; start < total; start += batchSize) {
    const end = Math.min(total, start + batchSize);
    runUnitOfWork(driver, "write", budget, (transaction) => {
      const staging = new StagingRepository(transaction, storage);
      for (let index = start; index < end; index += 1)
        staging.putEntry(leaseId, index, sharedHash, sharedBytes.length);
    });
  }
  const workspace = {
    writeNode(record: {
      readonly level: number;
      readonly index: number;
      readonly value: { readonly hash: Uint8Array; readonly encoded: Uint8Array };
      readonly child: { readonly span: number; readonly entryCount: number };
    }) {
      runUnitOfWork(driver, "write", budget, (transaction) => {
        const staging = new StagingRepository(transaction, storage);
        new ContentRepository(transaction, storage, cache).putManifestNode(
          record.value.hash,
          record.value.encoded,
        );
        staging.putLevelRecord(
          leaseId,
          record.level,
          record.index,
          record.value.hash,
          record.child.span,
          record.child.entryCount,
        );
        staging.appendBatch(leaseId, nonce, [
          {
            kind: "manifest-node",
            hash: record.value.hash,
            size: record.value.encoded.length,
          },
        ]);
      });
    },
    readLevel(level: number, afterIndex: number, limit: number) {
      return runUnitOfWork(driver, "read", budget, (transaction) =>
        new StagingRepository(transaction, storage)
          .levelRecordsAfter(leaseId, level, afterIndex, limit, 1024 * 1024)
          .map((row) => ({
            index: row.record_index,
            child: {
              hash: row.node_hash,
              span: row.span,
              entryCount: row.entry_count,
            },
          })),
      );
    },
  };
  function* entries() {
    let cursor = -1;
    for (;;) {
      const rows = runUnitOfWork(driver, "read", budget, (transaction) =>
        new StagingRepository(transaction, storage).entriesAfter(
          leaseId,
          cursor,
          batchSize,
          64 * 1024,
        ),
      );
      if (rows.length === 0) return;
      for (const row of rows) {
        cursor = row.entry_index;
        yield { hash: row.object_hash, length: row.length };
      }
    }
  }
  const built = buildManifestFromEntries(
    entries(),
    { minimum: 8, average: 8, maximum: 8 },
    workspace,
    { readBatchRecords: 31, maxDepth: storage.maxManifestDepth },
  );
  const reconciliationCounter = { value: 0 };
  const counted = countedDriver(driver, reconciliationCounter);
  const certificate = runUnitOfWork(counted, "write", budget, (transaction) => {
    const content = new ContentRepository(transaction, storage, cache);
    content.putManifestRoot(built.rootHash, built.root);
    const staging = new StagingRepository(transaction, storage, cache);
    staging.appendBatch(leaseId, nonce, [
      { kind: "manifest-root", hash: built.rootHash, size: built.root.length },
    ]);
    staging.beginReconciliation(leaseId, nonce, built.rootHash);
    return { ...staging.snapshot(leaseId, nonce), manifestHash: built.rootHash };
  });
  let complete = false;
  while (!complete)
    complete = runUnitOfWork(counted, "write", budget, (transaction) =>
      new StagingRepository(transaction, storage, cache).reconcileBatch(
        leaseId,
        nonce,
        storage.maxQueryBatchSize,
      ),
    ).complete;
  runUnitOfWork(counted, "write", budget, (transaction) =>
    new StagingRepository(transaction, storage).seal(certificate),
  );

  const finalCounter = { value: 0 };
  const finalCounted = countedDriver(driver, finalCounter);
  runUnitOfWork(finalCounted, "read", budget, (transaction) =>
    new StagingRepository(transaction, storage).validateSealed(certificate, 2),
  );

  const invalidCertificates = [
    { ...certificate, leaseId: `${certificate.leaseId}-wrong` },
    { ...certificate, ownerNonce: mutatedBytes(certificate.ownerNonce) },
    { ...certificate, manifestHash: mutatedBytes(certificate.manifestHash) },
    { ...certificate, chainDigest: mutatedBytes(certificate.chainDigest) },
    { ...certificate, chainFold: mutatedBytes(certificate.chainFold) },
    { ...certificate, objectCount: certificate.objectCount + 1 },
    { ...certificate, objectBytes: certificate.objectBytes + 1 },
    { ...certificate, nodeCount: certificate.nodeCount + 1 },
    { ...certificate, nodeBytes: certificate.nodeBytes + 1 },
    { ...certificate, membershipCount: certificate.membershipCount + 1 },
  ];
  let certificateFieldsRejected = 0;
  for (const invalid of invalidCertificates) {
    try {
      runUnitOfWork(driver, "read", budget, (transaction) =>
        new StagingRepository(transaction, storage).validateSealed(invalid, 2),
      );
    } catch {
      certificateFieldsRejected += 1;
    }
  }

  let sealedMembershipMutationsRejected = 0;
  for (const table of ["efs_lease_objects", "efs_lease_staged_manifests"] as const) {
    try {
      driver.transaction("write", (transaction) =>
        transaction.run(`DELETE FROM ${table} WHERE lease_id=?`, [leaseId]),
      );
    } catch {
      sealedMembershipMutationsRejected += 1;
    }
  }

  const metadata = driver.transaction(
    "read",
    (transaction) =>
      transaction.all<{
        readonly charged_metadata_bytes: number;
        readonly entries: number;
      }>(
        "SELECT charged_metadata_bytes,(SELECT count(*) FROM efs_staging_entries WHERE lease_id=?) entries FROM efs_usage",
        [leaseId],
        { maxRows: 1, maxBytes: 128 },
      )[0],
  );
  invariant(built.entryCount === total, "manifest entry count changed");
  invariant(metadata?.entries === total, "durable staging entry count changed");
  invariant(
    (metadata?.charged_metadata_bytes ?? 0) >= (3 + total) * CHARGED_ROW_BYTES,
    "staging metadata accounting is incomplete",
  );

  return Object.freeze({
    schema: "efs-portable-staging-closure-v1",
    manifestEntries: total,
    uniqueClosureMembers: certificate.membershipCount,
    reconciliationStatements: reconciliationCounter.value,
    finalValidationStatements: finalCounter.value as 1,
    certificateFieldsRejected,
    sealedMembershipMutationsRejected,
  });
}

const crashLeaseId = "portable-staging-crash";
const crashNonce = Uint8Array.from({ length: 16 }, (_, index) => 0xa0 + index);
const crashBytes = new Uint8Array(32).fill(0x5a);
const crashHash = sha256(crashBytes);

function storageFor(driver: FilesystemSQLiteDriver) {
  return constrainStorageLimits(PORTABLE_STORAGE_STORAGE_LIMITS, driver.capabilities);
}

function initializePortableStorage(driver: FilesystemSQLiteDriver) {
  const storage = storageFor(driver);
  initializeOrValidateSchema(driver, {
    maxManifestEntries: storage.maxManifestEntries,
    maxManifestDepth: storage.maxManifestDepth,
    maxFileBytes: storage.maxFileBytes,
    maxContentObjectBytes: maxPersistedContentObjectBytes(storage),
    writerProfile: persistedWriterProfile(
      DEFAULT_FILESYSTEM_LIMITS,
      storage,
      DEFAULT_BRANCH_CONFIGURATION,
    ),
  });
  return storage;
}

async function stageCrashBatch(
  driver: FilesystemSQLiteDriver,
  batch: number,
): Promise<{ readonly durableEntries: number }> {
  invariant(Number.isInteger(batch) && batch >= 0 && batch < 3, "invalid crash batch");
  const storage = initializePortableStorage(driver);
  const budget = {
    maxRows: storage.maxFinalTransactionRows,
    maxBytes: storage.maxFinalTransactionBytes,
  };
  if (batch === 0)
    runUnitOfWork(driver, "write", budget, (transaction) => {
      const staging = new StagingRepository(transaction, storage);
      staging.begin({
        leaseId: crashLeaseId,
        ownerId: "portable-crash-owner",
        ownerNonce: crashNonce,
        now: 1,
        expiresAt: 100,
      });
      new ContentRepository(transaction, storage).putObject(crashHash, crashBytes);
      staging.appendBatch(crashLeaseId, crashNonce, [
        { kind: "object", hash: crashHash, size: crashBytes.length },
      ]);
    });
  const before = driver.transaction(
    "read",
    (transaction) =>
      transaction.all<{ readonly value: number }>(
        "SELECT count(*) value FROM efs_staging_entries WHERE lease_id=?",
        [crashLeaseId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.value ?? 0,
  );
  invariant(before === batch * 3, `staging crash batch ${batch} lost prior entries`);
  runUnitOfWork(driver, "write", budget, (transaction) => {
    const staging = new StagingRepository(transaction, storage);
    for (let offset = 0; offset < 3; offset += 1) {
      const index = batch * 3 + offset;
      staging.putEntry(crashLeaseId, index, crashHash, crashBytes.length);
    }
  });
  const durableEntries = driver.transaction(
    "read",
    (transaction) =>
      transaction.all<{ readonly value: number }>(
        "SELECT count(*) value FROM efs_staging_entries WHERE lease_id=?",
        [crashLeaseId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.value ?? 0,
  );
  return Object.freeze({ durableEntries });
}

async function recoverStagingCrash(driver: FilesystemSQLiteDriver) {
  initializePortableStorage(driver);
  const filesystem = await EphemeralFS.open({
    database: driver,
    ownsDatabase: false,
    clock: () => 10_000,
    storage: PORTABLE_STORAGE_STORAGE_LIMITS,
    runtime: PORTABLE_STORAGE_RUNTIME_LIMITS,
  });
  let collection = await filesystem.maintenance.collectGarbage({
    runId: "portable-staging-crash-recovery",
    maxBatches: 1,
  });
  for (let call = 0; call < 10_000 && collection.state !== "complete"; call += 1)
    collection = await filesystem.maintenance.collectGarbage({
      runId: "portable-staging-crash-recovery",
      maxBatches: 1,
    });
  invariant(collection.state === "complete", "staging crash recovery did not finish");
  await filesystem.close();
  const row = driver.transaction(
    "read",
    (transaction) =>
      transaction.all<{
        readonly activeLeases: number;
        readonly stagingCertificates: number;
        readonly stagingEntries: number;
        readonly stagingBytes: number;
        readonly ingestReservationBytes: number;
      }>(
        "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) activeLeases,(SELECT count(*) FROM efs_staging_certificates) stagingCertificates,(SELECT count(*) FROM efs_staging_entries) stagingEntries,staging_bytes stagingBytes,ingest_reservation_bytes ingestReservationBytes FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(row !== undefined, "staging crash recovery counters are missing");
  return Object.freeze(row);
}

export const portableStorageInternals: PortableStorageInternals = Object.freeze({
  runStagingClosure,
  stageCrashBatch,
  recoverStagingCrash,
});

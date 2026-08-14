import type { FilesystemSQLiteDriver } from "./driver.js";
import { runUnitOfWork } from "./unit-of-work.js";
import { initializeOrValidateSchema } from "./schema.js";
import { ContentRepository } from "./content-repository.js";
import { NamespaceRepository } from "./namespace-repository.js";
import { BranchRepository } from "./branch-repository.js";
import { StagingRepository } from "./staging-repository.js";
import { MaintenanceRepository } from "./maintenance-repository.js";
import { OverlayRepository } from "./overlay-repository.js";
import { ManifestTreeRepository } from "./manifest-tree-repository.js";
import { ReplicationSessionRepository } from "./replication-repository.js";
import { createReplicationTransferRepository } from "./replication-transfer-repository.js";
import { sha256 } from "../cas/sha256.js";
import type {
  OperationsStorage,
  StorageTransactionPorts,
} from "../operations/storage-ports.js";
import type { ContentCache } from "../cache/content-cache.js";
import type { CowPageBytes } from "../cow/pages.js";
import type { FilesystemLimits, StorageLimits } from "../resources/limits.js";

export function createSqliteOperationsStorage(
  driver: FilesystemSQLiteDriver,
): OperationsStorage {
  const hashBytes = driver.hashBytes ?? sha256;
  return Object.freeze({
    readOnly: driver.readOnly,
    capabilities: Object.freeze({
      ...driver.capabilities,
      journalQuotaPolicy: driver.capabilities.journalQuotaPolicy ?? "runtime-enforced",
      journalSizeLimitIsHard: false,
    }),
    hashBytes,
    ...(driver.hashBytesAsync === undefined
      ? {}
      : { hashBytesAsync: driver.hashBytesAsync }),
    initialize: (options = {}) => initializeOrValidateSchema(driver, options),
    transaction: <T>(
      mode: "read" | "write" | "exclusive",
      budget: { readonly maxRows: number; readonly maxBytes: number },
      callback: (ports: StorageTransactionPorts) => T,
    ): T =>
      runUnitOfWork(driver, mode, budget, (tx) => {
        let transactionLimits: StorageLimits | undefined;
        const limitsFor = (limits: StorageLimits): StorageLimits => {
          if (!transactionLimits) {
            transactionLimits = Object.freeze({ ...limits });
            return transactionLimits;
          }
          for (const name of Object.keys(transactionLimits) as Array<
            keyof StorageLimits
          >)
            if (transactionLimits[name] !== limits[name])
              throw new Error(
                "EINVAL: one SQLite transaction cannot mix storage limit profiles",
              );
          return transactionLimits;
        };
        const ports: StorageTransactionPorts = Object.freeze({
          content: (limits: StorageLimits, cache?: ContentCache) =>
            new ContentRepository(tx, limitsFor(limits), cache, hashBytes),
          manifestTree: (limits: StorageLimits, cache?: ContentCache) =>
            new ManifestTreeRepository(tx, limitsFor(limits), cache, hashBytes),
          namespace: (
            filesystem: FilesystemLimits,
            storage: StorageLimits,
            syscall: string,
          ) => new NamespaceRepository(tx, filesystem, limitsFor(storage), syscall),
          branches: (limits: StorageLimits) =>
            new BranchRepository(tx, limitsFor(limits)),
          staging: (limits: StorageLimits, cache?: ContentCache) =>
            new StagingRepository(
              tx,
              limitsFor(limits),
              cache,
              hashBytes,
              driver.capabilities.maxBindings,
            ),
          maintenance: (limits: StorageLimits) =>
            new MaintenanceRepository(tx, limitsFor(limits)),
          overlay: (limits: StorageLimits, pageBytes: CowPageBytes) =>
            new OverlayRepository(tx, limitsFor(limits), pageBytes),
          replication: (limits?: StorageLimits) =>
            new ReplicationSessionRepository(
              tx,
              hashBytes,
              limits === undefined ? undefined : limitsFor(limits),
            ),
          replicationTransfer: (
            limits: StorageLimits,
            cache?: ContentCache,
            branchDigest?: (branchId: string, generation: number) => string,
          ) =>
            createReplicationTransferRepository(
              tx,
              limitsFor(limits),
              hashBytes,
              driver.capabilities.maxBindings,
              branchDigest,
              cache,
            ),
        });
        return callback(ports);
      }),
    physicalStorage: () => driver.physicalStorage?.() ?? Object.freeze({}),
    checkpoint: (mode: "passive" | "restart" | "truncate" = "passive") =>
      driver.checkpoint?.(mode),
    close: () => driver.close(),
  });
}

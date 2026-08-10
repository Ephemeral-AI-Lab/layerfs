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
import type {
  OperationsStorage,
  StorageTransactionPorts,
} from "../operations/storage-ports.js";
import type { ContentCache } from "../cache/content-cache.js";
import type { CowPageBytes } from "../cow/pages.js";
import type { FilesystemLimits, StorageLimits } from "../resources/limits.js";
import { UsageRepository } from "./usage-repository.js";

export function createSqliteOperationsStorage(
  driver: FilesystemSQLiteDriver,
): OperationsStorage {
  return Object.freeze({
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    initialize: (options = {}) => initializeOrValidateSchema(driver, options),
    transaction: <T>(
      mode: "read" | "write" | "exclusive",
      budget: { readonly maxRows: number; readonly maxBytes: number },
      callback: (ports: StorageTransactionPorts) => T,
    ): T =>
      runUnitOfWork(driver, mode, budget, (tx) => {
        let transactionLimits: StorageLimits | undefined;
        const limitsFor = (limits: StorageLimits): StorageLimits => {
          transactionLimits ??= limits;
          return limits;
        };
        const ports: StorageTransactionPorts = Object.freeze({
          content: (limits: StorageLimits, cache?: ContentCache) =>
            new ContentRepository(tx, limitsFor(limits), cache),
          manifestTree: (limits: StorageLimits, cache?: ContentCache) =>
            new ManifestTreeRepository(tx, limitsFor(limits), cache),
          namespace: (
            filesystem: FilesystemLimits,
            storage: StorageLimits,
            syscall: string,
          ) => new NamespaceRepository(tx, filesystem, limitsFor(storage), syscall),
          branches: (limits: StorageLimits) =>
            new BranchRepository(tx, limitsFor(limits)),
          staging: (limits: StorageLimits) =>
            new StagingRepository(tx, limitsFor(limits)),
          maintenance: (limits: StorageLimits) =>
            new MaintenanceRepository(tx, limitsFor(limits)),
          overlay: (limits: StorageLimits, pageBytes: CowPageBytes) =>
            new OverlayRepository(tx, limitsFor(limits), pageBytes),
        });
        const result = callback(ports);
        if (mode !== "read" && transactionLimits)
          new UsageRepository(tx, transactionLimits).reconcileDerivedUsage();
        return result;
      }),
    physicalStorage: () => driver.physicalStorage?.() ?? Object.freeze({}),
    checkpoint: (mode: "passive" | "restart" | "truncate" = "passive") =>
      driver.checkpoint?.(mode),
    close: () => driver.close(),
  });
}

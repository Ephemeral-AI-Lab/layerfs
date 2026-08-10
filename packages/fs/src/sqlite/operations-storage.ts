import type { FilesystemSQLiteDriver } from "./driver.js";
import { runUnitOfWork } from "./unit-of-work.js";
import { initializeOrValidateSchema } from "./schema.js";
import { ContentRepository } from "./content-repository.js";
import { NamespaceRepository } from "./namespace-repository.js";
import { BranchRepository } from "./branch-repository.js";
import { StagingRepository } from "./staging-repository.js";
import { MaintenanceRepository } from "./maintenance-repository.js";
import { OverlayRepository } from "./overlay-repository.js";
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
        const ports: StorageTransactionPorts = Object.freeze({
          content: (limits: StorageLimits, cache?: ContentCache) =>
            new ContentRepository(tx, limits, cache),
          namespace: (
            filesystem: FilesystemLimits,
            storage: StorageLimits,
            syscall: string,
          ) => new NamespaceRepository(tx, filesystem, storage, syscall),
          branches: (limits: StorageLimits) => new BranchRepository(tx, limits),
          staging: (limits: StorageLimits) => new StagingRepository(tx, limits),
          maintenance: (limits: StorageLimits) => new MaintenanceRepository(tx, limits),
          overlay: (limits: StorageLimits, pageBytes: CowPageBytes) =>
            new OverlayRepository(tx, limits, pageBytes),
        });
        return callback(ports);
      }),
    physicalStorage: () => driver.physicalStorage?.() ?? Object.freeze({}),
    checkpoint: (mode: "passive" | "restart" | "truncate" = "passive") =>
      driver.checkpoint?.(mode),
    close: () => driver.close(),
  });
}

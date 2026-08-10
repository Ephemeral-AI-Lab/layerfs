import type { StorageFormatOptions } from "../filesystem/types.js";
import {
  createNodeVfsOperationsBridge,
  type NodeVfsFilesystemBridge,
  type SyncPreparedContent,
} from "../operations/node-vfs-bridge.js";
import type { FilesystemLimits, RuntimeLimits, StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteDriver } from "../sqlite/driver.js";
import { createSqliteOperationsStorage } from "../sqlite/operations-storage.js";

/** Public composition-root options for the synchronous Node VFS bridge. */
export interface CreateNodeVfsBridgeOptions {
  readonly database: FilesystemSQLiteDriver;
  readonly filesystem?: Partial<FilesystemLimits>;
  readonly storage?: Partial<StorageLimits>;
  readonly runtime?: Partial<RuntimeLimits>;
  readonly format?: StorageFormatOptions;
  readonly clock?: () => number;
}

/** Compose the public bridge with the private SQLite storage implementation. */
export function createNodeVfsBridge(options: CreateNodeVfsBridgeOptions): NodeVfsFilesystemBridge {
  const { database, ...operationOptions } = options;
  return createNodeVfsOperationsBridge({
    ...operationOptions,
    port: createSqliteOperationsStorage(database),
  });
}

export type { NodeVfsFilesystemBridge, SyncPreparedContent };

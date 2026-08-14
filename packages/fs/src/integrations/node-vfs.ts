import type {
  EphemeralFilesystem,
  OpenFilesystemOptions,
  StorageFormatOptions,
} from "../filesystem/types.js";
import type { EphemeralFS as PublicEphemeralFS } from "../filesystem/ephemeral-fs.js";
import { EphemeralFS as OperationsFilesystem } from "../operations/filesystem.js";
import {
  createNodeVfsOperationsBridge,
  type NodeVfsFilesystemBridge,
  type NodeVfsManagedSlab,
  type NodeVfsPreparedContent,
  type NodeVfsPinnedReadBridge,
  type SynchronousContentSource,
} from "../operations/node-vfs-bridge.js";
import type {
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "../resources/limits.js";
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

export interface OpenNodeVfsBridgeResult {
  /** Async view matching the bridge: main, or the selected private branch. */
  readonly filesystem: EphemeralFilesystem;
  /** Owner of the shared cache, admission controller, and all branch handles. */
  readonly runtime: PublicEphemeralFS;
  readonly bridge: NodeVfsFilesystemBridge;
}
export interface OpenNodeVfsBridgeOptions extends OpenFilesystemOptions {
  readonly branchId?: string;
}

/**
 * Open the portable filesystem and its synchronous bridge as one core instance.
 * This is the production Node VFS composition root: both views share limits,
 * caches, concurrency, and the aggregate admission controller.
 */
export async function openNodeVfsBridge(
  options: OpenNodeVfsBridgeOptions,
): Promise<OpenNodeVfsBridgeResult> {
  const { branchId, ...filesystemOptions } = options;
  const filesystem = await OperationsFilesystem.open(
    filesystemOptions,
    createSqliteOperationsStorage(filesystemOptions.database),
  );
  try {
    const bridge = filesystem.createNodeVfsBridge(branchId);
    const view =
      branchId === undefined ? filesystem : await filesystem.branches.open(branchId);
    return Object.freeze({
      filesystem: view,
      runtime: filesystem as unknown as PublicEphemeralFS,
      bridge,
    });
  } catch (error) {
    await filesystem.close();
    throw error;
  }
}

/** Compose the public bridge with the private SQLite storage implementation. */
export function createNodeVfsBridge(
  options: CreateNodeVfsBridgeOptions,
): NodeVfsFilesystemBridge {
  const { database, ...operationOptions } = options;
  return createNodeVfsOperationsBridge({
    ...operationOptions,
    port: createSqliteOperationsStorage(database),
  });
}

export type {
  NodeVfsFilesystemBridge,
  NodeVfsManagedSlab,
  NodeVfsPreparedContent,
  NodeVfsPinnedReadBridge,
  SynchronousContentSource,
};

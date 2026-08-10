import { EphemeralFS as FilesystemOperations } from "../operations/filesystem.js";
import { createSqliteOperationsStorage } from "../sqlite/operations-storage.js";
import type { EphemeralFilesystem, OpenFilesystemOptions } from "./types.js";

/** Public composition root: injects the private SQLite storage-port adapter. */
export class EphemeralFS {
  static open(options: OpenFilesystemOptions): Promise<EphemeralFilesystem> {
    return FilesystemOperations.open(options, createSqliteOperationsStorage(options.database));
  }
}

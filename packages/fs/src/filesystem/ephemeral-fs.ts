import { EphemeralFS as FilesystemOperations } from "../operations/filesystem.js";
import { createSqliteOperationsStorage } from "../sqlite/operations-storage.js";
import type { OpenFilesystemOptions } from "./types.js";

/** Public composition root: injects the private SQLite storage-port adapter. */
export class EphemeralFS {
  private constructor() {}
  static open(options: OpenFilesystemOptions): Promise<EphemeralFS> {
    return FilesystemOperations.open(
      options,
      createSqliteOperationsStorage(options.database),
    ) as Promise<EphemeralFS>;
  }
}

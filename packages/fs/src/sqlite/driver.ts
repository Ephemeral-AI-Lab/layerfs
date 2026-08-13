export type SqliteValue = null | string | number | Uint8Array;
export type SqliteBindings = readonly SqliteValue[];
export type SqliteRow = Readonly<Record<string, SqliteValue>>;
export interface SqliteRunResult {
  readonly changes: number;
  /** Includes trigger/FK side effects when the adapter can report them. */
  readonly totalChanges?: number;
  readonly lastInsertRowid?: number;
}
export interface QueryBudget {
  readonly maxRows: number;
  readonly maxBytes: number;
}
export interface FilesystemSQLiteTransaction {
  readonly scope: symbol;
  run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
  all<Row extends SqliteRow = SqliteRow>(
    sql: string,
    bindings: SqliteBindings,
    budget: QueryBudget,
  ): readonly Row[];
}
export type TransactionMode = "read" | "write" | "exclusive";
export type SQLiteSchemaIdentityMode = "sqlite-header" | "durable-table";
export type SQLitePageMetricsMode = "sqlite-pragma" | "runtime-size-only";
export interface SQLiteDriverCapabilities {
  readonly maxBlobBytes: number;
  readonly maxBindings: number;
  readonly durability: "acknowledged" | "relaxed-test";
  readonly journalMode: "wal" | "rollback" | "runtime-managed";
  readonly memoryPolicy: "configured" | "runtime-managed";
  readonly cacheTargetBytes?: number;
  readonly mmapLimitBytes?: number;
  readonly maxPhysicalDatabaseBytes: number;
  readonly maxJournalBytes: number;
  readonly physicalQuotaPolicy: "driver-enforced" | "runtime-enforced";
  readonly journalQuotaPolicy?: "checkpoint-backpressure" | "runtime-enforced";
  readonly journalSizeLimitIsHard?: false;
  /**
   * Selects the durable schema identity representation. Omission preserves the
   * native SQLite-header contract for existing third-party adapters.
   */
  readonly schemaIdentityMode?: SQLiteSchemaIdentityMode;
  /** Selects native page/freelist PRAGMAs or a runtime-owned size-only counter. */
  readonly pageMetricsMode?: SQLitePageMetricsMode;
}
export interface SQLitePhysicalStorage {
  readonly mainFileBytes?: number;
  readonly walBytes?: number;
}
export interface SQLiteCheckpointResult {
  readonly mode: "passive" | "restart" | "truncate";
  readonly busy: number;
  readonly logFrames: number;
  readonly checkpointedFrames: number;
  readonly walBytes?: number;
}
export type SqliteHashFunction = (bytes: Uint8Array) => Uint8Array;
export type SqliteAsyncHashFunction = (bytes: Uint8Array) => Promise<Uint8Array>;
export interface FilesystemSQLiteDriver {
  readonly kind: "sqlite";
  readonly readOnly: boolean;
  readonly capabilities: SQLiteDriverCapabilities;
  /**
   * Optional synchronous SHA-256 hasher. When the host adapter provides one
   * (node:crypto on Node), the operations storage uses it for content
   * hashing and verification; hosts without a synchronous native hasher
   * fall back to the byte-identical pure-JS implementation.
   */
  readonly hashBytes?: SqliteHashFunction;
  /**
   * Optional asynchronous SHA-256 hasher for write-path chunk hashing
   * (WebCrypto on workerd). When present, the streaming write pipeline hashes
   * its chunk batches concurrently with bounded parallelism; digests are
   * byte-identical to the synchronous implementations.
   */
  readonly hashBytesAsync?: SqliteAsyncHashFunction;
  transaction<T>(
    mode: TransactionMode,
    callback: (tx: FilesystemSQLiteTransaction) => T,
  ): T;
  physicalStorage?(): SQLitePhysicalStorage;
  checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
  close(): void | Promise<void>;
}

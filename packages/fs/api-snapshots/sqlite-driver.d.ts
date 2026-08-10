/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./sqlite-driver; entry: packages/fs/dist/sqlite/driver.d.ts */

/* export: FilesystemSQLiteDriver; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export interface FilesystemSQLiteDriver {
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    physicalStorage?(): SQLitePhysicalStorage;
    checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
    close(): void | Promise<void>;
}

/* export: FilesystemSQLiteTransaction; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export interface FilesystemSQLiteTransaction {
    readonly scope: symbol;
    run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
    all<Row extends SqliteRow = SqliteRow>(sql: string, bindings: SqliteBindings, budget: QueryBudget): readonly Row[];
}

/* export: QueryBudget; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export interface QueryBudget {
    readonly maxRows: number;
    readonly maxBytes: number;
}

/* export: SqliteBindings; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export type SqliteBindings = readonly SqliteValue[];

/* export: SQLiteCheckpointResult; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export interface SQLiteCheckpointResult {
    readonly mode: "passive" | "restart" | "truncate";
    readonly busy: number;
    readonly logFrames: number;
    readonly checkpointedFrames: number;
    readonly walBytes?: number;
}

/* export: SQLiteDriverCapabilities; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
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
    readonly journalQuotaPolicy: "checkpoint-backpressure" | "runtime-enforced";
    readonly journalSizeLimitIsHard: false;
}

/* export: SQLitePhysicalStorage; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export interface SQLitePhysicalStorage {
    readonly mainFileBytes?: number;
    readonly walBytes?: number;
}

/* export: SqliteRow; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export type SqliteRow = Readonly<Record<string, SqliteValue>>;

/* export: SqliteRunResult; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export interface SqliteRunResult {
    readonly changes: number;
    readonly lastInsertRowid?: number;
}

/* export: SqliteValue; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export type SqliteValue = null | string | number | Uint8Array;

/* export: TransactionMode; kinds: type */
/* source: packages/fs/dist/sqlite/driver.d.ts */
export type TransactionMode = "read" | "write" | "exclusive";

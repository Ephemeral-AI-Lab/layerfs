/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-sqlite-cloudflare; subpath: .; entry: packages/sqlite-cloudflare/dist/index.d.ts */

/* ===== packages/fs/dist/sqlite/driver.d.ts ===== */
export type SqliteValue = null | string | number | Uint8Array;
export type SqliteBindings = readonly SqliteValue[];
export type SqliteRow = Readonly<Record<string, SqliteValue>>;
export interface SqliteRunResult {
    readonly changes: number;
    readonly lastInsertRowid?: number;
}
export interface QueryBudget {
    readonly maxRows: number;
    readonly maxBytes: number;
}
export interface FilesystemSQLiteTransaction {
    readonly scope: symbol;
    run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
    all<Row extends SqliteRow = SqliteRow>(sql: string, bindings: SqliteBindings, budget: QueryBudget): readonly Row[];
}
export type TransactionMode = "read" | "write" | "exclusive";
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
}
export interface FilesystemSQLiteDriver {
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void | Promise<void>;
}

/* ===== packages/sqlite-cloudflare/dist/index.d.ts ===== */
import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SQLiteDriverCapabilities, TransactionMode } from "@ephemeralai/fs/sqlite-driver";
export interface DurableObjectSqlCursor<Row extends Record<string, unknown> = Record<string, unknown>> extends Iterable<Row> {
    readonly rowsRead: number;
    readonly rowsWritten: number;
    toArray(): Row[];
}
export interface DurableObjectSqlStorage {
    exec<Row extends Record<string, unknown> = Record<string, unknown>>(query: string, ...bindings: unknown[]): DurableObjectSqlCursor<Row>;
    readonly databaseSize: number;
}
export interface DurableObjectSQLiteStorage {
    readonly sql: DurableObjectSqlStorage;
    transactionSync<T>(callback: () => T): T;
}
export interface OpenCloudflareSqliteOptions {
    readonly storage: DurableObjectSQLiteStorage;
    readonly maxManagedPayloadBytes?: number;
    readonly maxJournalBytes?: number;
}
export declare class CloudflareSQLiteDriver implements FilesystemSQLiteDriver {
    #private;
    readonly kind: "sqlite";
    readonly readOnly = false;
    readonly capabilities: SQLiteDriverCapabilities;
    constructor(options: OpenCloudflareSqliteOptions);
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void;
    get databaseSize(): number;
}
export declare function openCloudflareSqlite(options: OpenCloudflareSqliteOptions): Promise<CloudflareSQLiteDriver>;

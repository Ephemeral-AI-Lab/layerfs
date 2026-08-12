/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-sqlite-cloudflare; subpath: .; entry: packages/sqlite-cloudflare/dist/index.d.ts */

/* ===== packages/fs/dist/sqlite/driver.d.ts ===== */
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
    readonly journalQuotaPolicy?: "checkpoint-backpressure" | "runtime-enforced";
    readonly journalSizeLimitIsHard?: false;
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
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    physicalStorage?(): SQLitePhysicalStorage;
    checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
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
    /**
     * WebCrypto SHA-256 for the streaming write pipeline. Digest output is
     * byte-identical to the pure-JS fallback (`cas/sha256.ts`), so golden
     * vectors and workerd parity are unaffected.
     */
    readonly hashBytesAsync: (bytes: Uint8Array) => Promise<Uint8Array>;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void;
    get databaseSize(): number;
}
export declare function openCloudflareSqlite(options: OpenCloudflareSqliteOptions): Promise<CloudflareSQLiteDriver>;

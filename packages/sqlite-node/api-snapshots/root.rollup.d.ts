/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-sqlite-node; subpath: .; entry: packages/sqlite-node/dist/index.d.ts */

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
export interface FilesystemSQLiteDriver {
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    physicalStorage?(): SQLitePhysicalStorage;
    checkpoint?(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
    close(): void | Promise<void>;
}

/* ===== packages/sqlite-node/dist/index.d.ts ===== */
import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SQLiteDriverCapabilities, SQLiteCheckpointResult, SQLitePhysicalStorage, TransactionMode } from "@ephemeralai/fs/sqlite-driver";
export interface OpenNodeSqliteOptions {
    readonly filename: string;
    readonly readOnly?: boolean;
    readonly create?: boolean;
    readonly busyTimeoutMs?: number;
    readonly durability?: "acknowledged" | "relaxed-test";
    readonly cacheTargetBytes?: number;
    readonly mmapLimitBytes?: number;
    readonly maxPhysicalDatabaseBytes?: number;
    readonly maxJournalBytes?: number;
}
export declare class NodeSQLiteDriver implements FilesystemSQLiteDriver {
    #private;
    readonly kind: "sqlite";
    readonly readOnly: boolean;
    readonly capabilities: SQLiteDriverCapabilities & {
        readonly journalQuotaPolicy: "checkpoint-backpressure";
        readonly journalSizeLimitIsHard: false;
    };
    constructor(options: OpenNodeSqliteOptions);
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void;
    physicalStorage(): SQLitePhysicalStorage;
    checkpoint(mode?: "passive" | "restart" | "truncate"): SQLiteCheckpointResult;
}
export declare function openNodeSqlite(options: OpenNodeSqliteOptions): Promise<NodeSQLiteDriver>;

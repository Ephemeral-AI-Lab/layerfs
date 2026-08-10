/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-sqlite-node; subpath: .; entry: packages/sqlite-node/dist/index.d.ts */

/* export: NodeSQLiteDriver; kinds: value,type */
/* source: packages/sqlite-node/dist/index.d.ts */
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

/* export: openNodeSqlite; kinds: value */
/* source: packages/sqlite-node/dist/index.d.ts */
export declare function openNodeSqlite(options: OpenNodeSqliteOptions): Promise<NodeSQLiteDriver>;

/* export: OpenNodeSqliteOptions; kinds: type */
/* source: packages/sqlite-node/dist/index.d.ts */
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

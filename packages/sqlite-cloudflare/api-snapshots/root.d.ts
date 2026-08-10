/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-sqlite-cloudflare; subpath: .; entry: packages/sqlite-cloudflare/dist/index.d.ts */

/* export: CloudflareSQLiteDriver; kinds: value,type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
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

/* export: DurableObjectSqlCursor; kinds: type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export interface DurableObjectSqlCursor<Row extends Record<string, unknown> = Record<string, unknown>> extends Iterable<Row> {
    readonly rowsRead: number;
    readonly rowsWritten: number;
    toArray(): Row[];
}

/* export: DurableObjectSQLiteStorage; kinds: type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export interface DurableObjectSQLiteStorage {
    readonly sql: DurableObjectSqlStorage;
    transactionSync<T>(callback: () => T): T;
}

/* export: DurableObjectSqlStorage; kinds: type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export interface DurableObjectSqlStorage {
    exec<Row extends Record<string, unknown> = Record<string, unknown>>(query: string, ...bindings: unknown[]): DurableObjectSqlCursor<Row>;
    readonly databaseSize: number;
}

/* export: openCloudflareSqlite; kinds: value */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export declare function openCloudflareSqlite(options: OpenCloudflareSqliteOptions): Promise<CloudflareSQLiteDriver>;

/* export: OpenCloudflareSqliteOptions; kinds: type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export interface OpenCloudflareSqliteOptions {
    readonly storage: DurableObjectSQLiteStorage;
    readonly maxManagedPayloadBytes?: number;
    readonly maxJournalBytes?: number;
}

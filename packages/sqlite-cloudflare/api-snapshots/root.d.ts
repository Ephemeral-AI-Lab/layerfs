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
    /**
     * WebCrypto SHA-256 for the streaming write pipeline. Digest output is
     * byte-identical to the pure-JS fallback (`cas/sha256.ts`), so golden
     * vectors and workerd parity are unaffected.
     */
    readonly hashBytesAsync: (bytes: Uint8Array) => Promise<Uint8Array>;
    transaction<T>(mode: TransactionMode, callback: (tx: FilesystemSQLiteTransaction) => T): T;
    close(): void;
    get databaseSize(): number;
    physicalStorage(): {
        readonly mainFileBytes: number;
    };
}

/* export: CloudflareSQLiteError; kinds: value,type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export declare class CloudflareSQLiteError extends Error {
    readonly name: "CloudflareSQLiteError";
    readonly category: CloudflareSQLiteErrorCategory;
    readonly code: string;
    constructor(category: CloudflareSQLiteErrorCategory, code: string, message: string, cause: unknown);
}

/* export: CloudflareSQLiteErrorCategory; kinds: type */
/* source: packages/sqlite-cloudflare/dist/index.d.ts */
export type CloudflareSQLiteErrorCategory = "constraint" | "busy" | "corruption" | "resource-limit";

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
    /** Conservative byte ceiling for the configured Durable Object plan. */
    readonly maxPhysicalDatabaseBytes?: number;
    readonly maxJournalBytes?: number;
}

/* Generated reachable public declaration rollup. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-testkit; subpath: .; entry: packages/testkit/dist/index.d.ts */

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

/* ===== packages/testkit/dist/index.d.ts ===== */
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
export type ConformanceCapability = "read-only-reopen" | "second-connection" | "schema-fixtures" | "fault-injection" | "garbage-collection" | "physical-reopen" | "crash-recovery" | "ownership";
export interface ConformanceFaultController {
    arm(point: string, occurrence?: number): void;
    clear(): void;
}
export interface ConformanceFixtureOptions {
    readonly label?: string;
    readonly seed?: number;
}
export interface ConformanceDatabase {
    readonly adapter: FilesystemSQLiteDriver;
    readonly capabilities: readonly ConformanceCapability[];
    readonly faults?: ConformanceFaultController;
    reopen(options?: {
        readOnly?: boolean;
        physical?: boolean;
    }): Promise<FilesystemSQLiteDriver>;
    openSecondConnection?(): Promise<FilesystemSQLiteDriver>;
    dispose(): Promise<void>;
}
export interface ConformanceAdapterFactory {
    readonly name: string;
    create(options?: ConformanceFixtureOptions): Promise<ConformanceDatabase>;
}
export interface CorrectnessResult {
    readonly schema: "efs-correctness-result-v1";
    readonly commit: string;
    readonly adapter: string;
    readonly driver: string;
    readonly capabilities: Readonly<Record<string, string | number | boolean | null>>;
    readonly limits: Readonly<Record<string, number>>;
    readonly schemaVersion: number;
    readonly formatVersion: string;
    readonly seed: number;
    readonly fixtureDigest: string;
    readonly faultPoint: string | null;
    readonly commands: readonly string[];
    readonly environment: Readonly<Record<string, string>>;
    readonly passed: number;
    readonly failed: number;
    readonly elapsedMs: number;
}
export interface BenchmarkResult {
    readonly schema: "efs-benchmark-result-v1";
    readonly benchmark: string;
    readonly commit: string;
    readonly engine: string;
    readonly driver: string;
    readonly fixture: Readonly<{
        name: string;
        sha256: string;
    }>;
    readonly configuration: Readonly<Record<string, unknown>>;
    readonly trials: number;
    readonly latencyMs: Readonly<{
        p50: number;
        p95: number;
        p99: number;
    }>;
    readonly counters: Readonly<Record<string, number>>;
    readonly pass: boolean;
}
export type RecordingEvent = Readonly<{
    type: "create";
    factory: string;
    label: string | null;
    seed: number | null;
}> | Readonly<{
    type: "reopen";
    readOnly: boolean;
    physical: boolean;
}> | Readonly<{
    type: "second-connection";
}> | Readonly<{
    type: "dispose";
}>;
/** Wraps a real test factory without weakening its restart or connection behavior. */
export declare function createRecordingFactory(factory: ConformanceAdapterFactory, events: RecordingEvent[]): ConformanceAdapterFactory;

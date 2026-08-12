/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-testkit; subpath: .; entry: packages/testkit/dist/index.d.ts */

/* export: BenchmarkResult; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
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

/* export: ConformanceAdapterFactory; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
export interface ConformanceAdapterFactory {
    readonly name: string;
    create(options?: ConformanceFixtureOptions): Promise<ConformanceDatabase>;
}

/* export: ConformanceCapability; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
export type ConformanceCapability = "read-only-reopen" | "second-connection" | "schema-fixtures" | "fault-injection" | "garbage-collection" | "physical-reopen" | "crash-recovery" | "ownership";

/* export: ConformanceDatabase; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
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

/* export: ConformanceFaultController; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
export interface ConformanceFaultController {
    arm(point: string, occurrence?: number): void;
    clear(): void;
}

/* export: ConformanceFixtureOptions; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
export interface ConformanceFixtureOptions {
    readonly label?: string;
    readonly seed?: number;
}

/* export: CorrectnessResult; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
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

/* export: createRecordingFactory; kinds: value */
/* source: packages/testkit/dist/index.d.ts */
/** Wraps a real test factory without weakening its restart or connection behavior. */
export declare function createRecordingFactory(factory: ConformanceAdapterFactory, events: RecordingEvent[]): ConformanceAdapterFactory;

/* export: RecordingEvent; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
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

import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";

export type ConformanceCapability = "read-only-reopen" | "second-connection" | "schema-fixtures" | "fault-injection" | "garbage-collection" | "physical-reopen" | "crash-recovery" | "ownership";
export interface ConformanceFaultController { arm(point: string, occurrence?: number): void; clear(): void }
export interface ConformanceFixtureOptions { readonly label?: string; readonly seed?: number }
export interface ConformanceDatabase {
  readonly adapter: FilesystemSQLiteDriver;
  readonly capabilities: readonly ConformanceCapability[];
  readonly faults?: ConformanceFaultController;
  reopen(options?: { readOnly?: boolean; physical?: boolean }): Promise<FilesystemSQLiteDriver>;
  openSecondConnection?(): Promise<FilesystemSQLiteDriver>;
  dispose(): Promise<void>;
}
export interface ConformanceAdapterFactory { readonly name: string; create(options?: ConformanceFixtureOptions): Promise<ConformanceDatabase> }
export interface CorrectnessResult { readonly schema: "efs-correctness-result-v1"; readonly commit: string; readonly adapter: string; readonly schemaVersion: number; readonly formatVersion: string; readonly seed: number; readonly fixtureDigest: string; readonly faultPoint: string | null; readonly passed: number; readonly failed: number; readonly elapsedMs: number }
export interface BenchmarkResult { readonly schema: "efs-benchmark-result-v1"; readonly benchmark: string; readonly commit: string; readonly engine: string; readonly driver: string; readonly fixture: Readonly<{ name: string; sha256: string }>; readonly configuration: Readonly<Record<string, unknown>>; readonly trials: number; readonly latencyMs: Readonly<{ p50: number; p95: number; p99: number }>; readonly counters: Readonly<Record<string, number>>; readonly pass: boolean }

export type RecordingEvent =
  | Readonly<{ type: "create"; factory: string; label: string | null; seed: number | null }>
  | Readonly<{ type: "reopen"; readOnly: boolean; physical: boolean }>
  | Readonly<{ type: "second-connection" }>
  | Readonly<{ type: "dispose" }>;

/** Wraps a real test factory without weakening its restart or connection behavior. */
export function createRecordingFactory(factory: ConformanceAdapterFactory, events: RecordingEvent[]): ConformanceAdapterFactory {
  return Object.freeze({
    name: `recording:${factory.name}`,
    async create(options: ConformanceFixtureOptions = {}): Promise<ConformanceDatabase> {
      events.push(Object.freeze({ type: "create", factory: factory.name, label: options.label ?? null, seed: options.seed ?? null }));
      const database = await factory.create(options); let disposed = false;
      return Object.freeze({
        adapter: database.adapter,
        capabilities: database.capabilities,
        ...(database.faults === undefined ? {} : { faults: database.faults }),
        async reopen(reopenOptions: { readOnly?: boolean; physical?: boolean } = {}) {
          events.push(Object.freeze({ type: "reopen", readOnly: reopenOptions.readOnly ?? false, physical: reopenOptions.physical ?? false }));
          return database.reopen(reopenOptions);
        },
        ...(database.openSecondConnection === undefined ? {} : { async openSecondConnection() { events.push(Object.freeze({ type: "second-connection" })); return database.openSecondConnection!(); } }),
        async dispose() { if (!disposed) { disposed = true; events.push(Object.freeze({ type: "dispose" })); await database.dispose(); } },
      });
    },
  });
}

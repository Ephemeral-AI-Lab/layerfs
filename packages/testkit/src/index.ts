import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";

export type ConformanceCapability = "read-only-reopen" | "second-connection" | "schema-fixtures" | "fault-injection" | "garbage-collection" | "physical-reopen" | "crash-recovery" | "ownership";
export interface ConformanceFaultController { arm(point: string, occurrence?: number): void; clear(): void }
export interface ConformanceDatabase {
  readonly adapter: FilesystemSQLiteDriver;
  readonly capabilities: readonly ConformanceCapability[];
  readonly faults?: ConformanceFaultController;
  reopen(options?: { readOnly?: boolean; physical?: boolean }): Promise<FilesystemSQLiteDriver>;
  openSecondConnection?(): Promise<FilesystemSQLiteDriver>;
  dispose(): Promise<void>;
}
export interface ConformanceAdapterFactory { readonly name: string; create(): Promise<ConformanceDatabase> }
export interface CorrectnessResult { readonly schema: "efs-correctness-v1"; readonly adapter: string; readonly seed: number; readonly passed: number; readonly failed: number; readonly elapsedMs: number }
export interface BenchmarkResult { readonly schema: "efs-benchmark-v1"; readonly id: string; readonly environment: string; readonly operations: number; readonly elapsedMs: number; readonly metrics: Readonly<Record<string, number>> }


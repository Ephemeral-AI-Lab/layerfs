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
    recordFixtureContext?(context: PortableFixtureContext): void | Promise<void>;
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
    reopenFromFixture?(fixtureName: string): Promise<FilesystemSQLiteDriver>;
    collectGarbage?(filesystem: EphemeralFS, options?: GarbageCollectionOptions): Promise<GarbageCollectionResult>;
    crashAndReopen?(): Promise<FilesystemSQLiteDriver>;
    createOwnershipProbe?(): Promise<{
        readonly adapter: FilesystemSQLiteDriver;
        closeCallCount(): number;
    }>;
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

/* export: createStatementFaultController; kinds: value */
/* source: packages/testkit/dist/fault.d.ts */
/** Adapter-neutral statement fault injection used by both required SQLite drivers. */
export declare function createStatementFaultController(): StatementFaultController;

/* export: filesystemConformance; kinds: value */
/* source: packages/testkit/dist/index.d.ts */
/** Registers the normative shared filesystem suite with Vitest. */
export declare function filesystemConformance(factory: ConformanceAdapterFactory): void;

/* export: PORTABLE_APPLICATION_ID; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_APPLICATION_ID = 1161905747

/* export: PORTABLE_BRANCH_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/branch.d.ts */
PORTABLE_BRANCH_CASE_IDS: readonly [
    "branch-frozen-base",
    "branch-50-independent",
    "branch-50-conflicting",
    "branch-sibling-order",
    "branch-aba-alias-conflicts",
    "branch-deterministic-results",
    "branch-pagination",
    "branch-recursive-conflict",
    "branch-terminal-handles",
    "branch-stream-snapshot",
    "branch-replay-reopen",
    "branch-result-expiry-reservation"
]

/* export: PORTABLE_CONFORMANCE_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/index.d.ts */
PORTABLE_CONFORMANCE_CASE_IDS: readonly [
    "storage-deduplication",
    "filesystem-namespace",
    "filesystem-path-errors",
    "filesystem-range-edges",
    "filesystem-link-semantics",
    "filesystem-rename-removal",
    "filesystem-metadata",
    "filesystem-pagination-cap",
    "filesystem-error-details",
    "stream-snapshot",
    "stream-abort-backpressure",
    "lease-staging-lifecycle",
    "read-side-effect-boundary",
    "overlapping-operations",
    "branch-publication",
    "maintenance-cursors",
    "resource-capabilities",
    "durable-reopen",
    "read-only-reopen",
    "second-connection",
    "close-lifecycle"
]

/* export: PORTABLE_COW_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/cow.d.ts */
PORTABLE_COW_CASE_IDS: readonly [
    "cow-repeated-page-head",
    "cow-boundary-crossing",
    "cow-final-partial-page",
    "cow-pinned-snapshot",
    "cow-physical-reopen",
    "cow-conflicting-format-refusal"
]

/* export: PORTABLE_COW_PAGE_SIZES; kinds: value */
/* source: packages/testkit/dist/cow.d.ts */
PORTABLE_COW_PAGE_SIZES: readonly [
    4096,
    8192,
    16384
]

/* export: PORTABLE_CURRENT_SCHEMA_VERSION; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_CURRENT_SCHEMA_VERSION = 13

/* export: PORTABLE_DRIVER_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/driver.d.ts */
PORTABLE_DRIVER_CASE_IDS: readonly [
    "driver-capabilities",
    "driver-transactions",
    "driver-callback-error-identity",
    "driver-integer-roundtrip",
    "driver-blob-ownership",
    "driver-bounds",
    "driver-sql-shape",
    "driver-reopen-lifecycle"
]

/* export: PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS: Readonly<{
    readonly 1: 365;
    readonly 2: 339;
    readonly 3: 292;
}>

/* export: PORTABLE_FAULT_OPERATION_POSITIONS; kinds: value */
/* source: packages/testkit/dist/fault.d.ts */
PORTABLE_FAULT_OPERATION_POSITIONS: Readonly<{
    readonly "writeFile-create": 214;
    readonly "writeFile-stream": 214;
    readonly writeRange: 74;
    readonly replaceRange: 74;
    readonly truncate: 74;
    readonly mkdir: 175;
    readonly chmod: 29;
    readonly link: 70;
    readonly symlink: 59;
    readonly rename: 60;
    readonly unlink: 49;
    readonly "rm-recursive": 114;
}>

/* export: PORTABLE_FAULT_POSITIONS; kinds: value */
/* source: packages/testkit/dist/fault.d.ts */
PORTABLE_FAULT_POSITIONS = 1206

/* export: PORTABLE_FAULT_SEED; kinds: value */
/* source: packages/testkit/dist/fault.d.ts */
PORTABLE_FAULT_SEED = 1024023

/* export: PORTABLE_FILESYSTEM_FAULT_OPERATIONS; kinds: value */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
PORTABLE_FILESYSTEM_FAULT_OPERATIONS: readonly ("writeRange" | "replaceRange" | "truncate" | "mkdir" | "chmod" | "link" | "symlink" | "rename" | "unlink" | "writeFile-create" | "writeFile-stream" | "rm-recursive")[]

/* export: PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS; kinds: value */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS: Readonly<{
    readonly writeRange: 78;
    readonly replaceRange: 78;
    readonly truncate: 78;
    readonly "writeFile-create": 214;
    readonly "writeFile-stream": 214;
    readonly mkdir: 175;
    readonly chmod: 29;
    readonly link: 70;
    readonly symlink: 59;
    readonly rename: 60;
    readonly unlink: 49;
    readonly "rm-recursive": 114;
}>

/* export: PORTABLE_FILESYSTEM_RESTART_FAULT_POSITIONS; kinds: value */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
PORTABLE_FILESYSTEM_RESTART_FAULT_POSITIONS = 1218

/* export: PORTABLE_FIXTURE_CONTEXT_SCHEMA; kinds: value */
/* source: packages/testkit/dist/fixture-context.d.ts */
PORTABLE_FIXTURE_CONTEXT_SCHEMA: "efs-portable-fixture-context-v1"

/* export: PORTABLE_MAINTENANCE_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/maintenance.d.ts */
PORTABLE_MAINTENANCE_CASE_IDS: readonly [
    "maintenance-snapshot-restart",
    "maintenance-gc-root-reconciliation",
    "maintenance-corruption-no-sweep",
    "maintenance-quota-rollback",
    "maintenance-resource-envelopes"
]

/* export: PORTABLE_MAINTENANCE_FAULT_TOPOLOGY; kinds: value */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
PORTABLE_MAINTENANCE_FAULT_TOPOLOGY: Readonly<{
    readonly snapshot: Readonly<{
        durableStatements: 110;
        committedBatches: 42;
        maxBatchStatements: 6;
    }>;
    readonly collection: Readonly<{
        durableStatements: 259;
        committedBatches: 128;
        maxBatchStatements: 3;
    }>;
    readonly abandoned: Readonly<{
        durableStatements: 61;
        committedBatches: 33;
        maxBatchStatements: 3;
    }>;
}>

/* export: PORTABLE_MIGRATION_STATEMENT_COUNTS; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_MIGRATION_STATEMENT_COUNTS: Readonly<{
    readonly 1: 339;
    readonly 2: 314;
    readonly 3: 269;
}>

/* export: PORTABLE_PUBLICATION_FAULT_POSITIONS; kinds: value */
/* source: packages/testkit/dist/publication-fault.d.ts */
PORTABLE_PUBLICATION_FAULT_POSITIONS: Readonly<{
    readonly direct: 95;
    readonly prepared: 91;
}>

/* export: PORTABLE_RELEASED_FIXTURE_BRANCH; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_RELEASED_FIXTURE_BRANCH = "fixture-branch"

/* export: PORTABLE_RELEASED_FIXTURE_BYTES; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_RELEASED_FIXTURE_BYTES: Uint8Array<ArrayBuffer>

/* export: PORTABLE_RELEASED_FIXTURE_FILE; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_RELEASED_FIXTURE_FILE = "fixture-file"

/* export: PORTABLE_RELEASED_SCHEMA_VERSIONS; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
PORTABLE_RELEASED_SCHEMA_VERSIONS: readonly [
    1,
    2,
    3
]

/* export: PORTABLE_RESTART_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/restart.d.ts */
PORTABLE_RESTART_CASE_IDS: readonly [
    "restart-committed-state",
    "restart-active-branch",
    "restart-lost-response-replay",
    "restart-abandoned-lease",
    "restart-interrupted-collection"
]

/* export: PORTABLE_RESTART_SEED; kinds: value */
/* source: packages/testkit/dist/restart.d.ts */
PORTABLE_RESTART_SEED = 98925095

/* export: PORTABLE_SCALE_SEED; kinds: value */
/* source: packages/testkit/dist/scale.d.ts */
PORTABLE_SCALE_SEED = 379422

/* export: PORTABLE_SMOKE_ACTORS_PER_KIND; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_ACTORS_PER_KIND = 16

/* export: PORTABLE_SMOKE_COW_EDITS; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_COW_EDITS = 5000

/* export: PORTABLE_SMOKE_DEADLINE_MS; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_DEADLINE_MS = 60000

/* export: PORTABLE_SMOKE_NAMESPACE_OPERATIONS; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_NAMESPACE_OPERATIONS = 2000

/* export: PORTABLE_SMOKE_OPERATIONS_PER_ACTOR; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_OPERATIONS_PER_ACTOR = 64

/* export: PORTABLE_SMOKE_PAYLOAD_BYTES; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_PAYLOAD_BYTES: number

/* export: PORTABLE_SMOKE_SEED; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
PORTABLE_SMOKE_SEED = 1592614637

/* export: PORTABLE_STORAGE_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/storage.d.ts */
PORTABLE_STORAGE_CASE_IDS: readonly ("storage-staging-closure-100001" | "storage-certificate-field-corruption" | "storage-sealed-membership-immutability" | "storage-concurrent-payload-quota" | "storage-usage-recount" | "storage-manifest-range-corruption" | "storage-staging-batch-crash-recovery")[]

/* export: PORTABLE_STORAGE_CONFORMANCE_CASE_IDS; kinds: value */
/* source: packages/testkit/dist/storage.d.ts */
PORTABLE_STORAGE_CONFORMANCE_CASE_IDS: readonly [
    "storage-staging-closure-100001",
    "storage-certificate-field-corruption",
    "storage-sealed-membership-immutability",
    "storage-concurrent-payload-quota",
    "storage-usage-recount",
    "storage-manifest-range-corruption"
]

/* export: PORTABLE_STORAGE_RUNTIME_LIMITS; kinds: value */
/* source: packages/testkit/dist/storage.d.ts */
PORTABLE_STORAGE_RUNTIME_LIMITS: Readonly<{
    maxManagedResidentBytes: number;
    maxCacheBytes: number;
    maxPendingWriteBytes: number;
    maxWriteSessionBytes: number;
    maxPrefetchBytes: number;
    maxQueryBatchBytes: number;
}>

/* export: PORTABLE_STORAGE_STORAGE_LIMITS; kinds: value */
/* source: packages/testkit/dist/storage.d.ts */
PORTABLE_STORAGE_STORAGE_LIMITS: Readonly<{
    maxManagedPayloadBytes: number;
    maxStagingPayloadBytes: number;
    maxChargedMetadataBytes: number;
    maxMaintenanceBytes: number;
    maintenanceReserveBytes: number;
    maxBranchOverlayBytes: number;
    maxQueryBatchSize: 32;
    maxGcBatchSize: 32;
}>

/* export: PortableBranchCaseId; kinds: type */
/* source: packages/testkit/dist/branch.d.ts */
export type PortableBranchCaseId = (typeof PORTABLE_BRANCH_CASE_IDS)[number];

/* export: PortableBranchCaseResult; kinds: type */
/* source: packages/testkit/dist/branch.d.ts */
export interface PortableBranchCaseResult {
    readonly id: PortableBranchCaseId;
    readonly status: "passed";
}

/* export: PortableConformanceCaseId; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
export type PortableConformanceCaseId = "storage-deduplication" | "filesystem-namespace" | "filesystem-path-errors" | "filesystem-range-edges" | "filesystem-link-semantics" | "filesystem-rename-removal" | "filesystem-metadata" | "filesystem-pagination-cap" | "filesystem-error-details" | "stream-snapshot" | "stream-abort-backpressure" | "lease-staging-lifecycle" | "read-side-effect-boundary" | "overlapping-operations" | "branch-publication" | "maintenance-cursors" | "resource-capabilities" | "durable-reopen" | "read-only-reopen" | "second-connection" | "close-lifecycle";

/* export: PortableConformanceCaseResult; kinds: type */
/* source: packages/testkit/dist/index.d.ts */
export interface PortableConformanceCaseResult {
    readonly id: PortableConformanceCaseId;
    readonly status: "passed" | "skipped";
    readonly reason?: string;
}

/* export: PortableCowPageSize; kinds: type */
/* source: packages/testkit/dist/cow.d.ts */
export type PortableCowPageSize = (typeof PORTABLE_COW_PAGE_SIZES)[number];

/* export: PortableCowPreparation; kinds: type */
/* source: packages/testkit/dist/cow.d.ts */
export interface PortableCowPreparation {
    readonly schema: "efs-portable-cow-preparation-v1";
    readonly pageBytes: PortableCowPageSize;
    readonly branchId: string;
    readonly fixtureDigest: string;
    readonly repeatedWrites: 1000;
}

/* export: PortableCowResult; kinds: type */
/* source: packages/testkit/dist/cow.d.ts */
export interface PortableCowResult extends PortableCowPreparation {
    readonly cases: typeof PORTABLE_COW_CASE_IDS;
    readonly pageHeadCount: number;
    readonly pageVersionCount: number;
    readonly finalPartialBytes: number;
}

/* export: PortableDriverCaseId; kinds: type */
/* source: packages/testkit/dist/driver.d.ts */
export type PortableDriverCaseId = "driver-capabilities" | "driver-transactions" | "driver-callback-error-identity" | "driver-integer-roundtrip" | "driver-blob-ownership" | "driver-bounds" | "driver-sql-shape" | "driver-reopen-lifecycle";

/* export: PortableDriverCaseResult; kinds: type */
/* source: packages/testkit/dist/driver.d.ts */
export interface PortableDriverCaseResult {
    readonly id: PortableDriverCaseId;
    readonly status: "passed";
}

/* export: PortableFaultMatrixResult; kinds: type */
/* source: packages/testkit/dist/fault.d.ts */
export interface PortableFaultMatrixResult {
    readonly schema: "efs-portable-fault-result-v1";
    readonly adapter: string;
    readonly seed: typeof PORTABLE_FAULT_SEED;
    readonly fixtureDigest: string;
    readonly faultPoint: typeof FAULT_POINT;
    readonly positions: number;
    readonly payloadBytes: number;
    readonly operationPositions: Readonly<Record<string, number>>;
}

/* export: PortableFilesystemFaultAttemptResult; kinds: type */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
export interface PortableFilesystemFaultAttemptResult {
    readonly operation: PortableFilesystemFaultOperation;
    readonly occurrence: number;
    readonly injected: boolean;
    readonly observedStatements: number;
    readonly seed: typeof PORTABLE_FAULT_SEED;
}

/* export: PortableFilesystemFaultOperation; kinds: type */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
export type PortableFilesystemFaultOperation = keyof typeof PORTABLE_FAULT_OPERATION_POSITIONS;

/* export: PortableFixtureContext; kinds: type */
/* source: packages/testkit/dist/fixture-context.d.ts */
export interface PortableFixtureContext {
    readonly schema: typeof PORTABLE_FIXTURE_CONTEXT_SCHEMA;
    readonly label: string;
    readonly seed: number;
    readonly fixtureDigest: string;
    readonly digestBasis: "sha256-utf8-canonical-fixture-descriptor";
}

/* export: PortableInitializationIdentityAttemptResult; kinds: type */
/* source: packages/testkit/dist/schema.d.ts */
export interface PortableInitializationIdentityAttemptResult {
    readonly schema: "efs-portable-initialization-identity-attempt-v1";
    readonly boundary: number;
    readonly observedBoundaries: number;
    readonly identityWrites: number;
    readonly injected: boolean;
}

/* export: PortableMaintenanceCaseId; kinds: type */
/* source: packages/testkit/dist/maintenance.d.ts */
export type PortableMaintenanceCaseId = (typeof PORTABLE_MAINTENANCE_CASE_IDS)[number];

/* export: PortableMaintenanceCaseResult; kinds: type */
/* source: packages/testkit/dist/maintenance.d.ts */
export interface PortableMaintenanceCaseResult {
    readonly id: PortableMaintenanceCaseId;
    readonly status: "passed";
}

/* export: PortableMaintenanceFaultAttempt; kinds: type */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
export interface PortableMaintenanceFaultAttempt {
    readonly schema: "efs-portable-maintenance-fault-attempt-v1";
    readonly variant: PortableMaintenanceFaultVariant;
    readonly kind: PortableMaintenanceFaultKind;
    readonly ordinal: number;
    readonly injected: boolean;
    readonly metrics: PortableMaintenanceFaultMetrics;
    readonly resultCounters?: Readonly<Record<string, number>>;
}

/* export: PortableMaintenanceFaultKind; kinds: type */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
export type PortableMaintenanceFaultKind = "statement" | "batch";

/* export: PortableMaintenanceFaultMetrics; kinds: type */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
export interface PortableMaintenanceFaultMetrics {
    readonly durableStatements: number;
    readonly committedBatches: number;
    readonly maxBatchStatements: number;
}

/* export: PortableMaintenanceFaultVariant; kinds: type */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
export type PortableMaintenanceFaultVariant = "snapshot" | "collection" | "abandoned";

/* export: PortableMigrationAttemptResult; kinds: type */
/* source: packages/testkit/dist/schema.d.ts */
export interface PortableMigrationAttemptResult {
    readonly schema: "efs-portable-migration-attempt-v1";
    readonly sourceVersion: 1 | 2 | 3;
    readonly occurrence: number;
    readonly observedStatements: number;
    readonly injected: boolean;
    readonly finalVersion: number;
}

/* export: PortablePublicationFaultAttempt; kinds: type */
/* source: packages/testkit/dist/publication-fault.d.ts */
export interface PortablePublicationFaultAttempt {
    readonly schema: "efs-portable-publication-fault-attempt-v1";
    readonly variant: PortablePublicationFaultVariant;
    readonly occurrence: number;
    readonly maxTransactionStatements: number;
    readonly injected: boolean;
}

/* export: PortablePublicationFaultVariant; kinds: type */
/* source: packages/testkit/dist/publication-fault.d.ts */
export type PortablePublicationFaultVariant = "direct" | "prepared";

/* export: PortableRestartCaseId; kinds: type */
/* source: packages/testkit/dist/restart.d.ts */
export type PortableRestartCaseId = (typeof PORTABLE_RESTART_CASE_IDS)[number];

/* export: PortableRestartPreparation; kinds: type */
/* source: packages/testkit/dist/restart.d.ts */
export interface PortableRestartPreparation {
    readonly schema: "efs-portable-restart-preparation-v1";
    readonly seed: typeof PORTABLE_RESTART_SEED;
    readonly fixtureDigest: string;
    readonly publicationResult: string;
    readonly activeLeaseRows: number;
    readonly collectionState: "paused";
}

/* export: PortableRestartResult; kinds: type */
/* source: packages/testkit/dist/restart.d.ts */
export interface PortableRestartResult {
    readonly schema: "efs-portable-restart-result-v1";
    readonly seed: typeof PORTABLE_RESTART_SEED;
    readonly fixtureDigest: string;
    readonly cases: readonly PortableRestartCaseId[];
    readonly verifiedEntities: number;
    readonly activeLeaseRows: number;
    readonly stagingRows: number;
    readonly collectionState: "complete";
}

/* export: PortableScalePhaseOutcome; kinds: type */
/* source: packages/testkit/dist/scale.d.ts */
export type PortableScalePhaseOutcome = Readonly<{
    status: "restart";
    completedPhase: "baseline-built" | "baseline-measured" | "full-built" | "full-measured" | "collection-paused";
}> | Readonly<{
    status: "complete";
    result: PortableScaleResult;
}>;

/* export: PortableScaleResult; kinds: type */
/* source: packages/testkit/dist/scale.d.ts */
export interface PortableScaleResult {
    readonly schema: "efs-portable-scale-result-v1";
    readonly adapter: string;
    readonly seed: typeof PORTABLE_SCALE_SEED;
    readonly fixtureDigest: string;
    readonly rows: 100000;
    readonly baselineRows: 10240;
    readonly objectRows: number;
    readonly namespaceRows: number;
    readonly manifestRootRows: number;
    readonly manifestNodeRows: number;
    readonly baselineManagedPeakBytes: number;
    readonly fullManagedPeakBytes: number;
    readonly peakStorageMarks: number;
    readonly peakGcMarks: number;
    readonly verifiedRows: number;
    readonly maxMaintenanceCallMs: number;
    readonly mainFileBytes: number;
    readonly physicalRestarts?: number;
}

/* export: PortableScaleSession; kinds: value,type */
/* source: packages/testkit/dist/scale.d.ts */
/** Host-coordinated scale gate whose four restart boundaries require real eviction. */
export declare class PortableScaleSession {
    #private;
    constructor(adapterName: string);
    recordPhysicalRestart(): void;
    run(adapter: FilesystemSQLiteDriver): Promise<PortableScalePhaseOutcome>;
}

/* export: PortableSmokeOperationMetric; kinds: type */
/* source: packages/testkit/dist/smoke.d.ts */
export interface PortableSmokeOperationMetric {
    readonly name: string;
    readonly elapsedMs: number;
}

/* export: PortableSmokePhaseOutcome; kinds: type */
/* source: packages/testkit/dist/smoke.d.ts */
export type PortableSmokePhaseOutcome = Readonly<{
    status: "restart";
    completedPhase: 0 | 1 | 2;
}> | Readonly<{
    status: "complete";
    result: PortableSmokeResult;
}>;

/* export: PortableSmokeResult; kinds: type */
/* source: packages/testkit/dist/smoke.d.ts */
export interface PortableSmokeResult {
    readonly schema: "efs-portable-smoke-result-v1";
    readonly adapter: string;
    readonly seed: number;
    readonly fixtureDigest: string;
    readonly finalPayloadDigest: string;
    readonly namespaceDigest: string;
    readonly elapsedMs: number;
    readonly completedOperationCount: number;
    readonly namespaceOperationCount: number;
    readonly restarts: number;
    readonly peakManagedResidentBytes: number;
    readonly objectCount: number;
    readonly manifestCount: number;
    readonly slowestOperations: readonly PortableSmokeOperationMetric[];
}

/* export: PortableSmokeSession; kinds: value,type */
/* source: packages/testkit/dist/smoke.d.ts */
/**
 * Host-coordinated form of the exact smoke profile. The caller MUST perform a real
 * physical restart/eviction after every `restart` outcome, then call
 * `recordPhysicalRestart()` before entering the next adapter context.
 */
export declare class PortableSmokeSession {
    #private;
    constructor(adapterName: string);
    recordPhysicalRestart(elapsedMs: number): void;
    run(adapter: FilesystemSQLiteDriver): Promise<PortableSmokePhaseOutcome>;
}

/* export: PortableStagingClosureEvidence; kinds: type */
/* source: packages/testkit/dist/storage.d.ts */
export interface PortableStagingClosureEvidence {
    readonly schema: "efs-portable-staging-closure-v1";
    readonly manifestEntries: 100001;
    readonly uniqueClosureMembers: number;
    readonly reconciliationStatements: number;
    readonly finalValidationStatements: 1;
    readonly certificateFieldsRejected: 10;
    readonly sealedMembershipMutationsRejected: 2;
}

/* export: PortableStagingCrashEvidence; kinds: type */
/* source: packages/testkit/dist/storage.d.ts */
export interface PortableStagingCrashEvidence {
    readonly schema: "efs-portable-staging-crash-v1";
    readonly batches: 3;
    readonly physicalRestarts: 3;
    readonly recovered: true;
}

/* export: PortableStagingCrashOutcome; kinds: type */
/* source: packages/testkit/dist/storage.d.ts */
export type PortableStagingCrashOutcome = {
    readonly status: "restart-required";
    readonly batch: number;
} | {
    readonly status: "complete";
    readonly result: PortableStagingCrashEvidence;
};

/* export: PortableStagingCrashSession; kinds: value,type */
/* source: packages/testkit/dist/storage.d.ts */
/** Host-coordinated staging crash scenario; adapters must physically restart between calls. */
export declare class PortableStagingCrashSession {
    #private;
    run(adapter: FilesystemSQLiteDriver, internals: PortableStorageInternals): Promise<PortableStagingCrashOutcome>;
}

/* export: PortableStorageCaseId; kinds: type */
/* source: packages/testkit/dist/storage.d.ts */
export type PortableStorageCaseId = (typeof PORTABLE_STORAGE_CASE_IDS)[number];

/* export: PortableStorageCaseResult; kinds: type */
/* source: packages/testkit/dist/storage.d.ts */
export interface PortableStorageCaseResult {
    readonly id: PortableStorageCaseId;
    readonly status: "passed";
}

/* export: PortableStorageInternals; kinds: type */
/* source: packages/testkit/dist/storage.d.ts */
export interface PortableStorageInternals {
    runStagingClosure(adapter: FilesystemSQLiteDriver): Promise<PortableStagingClosureEvidence>;
    stageCrashBatch(adapter: FilesystemSQLiteDriver, batch: number): Promise<{
        readonly durableEntries: number;
    }>;
    recoverStagingCrash(adapter: FilesystemSQLiteDriver): Promise<{
        readonly activeLeases: number;
        readonly stagingCertificates: number;
        readonly stagingEntries: number;
        readonly stagingBytes: number;
        readonly ingestReservationBytes: number;
    }>;
}

/* export: prepareFilesystemFaultAttempt; kinds: value */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
/**
 * Execute one selected mutation occurrence without orderly close. The caller owns the
 * physical driver/runtime restart before invoking `verifyFilesystemFaultAttempt`.
 */
export declare function prepareFilesystemFaultAttempt(adapter: FilesystemSQLiteDriver, operation: PortableFilesystemFaultOperation, occurrence: number, faults?: StatementFaultController): Promise<PortableFilesystemFaultAttemptResult>;

/* export: preparePortableCowPageSize; kinds: value */
/* source: packages/testkit/dist/cow.d.ts */
/** Prepare all public COW mutations, intentionally separate from physical reopen. */
export declare function preparePortableCowPageSize(adapter: FilesystemSQLiteDriver, pageBytes: PortableCowPageSize): Promise<PortableCowPreparation>;

/* export: preparePortableRestart; kinds: value */
/* source: packages/testkit/dist/restart.d.ts */
/**
 * Establish durable state immediately before an unorderly physical/runtime restart.
 * The caller MUST destroy the Node connection or evict the Durable Object after this
 * function returns, without orderly filesystem or branch close.
 */
export declare function preparePortableRestart(adapter: FilesystemSQLiteDriver): Promise<PortableRestartPreparation>;

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

/* export: recordPortableFixtureContext; kinds: value */
/* source: packages/testkit/dist/fixture-context.d.ts */
/** Record the exact deterministic descriptor used to create one portable fixture. */
export declare function recordPortableFixtureContext(recorder: FixtureContextRecorder, adapter: FilesystemSQLiteDriver, label: string, seed: number): Promise<void>;

/* export: runBranchConformance; kinds: value */
/* source: packages/testkit/dist/branch.d.ts */
/** Shared 50-writer, conflict, snapshot, replay, and restart branch suite. */
export declare function runBranchConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableBranchCaseResult[]>;

/* export: runFilesystemConformance; kinds: value */
/* source: packages/testkit/dist/index.d.ts */
/**
 * Runs the same host-neutral milestone conformance scenario against a real adapter
 * factory. Runtime harnesses may invoke this inside their storage-owning isolate.
 */
export declare function runFilesystemConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableConformanceCaseResult[]>;

/* export: runFilesystemFaultMatrix; kinds: value */
/* source: packages/testkit/dist/fault.d.ts */
/**
 * Fail after every SQL statement in every public filesystem mutation family.
 * Every injected position must reopen to the complete old state; the first
 * position beyond each operation must reopen to the complete new state.
 */
export declare function runFilesystemFaultMatrix(factory: ConformanceAdapterFactory): Promise<PortableFaultMatrixResult>;

/* export: runFilesystemSmoke; kinds: value */
/* source: packages/testkit/dist/smoke.d.ts */
/** Execute the exact finite 60-second profile against a real adapter factory. */
export declare function runFilesystemSmoke(factory: ConformanceAdapterFactory): Promise<PortableSmokeResult>;

/* export: runMaintenanceConformance; kinds: value */
/* source: packages/testkit/dist/maintenance.d.ts */
/** Shared bounded maintenance, recovery, corruption, quota, and resource suite. */
export declare function runMaintenanceConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableMaintenanceCaseResult[]>;

/* export: runPortableInitializationIdentityAttempt; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
/** Fault before and after every selected schema-identity write during initialization. */
export declare function runPortableInitializationIdentityAttempt(adapter: FilesystemSQLiteDriver, boundary: number): Promise<PortableInitializationIdentityAttemptResult>;

/* export: runPortableMaintenanceFaultAttempt; kinds: value */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
/** Run one fresh post-commit maintenance fault attempt. */
export declare function runPortableMaintenanceFaultAttempt(adapter: FilesystemSQLiteDriver, variant: PortableMaintenanceFaultVariant, kind: PortableMaintenanceFaultKind, ordinal: number): Promise<PortableMaintenanceFaultAttempt>;

/* export: runPortableMigrationAttempt; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
/**
 * Run one fresh released-schema migration with a fault after the selected statement.
 * A caught fault must leave the exact source version usable; the first out-of-range
 * occurrence must migrate and open the current filesystem successfully.
 */
export declare function runPortableMigrationAttempt(adapter: FilesystemSQLiteDriver, sourceVersion: 1 | 2 | 3, occurrence: number): Promise<PortableMigrationAttemptResult>;

/* export: runPortablePublicationFaultAttempt; kinds: value */
/* source: packages/testkit/dist/publication-fault.d.ts */
/** Run one fresh publication attempt with a fault at one final-transaction position. */
export declare function runPortablePublicationFaultAttempt(adapter: FilesystemSQLiteDriver, variant: PortablePublicationFaultVariant, occurrence: number): Promise<PortablePublicationFaultAttempt>;

/* export: runScaleConformance; kinds: value */
/* source: packages/testkit/dist/scale.d.ts */
/** Shared 100,000-row cursor, restart, memory, and collection scale gate. */
export declare function runScaleConformance(factory: ConformanceAdapterFactory): Promise<PortableScaleResult>;

/* export: runSQLiteDriverConformance; kinds: value */
/* source: packages/testkit/dist/driver.d.ts */
/** Run the identical callback-scoped SQLite contract against a fresh adapter. */
export declare function runSQLiteDriverConformance(factory: ConformanceAdapterFactory): Promise<readonly PortableDriverCaseResult[]>;

/* export: runStorageConformance; kinds: value */
/* source: packages/testkit/dist/storage.d.ts */
/**
 * Runs the adapter-neutral M2 storage case whose implementation port remains private.
 * The injected port is shared by the Node and workerd harnesses; it may use private
 * repositories without widening the package export boundary.
 */
export declare function runStorageConformance(factory: ConformanceAdapterFactory, internals: PortableStorageInternals): Promise<readonly PortableStorageCaseResult[]>;

/* export: StatementFaultController; kinds: type */
/* source: packages/testkit/dist/fault.d.ts */
export interface StatementFaultController extends ConformanceFaultController {
    wrap(driver: FilesystemSQLiteDriver): FilesystemSQLiteDriver;
    statementCount(): number;
}

/* export: verifyFilesystemFaultAttempt; kinds: value */
/* source: packages/testkit/dist/filesystem-fault-attempt.d.ts */
/** Verify complete old/new state after the caller has physically restarted storage. */
export declare function verifyFilesystemFaultAttempt(adapter: FilesystemSQLiteDriver, operation: PortableFilesystemFaultOperation, committed: boolean): Promise<void>;

/* export: verifyPortableCowPageSize; kinds: value */
/* source: packages/testkit/dist/cow.d.ts */
/** Verify exact state and format refusal after the caller physically restarts storage. */
export declare function verifyPortableCowPageSize(adapter: FilesystemSQLiteDriver, preparation: PortableCowPreparation): Promise<PortableCowResult>;

/* export: verifyPortableCurrentSchema; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
/** Validate a freshly initialized or migrated current schema through public behavior. */
export declare function verifyPortableCurrentSchema(adapter: FilesystemSQLiteDriver): Promise<void>;

/* export: verifyPortableEmptyInitialization; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
/** Prove an interrupted empty-database initialization retained no identity or schema. */
export declare function verifyPortableEmptyInitialization(adapter: FilesystemSQLiteDriver): void;

/* export: verifyPortableMaintenanceFaultRecovery; kinds: value */
/* source: packages/testkit/dist/maintenance-fault.d.ts */
/** Resume and verify one selected maintenance operation after host restart/eviction. */
export declare function verifyPortableMaintenanceFaultRecovery(adapter: FilesystemSQLiteDriver, variant: PortableMaintenanceFaultVariant): Promise<Readonly<Record<string, number>>>;

/* export: verifyPortablePublicationFaultRecovery; kinds: value */
/* source: packages/testkit/dist/publication-fault.d.ts */
/** Verify old state after the caller has physically recreated the driver/runtime. */
export declare function verifyPortablePublicationFaultRecovery(adapter: FilesystemSQLiteDriver, variant: PortablePublicationFaultVariant): Promise<void>;

/* export: verifyPortableRecoverableMigrationState; kinds: value */
/* source: packages/testkit/dist/schema.d.ts */
/**
 * Validate that an injected migration left a transactionally self-consistent source,
 * intermediate, or current schema after the host has recreated the driver/isolate.
 */
export declare function verifyPortableRecoverableMigrationState(adapter: FilesystemSQLiteDriver, minimumVersion: 1 | 2 | 3, expectedVersion: number): void;

/* export: verifyPortableRestart; kinds: value */
/* source: packages/testkit/dist/restart.d.ts */
/** Verify and finish the shared recovery scenario after a real physical/runtime restart. */
export declare function verifyPortableRestart(adapter: FilesystemSQLiteDriver, preparation: PortableRestartPreparation): Promise<PortableRestartResult>;

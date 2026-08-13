import { EphemeralFS } from "@ephemeralai/fs";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  SqliteBindings,
  SqliteRow,
} from "@ephemeralai/fs/sqlite-driver";

export const PORTABLE_RELEASED_SCHEMA_VERSIONS = Object.freeze([1, 2, 3] as const);
export const PORTABLE_CURRENT_SCHEMA_VERSION = 13;
export const PORTABLE_APPLICATION_ID = 0x45414653;
export const PORTABLE_MIGRATION_STATEMENT_COUNTS = Object.freeze({
  1: 335,
  2: 310,
  3: 265,
} as const);
export const PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS = Object.freeze({
  1: 337,
  2: 312,
  3: 266,
} as const);

export interface PortableMigrationAttemptResult {
  readonly schema: "efs-portable-migration-attempt-v1";
  readonly sourceVersion: 1 | 2 | 3;
  readonly occurrence: number;
  readonly observedStatements: number;
  readonly injected: boolean;
  readonly finalVersion: number;
}

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable schema conformance: ${message}`);
}

function identityVersion(adapter: FilesystemSQLiteDriver): number {
  const mode = adapter.capabilities.schemaIdentityMode ?? "sqlite-header";
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{ readonly value: number }>(
        mode === "durable-table"
          ? "SELECT user_version value FROM efs_schema_identity WHERE singleton=1"
          : "SELECT user_version value FROM pragma_user_version",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0],
  );
  invariant(
    row !== undefined && Number.isSafeInteger(row.value),
    "identity version is missing",
  );
  return row.value;
}

function faultingMigrationDriver(
  adapter: FilesystemSQLiteDriver,
  occurrence: number,
  observed: { value: number },
): FilesystemSQLiteDriver {
  const afterStatement = (): void => {
    observed.value += 1;
    if (observed.value === occurrence)
      throw new Error(`portable migration fault ${occurrence}`);
  };
  return Object.freeze({
    kind: adapter.kind,
    readOnly: adapter.readOnly,
    capabilities: adapter.capabilities,
    ...(adapter.hashBytes === undefined
      ? {}
      : { hashBytes: adapter.hashBytes.bind(adapter) }),
    ...(adapter.hashBytesAsync === undefined
      ? {}
      : { hashBytesAsync: adapter.hashBytesAsync.bind(adapter) }),
    transaction<T>(
      mode: "read" | "write" | "exclusive",
      callback: (tx: FilesystemSQLiteTransaction) => T,
    ): T {
      return adapter.transaction(mode, (tx) =>
        callback(
          Object.freeze({
            scope: tx.scope,
            run(sql: string, bindings: SqliteBindings = []) {
              const result = tx.run(sql, bindings);
              afterStatement();
              return result;
            },
            all<Row extends SqliteRow = SqliteRow>(
              sql: string,
              bindings: SqliteBindings,
              budget: { readonly maxRows: number; readonly maxBytes: number },
            ): readonly Row[] {
              const result = tx.all<Row>(sql, bindings, budget);
              afterStatement();
              return result;
            },
          }),
        ),
      );
    },
    physicalStorage: () => adapter.physicalStorage?.() ?? Object.freeze({}),
    ...(adapter.checkpoint === undefined
      ? {}
      : { checkpoint: adapter.checkpoint.bind(adapter) }),
    close: () => adapter.close(),
  });
}

/**
 * Run one fresh released-schema migration with a fault after the selected statement.
 * A caught fault must leave the exact source version usable; the first out-of-range
 * occurrence must migrate and open the current filesystem successfully.
 */
export async function runPortableMigrationAttempt(
  adapter: FilesystemSQLiteDriver,
  sourceVersion: 1 | 2 | 3,
  occurrence: number,
): Promise<PortableMigrationAttemptResult> {
  invariant(
    PORTABLE_RELEASED_SCHEMA_VERSIONS.includes(sourceVersion),
    "unsupported released schema fixture",
  );
  invariant(
    Number.isSafeInteger(occurrence) && occurrence > 0,
    "invalid migration fault occurrence",
  );
  invariant(identityVersion(adapter) === sourceVersion, "source identity differs");
  const observed = { value: 0 };
  const selected = faultingMigrationDriver(adapter, occurrence, observed);
  let filesystem: EphemeralFS | undefined;
  let injected = false;
  try {
    filesystem = await EphemeralFS.open({ database: selected, ownsDatabase: false });
  } catch (error) {
    if (!String(error).includes(`portable migration fault ${occurrence}`)) throw error;
    injected = true;
  }
  if (injected) {
    const finalVersion = identityVersion(adapter);
    invariant(
      finalVersion >= sourceVersion && finalVersion <= PORTABLE_CURRENT_SCHEMA_VERSION,
      "failed migration did not retain a usable source or intermediate version",
    );
  } else {
    invariant(
      identityVersion(adapter) === PORTABLE_CURRENT_SCHEMA_VERSION,
      "successful migration did not reach the current version",
    );
    await filesystem!.close();
  }
  return Object.freeze({
    schema: "efs-portable-migration-attempt-v1",
    sourceVersion,
    occurrence,
    observedStatements: observed.value,
    injected,
    finalVersion: identityVersion(adapter),
  });
}

/** Validate a freshly initialized or migrated current schema through public behavior. */
export async function verifyPortableCurrentSchema(
  adapter: FilesystemSQLiteDriver,
): Promise<void> {
  const filesystem = await EphemeralFS.open({ database: adapter, ownsDatabase: false });
  try {
    invariant((await filesystem.stat("/")).isDirectory(), "root is not a directory");
    invariant(
      identityVersion(adapter) === PORTABLE_CURRENT_SCHEMA_VERSION,
      "current identity version differs",
    );
  } finally {
    await filesystem.close();
  }
}

/**
 * Validate that an injected migration left a transactionally self-consistent source,
 * intermediate, or current schema after the host has recreated the driver/isolate.
 */
export function verifyPortableRecoverableMigrationState(
  adapter: FilesystemSQLiteDriver,
  minimumVersion: 1 | 2 | 3,
  expectedVersion: number,
): void {
  const version = identityVersion(adapter);
  invariant(version === expectedVersion, "reopened identity version changed");
  invariant(
    version >= minimumVersion && version <= PORTABLE_CURRENT_SCHEMA_VERSION,
    "reopened migration version is outside the usable chain",
  );
  const state = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{
        readonly schema_version: number;
        readonly root_inode: string;
        readonly root_rows: number;
        readonly revision_rows: number;
      }>(
        "SELECT m.schema_version,m.root_inode,(SELECT count(*) FROM efs_inodes i WHERE i.id=m.root_inode AND i.type=1) root_rows,(SELECT count(*) FROM efs_revisions WHERE revision=0) revision_rows FROM efs_meta m WHERE singleton=1",
        [],
        { maxRows: 1, maxBytes: 512 },
      )[0],
  );
  invariant(state !== undefined, "reopened migration metadata is missing");
  invariant(
    state.schema_version === version,
    "identity and metadata versions are not atomic",
  );
  invariant(
    state.root_rows === 1 && state.revision_rows === 1,
    "reopened migration lost its root or bootstrap revision",
  );
}

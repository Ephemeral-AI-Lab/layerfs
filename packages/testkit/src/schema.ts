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
  1: 339,
  2: 314,
  3: 269,
} as const);
export const PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS = Object.freeze({
  1: 365,
  2: 339,
  3: 292,
} as const);
export const PORTABLE_RELEASED_FIXTURE_FILE = "fixture-file";
export const PORTABLE_RELEASED_FIXTURE_BRANCH = "fixture-branch";
export const PORTABLE_RELEASED_FIXTURE_BYTES = Uint8Array.of(0x45, 0x46, 0x53, 0x36);

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

function identityWrite(sql: string, mode: "sqlite-header" | "durable-table"): boolean {
  const normalized = sql.replaceAll(/\s+/gu, " ").trim().toLowerCase();
  return mode === "durable-table"
    ? normalized.includes("efs_schema_identity")
    : normalized.startsWith("pragma application_id=") ||
        normalized.startsWith("pragma user_version=");
}

function faultingInitializationDriver(
  adapter: FilesystemSQLiteDriver,
  boundary: number,
  observed: { boundaries: number; writes: number },
): FilesystemSQLiteDriver {
  const mode = adapter.capabilities.schemaIdentityMode ?? "sqlite-header";
  const crossBoundary = (): void => {
    observed.boundaries += 1;
    if (observed.boundaries === boundary)
      throw new Error(`portable initialization identity fault ${boundary}`);
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
      transactionMode: "read" | "write" | "exclusive",
      callback: (tx: FilesystemSQLiteTransaction) => T,
    ): T {
      return adapter.transaction(transactionMode, (tx) =>
        callback(
          Object.freeze({
            scope: tx.scope,
            run(sql: string, bindings: SqliteBindings = []) {
              if (!identityWrite(sql, mode)) return tx.run(sql, bindings);
              observed.writes += 1;
              crossBoundary();
              const result = tx.run(sql, bindings);
              crossBoundary();
              return result;
            },
            all<Row extends SqliteRow = SqliteRow>(
              sql: string,
              bindings: SqliteBindings,
              budget: { readonly maxRows: number; readonly maxBytes: number },
            ): readonly Row[] {
              return tx.all<Row>(sql, bindings, budget);
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

export interface PortableInitializationIdentityAttemptResult {
  readonly schema: "efs-portable-initialization-identity-attempt-v1";
  readonly boundary: number;
  readonly observedBoundaries: number;
  readonly identityWrites: number;
  readonly injected: boolean;
}

/** Fault before and after every selected schema-identity write during initialization. */
export async function runPortableInitializationIdentityAttempt(
  adapter: FilesystemSQLiteDriver,
  boundary: number,
): Promise<PortableInitializationIdentityAttemptResult> {
  invariant(
    Number.isSafeInteger(boundary) && boundary > 0,
    "invalid identity boundary",
  );
  const observed = { boundaries: 0, writes: 0 };
  const selected = faultingInitializationDriver(adapter, boundary, observed);
  let filesystem: EphemeralFS | undefined;
  let injected = false;
  try {
    filesystem = await EphemeralFS.open({ database: selected, ownsDatabase: false });
  } catch (error) {
    if (!String(error).includes(`portable initialization identity fault ${boundary}`))
      throw error;
    injected = true;
  }
  await filesystem?.close();
  return Object.freeze({
    schema: "efs-portable-initialization-identity-attempt-v1",
    boundary,
    observedBoundaries: observed.boundaries,
    identityWrites: observed.writes,
    injected,
  });
}

/** Prove an interrupted empty-database initialization retained no identity or schema. */
export function verifyPortableEmptyInitialization(
  adapter: FilesystemSQLiteDriver,
): void {
  const mode = adapter.capabilities.schemaIdentityMode ?? "sqlite-header";
  const state = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{
        readonly objects: number;
        readonly application_id: number;
        readonly user_version: number;
      }>(
        mode === "durable-table"
          ? "SELECT (SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%') objects,0 application_id,0 user_version"
          : "SELECT (SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%') objects,(SELECT application_id FROM pragma_application_id) application_id,(SELECT user_version FROM pragma_user_version) user_version",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0],
  );
  invariant(
    state !== undefined &&
      state.objects === 0 &&
      state.application_id === 0 &&
      state.user_version === 0,
    "interrupted initialization retained partial identity or schema",
  );
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  return (
    left.byteLength === right.byteLength &&
    left.every((value, index) => value === right[index])
  );
}

function verifyReleasedFixtureRows(adapter: FilesystemSQLiteDriver): void {
  const state = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{
        readonly main_revision: number;
        readonly root_rows: number;
        readonly revision_rows: number;
        readonly entry_rows: number;
        readonly file_rows: number;
        readonly branch_rows: number;
        readonly object_rows: number;
        readonly root_manifest_rows: number;
        readonly node_rows: number;
        readonly revision_manifest_rows: number;
        readonly usage_objects: number;
        readonly usage_object_bytes: number;
        readonly usage_roots: number;
        readonly usage_nodes: number;
        readonly permanent_ids: number;
      }>(
        "SELECT m.main_revision,(SELECT count(*) FROM efs_inodes i WHERE i.id=m.root_inode AND i.type=1) root_rows,(SELECT count(*) FROM efs_revisions) revision_rows,(SELECT count(*) FROM efs_entries WHERE parent_inode=m.root_inode AND name='fixture-file' AND inode_id='fixture-file-inode') entry_rows,(SELECT count(*) FROM efs_inodes WHERE id='fixture-file-inode' AND type=0 AND size=4) file_rows,(SELECT count(*) FROM efs_branches WHERE id='fixture-branch' AND base_revision=1 AND state=0) branch_rows,(SELECT count(*) FROM efs_cas_objects) object_rows,(SELECT count(*) FROM efs_manifest_roots) root_manifest_rows,(SELECT count(*) FROM efs_manifest_nodes) node_rows,(SELECT count(*) FROM efs_revision_manifest_roots WHERE revision=1 AND inode_id='fixture-file-inode') revision_manifest_rows,u.object_count usage_objects,u.object_bytes usage_object_bytes,u.manifest_root_count usage_roots,u.manifest_node_count usage_nodes,u.permanent_identifiers permanent_ids FROM efs_meta m JOIN efs_usage u ON u.singleton=m.singleton WHERE m.singleton=1",
        [],
        { maxRows: 1, maxBytes: 2048 },
      )[0],
  );
  invariant(state !== undefined, "released fixture metadata is missing");
  invariant(
    state.main_revision === 1 &&
      state.root_rows === 1 &&
      state.revision_rows === 2 &&
      state.entry_rows === 1 &&
      state.file_rows === 1 &&
      state.branch_rows === 1 &&
      state.object_rows === 1 &&
      state.root_manifest_rows === 1 &&
      state.node_rows === 1 &&
      state.revision_manifest_rows === 1 &&
      state.usage_objects === 1 &&
      state.usage_object_bytes === PORTABLE_RELEASED_FIXTURE_BYTES.byteLength &&
      state.usage_roots === 1 &&
      state.usage_nodes === 1 &&
      state.permanent_ids === 1,
    "released fixture namespace, content, branch, revision, or accounting changed",
  );
  const objectBytes = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{ readonly bytes: Uint8Array }>("SELECT bytes FROM efs_cas_objects", [], {
        maxRows: 1,
        maxBytes: 64,
      })[0]?.bytes,
  );
  invariant(
    objectBytes instanceof Uint8Array &&
      sameBytes(objectBytes, PORTABLE_RELEASED_FIXTURE_BYTES),
    "released fixture CAS bytes changed",
  );
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
    await verifyPortableCurrentSchema(adapter);
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
    verifyReleasedFixtureRows(adapter);
    invariant(
      sameBytes(
        await filesystem.readFile(`/${PORTABLE_RELEASED_FIXTURE_FILE}`),
        PORTABLE_RELEASED_FIXTURE_BYTES,
      ),
      "migrated public content bytes differ",
    );
    const branch = await filesystem.branches.get(PORTABLE_RELEASED_FIXTURE_BRANCH);
    invariant(
      branch.state === "active" && branch.baseRevision === "1",
      "migrated public branch differs",
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
        "SELECT m.schema_version,m.root_inode,(SELECT count(*) FROM efs_inodes i WHERE i.id=m.root_inode AND i.type=1) root_rows,(SELECT count(*) FROM efs_revisions) revision_rows FROM efs_meta m WHERE singleton=1",
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
    state.root_rows === 1 && state.revision_rows === 2,
    "reopened migration lost its root or bootstrap revision",
  );
  verifyReleasedFixtureRows(adapter);
}

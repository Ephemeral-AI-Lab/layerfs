import { checkedAdd } from "../resources/safe-integers.js";
import { bytesToHex, hexToBytes } from "../cas/bytes.js";
import { DURABLE_METADATA_ROW_BYTES, type StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow, SqliteValue } from "./driver.js";

interface MetadataChargeBatch {
  bytes: number;
}

interface UsageMutationBatch {
  readonly base: UsageSnapshot;
  readonly delta: Partial<Record<UsageCounter, number>>;
  touched: boolean;
}

/**
 * A local durable edit already holds one IMMEDIATE write transaction. Metadata
 * rows created by its staging, summary, and authenticated-claim phases can
 * therefore publish one usage mutation at the transaction boundary instead of
 * one mutation per row family. The batch is transaction-scoped and is never
 * enabled for generic callers.
 */
const metadataChargeBatches = new WeakMap<
  FilesystemSQLiteTransaction,
  MetadataChargeBatch
>();

const usageMutationBatches = new WeakMap<
  FilesystemSQLiteTransaction,
  UsageMutationBatch
>();

export function beginMetadataChargeBatch(tx: FilesystemSQLiteTransaction): void {
  if (!metadataChargeBatches.has(tx)) metadataChargeBatches.set(tx, { bytes: 0 });
}

export function applyChargedMetadata(
  tx: FilesystemSQLiteTransaction,
  limits: StorageLimits,
  bytes: number,
  reason: string,
): void {
  if (bytes === 0) return;
  const batch = metadataChargeBatches.get(tx);
  if (batch) {
    batch.bytes = checkedAdd(batch.bytes, bytes, "batched charged metadata");
    return;
  }
  new UsageRepository(tx, limits).apply({ charged_metadata_bytes: bytes }, reason);
}

export function flushMetadataChargeBatch(
  tx: FilesystemSQLiteTransaction,
  limits: StorageLimits,
): void {
  const batch = metadataChargeBatches.get(tx);
  if (!batch) return;
  metadataChargeBatches.delete(tx);
  if (batch.bytes !== 0)
    new UsageRepository(tx, limits).apply(
      { charged_metadata_bytes: batch.bytes },
      "durable local-rebuild metadata accounting",
    );
}

export function beginUsageMutationBatch(
  tx: FilesystemSQLiteTransaction,
  limits: StorageLimits,
): void {
  if (!usageMutationBatches.has(tx))
    usageMutationBatches.set(tx, {
      base: new UsageRepository(tx, limits).snapshot(),
      delta: {},
      touched: false,
    });
}

function batchedUsageSnapshot(batch: UsageMutationBatch): UsageSnapshot {
  const next = {} as Record<UsageCounter, number>;
  for (const column of USAGE_COUNTER_COLUMNS)
    next[column] = add(
      batch.base[column],
      batch.delta[column] ?? 0,
      `batched ${column}`,
    );
  const changed = USAGE_COUNTER_COLUMNS.some(
    (column) => (batch.delta[column] ?? 0) !== 0,
  );
  const mutationSequence = checkedAdd(
    batch.base.mutation_sequence,
    changed || batch.touched ? 1 : 0,
    "batched usage mutation sequence",
  );
  return Object.freeze({
    ...next,
    mutation_sequence: mutationSequence,
    integrity_token: usageIntegrityToken({
      ...next,
      mutation_sequence: mutationSequence,
    }),
  }) as UsageSnapshot;
}

export function flushUsageMutationBatch(
  tx: FilesystemSQLiteTransaction,
  limits: StorageLimits,
): void {
  const batch = usageMutationBatches.get(tx);
  if (!batch) return;
  usageMutationBatches.delete(tx);
  const changed = USAGE_COUNTER_COLUMNS.some(
    (column) => (batch.delta[column] ?? 0) !== 0,
  );
  if (changed)
    new UsageRepository(tx, limits).apply(
      batch.delta,
      "durable local-rebuild usage accounting",
    );
  else if (batch.touched) new UsageRepository(tx, limits).touch();
}

/** Conservative fixed metadata envelope; variable payload classes are charged separately. */
export const CHARGED_ROW_BYTES = DURABLE_METADATA_ROW_BYTES;
/** Logical per-content-row reservation that is exchanged for one live GC mark. */
export const GC_MARK_RESERVATION_BYTES = 704;
/** Fixed durable envelope reserved while a bounded storage snapshot is resumable. */
export const STORAGE_SNAPSHOT_STATE_BYTES = 2048;
/** Storage marks consume the per-content GC reservation and add no second charge. */
export const STORAGE_SNAPSHOT_MARK_BYTES = GC_MARK_RESERVATION_BYTES;

export const CHARGED_METADATA_TABLES = Object.freeze([
  "efs_cas_objects",
  "efs_manifest_roots",
  "efs_manifest_nodes",
  "efs_manifest_subtree_summaries",
  "efs_manifest_validations",
  "efs_revisions",
  "efs_inodes",
  "efs_entries",
  "efs_inode_revisions",
  "efs_revision_manifest_roots",
  "efs_entry_revisions",
  "efs_branches",
  "efs_branch_ids",
  "efs_branch_changes",
  "efs_branch_inode_expectations",
  "efs_branch_inode_overlays",
  "efs_branch_manifest_roots",
  "efs_subtree_tokens",
  "efs_cow_page_versions",
  "efs_cow_page_heads",
  "efs_patches",
  "efs_patch_segments",
  "efs_leases",
  "efs_lease_manifests",
  "efs_lease_objects",
  "efs_lease_staged_manifests",
  "efs_staging_entries",
  "efs_staging_level_records",
  "efs_lease_cow_pages",
  "efs_lease_patches",
  "efs_staging_certificates",
  "efs_staging_reconciliations",
  "efs_staging_reconciliation_queue",
  "efs_staging_manifest_validation_queue",
  "efs_staging_workspaces",
  "efs_staging_reused_subtrees",
  "efs_operation_ids",
  "efs_operation_results",
  "efs_revision_checkpoints",
  "efs_checkpoint_inodes",
  "efs_checkpoint_entries",
  "efs_checkpoint_manifest_roots",
] as const);

const DIRECT_VARIABLE_METADATA_TERMS = Object.freeze([
  "(SELECT coalesce(sum(length(CAST(writer_id AS BLOB))),0) FROM efs_revisions)",
  "(SELECT coalesce(sum(coalesce(length(CAST(symlink_target AS BLOB)),0)),0) FROM efs_inodes)",
  "(SELECT coalesce(sum(length(name_sort)+coalesce(length(CAST(name AS BLOB)),0)),0) FROM efs_entries)",
  "(SELECT coalesce(sum(coalesce(length(encoded),0)),0) FROM efs_inode_revisions)",
  "(SELECT coalesce(sum(length(name_sort)+coalesce(length(encoded),0)),0) FROM efs_entry_revisions)",
  "(SELECT coalesce(sum(length(path)+coalesce(length(encoded),0)),0) FROM efs_branch_changes)",
  "(SELECT coalesce(sum(length(encoded)),0) FROM efs_branch_inode_overlays)",
  "(SELECT coalesce(sum(length(path)),0) FROM efs_branch_manifest_roots)",
  "(SELECT coalesce(sum(coalesce(length(cdc_buffer),0)),0) FROM efs_staging_workspaces)",
  "(SELECT coalesce(sum(metadata_reservation_bytes),0) FROM efs_staging_certificates)",
  "(SELECT coalesce(sum(coalesce(length(encoded),0)),0) FROM efs_checkpoint_inodes)",
  "(SELECT coalesce(sum(length(name_sort)+coalesce(length(encoded),0)),0) FROM efs_checkpoint_entries)",
] as const);
export const DIRECT_CHARGED_METADATA_EXPRESSION = `${CHARGED_ROW_BYTES}*(${CHARGED_METADATA_TABLES.map(
  (table) => `(SELECT count(*) FROM ${table})`,
).join("+")})+${DIRECT_VARIABLE_METADATA_TERMS.join("+")}`;
export const DIRECT_CHARGED_METADATA_EXPRESSION_LEGACY = `${CHARGED_ROW_BYTES}*(${CHARGED_METADATA_TABLES.filter(
  (table) =>
    table !== "efs_manifest_subtree_summaries" &&
    table !== "efs_branch_inode_overlays" &&
    table !== "efs_subtree_tokens" &&
    table !== "efs_revision_checkpoints" &&
    !table.startsWith("efs_checkpoint_"),
)
  .map((table) => `(SELECT count(*) FROM ${table})`)
  .join("+")})+${DIRECT_VARIABLE_METADATA_TERMS.filter(
  (term) =>
    !term.includes("efs_branch_inode_overlays") &&
    !term.includes("efs_subtree_tokens") &&
    !term.includes("efs_checkpoint_"),
).join("+")}`;
export const DIRECT_CHARGED_METADATA_SQL = `SELECT ${DIRECT_CHARGED_METADATA_EXPRESSION} value`;
export const DIRECT_STAGING_BYTES_SQL =
  "SELECT (SELECT coalesce(sum(o.size),0) FROM efs_lease_objects o JOIN efs_leases l ON l.id=o.lease_id WHERE l.state IN (0,1))+(SELECT coalesce(sum(m.size),0) FROM efs_lease_staged_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1)) value";
export const DIRECT_INGEST_RESERVATION_SQL =
  "SELECT coalesce(sum(c.ingest_reservation_bytes),0) value FROM efs_staging_certificates c JOIN efs_leases l ON l.id=c.lease_id WHERE l.state IN (0,1)";

export const DIRECT_USAGE_TABLES = Object.freeze([
  ...CHARGED_METADATA_TABLES,
  "efs_root_journal",
  "efs_gc_runs",
  "efs_gc_marks",
  "efs_lease_cleanups",
  "efs_storage_snapshots",
  "efs_storage_marks",
  "efs_root_holds",
] as const);

const DIRECT_USAGE_SQL = `SELECT
  (SELECT count(*) FROM efs_cas_objects) object_count,
  (SELECT coalesce(sum(size),0) FROM efs_cas_objects) object_bytes,
  (SELECT count(*) FROM efs_manifest_roots) manifest_root_count,
  (SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_roots) manifest_root_bytes,
  (SELECT count(*) FROM efs_manifest_nodes) manifest_node_count,
  (SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_nodes) manifest_node_bytes,
  (SELECT count(*) FROM efs_cow_page_versions) page_count,
  (SELECT coalesce(sum(length(bytes)),0) FROM efs_cow_page_versions) page_bytes,
  (SELECT count(*) FROM efs_patches) patch_count,
  (SELECT coalesce(sum(length(bytes)),0) FROM efs_patch_segments) patch_bytes,
  (${DIRECT_STAGING_BYTES_SQL.replace(/^SELECT /u, "").replace(/ value$/u, "")}) staging_bytes,
  (${DIRECT_INGEST_RESERVATION_SQL.replace(/ value FROM/u, " FROM")}) ingest_reservation_bytes,
  (SELECT coalesce(sum(length(encoded)),0) FROM efs_operation_results) result_bytes,
  ((SELECT (count(*)*${GC_MARK_RESERVATION_BYTES}) FROM efs_cas_objects)+(SELECT (count(*)*${GC_MARK_RESERVATION_BYTES}) FROM efs_manifest_roots)+(SELECT (count(*)*${GC_MARK_RESERVATION_BYTES}) FROM efs_manifest_nodes)+(SELECT count(*)*${CHARGED_ROW_BYTES}+coalesce(sum(length(root_id)),0) FROM efs_root_journal)+(SELECT count(*)*512+coalesce(sum(2*length(CAST(id AS BLOB))),0) FROM efs_gc_runs)+(SELECT count(*)*${CHARGED_ROW_BYTES} FROM efs_lease_cleanups)+(SELECT count(*)*${STORAGE_SNAPSHOT_STATE_BYTES} FROM efs_storage_snapshots)+(SELECT count(*)*${CHARGED_ROW_BYTES}+coalesce(sum(length(root_id)),0) FROM efs_root_holds)) maintenance_bytes,
  ((SELECT count(*) FROM efs_branch_ids)+(SELECT count(*) FROM efs_operation_ids)) permanent_identifiers,
  (${DIRECT_CHARGED_METADATA_EXPRESSION}) charged_metadata_bytes`;

export const USAGE_COUNTER_COLUMNS = Object.freeze([
  "object_count",
  "object_bytes",
  "manifest_root_count",
  "manifest_root_bytes",
  "manifest_node_count",
  "manifest_node_bytes",
  "page_count",
  "page_bytes",
  "patch_count",
  "patch_bytes",
  "staging_bytes",
  "ingest_reservation_bytes",
  "result_bytes",
  "maintenance_bytes",
  "permanent_identifiers",
  "charged_metadata_bytes",
] as const);

export const USAGE_INTEGRITY_SQL = [...USAGE_COUNTER_COLUMNS, "mutation_sequence"]
  .map((column) => `CAST(${column} AS TEXT)`)
  .join("||':'||");

export type UsageCounter = (typeof USAGE_COUNTER_COLUMNS)[number];
export type UsageDelta = Partial<Readonly<Record<UsageCounter, number>>>;
export type UsageSnapshot = SqliteRow &
  Readonly<Record<UsageCounter, number>> & {
    readonly mutation_sequence: number;
    readonly integrity_token: string;
  };

const usageSnapshots = new WeakMap<FilesystemSQLiteTransaction, UsageSnapshot>();

export function usageIntegrityToken(
  usage: Readonly<Record<UsageCounter, number>> & {
    readonly mutation_sequence: number;
  },
): string {
  const columns = [...USAGE_COUNTER_COLUMNS, "mutation_sequence"] as const;
  return columns.map((column) => String(usage[column])).join(":");
}

type RecountKeyKind = "number" | "string" | "blob";
interface RecountPhase {
  readonly table: string;
  readonly keys: readonly {
    readonly column: string;
    readonly kind: RecountKeyKind;
  }[];
  readonly contributions: UsageDeltaSql;
}
type UsageDeltaSql = Partial<Readonly<Record<UsageCounter, string>>>;

function metadataContributions(
  extra: UsageDeltaSql = {},
  variableBytes = "0",
): UsageDeltaSql {
  return Object.freeze({
    ...extra,
    charged_metadata_bytes:
      variableBytes === "0"
        ? String(CHARGED_ROW_BYTES)
        : `${CHARGED_ROW_BYTES}+(${variableBytes})`,
  });
}

const key = (column: string, kind: RecountKeyKind) => Object.freeze({ column, kind });
const activeLeaseSize =
  "CASE WHEN EXISTS(SELECT 1 FROM efs_leases l WHERE l.id=t.lease_id AND l.state IN (0,1)) THEN t.size ELSE 0 END";
const activeIngestReservation =
  "CASE WHEN EXISTS(SELECT 1 FROM efs_leases l WHERE l.id=t.lease_id AND l.state IN (0,1)) THEN t.ingest_reservation_bytes ELSE 0 END";

const USAGE_RECOUNT_PHASES: readonly RecountPhase[] = Object.freeze([
  {
    table: "efs_cas_objects",
    keys: [key("hash", "blob")],
    contributions: metadataContributions({
      object_count: "1",
      object_bytes: "t.size",
      maintenance_bytes: String(GC_MARK_RESERVATION_BYTES),
    }),
  },
  {
    table: "efs_manifest_roots",
    keys: [key("hash", "blob")],
    contributions: metadataContributions({
      manifest_root_count: "1",
      manifest_root_bytes: "length(t.encoded)",
      maintenance_bytes: String(GC_MARK_RESERVATION_BYTES),
    }),
  },
  {
    table: "efs_manifest_nodes",
    keys: [key("hash", "blob")],
    contributions: metadataContributions({
      manifest_node_count: "1",
      manifest_node_bytes: "length(t.encoded)",
      maintenance_bytes: String(GC_MARK_RESERVATION_BYTES),
    }),
  },
  {
    table: "efs_manifest_subtree_summaries",
    keys: [key("node_hash", "blob")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_manifest_validations",
    keys: [key("manifest_hash", "blob")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_revisions",
    keys: [key("revision", "number")],
    contributions: metadataContributions({}, "length(CAST(t.writer_id AS BLOB))"),
  },
  {
    table: "efs_inodes",
    keys: [key("id", "string")],
    contributions: metadataContributions(
      {},
      "coalesce(length(CAST(t.symlink_target AS BLOB)),0)",
    ),
  },
  {
    table: "efs_entries",
    keys: [key("parent_inode", "string"), key("name_sort", "blob")],
    contributions: metadataContributions(
      {},
      "length(t.name_sort)+coalesce(length(CAST(t.name AS BLOB)),0)",
    ),
  },
  {
    table: "efs_inode_revisions",
    keys: [key("revision", "number"), key("inode_id", "string")],
    contributions: metadataContributions({}, "coalesce(length(t.encoded),0)"),
  },
  {
    table: "efs_revision_manifest_roots",
    keys: [
      key("revision", "number"),
      key("inode_id", "string"),
      key("manifest_hash", "blob"),
    ],
    contributions: metadataContributions(),
  },
  {
    table: "efs_entry_revisions",
    keys: [
      key("revision", "number"),
      key("parent_inode", "string"),
      key("name_sort", "blob"),
    ],
    contributions: metadataContributions(
      {},
      "length(t.name_sort)+coalesce(length(t.encoded),0)",
    ),
  },
  {
    table: "efs_branches",
    keys: [key("id", "string")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_branch_ids",
    keys: [key("id", "string")],
    contributions: metadataContributions({ permanent_identifiers: "1" }),
  },
  {
    table: "efs_branch_changes",
    keys: [key("branch_id", "string"), key("path", "blob")],
    contributions: metadataContributions(
      {},
      "length(t.path)+coalesce(length(t.encoded),0)",
    ),
  },
  {
    table: "efs_branch_inode_expectations",
    keys: [key("branch_id", "string"), key("inode_id", "string")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_branch_inode_overlays",
    keys: [key("branch_id", "string"), key("inode_id", "string")],
    contributions: metadataContributions({}, "length(t.encoded)"),
  },
  {
    table: "efs_branch_manifest_roots",
    keys: [
      key("branch_id", "string"),
      key("path", "blob"),
      key("manifest_hash", "blob"),
    ],
    contributions: metadataContributions({}, "length(t.path)"),
  },
  {
    table: "efs_subtree_tokens",
    keys: [key("inode_id", "string")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_cow_page_versions",
    keys: [
      key("branch_id", "string"),
      key("inode_id", "string"),
      key("page_index", "number"),
      key("generation", "number"),
    ],
    contributions: metadataContributions({
      page_count: "1",
      page_bytes: "length(t.bytes)",
    }),
  },
  {
    table: "efs_cow_page_heads",
    keys: [
      key("branch_id", "string"),
      key("inode_id", "string"),
      key("page_index", "number"),
    ],
    contributions: metadataContributions(),
  },
  {
    table: "efs_patches",
    keys: [
      key("branch_id", "string"),
      key("inode_id", "string"),
      key("sequence", "number"),
    ],
    contributions: metadataContributions({
      patch_count: "1",
    }),
  },
  {
    table: "efs_patch_segments",
    keys: [
      key("branch_id", "string"),
      key("inode_id", "string"),
      key("sequence", "number"),
      key("segment_index", "number"),
    ],
    contributions: metadataContributions({ patch_bytes: "length(t.bytes)" }),
  },
  {
    table: "efs_leases",
    keys: [key("id", "string")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_lease_manifests",
    keys: [key("lease_id", "string"), key("manifest_hash", "blob")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_lease_objects",
    keys: [key("lease_id", "string"), key("object_hash", "blob")],
    contributions: metadataContributions({ staging_bytes: activeLeaseSize }),
  },
  {
    table: "efs_lease_staged_manifests",
    keys: [
      key("lease_id", "string"),
      key("kind", "number"),
      key("manifest_hash", "blob"),
    ],
    contributions: metadataContributions({ staging_bytes: activeLeaseSize }),
  },
  {
    table: "efs_staging_entries",
    keys: [key("lease_id", "string"), key("entry_index", "number")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_staging_level_records",
    keys: [
      key("lease_id", "string"),
      key("level", "number"),
      key("record_index", "number"),
    ],
    contributions: metadataContributions(),
  },
  {
    table: "efs_lease_cow_pages",
    keys: [
      key("lease_id", "string"),
      key("branch_id", "string"),
      key("inode_id", "string"),
      key("page_index", "number"),
      key("generation", "number"),
    ],
    contributions: metadataContributions(),
  },
  {
    table: "efs_lease_patches",
    keys: [
      key("lease_id", "string"),
      key("branch_id", "string"),
      key("inode_id", "string"),
      key("sequence", "number"),
    ],
    contributions: metadataContributions(),
  },
  {
    table: "efs_staging_certificates",
    keys: [key("lease_id", "string")],
    contributions: metadataContributions(
      { ingest_reservation_bytes: activeIngestReservation },
      "t.metadata_reservation_bytes",
    ),
  },
  {
    table: "efs_staging_reconciliations",
    keys: [key("lease_id", "string")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_staging_reconciliation_queue",
    keys: [key("lease_id", "string"), key("kind", "number"), key("hash", "blob")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_staging_manifest_validation_queue",
    keys: [key("lease_id", "string"), key("path", "blob")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_staging_workspaces",
    keys: [key("lease_id", "string")],
    contributions: metadataContributions({}, "length(t.cdc_buffer)"),
  },
  {
    table: "efs_staging_reused_subtrees",
    keys: [key("lease_id", "string"), key("node_hash", "blob")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_operation_ids",
    keys: [key("id", "string")],
    contributions: metadataContributions({ permanent_identifiers: "1" }),
  },
  {
    table: "efs_operation_results",
    keys: [key("operation_id", "string")],
    contributions: metadataContributions({ result_bytes: "length(t.encoded)" }),
  },
  {
    table: "efs_revision_checkpoints",
    keys: [key("target_revision", "number")],
    contributions: metadataContributions(),
  },
  {
    table: "efs_checkpoint_inodes",
    keys: [key("target_revision", "number"), key("inode_id", "string")],
    contributions: metadataContributions({}, "coalesce(length(t.encoded),0)"),
  },
  {
    table: "efs_checkpoint_entries",
    keys: [
      key("target_revision", "number"),
      key("parent_inode", "string"),
      key("name_sort", "blob"),
    ],
    contributions: metadataContributions(
      {},
      "length(t.name_sort)+coalesce(length(t.encoded),0)",
    ),
  },
  {
    table: "efs_checkpoint_manifest_roots",
    keys: [
      key("target_revision", "number"),
      key("inode_id", "string"),
      key("manifest_hash", "blob"),
    ],
    contributions: metadataContributions(),
  },
  {
    table: "efs_root_journal",
    keys: [key("generation", "number")],
    contributions: { maintenance_bytes: `${CHARGED_ROW_BYTES}+length(t.root_id)` },
  },
  {
    table: "efs_gc_runs",
    keys: [key("id", "string")],
    contributions: { maintenance_bytes: "512+2*length(CAST(t.id AS BLOB))" },
  },
  {
    table: "efs_gc_marks",
    keys: [key("run_id", "string"), key("kind", "number"), key("hash", "blob")],
    contributions: {},
  },
  {
    table: "efs_lease_cleanups",
    keys: [key("lease_id", "string")],
    contributions: { maintenance_bytes: String(CHARGED_ROW_BYTES) },
  },
  {
    table: "efs_storage_snapshots",
    keys: [key("singleton", "number")],
    contributions: { maintenance_bytes: String(STORAGE_SNAPSHOT_STATE_BYTES) },
  },
  {
    table: "efs_storage_marks",
    keys: [key("kind", "number"), key("hash", "blob")],
    contributions: {},
  },
  {
    table: "efs_root_holds",
    keys: [key("id", "string")],
    contributions: {
      maintenance_bytes: `${CHARGED_ROW_BYTES}+length(t.root_id)`,
    },
  },
]);

if (
  USAGE_RECOUNT_PHASES.length !== DIRECT_USAGE_TABLES.length ||
  USAGE_RECOUNT_PHASES.some(
    (phase, index) => phase.table !== DIRECT_USAGE_TABLES[index],
  )
)
  throw new Error("usage recount phases differ from the authoritative table list");

export const USAGE_RECOUNT_PHASE_COUNT = USAGE_RECOUNT_PHASES.length;
export interface UsageRecountBatch {
  readonly checkedRows: number;
  readonly deltas: readonly number[];
  readonly nextKey: string | null;
  readonly complete: boolean;
}

function encodeRecountKey(phase: RecountPhase, row: SqliteRow): string {
  return JSON.stringify(
    phase.keys.map(({ kind }, index) => {
      const value = row[`__key${index}`];
      if (kind === "blob") {
        if (!(value instanceof Uint8Array))
          throw new Error("ECORRUPT: usage recount returned a non-BLOB key");
        return { blob: bytesToHex(value) };
      }
      if (kind === "number") {
        if (!Number.isSafeInteger(value))
          throw new Error("ECORRUPT: usage recount returned an invalid numeric key");
        return value;
      }
      if (typeof value !== "string")
        throw new Error("ECORRUPT: usage recount returned a non-text key");
      return value;
    }),
  );
}

function decodeRecountKey(phase: RecountPhase, encoded: string): SqliteValue[] {
  let values: unknown;
  try {
    values = JSON.parse(encoded);
  } catch {
    throw new RangeError("invalid usage recount key");
  }
  if (!Array.isArray(values) || values.length !== phase.keys.length)
    throw new RangeError("invalid usage recount key");
  return values.map((value, index) => {
    const kind = phase.keys[index]!.kind;
    if (kind === "number") {
      if (!Number.isSafeInteger(value))
        throw new RangeError("invalid usage recount key");
      return value as number;
    }
    if (kind === "string") {
      if (typeof value !== "string") throw new RangeError("invalid usage recount key");
      return value;
    }
    if (
      !value ||
      typeof value !== "object" ||
      !("blob" in value) ||
      typeof value.blob !== "string"
    )
      throw new RangeError("invalid usage recount key");
    try {
      return value.blob === "" ? new Uint8Array() : hexToBytes(value.blob);
    } catch {
      throw new RangeError("invalid usage recount key");
    }
  });
}

function add(left: number, right: number, name: string): number {
  if (!Number.isSafeInteger(right)) throw new RangeError(`invalid usage delta ${name}`);
  if (right >= 0) return checkedAdd(left, right, `usage ${name}`);
  const value = left + right;
  if (!Number.isSafeInteger(value) || value < 0)
    throw new Error(`ECORRUPT: usage counter underflow for ${name}`);
  return value;
}

/** The sole authority for durable quota admission and usage-counter mutation. */
export class UsageRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  constructor(tx: FilesystemSQLiteTransaction, limits: StorageLimits) {
    this.#tx = tx;
    this.#limits = limits;
  }

  snapshot(): UsageSnapshot {
    const batch = usageMutationBatches.get(this.#tx);
    if (batch) return batchedUsageSnapshot(batch);
    const cached = usageSnapshots.get(this.#tx);
    if (cached) return cached;
    const row = this.#tx.all<UsageSnapshot>(
      `SELECT ${USAGE_COUNTER_COLUMNS.join(",")},mutation_sequence,integrity_token FROM efs_usage WHERE singleton=1`,
      [],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (!row) throw new Error("ECORRUPT: missing usage singleton");
    for (const column of [...USAGE_COUNTER_COLUMNS, "mutation_sequence"] as const)
      if (!Number.isSafeInteger(row[column]) || row[column] < 0)
        throw new Error(`ECORRUPT: invalid usage counter ${column}`);
    if (
      typeof row.integrity_token !== "string" ||
      row.integrity_token !== usageIntegrityToken(row)
    )
      throw new Error("ECORRUPT: usage integrity token mismatch");
    const snapshot = Object.freeze(row) as UsageSnapshot;
    usageSnapshots.set(this.#tx, snapshot);
    return snapshot;
  }

  apply(
    delta: UsageDelta,
    reason = "durable storage",
    options: { readonly preserveMaintenanceBytes?: number } = {},
  ): UsageSnapshot {
    const current = this.snapshot();
    const next = {} as Record<UsageCounter, number>;
    for (const column of USAGE_COUNTER_COLUMNS)
      next[column] = add(current[column], delta[column] ?? 0, column);

    const contentBytes = checkedAdd(
      checkedAdd(next.object_bytes, next.manifest_root_bytes, "content payload"),
      next.manifest_node_bytes,
      "content payload",
    );
    const overlayBytes = checkedAdd(
      next.page_bytes,
      next.patch_bytes,
      "branch overlay payload",
    );
    let normalBytes = checkedAdd(contentBytes, overlayBytes, "managed payload");
    normalBytes = checkedAdd(normalBytes, next.staging_bytes, "managed payload");
    normalBytes = checkedAdd(
      normalBytes,
      next.ingest_reservation_bytes,
      "managed payload",
    );
    normalBytes = checkedAdd(normalBytes, next.result_bytes, "managed payload");
    if (
      normalBytes >
      this.#limits.maxManagedPayloadBytes - this.#limits.maintenanceReserveBytes
    )
      throw new Error(`ENOSPC: ${reason} exceeds aggregate managed payload quota`);
    const managedWithMaintenance = checkedAdd(
      normalBytes,
      next.maintenance_bytes,
      "managed payload",
    );
    if (
      checkedAdd(
        managedWithMaintenance,
        options.preserveMaintenanceBytes ?? 0,
        "managed maintenance progress",
      ) > this.#limits.maxManagedPayloadBytes
    )
      throw new Error(
        `ENOSPC: ${reason} exceeds managed payload including maintenance reserve`,
      );
    if (overlayBytes > this.#limits.maxBranchOverlayBytes)
      throw new Error(`ENOSPC: ${reason} exceeds branch overlay quota`);
    if (next.staging_bytes > this.#limits.maxStagingPayloadBytes)
      throw new Error(`ENOSPC: ${reason} exceeds staging payload quota`);
    const maintenanceLimit =
      this.#limits.maxMaintenanceBytes - (options.preserveMaintenanceBytes ?? 0);
    if (maintenanceLimit < 0 || next.maintenance_bytes > maintenanceLimit)
      throw new Error(`ENOSPC: ${reason} exceeds maintenance quota`);
    if (next.permanent_identifiers > this.#limits.maxPermanentIdentifiers)
      throw new Error(`ENOSPC: ${reason} exceeds permanent identifier quota`);
    if (next.charged_metadata_bytes > this.#limits.maxChargedMetadataBytes)
      throw new Error(`ENOSPC: ${reason} exceeds charged metadata quota`);

    const changed = USAGE_COUNTER_COLUMNS.filter(
      (column) => (delta[column] ?? 0) !== 0,
    );
    if (!changed.length) return current;
    const batch = usageMutationBatches.get(this.#tx);
    if (batch) {
      for (const column of changed) {
        const prior = batch.delta[column] ?? 0;
        const change = delta[column] ?? 0;
        const nextDelta = prior + change;
        if (!Number.isSafeInteger(nextDelta))
          throw new RangeError(`usage ${column} batch exceeds safe integer range`);
        batch.delta[column] = nextDelta;
      }
      return batchedUsageSnapshot(batch);
    }
    const mutationSequence = checkedAdd(
      current.mutation_sequence,
      1,
      "usage mutation sequence",
    );
    const integrityToken = usageIntegrityToken({
      ...next,
      mutation_sequence: mutationSequence,
    });
    const result = this.#tx.run(
      `UPDATE efs_usage SET ${changed
        .map((column) => `${column}=${column}+?`)
        .join(
          ",",
        )},mutation_sequence=mutation_sequence+1,integrity_token=? WHERE singleton=1 AND mutation_sequence=?`,
      [
        ...changed.map((column) => delta[column] ?? 0),
        integrityToken,
        current.mutation_sequence,
      ],
    );
    if (result.changes !== 1)
      throw new Error("ECORRUPT: concurrent or missing usage singleton update");
    const snapshot = Object.freeze({
      ...next,
      mutation_sequence: mutationSequence,
      integrity_token: integrityToken,
    }) as UsageSnapshot;
    usageSnapshots.set(this.#tx, snapshot);
    return snapshot;
  }

  touch(reason = "durable usage contributor transfer"): void {
    const batch = usageMutationBatches.get(this.#tx);
    if (batch) {
      batch.touched = true;
      return;
    }
    const current = this.snapshot();
    const mutationSequence = checkedAdd(
      current.mutation_sequence,
      1,
      "usage mutation sequence",
    );
    const integrityToken = usageIntegrityToken({
      ...current,
      mutation_sequence: mutationSequence,
    });
    const changed = this.#tx.run(
      "UPDATE efs_usage SET mutation_sequence=mutation_sequence+1,integrity_token=? WHERE singleton=1 AND mutation_sequence=?",
      [integrityToken, current.mutation_sequence],
    );
    if (changed.changes !== 1)
      throw new Error(`ECORRUPT: ${reason} lost the usage authority`);
    const snapshot = Object.freeze({
      ...current,
      mutation_sequence: mutationSequence,
      integrity_token: integrityToken,
    }) as UsageSnapshot;
    usageSnapshots.set(this.#tx, snapshot);
  }

  recountBatch(
    phaseIndex: number,
    afterKey: string | null,
    limit: number,
    maxBytes: number,
  ): UsageRecountBatch {
    if (
      !Number.isSafeInteger(phaseIndex) ||
      phaseIndex < 0 ||
      phaseIndex >= USAGE_RECOUNT_PHASES.length
    )
      throw new RangeError("invalid usage recount phase");
    if (
      !Number.isSafeInteger(limit) ||
      limit <= 0 ||
      limit > this.#limits.maxQueryBatchSize
    )
      throw new RangeError("invalid usage recount row limit");
    if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0)
      throw new RangeError("invalid usage recount byte limit");
    const phase = USAGE_RECOUNT_PHASES[phaseIndex]!;
    const bindings = afterKey === null ? [] : decodeRecountKey(phase, afterKey);
    const keyColumns = phase.keys.map(({ column }) => `t.${column}`);
    const keyProjection = keyColumns
      .map((column, index) => `${column} __key${index}`)
      .join(",");
    const contributionProjection = Object.entries(phase.contributions)
      .map(([column, expression]) => `${expression} ${column}`)
      .join(",");
    const where =
      afterKey === null
        ? ""
        : keyColumns.length === 1
          ? `WHERE ${keyColumns[0]}>?`
          : `WHERE (${keyColumns.join(",")})>(${keyColumns.map(() => "?").join(",")})`;
    const rows = this.#tx.all(
      `SELECT ${keyProjection}${contributionProjection ? `,${contributionProjection}` : ""} FROM ${phase.table} t ${where} ORDER BY ${keyColumns.join(",")} LIMIT ?`,
      [...bindings, limit],
      { maxRows: limit, maxBytes },
    );
    const deltas = USAGE_COUNTER_COLUMNS.map(() => 0);
    for (const row of rows)
      for (const [column, index] of USAGE_COUNTER_COLUMNS.map(
        (name, index) => [name, index] as const,
      )) {
        const value = row[column];
        if (value === undefined) continue;
        if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
          throw new Error(`ECORRUPT: invalid usage recount contribution ${column}`);
        deltas[index] = checkedAdd(deltas[index]!, value, `usage recount ${column}`);
      }
    return Object.freeze({
      checkedRows: rows.length,
      deltas: Object.freeze(deltas),
      nextKey: rows.length ? encodeRecountKey(phase, rows.at(-1)!) : null,
      complete: rows.length < limit,
    });
  }

  directChargedMetadataBytes(): number {
    this.#assertDirectRecountBounded();
    const value = this.#tx.all<{ value: number } & SqliteRow>(
      DIRECT_CHARGED_METADATA_SQL,
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.value;
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
      throw new Error("ECORRUPT: invalid direct charged-metadata recount");
    return value;
  }

  directStagingBytes(): number {
    this.#assertDirectRecountBounded();
    const value = this.#tx.all<{ value: number } & SqliteRow>(
      DIRECT_STAGING_BYTES_SQL,
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.value;
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
      throw new Error("ECORRUPT: invalid direct staging-payload recount");
    return value;
  }

  directIngestReservationBytes(): number {
    this.#assertDirectRecountBounded();
    const value = this.#tx.all<{ value: number } & SqliteRow>(
      DIRECT_INGEST_RESERVATION_SQL,
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.value;
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
      throw new Error("ECORRUPT: invalid direct ingest-reservation recount");
    return value;
  }

  directUsage(): Readonly<Record<UsageCounter, number>> {
    this.#assertDirectRecountBounded();
    const row = this.#tx.all<Readonly<Record<UsageCounter, number>> & SqliteRow>(
      DIRECT_USAGE_SQL,
      [],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (!row) throw new Error("ECORRUPT: direct usage recount returned no row");
    for (const column of USAGE_COUNTER_COLUMNS)
      if (!Number.isSafeInteger(row[column]) || row[column] < 0)
        throw new Error(`ECORRUPT: invalid direct usage counter ${column}`);
    return Object.freeze(
      Object.fromEntries(
        USAGE_COUNTER_COLUMNS.map((column) => [column, row[column]]),
      ) as unknown as Readonly<Record<UsageCounter, number>>,
    );
  }

  reconcileChargedMetadata(reason = "durable metadata rows"): UsageSnapshot {
    const current = this.snapshot();
    const direct = this.directChargedMetadataBytes();
    return this.apply(
      { charged_metadata_bytes: direct - current.charged_metadata_bytes },
      reason,
    );
  }

  reconcileDerivedUsage(reason = "durable row accounting"): UsageSnapshot {
    const current = this.snapshot();
    const direct = this.directUsage();
    return this.apply(
      Object.fromEntries(
        USAGE_COUNTER_COLUMNS.map((column) => [
          column,
          direct[column] - current[column],
        ]),
      ) as UsageDelta,
      reason,
    );
  }

  verifyChargedMetadata(): void {
    const current = this.snapshot();
    const direct = this.directChargedMetadataBytes();
    if (direct !== current.charged_metadata_bytes)
      throw new Error(
        `ECORRUPT: charged metadata differs from direct recount (${current.charged_metadata_bytes} != ${direct})`,
      );
  }

  verifyDerivedUsage(): void {
    const current = this.snapshot();
    const direct = this.directUsage();
    for (const column of USAGE_COUNTER_COLUMNS)
      if (direct[column] !== current[column])
        throw new Error(`ECORRUPT: ${column} differs from bounded direct recount`);
  }

  #assertDirectRecountBounded(): void {
    let remaining = this.#limits.maxQueryBatchSize;
    for (const table of DIRECT_USAGE_TABLES) {
      if (remaining <= 0)
        throw new RangeError(
          "direct usage recount exceeds the configured bounded row envelope",
        );
      const limit = remaining + 1;
      const rows = this.#tx.all(`SELECT 1 present FROM ${table} LIMIT ?`, [limit], {
        maxRows: limit,
        maxBytes: limit * 64,
      });
      if (rows.length > remaining)
        throw new RangeError(
          "direct usage recount exceeds the configured bounded row envelope",
        );
      remaining -= rows.length;
    }
  }
}

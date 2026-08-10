import { checkedAdd } from "../resources/safe-integers.js";
import type { StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";

export const CHARGED_ROW_BYTES = 96;

export const CHARGED_METADATA_TABLES = Object.freeze([
  "efs_cas_objects",
  "efs_manifest_roots",
  "efs_manifest_nodes",
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
  "efs_branch_manifest_roots",
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
  "efs_staging_workspaces",
  "efs_staging_reused_subtrees",
  "efs_operation_ids",
  "efs_operation_results",
] as const);

export const DIRECT_CHARGED_METADATA_SQL = `SELECT ${CHARGED_ROW_BYTES}*(${CHARGED_METADATA_TABLES.map(
  (table) => `(SELECT count(*) FROM ${table})`,
).join("+")}) value`;
export const DIRECT_STAGING_BYTES_SQL =
  "SELECT (SELECT coalesce(sum(o.size),0) FROM efs_lease_objects o JOIN efs_leases l ON l.id=o.lease_id WHERE l.state IN (0,1))+(SELECT coalesce(sum(m.size),0) FROM efs_lease_staged_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1)) value";
export const DIRECT_INGEST_RESERVATION_SQL =
  "SELECT coalesce(sum(c.ingest_reservation_bytes),0) value FROM efs_staging_certificates c JOIN efs_leases l ON l.id=c.lease_id WHERE l.state IN (0,1)";

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

type UsageCounter = (typeof USAGE_COUNTER_COLUMNS)[number];
export type UsageDelta = Partial<Readonly<Record<UsageCounter, number>>>;
export type UsageSnapshot = SqliteRow &
  Readonly<Record<UsageCounter, number>> & {
    readonly mutation_sequence: number;
  };

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
    const row = this.#tx.all<UsageSnapshot>(
      `SELECT ${USAGE_COUNTER_COLUMNS.join(",")},mutation_sequence FROM efs_usage WHERE singleton=1`,
      [],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (!row) throw new Error("ECORRUPT: missing usage singleton");
    for (const column of [...USAGE_COUNTER_COLUMNS, "mutation_sequence"] as const)
      if (!Number.isSafeInteger(row[column]) || row[column] < 0)
        throw new Error(`ECORRUPT: invalid usage counter ${column}`);
    return row;
  }

  apply(delta: UsageDelta, reason = "durable storage"): UsageSnapshot {
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
    if (
      checkedAdd(normalBytes, next.maintenance_bytes, "managed payload") >
      this.#limits.maxManagedPayloadBytes
    )
      throw new Error(
        `ENOSPC: ${reason} exceeds managed payload including maintenance reserve`,
      );
    if (overlayBytes > this.#limits.maxBranchOverlayBytes)
      throw new Error(`ENOSPC: ${reason} exceeds branch overlay quota`);
    if (next.staging_bytes > this.#limits.maxStagingPayloadBytes)
      throw new Error(`ENOSPC: ${reason} exceeds staging payload quota`);
    if (next.maintenance_bytes > this.#limits.maxMaintenanceBytes)
      throw new Error(`ENOSPC: ${reason} exceeds maintenance quota`);
    if (next.permanent_identifiers > this.#limits.maxPermanentIdentifiers)
      throw new Error(`ENOSPC: ${reason} exceeds permanent identifier quota`);
    if (next.charged_metadata_bytes > this.#limits.maxChargedMetadataBytes)
      throw new Error(`ENOSPC: ${reason} exceeds charged metadata quota`);

    const changed = USAGE_COUNTER_COLUMNS.filter(
      (column) => (delta[column] ?? 0) !== 0,
    );
    if (!changed.length) return current;
    const result = this.#tx.run(
      `UPDATE efs_usage SET ${changed
        .map((column) => `${column}=${column}+?`)
        .join(
          ",",
        )},mutation_sequence=mutation_sequence+1 WHERE singleton=1 AND mutation_sequence=?`,
      [...changed.map((column) => delta[column] ?? 0), current.mutation_sequence],
    );
    if (result.changes !== 1)
      throw new Error("ECORRUPT: concurrent or missing usage singleton update");
    return Object.freeze({
      ...next,
      mutation_sequence: checkedAdd(
        current.mutation_sequence,
        1,
        "usage mutation sequence",
      ),
    }) as UsageSnapshot;
  }

  directChargedMetadataBytes(): number {
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
    const value = this.#tx.all<{ value: number } & SqliteRow>(
      DIRECT_INGEST_RESERVATION_SQL,
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.value;
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
      throw new Error("ECORRUPT: invalid direct ingest-reservation recount");
    return value;
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
    const chargedMetadata = this.directChargedMetadataBytes();
    const stagingBytes = this.directStagingBytes();
    const ingestReservationBytes = this.directIngestReservationBytes();
    return this.apply(
      {
        charged_metadata_bytes: chargedMetadata - current.charged_metadata_bytes,
        staging_bytes: stagingBytes - current.staging_bytes,
        ingest_reservation_bytes:
          ingestReservationBytes - current.ingest_reservation_bytes,
      },
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
    const metadata = this.directChargedMetadataBytes();
    const staging = this.directStagingBytes();
    const ingestReservation = this.directIngestReservationBytes();
    if (metadata !== current.charged_metadata_bytes)
      throw new Error("ECORRUPT: charged metadata differs from direct recount");
    if (staging !== current.staging_bytes)
      throw new Error("ECORRUPT: logical staging bytes differ from direct recount");
    if (ingestReservation !== current.ingest_reservation_bytes)
      throw new Error("ECORRUPT: ingest reservation differs from direct recount");
  }
}

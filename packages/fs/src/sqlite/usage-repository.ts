import { checkedAdd } from "../resources/safe-integers.js";
import type { StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";

export const CHARGED_ROW_BYTES = 96;

const COUNTER_COLUMNS = [
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
  "result_bytes",
  "maintenance_bytes",
  "permanent_identifiers",
  "charged_metadata_bytes",
] as const;

type UsageCounter = (typeof COUNTER_COLUMNS)[number];
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
      `SELECT ${COUNTER_COLUMNS.join(",")},mutation_sequence FROM efs_usage WHERE singleton=1`,
      [],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (!row) throw new Error("ECORRUPT: missing usage singleton");
    for (const column of [...COUNTER_COLUMNS, "mutation_sequence"] as const)
      if (!Number.isSafeInteger(row[column]) || row[column] < 0)
        throw new Error(`ECORRUPT: invalid usage counter ${column}`);
    return row;
  }

  apply(delta: UsageDelta, reason = "durable storage"): UsageSnapshot {
    const current = this.snapshot();
    const next = {} as Record<UsageCounter, number>;
    for (const column of COUNTER_COLUMNS)
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

    const changed = COUNTER_COLUMNS.filter((column) => (delta[column] ?? 0) !== 0);
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
}

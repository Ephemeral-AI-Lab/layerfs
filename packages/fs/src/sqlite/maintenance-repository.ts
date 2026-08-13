import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import {
  MAX_MAINTENANCE_MARK_ROW_BYTES,
  type StorageLimits,
} from "../resources/limits.js";
import {
  CHARGED_ROW_BYTES,
  GC_MARK_RESERVATION_BYTES,
  STORAGE_SNAPSHOT_MARK_BYTES,
  USAGE_COUNTER_COLUMNS,
  USAGE_RECOUNT_PHASE_COUNT,
  UsageRepository,
  STORAGE_SNAPSHOT_STATE_BYTES,
} from "./usage-repository.js";
import { utf8ByteLength } from "../namespace/utf8.js";

const MAX_GC_RUN_ID_BYTES = 256;
const GC_RUN_BASE_BYTES = 512;
const GC_MARK_BASE_BYTES = 192;
const STORAGE_SCOPE_MAIN = 1;
const STORAGE_SCOPE_BRANCH = 2;
const STORAGE_SCOPE_OTHER = 4;

function historicManifestRootsSql(targetsSql: string): string {
  const checkpoint =
    "(SELECT max(cp.target_revision) FROM efs_revision_checkpoints cp WHERE cp.state=1 AND cp.target_revision<=t.revision)";
  return `SELECT DISTINCT 0 kind,m.hash hash FROM efs_manifest_roots m WHERE m.hash>? AND EXISTS(SELECT 1 FROM (${targetsSql}) t WHERE EXISTS(SELECT 1 FROM efs_checkpoint_inodes c JOIN efs_checkpoint_manifest_roots cm ON cm.target_revision=c.target_revision AND cm.inode_id=c.inode_id WHERE c.target_revision=${checkpoint} AND c.tombstone=0 AND cm.manifest_hash=m.hash AND NOT EXISTS(SELECT 1 FROM efs_inode_revisions newer WHERE newer.inode_id=c.inode_id AND newer.revision>${checkpoint} AND newer.revision<=t.revision)) OR EXISTS(SELECT 1 FROM efs_inode_revisions r JOIN efs_revision_manifest_roots rm ON rm.revision=r.revision AND rm.inode_id=r.inode_id WHERE r.revision>coalesce(${checkpoint},-1) AND r.revision<=t.revision AND r.tombstone=0 AND rm.manifest_hash=m.hash AND NOT EXISTS(SELECT 1 FROM efs_inode_revisions newer WHERE newer.inode_id=r.inode_id AND newer.revision>r.revision AND newer.revision<=t.revision))) ORDER BY m.hash LIMIT ?`;
}

function runIdBytes(runId: string): number {
  if (typeof runId !== "string" || runId.length === 0 || runId.includes("\0"))
    throw new RangeError("invalid garbage-collection run id");
  const bytes = utf8ByteLength(runId);
  if (bytes > MAX_GC_RUN_ID_BYTES)
    throw new RangeError("garbage-collection run id exceeds byte limit");
  return bytes;
}
function runCharge(runId: string): number {
  return GC_RUN_BASE_BYTES + runIdBytes(runId) * 2;
}

export interface GcRunRow extends SqliteRow {
  id: string;
  state: number;
  high_water: number;
  root_generation: number;
  cursor_kind: number;
  cursor_value: Uint8Array | null;
  examined_roots: number;
  deleted_roots: number;
  examined_nodes: number;
  deleted_nodes: number;
  examined_objects: number;
  deleted_objects: number;
  reclaimed_object_bytes: number;
  reclaimed_manifest_bytes: number;
  reclaimed_overlay_bytes: number;
}
export interface GcMarkRow extends SqliteRow {
  kind: number;
  hash: Uint8Array;
  edge_cursor: number;
  payload_size: number;
}
export interface PayloadRow extends SqliteRow {
  hash: Uint8Array;
  size: number;
  allocation_sequence: number;
  metadata_rows?: number;
  eligible?: number;
  scanned_count?: number;
  scanned_through?: number;
  eligible_count?: number;
}
export interface SnapshotRow extends SqliteRow {
  object_count: number;
  object_bytes: number;
  manifest_root_count: number;
  manifest_root_bytes: number;
  manifest_node_count: number;
  manifest_node_bytes: number;
  page_bytes: number;
  patch_bytes: number;
  charged_metadata_bytes: number;
  result_bytes: number;
  generation: number;
  logical_bytes: number;
  revisions: number;
}
export interface HashRow extends SqliteRow {
  hash: Uint8Array;
  encoded: Uint8Array;
}
export interface InodeVerifyRow extends SqliteRow {
  id: string;
  type: number;
  size: number | null;
  manifest_hash: Uint8Array | null;
  nlink: number;
  actual_links: number;
}
export interface StorageSnapshotRunRow extends SqliteRow {
  state: number;
  high_water: number;
  root_generation: number;
  last_root_removal_generation: number;
  evaluation_time_ms: number;
  next_root_expiry_ms: number | null;
  root_kind: number;
  root_cursor: Uint8Array | null;
  mark_kind: number;
  mark_cursor: Uint8Array | null;
  stored_kind: number;
  stored_cursor: number;
  logical_cursor: string;
  logical_complete: number;
  logical_bytes: number;
  overlay_kind: number;
  overlay_branch_cursor: string;
  overlay_inode_cursor: string;
  overlay_sequence_cursor: number;
  overlay_index_cursor: number;
  stored_page_bytes: number;
  stored_patch_bytes: number;
  reclaimable_overlay_bytes: number;
  result_bytes: number;
  charged_metadata_bytes: number;
  revision_count: number;
  stored_object_count: number;
  stored_object_bytes: number;
  stored_manifest_root_count: number;
  stored_manifest_root_bytes: number;
  stored_manifest_node_count: number;
  stored_manifest_node_bytes: number;
  reachable_object_count: number;
  reachable_object_bytes: number;
  reachable_manifest_root_count: number;
  reachable_manifest_root_bytes: number;
  reachable_manifest_node_count: number;
  reachable_manifest_node_bytes: number;
  branch_exclusive_object_bytes: number;
  branch_exclusive_manifest_root_bytes: number;
  branch_exclusive_manifest_node_bytes: number;
  committed_batches: number;
  created_at_ms: number;
  updated_at_ms: number;
  current?: number;
}
export interface StorageSnapshotMarkRow extends SqliteRow {
  kind: number;
  hash: Uint8Array;
  edge_cursor: number;
  accounted: number;
  scope_mask: number;
  payload_size: number;
}
export interface StoragePayloadRow extends SqliteRow {
  hash: Uint8Array;
  size: number;
  allocation_sequence: number;
  scope_mask: number;
}
export interface StorageInodeRow extends SqliteRow {
  id: string;
  size: number | null;
}

export class MaintenanceRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  constructor(tx: FilesystemSQLiteTransaction, limits: StorageLimits) {
    this.#tx = tx;
    this.#limits = limits;
  }
  beginRun(runId: string, now: number): void {
    runIdBytes(runId);
    if (this.run(runId)) return;
    if (
      this.#tx.all(
        "SELECT 1 present FROM efs_storage_snapshots WHERE state<>6 LIMIT 1",
        [],
        { maxRows: 1, maxBytes: 64 },
      ).length
    )
      throw new Error("EBUSY: storage snapshot owns the maintenance mark reserve");
    const active = this.#tx.all<{ id: string } & SqliteRow>(
      "SELECT id FROM efs_gc_runs WHERE state<>7 LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (active) throw new Error("EBUSY: another garbage-collection run is nonterminal");
    const meta = this.#tx.all<
      { next_allocation_sequence: number; root_mutation_generation: number } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: runCharge(runId) },
      "garbage-collection run",
      { preserveMaintenanceBytes: MAX_MAINTENANCE_MARK_ROW_BYTES },
    );
    this.#tx.run(
      "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES(?,0,?,?,0,NULL,?)",
      [runId, meta.next_allocation_sequence - 1, meta.root_mutation_generation, now],
    );
  }
  abandonRun(runId: string, completeState: number, abandonedState: number): void {
    this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=? AND state<>?", [
      abandonedState,
      runId,
      completeState,
    ]);
  }
  resumeAbandonedRun(
    runId: string,
    abandonedState: number,
    cleanupMarksState: number,
  ): void {
    const changed = this.#tx.run(
      "UPDATE efs_gc_runs SET state=? WHERE id=? AND state=?",
      [cleanupMarksState, runId, abandonedState],
    );
    if (changed.changes !== 1)
      throw new Error("ECORRUPT: abandoned garbage-collection run changed");
  }
  run(id: string): GcRunRow | undefined {
    runIdBytes(id);
    return this.#tx.all<GcRunRow>(
      "SELECT id,state,high_water,root_generation,cursor_kind,cursor_value,examined_roots,deleted_roots,examined_nodes,deleted_nodes,examined_objects,deleted_objects,reclaimed_object_bytes,reclaimed_manifest_bytes,reclaimed_overlay_bytes FROM efs_gc_runs WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  activeRun(): GcRunRow | undefined {
    return this.#tx.all<GcRunRow>(
      "SELECT id,state,high_water,root_generation,cursor_kind,cursor_value,examined_roots,deleted_roots,examined_nodes,deleted_nodes,examined_objects,deleted_objects,reclaimed_object_bytes,reclaimed_manifest_bytes,reclaimed_overlay_bytes FROM efs_gc_runs WHERE state<>7 ORDER BY created_at_ms,id LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  snapshot(): SnapshotRow | undefined {
    return this.#tx.all<SnapshotRow>(
      "SELECT u.object_count,u.object_bytes,u.manifest_root_count,u.manifest_root_bytes,u.manifest_node_count,u.manifest_node_bytes,u.page_bytes,u.patch_bytes,u.result_bytes,u.charged_metadata_bytes,m.root_mutation_generation generation,(SELECT coalesce(sum(size),0) FROM efs_inodes WHERE type=0) logical_bytes,(SELECT count(*) FROM efs_revisions) revisions FROM efs_usage u JOIN efs_meta m ON m.singleton=u.singleton",
      [],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  storageSnapshot(): StorageSnapshotRunRow | undefined {
    return this.#tx.all<StorageSnapshotRunRow>(
      "SELECT state,high_water,root_generation,last_root_removal_generation,evaluation_time_ms,next_root_expiry_ms,root_kind,root_cursor,mark_kind,mark_cursor,stored_kind,stored_cursor,logical_cursor,logical_complete,logical_bytes,overlay_kind,overlay_branch_cursor,overlay_inode_cursor,overlay_sequence_cursor,overlay_index_cursor,stored_page_bytes,stored_patch_bytes,reclaimable_overlay_bytes,result_bytes,charged_metadata_bytes,revision_count,stored_object_count,stored_object_bytes,stored_manifest_root_count,stored_manifest_root_bytes,stored_manifest_node_count,stored_manifest_node_bytes,reachable_object_count,reachable_object_bytes,reachable_manifest_root_count,reachable_manifest_root_bytes,reachable_manifest_node_count,reachable_manifest_node_bytes,branch_exclusive_object_bytes,branch_exclusive_manifest_root_bytes,branch_exclusive_manifest_node_bytes,committed_batches,created_at_ms,updated_at_ms FROM efs_storage_snapshots WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  storageSnapshotCurrent(now: number): boolean {
    return this.storageSnapshotResult(now)?.current === 1;
  }
  storageSnapshotResult(now: number): StorageSnapshotRunRow | undefined {
    if (!Number.isSafeInteger(now) || now < 0)
      throw new RangeError("invalid storage snapshot currentness time");
    return this.#tx.all<StorageSnapshotRunRow>(
      "SELECT s.*,CASE WHEN s.state=6 AND s.high_water=m.next_allocation_sequence-1 AND s.root_generation=m.root_mutation_generation AND s.last_root_removal_generation=m.last_root_removal_generation AND (s.next_root_expiry_ms IS NULL OR s.next_root_expiry_ms>?) AND s.stored_object_count=u.object_count AND s.stored_object_bytes=u.object_bytes AND s.stored_manifest_root_count=u.manifest_root_count AND s.stored_manifest_root_bytes=u.manifest_root_bytes AND s.stored_manifest_node_count=u.manifest_node_count AND s.stored_manifest_node_bytes=u.manifest_node_bytes AND s.stored_page_bytes=u.page_bytes AND s.stored_patch_bytes=u.patch_bytes AND s.result_bytes=u.result_bytes AND s.charged_metadata_bytes=u.charged_metadata_bytes AND s.revision_count=(SELECT count(*) FROM efs_revisions) THEN 1 ELSE 0 END current FROM efs_storage_snapshots s JOIN efs_meta m ON m.singleton=s.singleton JOIN efs_usage u ON u.singleton=s.singleton WHERE s.singleton=1",
      [now],
      { maxRows: 1, maxBytes: 8192 },
    )[0];
  }
  beginStorageSnapshot(now: number): void {
    if (!Number.isSafeInteger(now) || now < 0)
      throw new RangeError("invalid storage snapshot time");
    const meta = this.#tx.all<
      {
        next_allocation_sequence: number;
        root_mutation_generation: number;
        last_root_removal_generation: number;
      } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation,last_root_removal_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    if (
      this.#tx.all("SELECT 1 present FROM efs_gc_runs WHERE state<>7 LIMIT 1", [], {
        maxRows: 1,
        maxBytes: 64,
      }).length
    )
      throw new Error("EBUSY: garbage collection owns the maintenance mark reserve");
    const existing = this.storageSnapshot();
    if (!existing) {
      new UsageRepository(this.#tx, this.#limits).apply(
        { maintenance_bytes: STORAGE_SNAPSHOT_STATE_BYTES },
        "storage snapshot state",
        { preserveMaintenanceBytes: MAX_MAINTENANCE_MARK_ROW_BYTES },
      );
      this.#tx.run(
        "INSERT INTO efs_storage_snapshots(singleton,state,high_water,root_generation,last_root_removal_generation,evaluation_time_ms,root_kind,root_cursor,mark_kind,mark_cursor,stored_kind,stored_cursor,logical_cursor,created_at_ms,updated_at_ms) VALUES(1,1,?,?,?,?,0,NULL,0,NULL,0,0,'',?,?)",
        [
          meta.next_allocation_sequence - 1,
          meta.root_mutation_generation,
          meta.last_root_removal_generation,
          now,
          now,
          now,
        ],
      );
      return;
    }
    if (existing.state !== 6 || this.storageSnapshotCurrent(now)) return;
    this.#restartStorageSnapshot(meta, now, true, true);
  }
  recordStorageSnapshotBatch(): void {
    const changed = this.#tx.run(
      "UPDATE efs_storage_snapshots SET committed_batches=committed_batches+1 WHERE singleton=1",
      [],
    );
    if (changed.changes !== 1)
      throw new Error("ECORRUPT: storage snapshot batch state is missing");
  }
  storageRootBatch(limit: number, maxBytes: number, now: number): boolean {
    const run = this.storageSnapshot();
    if (!run || run.state !== 1)
      throw new Error("ECORRUPT: storage snapshot is not enumerating roots");
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("invalid storage snapshot root limit");
    if (!Number.isSafeInteger(now) || now < 0)
      throw new RangeError("invalid storage snapshot root time");
    const retained = this.#limits.maxRetainedRevisions;
    const queries = [
      {
        sql: "SELECT 0 kind,manifest_hash hash FROM efs_inodes WHERE manifest_hash IS NOT NULL AND manifest_hash>? ORDER BY manifest_hash LIMIT ?",
        bindings: (after: Uint8Array) => [after, limit],
        scope: STORAGE_SCOPE_MAIN,
      },
      {
        sql: historicManifestRootsSql(
          "SELECT r.revision FROM efs_revisions r JOIN efs_meta m ON m.singleton=1 WHERE r.revision>=CASE WHEN m.main_revision>=? THEN m.main_revision-?+1 ELSE 0 END AND r.revision<=m.main_revision",
        ),
        bindings: (after: Uint8Array) => [after, retained, retained, limit],
        scope: STORAGE_SCOPE_MAIN,
      },
      {
        sql: historicManifestRootsSql(
          "SELECT DISTINCT base_revision revision FROM efs_branches WHERE state=0",
        ),
        bindings: (after: Uint8Array) => [after, limit],
        scope: STORAGE_SCOPE_BRANCH,
      },
      {
        sql: "SELECT DISTINCT 0 kind,r.manifest_hash hash FROM efs_branch_manifest_roots r JOIN efs_branches b ON b.id=r.branch_id WHERE b.state=0 AND r.manifest_hash>? ORDER BY r.manifest_hash LIMIT ?",
        bindings: (after: Uint8Array) => [after, limit],
        scope: STORAGE_SCOPE_BRANCH,
      },
      {
        sql: "SELECT DISTINCT 0 kind,m.manifest_hash hash FROM efs_lease_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1) AND l.expires_at_ms>? AND m.manifest_hash>? ORDER BY m.manifest_hash LIMIT ?",
        bindings: (after: Uint8Array) => [run.evaluation_time_ms, after, limit],
        scope: STORAGE_SCOPE_OTHER,
      },
      {
        sql: "SELECT DISTINCT m.kind kind,m.manifest_hash hash FROM efs_lease_staged_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1) AND l.expires_at_ms>? AND m.manifest_hash>? ORDER BY m.manifest_hash LIMIT ?",
        bindings: (after: Uint8Array) => [run.evaluation_time_ms, after, limit],
        scope: STORAGE_SCOPE_OTHER,
      },
      {
        sql: "SELECT DISTINCT 2 kind,o.object_hash hash FROM efs_lease_objects o JOIN efs_leases l ON l.id=o.lease_id WHERE l.state IN (0,1) AND l.expires_at_ms>? AND o.object_hash>? ORDER BY o.object_hash LIMIT ?",
        bindings: (after: Uint8Array) => [run.evaluation_time_ms, after, limit],
        scope: STORAGE_SCOPE_OTHER,
      },
      {
        sql: "SELECT DISTINCT 0 kind,manifest_hash hash FROM efs_checkpoint_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
        bindings: (after: Uint8Array) => [after, limit],
        scope: STORAGE_SCOPE_MAIN,
      },
      {
        sql: historicManifestRootsSql(
          "SELECT DISTINCT revision FROM efs_operation_results WHERE outcome=1 AND revision IS NOT NULL AND expires_at_ms>?",
        ),
        bindings: (after: Uint8Array) => [after, run.evaluation_time_ms, limit],
        scope: STORAGE_SCOPE_MAIN,
      },
      {
        sql: "SELECT kind,root_id hash FROM efs_root_holds WHERE root_id>? ORDER BY root_id LIMIT ?",
        bindings: (after: Uint8Array) => [after, limit],
        scope: STORAGE_SCOPE_OTHER,
      },
    ] as const;
    const after = run.root_cursor ?? Uint8Array.of(0);
    const query = queries[run.root_kind];
    if (!query) throw new Error("ECORRUPT: invalid storage snapshot root kind");
    const rows = this.#tx.all<{ kind: number; hash: Uint8Array } & SqliteRow>(
      query.sql,
      query.bindings(after),
      { maxRows: limit, maxBytes },
    );
    for (const row of rows) this.addStorageMark(row.kind, row.hash, query.scope);
    if (rows.length === limit) {
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET root_cursor=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [rows.at(-1)!.hash],
      );
      return true;
    }
    if (run.root_kind < queries.length - 1) {
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET root_kind=root_kind+1,root_cursor=NULL,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
      return true;
    }
    const meta = this.#tx.all<
      {
        next_allocation_sequence: number;
        root_mutation_generation: number;
        last_root_removal_generation: number;
      } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation,last_root_removal_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    if (meta.root_mutation_generation !== run.root_generation) {
      this.#restartStorageSnapshot(
        meta,
        now,
        meta.last_root_removal_generation > run.root_generation,
        false,
      );
      return true;
    }
    const nextExpiry = this.#tx.all<{ expires: number | null } & SqliteRow>(
      "SELECT min(expires_at_ms) expires FROM (SELECT expires_at_ms FROM efs_leases WHERE state IN (0,1) AND expires_at_ms>? UNION ALL SELECT expires_at_ms FROM efs_operation_results WHERE outcome=1 AND revision IS NOT NULL AND length(encoded)>0 AND expires_at_ms>?)",
      [run.evaluation_time_ms, run.evaluation_time_ms],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.expires;
    this.#tx.run(
      "UPDATE efs_storage_snapshots SET state=2,mark_kind=0,mark_cursor=NULL,next_root_expiry_ms=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
      [nextExpiry ?? null],
    );
    return true;
  }
  storageMarks(limit: number, maxBytes: number): readonly StorageSnapshotMarkRow[] {
    return this.#tx.all<StorageSnapshotMarkRow>(
      "SELECT kind,hash,edge_cursor,accounted,scope_mask,CASE kind WHEN 0 THEN (SELECT length(encoded) FROM efs_manifest_roots WHERE hash=efs_storage_marks.hash) WHEN 1 THEN (SELECT length(encoded) FROM efs_manifest_nodes WHERE hash=efs_storage_marks.hash) ELSE (SELECT size FROM efs_cas_objects WHERE hash=efs_storage_marks.hash) END payload_size FROM efs_storage_marks WHERE processed=0 ORDER BY kind,hash LIMIT ?",
      [limit],
      { maxRows: limit, maxBytes },
    );
  }
  addStorageMark(kind: number, hash: Uint8Array, scopeMask: number): boolean {
    if (
      !Number.isSafeInteger(scopeMask) ||
      scopeMask <= 0 ||
      scopeMask > (STORAGE_SCOPE_MAIN | STORAGE_SCOPE_BRANCH | STORAGE_SCOPE_OTHER)
    )
      throw new RangeError("invalid storage snapshot scope mask");
    const existing = this.#tx.all<{ scope_mask: number } & SqliteRow>(
      "SELECT scope_mask FROM efs_storage_marks WHERE kind=? AND hash=?",
      [kind, hash],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    if (!existing) {
      this.#tx.run(
        "INSERT INTO efs_storage_marks(kind,hash,edge_cursor,processed,accounted,scope_mask) VALUES(?,?,0,0,0,?)",
        [kind, hash, scopeMask],
      );
      return true;
    }
    const expanded = existing.scope_mask | scopeMask;
    if (expanded === existing.scope_mask) return false;
    this.#tx.run(
      "UPDATE efs_storage_marks SET scope_mask=?,edge_cursor=0,processed=0 WHERE kind=? AND hash=?",
      [expanded, kind, hash],
    );
    return true;
  }
  accountStorageMark(kind: number, hash: Uint8Array, payloadBytes: number): boolean {
    if (!Number.isSafeInteger(payloadBytes) || payloadBytes < 0)
      throw new RangeError("invalid storage snapshot payload size");
    const marked = this.#tx.run(
      "UPDATE efs_storage_marks SET accounted=1 WHERE kind=? AND hash=? AND accounted=0",
      [kind, hash],
    );
    if (marked.changes !== 1) return false;
    const columns =
      kind === 0
        ? ["reachable_manifest_root_count", "reachable_manifest_root_bytes"]
        : kind === 1
          ? ["reachable_manifest_node_count", "reachable_manifest_node_bytes"]
          : ["reachable_object_count", "reachable_object_bytes"];
    this.#tx.run(
      `UPDATE efs_storage_snapshots SET ${columns[0]}=${columns[0]}+1,${columns[1]}=${columns[1]}+? WHERE singleton=1`,
      [payloadBytes],
    );
    return true;
  }
  storagePayloadSize(kind: number, hash: Uint8Array): number | undefined {
    const table =
      kind === 0
        ? "efs_manifest_roots"
        : kind === 1
          ? "efs_manifest_nodes"
          : "efs_cas_objects";
    const expression = kind === 2 ? "size" : "length(encoded)";
    return this.#tx.all<{ size: number } & SqliteRow>(
      `SELECT ${expression} size FROM ${table} WHERE hash=?`,
      [hash],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.size;
  }
  advanceStorageMark(
    kind: number,
    hash: Uint8Array,
    edgeCursor: number,
    processed: boolean,
  ): void {
    this.#tx.run(
      "UPDATE efs_storage_marks SET edge_cursor=?,processed=? WHERE kind=? AND hash=?",
      [edgeCursor, processed ? 1 : 0, kind, hash],
    );
    this.#tx.run(
      "UPDATE efs_storage_snapshots SET mark_kind=?,mark_cursor=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1 AND state=2",
      [kind, hash],
    );
  }
  reconcileStorageSnapshotGeneration(now: number): boolean {
    const run = this.storageSnapshot();
    if (!run) return false;
    const meta = this.#tx.all<
      {
        next_allocation_sequence: number;
        root_mutation_generation: number;
        last_root_removal_generation: number;
      } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation,last_root_removal_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    if (meta.root_mutation_generation === run.root_generation) return false;
    this.#restartStorageSnapshot(
      meta,
      now,
      meta.last_root_removal_generation > run.root_generation,
      false,
    );
    return true;
  }
  finishStorageMarking(now: number): boolean {
    if (this.reconcileStorageSnapshotGeneration(now)) return false;
    const run = this.storageSnapshot();
    if (!run || run.state !== 2)
      throw new Error("ECORRUPT: storage snapshot marking state changed");
    this.#tx.run(
      "UPDATE efs_storage_snapshots SET state=3,stored_kind=0,stored_cursor=0,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
      [],
    );
    return true;
  }
  storageStoredBatch(limit: number, maxBytes: number, now: number): boolean {
    if (this.reconcileStorageSnapshotGeneration(now)) return true;
    const run = this.storageSnapshot();
    if (!run || run.state !== 3)
      throw new Error("ECORRUPT: storage snapshot is not enumerating payload");
    const tables = [
      "efs_manifest_roots",
      "efs_manifest_nodes",
      "efs_cas_objects",
    ] as const;
    if (run.stored_kind >= tables.length) {
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET state=4,logical_cursor='',updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
      return true;
    }
    const table = tables[run.stored_kind]!;
    const size = run.stored_kind === 2 ? "size" : "length(encoded)";
    const rows = this.#tx.all<StoragePayloadRow>(
      `SELECT p.hash,${size} size,p.allocation_sequence,coalesce(m.scope_mask,0) scope_mask FROM ${table} p LEFT JOIN efs_storage_marks m ON m.kind=? AND m.hash=p.hash WHERE p.allocation_sequence>? AND p.allocation_sequence<=? ORDER BY p.allocation_sequence LIMIT ?`,
      [run.stored_kind, run.stored_cursor, run.high_water, limit],
      { maxRows: limit, maxBytes },
    );
    if (rows.length) {
      const countColumn =
        run.stored_kind === 0
          ? "stored_manifest_root_count"
          : run.stored_kind === 1
            ? "stored_manifest_node_count"
            : "stored_object_count";
      const bytesColumn =
        run.stored_kind === 0
          ? "stored_manifest_root_bytes"
          : run.stored_kind === 1
            ? "stored_manifest_node_bytes"
            : "stored_object_bytes";
      const bytes = rows.reduce((total, row) => total + row.size, 0);
      const exclusiveBytes = rows.reduce(
        (total, row) =>
          (row.scope_mask & STORAGE_SCOPE_BRANCH) !== 0 &&
          (row.scope_mask & STORAGE_SCOPE_MAIN) === 0
            ? total + row.size
            : total,
        0,
      );
      const exclusiveColumn =
        run.stored_kind === 0
          ? "branch_exclusive_manifest_root_bytes"
          : run.stored_kind === 1
            ? "branch_exclusive_manifest_node_bytes"
            : "branch_exclusive_object_bytes";
      this.#tx.run(
        `UPDATE efs_storage_snapshots SET ${countColumn}=${countColumn}+?,${bytesColumn}=${bytesColumn}+?,${exclusiveColumn}=${exclusiveColumn}+?,stored_cursor=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1`,
        [rows.length, bytes, exclusiveBytes, rows.at(-1)!.allocation_sequence],
      );
      return true;
    }
    if (run.stored_kind === tables.length - 1)
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET state=4,stored_kind=3,stored_cursor=0,logical_cursor='',updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
    else
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET stored_kind=stored_kind+1,stored_cursor=0,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
    return true;
  }
  storageLogicalBatch(limit: number, maxBytes: number, now: number): boolean {
    if (this.reconcileStorageSnapshotGeneration(now)) return true;
    const run = this.storageSnapshot();
    if (!run || run.state !== 4)
      throw new Error("ECORRUPT: storage snapshot is not enumerating namespace");
    if (!run.logical_complete) {
      const rows = this.#tx.all<StorageInodeRow>(
        "SELECT id,size FROM efs_inodes WHERE type=0 AND id>? ORDER BY id LIMIT ?",
        [run.logical_cursor, limit],
        { maxRows: limit, maxBytes },
      );
      if (rows.length) {
        const bytes = rows.reduce((total, row) => total + (row.size ?? 0), 0);
        this.#tx.run(
          "UPDATE efs_storage_snapshots SET logical_cursor=?,logical_bytes=logical_bytes+?,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
          [rows.at(-1)!.id, bytes],
        );
        return true;
      }
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET logical_complete=1,overlay_kind=0,overlay_branch_cursor='',overlay_inode_cursor='',overlay_sequence_cursor=-1,overlay_index_cursor=-1,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
      return true;
    }
    return this.#storageOverlayBatch(run, limit, maxBytes);
  }
  cleanupStorageMarks(limit: number, maxBytes: number, now: number): boolean {
    const run = this.storageSnapshot();
    if (!run || run.state !== 5)
      throw new Error("ECORRUPT: storage snapshot is not cleaning marks");
    const meta = this.#snapshotMeta();
    const rootExpired =
      run.next_root_expiry_ms !== null && run.next_root_expiry_ms <= now;
    if (meta.root_mutation_generation !== run.root_generation || rootExpired) {
      this.#restartStorageSnapshot(meta, now, true, false);
      return true;
    }
    const rows = this.#tx.all<StorageSnapshotMarkRow>(
      "SELECT kind,hash,edge_cursor,accounted,scope_mask FROM efs_storage_marks ORDER BY kind,hash LIMIT ?",
      [limit],
      { maxRows: limit, maxBytes },
    );
    for (const row of rows)
      this.#tx.run("DELETE FROM efs_storage_marks WHERE kind=? AND hash=?", [
        row.kind,
        row.hash,
      ]);
    if (!rows.length) this.#finishStorageSnapshot(run, meta, now);
    return rows.length > 0;
  }
  resetStorageMarksBatch(limit: number, maxBytes: number): boolean {
    const run = this.storageSnapshot();
    if (!run || run.state !== 7)
      throw new Error("ECORRUPT: storage snapshot is not resetting marks");
    const rows = this.#tx.all<StorageSnapshotMarkRow>(
      "SELECT kind,hash,edge_cursor,accounted,scope_mask FROM efs_storage_marks WHERE scope_mask<>0 OR processed<>1 OR accounted<>0 OR edge_cursor<>0 ORDER BY kind,hash LIMIT ?",
      [limit],
      { maxRows: limit, maxBytes },
    );
    for (const row of rows)
      this.#tx.run(
        "UPDATE efs_storage_marks SET edge_cursor=0,processed=1,accounted=0,scope_mask=0 WHERE kind=? AND hash=?",
        [row.kind, row.hash],
      );
    if (!rows.length)
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET state=1,root_kind=0,root_cursor=NULL,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
    return rows.length > 0;
  }
  physical(): {
    readonly pageCount: number;
    readonly pageSize: number;
    readonly freePages: number;
  } {
    return {
      pageCount: this.#scalar("SELECT page_count value FROM pragma_page_count"),
      pageSize: this.#scalar("SELECT page_size value FROM pragma_page_size"),
      freePages: this.#scalar("SELECT freelist_count value FROM pragma_freelist_count"),
    };
  }
  generation(): number {
    return this.#scalar(
      "SELECT root_mutation_generation value FROM efs_meta WHERE singleton=1",
    );
  }
  usageVerificationState(): {
    readonly mutationSequence: number;
    readonly counters: readonly number[];
  } {
    const snapshot = new UsageRepository(this.#tx, this.#limits).snapshot();
    return Object.freeze({
      mutationSequence: snapshot.mutation_sequence,
      counters: Object.freeze(USAGE_COUNTER_COLUMNS.map((column) => snapshot[column])),
    });
  }
  usageVerificationPhaseCount(): number {
    return USAGE_RECOUNT_PHASE_COUNT;
  }
  usageVerificationBatch(
    phase: number,
    afterKey: string | null,
    limit: number,
    maxBytes: number,
  ) {
    return new UsageRepository(this.#tx, this.#limits).recountBatch(
      phase,
      afterKey,
      limit,
      maxBytes,
    );
  }
  hashes(
    kind: "roots" | "nodes",
    after: Uint8Array,
    limit: number,
    maxBytes: number,
  ): readonly HashRow[] {
    const table = kind === "roots" ? "efs_manifest_roots" : "efs_manifest_nodes";
    return this.#tx.all<HashRow>(
      `SELECT hash,encoded FROM ${table} WHERE hash>? ORDER BY hash LIMIT ?`,
      [after, limit],
      { maxRows: limit, maxBytes },
    );
  }
  objects(after: Uint8Array, limit: number, maxBytes: number): readonly PayloadRow[] {
    return this.#tx.all<PayloadRow>(
      "SELECT hash,size,allocation_sequence FROM efs_cas_objects WHERE hash>? ORDER BY hash LIMIT ?",
      [after, limit],
      { maxRows: limit, maxBytes },
    );
  }
  inodes(after: string, limit: number, maxBytes: number): readonly InodeVerifyRow[] {
    return this.#tx.all<InodeVerifyRow>(
      "SELECT i.id,i.type,i.size,i.manifest_hash,i.nlink,(SELECT count(*) FROM efs_entries e WHERE e.inode_id=i.id) actual_links FROM efs_inodes i WHERE i.id>? ORDER BY i.id LIMIT ?",
      [after, limit],
      { maxRows: limit, maxBytes },
    );
  }
  pendingMarks(runId: string, limit: number, maxBytes: number): readonly GcMarkRow[] {
    return this.#tx.all<GcMarkRow>(
      "SELECT kind,hash,edge_cursor,CASE kind WHEN 0 THEN (SELECT length(encoded) FROM efs_manifest_roots WHERE hash=efs_gc_marks.hash) WHEN 1 THEN (SELECT length(encoded) FROM efs_manifest_nodes WHERE hash=efs_gc_marks.hash) ELSE (SELECT size FROM efs_cas_objects WHERE hash=efs_gc_marks.hash) END payload_size FROM efs_gc_marks WHERE run_id=? AND processed=0 ORDER BY kind,hash LIMIT ?",
      [runId, limit],
      { maxRows: limit, maxBytes },
    );
  }
  addMark(runId: string, kind: number, hash: Uint8Array): void {
    const result = this.#tx.run(
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) VALUES(?,?,?,0)",
      [runId, kind, hash],
    );
    void result;
  }
  advanceMark(
    runId: string,
    kind: number,
    hash: Uint8Array,
    edgeCursor: number,
    processed: boolean,
  ): void {
    this.#tx.run(
      "UPDATE efs_gc_marks SET edge_cursor=?,processed=? WHERE run_id=? AND kind=? AND hash=?",
      [edgeCursor, processed ? 1 : 0, runId, kind, hash],
    );
  }
  addExamined(runId: string, roots: number, nodes: number, objects: number): void {
    this.#tx.run(
      "UPDATE efs_gc_runs SET examined_roots=examined_roots+?,examined_nodes=examined_nodes+?,examined_objects=examined_objects+? WHERE id=?",
      [roots, nodes, objects, runId],
    );
  }
  addReclaimedOverlayBytes(runId: string, bytes: number): void {
    if (!Number.isSafeInteger(bytes) || bytes < 0)
      throw new RangeError("invalid reclaimed overlay byte count");
    if (!bytes) return;
    const changed = this.#tx.run(
      "UPDATE efs_gc_runs SET reclaimed_overlay_bytes=reclaimed_overlay_bytes+? WHERE id=? AND state<>7",
      [bytes, runId],
    );
    if (changed.changes !== 1)
      throw new Error(
        "ECORRUPT: garbage-collection run changed during overlay cleanup",
      );
  }
  seedRootsBatch(runId: string, limit: number, maxBytes: number): boolean {
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("invalid GC root batch limit");
    const run = this.#tx.all<
      {
        cursor_kind: number;
        cursor_value: Uint8Array | null;
        root_generation: number;
        created_at_ms: number;
      } & SqliteRow
    >(
      "SELECT cursor_kind,cursor_value,root_generation,created_at_ms FROM efs_gc_runs WHERE id=? AND state=0",
      [runId],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (!run) throw new Error("ECORRUPT: missing active garbage-collection run");
    if (!Number.isSafeInteger(run.cursor_kind) || run.cursor_kind < 0)
      throw new Error("ECORRUPT: invalid garbage-collection root cursor");
    if (run.cursor_kind >= 9) return this.#finishRootPass(runId, run.root_generation);
    // A one-byte zero BLOB sorts before every 32-byte digest, while avoiding
    // runtimes that normalize reconstructed empty typed-array views to NULL.
    const after = run.cursor_value ?? Uint8Array.of(0);
    const queries = [
      "SELECT DISTINCT 0 kind,manifest_hash hash FROM efs_inodes WHERE manifest_hash IS NOT NULL AND manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT 0 kind,manifest_hash hash FROM efs_revision_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT 0 kind,r.manifest_hash hash FROM efs_branch_manifest_roots r JOIN efs_branches b ON b.id=r.branch_id WHERE b.state=0 AND r.manifest_hash>? ORDER BY r.manifest_hash LIMIT ?",
      "SELECT DISTINCT 0 kind,m.manifest_hash hash FROM efs_lease_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1) AND l.expires_at_ms>? AND m.manifest_hash>? ORDER BY m.manifest_hash LIMIT ?",
      "SELECT DISTINCT m.kind kind,m.manifest_hash hash FROM efs_lease_staged_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1) AND l.expires_at_ms>? AND m.manifest_hash>? ORDER BY m.manifest_hash LIMIT ?",
      "SELECT DISTINCT 2 kind,o.object_hash hash FROM efs_lease_objects o JOIN efs_leases l ON l.id=o.lease_id WHERE l.state IN (0,1) AND l.expires_at_ms>? AND o.object_hash>? ORDER BY o.object_hash LIMIT ?",
      "SELECT DISTINCT 0 kind,manifest_hash hash FROM efs_checkpoint_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT 0 kind,r.manifest_hash hash FROM efs_operation_results o JOIN efs_revision_manifest_roots r ON r.revision=o.revision WHERE o.outcome=1 AND o.revision IS NOT NULL AND o.expires_at_ms>? AND r.manifest_hash>? ORDER BY r.manifest_hash LIMIT ?",
      "SELECT kind,root_id hash FROM efs_root_holds WHERE root_id>? ORDER BY root_id LIMIT ?",
    ] as const;
    const timed = run.cursor_kind >= 3 && run.cursor_kind <= 5;
    const rows = this.#tx.all<{ kind: number; hash: Uint8Array } & SqliteRow>(
      queries[run.cursor_kind]!,
      timed || run.cursor_kind === 7
        ? [run.created_at_ms, after, limit]
        : [after, limit],
      { maxRows: limit, maxBytes },
    );
    for (const row of rows) this.addMark(runId, row.kind, row.hash);
    if (rows.length === limit) {
      this.#tx.run("UPDATE efs_gc_runs SET cursor_value=? WHERE id=?", [
        rows.at(-1)!.hash,
        runId,
      ]);
      return false;
    }
    const nextKind = run.cursor_kind + 1;
    this.#tx.run("UPDATE efs_gc_runs SET cursor_kind=?,cursor_value=NULL WHERE id=?", [
      nextKind,
      runId,
    ]);
    return false;
  }
  sweepCandidates(
    runId: string,
    state: number,
    highWater: number,
    afterAllocationSequence: number,
    resultLimit: number,
    scanLimit: number,
    maxBytes: number,
  ): readonly PayloadRow[] {
    const table =
      state === 1
        ? "efs_manifest_roots"
        : state === 2
          ? "efs_manifest_nodes"
          : "efs_cas_objects";
    const kind = state - 1;
    const size = state === 3 ? "size" : "length(encoded)";
    const metadataRows =
      state === 1
        ? "1+CASE WHEN EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=efs_manifest_roots.hash) THEN 1 ELSE 0 END"
        : state === 2
          ? "1+CASE WHEN EXISTS(SELECT 1 FROM efs_manifest_subtree_summaries s WHERE s.node_hash=efs_manifest_nodes.hash) THEN 1 ELSE 0 END"
          : "1";
    const unreferenced =
      state === 1
        ? "NOT EXISTS(SELECT 1 FROM efs_inodes i WHERE i.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_revision_manifest_roots r WHERE r.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_checkpoint_manifest_roots c WHERE c.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_branch_manifest_roots b WHERE b.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_lease_manifests l WHERE l.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_lease_staged_manifests m WHERE m.kind=0 AND m.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_root_holds h WHERE h.kind=0 AND h.root_id=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_reused_subtrees s WHERE s.source_manifest_hash=efs_manifest_roots.hash)"
        : state === 2
          ? "NOT EXISTS(SELECT 1 FROM efs_lease_staged_manifests m WHERE m.kind=1 AND m.manifest_hash=efs_manifest_nodes.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_level_records l WHERE l.node_hash=efs_manifest_nodes.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_reused_subtrees s WHERE s.node_hash=efs_manifest_nodes.hash)"
          : "NOT EXISTS(SELECT 1 FROM efs_lease_objects l WHERE l.object_hash=efs_cas_objects.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_entries s WHERE s.object_hash=efs_cas_objects.hash)";
    return this.#tx.all<PayloadRow>(
      `SELECT hash,size,allocation_sequence,metadata_rows,eligible,scanned_count,scanned_through,eligible_count FROM (SELECT hash,size,allocation_sequence,metadata_rows,eligible,count(*) OVER () scanned_count,max(allocation_sequence) OVER () scanned_through,sum(eligible) OVER () eligible_count,row_number() OVER (ORDER BY allocation_sequence DESC) final_rank FROM (SELECT hash,${size} size,allocation_sequence,${metadataRows} metadata_rows,CASE WHEN NOT EXISTS(SELECT 1 FROM efs_gc_marks m WHERE m.run_id=? AND m.kind=? AND m.hash=${table}.hash) AND ${unreferenced} THEN 1 ELSE 0 END eligible FROM ${table} WHERE allocation_sequence>? AND allocation_sequence<=? ORDER BY allocation_sequence LIMIT ?)) WHERE eligible=1 OR (eligible_count=0 AND final_rank=1) ORDER BY allocation_sequence LIMIT ?`,
      [runId, kind, afterAllocationSequence, highWater, scanLimit, resultLimit],
      { maxRows: resultLimit, maxBytes },
    );
  }
  reconcileSweepGeneration(runId: string, state: number): boolean {
    const row = this.#tx.all<
      { state: number; root_generation: number; generation: number } & SqliteRow
    >(
      "SELECT r.state,r.root_generation,m.root_mutation_generation generation FROM efs_gc_runs r JOIN efs_meta m ON m.singleton=1 WHERE r.id=?",
      [runId],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (!row || row.state !== state)
      throw new Error("ECORRUPT: garbage-collection sweep state changed");
    if (row.root_generation === row.generation) return true;
    this.#tx.run(
      "UPDATE efs_gc_runs SET state=0,root_generation=?,cursor_kind=0,cursor_value=NULL WHERE id=? AND state=?",
      [row.generation, runId, state],
    );
    return false;
  }
  applySweep(
    runId: string,
    state: number,
    rows: readonly PayloadRow[],
    completeState: number,
    scannedThrough: number,
    scanComplete: boolean,
  ): void {
    const table =
      state === 1
        ? "efs_manifest_roots"
        : state === 2
          ? "efs_manifest_nodes"
          : "efs_cas_objects";
    let bytes = 0;
    let metadataRows = 0;
    for (const row of rows) {
      if (state === 1 && (row.metadata_rows ?? 1) === 2) {
        const validation = this.#tx.run(
          "DELETE FROM efs_manifest_validations WHERE manifest_hash=?",
          [row.hash],
        );
        if (validation.changes !== 1)
          throw new Error("ECORRUPT: manifest validation certificate changed");
      }
      if (state === 2 && (row.metadata_rows ?? 1) === 2) {
        const summary = this.#tx.run(
          "DELETE FROM efs_manifest_subtree_summaries WHERE node_hash=?",
          [row.hash],
        );
        if (summary.changes !== 1)
          throw new Error("ECORRUPT: manifest subtree summary changed");
      }
      this.#tx.run(`DELETE FROM ${table} WHERE hash=?`, [row.hash]);
      bytes += row.size;
      metadataRows += row.metadata_rows ?? 1;
    }
    if (rows.length) {
      const countColumn =
        state === 1
          ? "manifest_root_count"
          : state === 2
            ? "manifest_node_count"
            : "object_count";
      const byteColumn =
        state === 1
          ? "manifest_root_bytes"
          : state === 2
            ? "manifest_node_bytes"
            : "object_bytes";
      const deletedColumn =
        state === 1
          ? "deleted_roots"
          : state === 2
            ? "deleted_nodes"
            : "deleted_objects";
      const reclaimedColumn =
        state === 3 ? "reclaimed_object_bytes" : "reclaimed_manifest_bytes";
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          [countColumn]: -rows.length,
          [byteColumn]: -bytes,
          charged_metadata_bytes: -metadataRows * CHARGED_ROW_BYTES,
          maintenance_bytes: -rows.length * GC_MARK_RESERVATION_BYTES,
        },
        "garbage-collection sweep",
      );
      if (scanComplete) {
        const next = state === 3 ? completeState : state + 1;
        this.#tx.run(
          `UPDATE efs_gc_runs SET ${deletedColumn}=${deletedColumn}+?,${reclaimedColumn}=${reclaimedColumn}+?,state=?,cursor_kind=0 WHERE id=?`,
          [rows.length, bytes, next, runId],
        );
      } else
        this.#tx.run(
          `UPDATE efs_gc_runs SET ${deletedColumn}=${deletedColumn}+?,${reclaimedColumn}=${reclaimedColumn}+?,cursor_kind=? WHERE id=?`,
          [rows.length, bytes, scannedThrough, runId],
        );
    } else {
      if (scanComplete) {
        const next = state === 3 ? completeState : state + 1;
        this.#tx.run("UPDATE efs_gc_runs SET state=?,cursor_kind=0 WHERE id=?", [
          next,
          runId,
        ]);
      } else
        this.#tx.run("UPDATE efs_gc_runs SET cursor_kind=? WHERE id=?", [
          scannedThrough,
          runId,
        ]);
    }
  }
  cleanupMarks(runId: string, limit: number, nextState: number): boolean {
    const rows = this.#tx.all<{ kind: number; hash: Uint8Array } & SqliteRow>(
      "SELECT kind,hash FROM efs_gc_marks WHERE run_id=? ORDER BY kind,hash LIMIT ?",
      [runId, limit],
      { maxRows: limit, maxBytes: Math.max(256, limit * 128) },
    );
    for (const row of rows)
      this.#tx.run("DELETE FROM efs_gc_marks WHERE run_id=? AND kind=? AND hash=?", [
        runId,
        row.kind,
        row.hash,
      ]);
    if (!rows.length)
      this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [nextState, runId]);
    return rows.length > 0;
  }
  cleanupRootJournal(runId: string, limit: number, nextState: number): boolean {
    const safeGeneration = this.#tx.all<{ root_generation: number } & SqliteRow>(
      "SELECT root_generation FROM efs_gc_runs WHERE id=?",
      [runId],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.root_generation;
    if (!Number.isSafeInteger(safeGeneration) || safeGeneration! < 0)
      throw new Error("ECORRUPT: invalid garbage-collection root generation");
    const rows = this.#tx.all<{ generation: number; bytes: number } & SqliteRow>(
      "SELECT generation,length(root_id) bytes FROM efs_root_journal WHERE generation<=? ORDER BY generation LIMIT ?",
      [safeGeneration!, limit],
      { maxRows: limit, maxBytes: Math.max(256, limit * 160) },
    );
    let charged = 0;
    for (const row of rows) {
      this.#tx.run("DELETE FROM efs_root_journal WHERE generation=?", [row.generation]);
      charged += CHARGED_ROW_BYTES + row.bytes;
    }
    if (rows.length)
      new UsageRepository(this.#tx, this.#limits).apply(
        { maintenance_bytes: -charged },
        "root journal cleanup",
      );
    else this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [nextState, runId]);
    return rows.length > 0;
  }
  cleanupTerminalRuns(
    runId: string,
    limit: number,
    completeState: number,
    abandonedState: number,
    nextState: number,
  ): boolean {
    const prior = this.#tx.all<{ id: string } & SqliteRow>(
      "SELECT id FROM efs_gc_runs WHERE id<>? AND state IN (?,?) ORDER BY created_at_ms,id LIMIT 1",
      [runId, completeState, abandonedState],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!prior) {
      this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [nextState, runId]);
      return false;
    }
    const marks = this.#tx.all<{ kind: number; hash: Uint8Array } & SqliteRow>(
      "SELECT kind,hash FROM efs_gc_marks WHERE run_id=? ORDER BY kind,hash LIMIT ?",
      [prior.id, limit],
      { maxRows: limit, maxBytes: Math.max(256, limit * 128) },
    );
    for (const mark of marks)
      this.#tx.run("DELETE FROM efs_gc_marks WHERE run_id=? AND kind=? AND hash=?", [
        prior.id,
        mark.kind,
        mark.hash,
      ]);
    if (marks.length) return true;
    const deleted = this.#tx.run("DELETE FROM efs_gc_runs WHERE id=?", [prior.id]);
    if (deleted.changes !== 1)
      throw new Error("ECORRUPT: terminal garbage-collection run changed");
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: -runCharge(prior.id) },
      "terminal garbage-collection run cleanup",
    );
    return true;
  }
  #finishRootPass(runId: string, expectedGeneration: number): boolean {
    const generation = this.generation();
    if (generation !== expectedGeneration) {
      this.#tx.run(
        "UPDATE efs_gc_runs SET root_generation=?,cursor_kind=0,cursor_value=NULL WHERE id=?",
        [generation, runId],
      );
      return false;
    }
    this.#tx.run(
      "UPDATE efs_gc_runs SET state=1,cursor_kind=0,cursor_value=NULL WHERE id=? AND state=0",
      [runId],
    );
    return true;
  }
  #storageOverlayBatch(
    run: StorageSnapshotRunRow,
    limit: number,
    maxBytes: number,
  ): boolean {
    if (run.overlay_kind === 0) {
      const rows = this.#tx.all<
        {
          branch_id: string;
          inode_id: string;
          page_index: number;
          generation: number;
          bytes: number;
          reclaimable: number;
        } & SqliteRow
      >(
        "SELECT v.branch_id,v.inode_id,v.page_index,v.generation,length(v.bytes) bytes,CASE WHEN (NOT EXISTS(SELECT 1 FROM efs_cow_page_heads h WHERE h.branch_id=v.branch_id AND h.inode_id=v.inode_id AND h.page_index=v.page_index AND h.generation=v.generation) OR b.state<>0 OR EXISTS(SELECT 1 FROM efs_cow_page_heads h JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=v.branch_id AND h.inode_id=v.inode_id AND h.page_index=v.page_index AND h.generation=v.generation AND h.generation<=CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER))) AND NOT EXISTS(SELECT 1 FROM efs_lease_cow_pages l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=v.branch_id AND l.inode_id=v.inode_id AND l.page_index=v.page_index AND l.generation=v.generation) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=v.branch_id AND e.generation>=v.generation AND e.state IN (1,2)) THEN 1 ELSE 0 END reclaimable FROM efs_cow_page_versions v JOIN efs_branches b ON b.id=v.branch_id WHERE (v.branch_id,v.inode_id,v.page_index,v.generation)>(?,?,?,?) ORDER BY v.branch_id,v.inode_id,v.page_index,v.generation LIMIT ?",
        [
          run.overlay_branch_cursor,
          run.overlay_inode_cursor,
          run.overlay_sequence_cursor,
          run.overlay_index_cursor,
          limit,
        ],
        { maxRows: limit, maxBytes },
      );
      if (rows.length) {
        const bytes = rows.reduce((sum, row) => sum + row.bytes, 0);
        const reclaimable = rows.reduce(
          (sum, row) => sum + (row.reclaimable ? row.bytes : 0),
          0,
        );
        const last = rows.at(-1)!;
        this.#tx.run(
          "UPDATE efs_storage_snapshots SET stored_page_bytes=stored_page_bytes+?,reclaimable_overlay_bytes=reclaimable_overlay_bytes+?,overlay_branch_cursor=?,overlay_inode_cursor=?,overlay_sequence_cursor=?,overlay_index_cursor=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
          [
            bytes,
            reclaimable,
            last.branch_id,
            last.inode_id,
            last.page_index,
            last.generation,
          ],
        );
        return true;
      }
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET overlay_kind=1,overlay_branch_cursor='',overlay_inode_cursor='',overlay_sequence_cursor=-1,overlay_index_cursor=-1,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
      return true;
    }
    if (run.overlay_kind === 1) {
      const rows = this.#tx.all<
        {
          branch_id: string;
          inode_id: string;
          sequence: number;
          segment_index: number;
          bytes: number;
          reclaimable: number;
        } & SqliteRow
      >(
        "SELECT s.branch_id,s.inode_id,s.sequence,s.segment_index,length(s.bytes) bytes,CASE WHEN (b.state<>0 OR EXISTS(SELECT 1 FROM efs_branch_inode_overlays o WHERE o.branch_id=p.branch_id AND o.inode_id=p.inode_id AND CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER)>=p.generation)) AND NOT EXISTS(SELECT 1 FROM efs_lease_patches l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=p.branch_id AND l.inode_id=p.inode_id AND l.sequence=p.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=p.branch_id AND e.generation>=p.generation AND e.state IN (1,2)) THEN 1 ELSE 0 END reclaimable FROM efs_patch_segments s JOIN efs_patches p ON p.branch_id=s.branch_id AND p.inode_id=s.inode_id AND p.sequence=s.sequence JOIN efs_branches b ON b.id=p.branch_id WHERE (s.branch_id,s.inode_id,s.sequence,s.segment_index)>(?,?,?,?) ORDER BY s.branch_id,s.inode_id,s.sequence,s.segment_index LIMIT ?",
        [
          run.overlay_branch_cursor,
          run.overlay_inode_cursor,
          run.overlay_sequence_cursor,
          run.overlay_index_cursor,
          limit,
        ],
        { maxRows: limit, maxBytes },
      );
      if (rows.length) {
        const bytes = rows.reduce((sum, row) => sum + row.bytes, 0);
        const reclaimable = rows.reduce(
          (sum, row) => sum + (row.reclaimable ? row.bytes : 0),
          0,
        );
        const last = rows.at(-1)!;
        this.#tx.run(
          "UPDATE efs_storage_snapshots SET stored_patch_bytes=stored_patch_bytes+?,reclaimable_overlay_bytes=reclaimable_overlay_bytes+?,overlay_branch_cursor=?,overlay_inode_cursor=?,overlay_sequence_cursor=?,overlay_index_cursor=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
          [
            bytes,
            reclaimable,
            last.branch_id,
            last.inode_id,
            last.sequence,
            last.segment_index,
          ],
        );
        return true;
      }
      this.#tx.run(
        "UPDATE efs_storage_snapshots SET state=5,overlay_kind=2,updated_at_ms=updated_at_ms+1 WHERE singleton=1",
        [],
      );
      return true;
    }
    throw new Error("ECORRUPT: invalid storage snapshot overlay cursor");
  }
  #snapshotMeta(): {
    readonly next_allocation_sequence: number;
    readonly root_mutation_generation: number;
    readonly last_root_removal_generation: number;
  } {
    const meta = this.#tx.all<
      {
        next_allocation_sequence: number;
        root_mutation_generation: number;
        last_root_removal_generation: number;
      } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation,last_root_removal_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    return meta;
  }
  #finishStorageSnapshot(
    run: StorageSnapshotRunRow,
    meta: {
      readonly next_allocation_sequence: number;
      readonly root_mutation_generation: number;
      readonly last_root_removal_generation: number;
    },
    now: number,
  ): void {
    const usage = new UsageRepository(this.#tx, this.#limits).snapshot();
    const contentChanged =
      run.high_water !== meta.next_allocation_sequence - 1 ||
      run.stored_object_count !== usage.object_count ||
      run.stored_object_bytes !== usage.object_bytes ||
      run.stored_manifest_root_count !== usage.manifest_root_count ||
      run.stored_manifest_root_bytes !== usage.manifest_root_bytes ||
      run.stored_manifest_node_count !== usage.manifest_node_count ||
      run.stored_manifest_node_bytes !== usage.manifest_node_bytes ||
      run.stored_page_bytes !== usage.page_bytes ||
      run.stored_patch_bytes !== usage.patch_bytes;
    if (
      contentChanged ||
      meta.root_mutation_generation !== run.root_generation ||
      meta.last_root_removal_generation !== run.last_root_removal_generation
    ) {
      this.#restartStorageSnapshot(meta, now, true, false);
      return;
    }
    const revisions = this.#scalar("SELECT count(*) value FROM efs_revisions");
    this.#tx.run(
      "UPDATE efs_storage_snapshots SET state=6,result_bytes=?,charged_metadata_bytes=?,revision_count=?,updated_at_ms=updated_at_ms+1 WHERE singleton=1 AND state=5",
      [usage.result_bytes, usage.charged_metadata_bytes, revisions],
    );
  }
  #restartStorageSnapshot(
    meta: {
      readonly next_allocation_sequence: number;
      readonly root_mutation_generation: number;
      readonly last_root_removal_generation: number;
    },
    now: number,
    resetMarks: boolean,
    resetBatches: boolean,
  ): void {
    const highWater = meta.next_allocation_sequence - 1;
    if (!Number.isSafeInteger(highWater) || highWater < 0)
      throw new Error("ECORRUPT: invalid storage snapshot allocation high-water");
    this.#tx.run(
      `UPDATE efs_storage_snapshots SET state=?,high_water=?,root_generation=?,last_root_removal_generation=?,evaluation_time_ms=?,next_root_expiry_ms=NULL,root_kind=0,root_cursor=NULL,mark_kind=0,mark_cursor=NULL,stored_kind=0,stored_cursor=0,logical_cursor='',logical_complete=0,logical_bytes=0,overlay_kind=0,overlay_branch_cursor='',overlay_inode_cursor='',overlay_sequence_cursor=-1,overlay_index_cursor=-1,stored_page_bytes=0,stored_patch_bytes=0,reclaimable_overlay_bytes=0,result_bytes=0,charged_metadata_bytes=0,revision_count=0,stored_object_count=0,stored_object_bytes=0,stored_manifest_root_count=0,stored_manifest_root_bytes=0,stored_manifest_node_count=0,stored_manifest_node_bytes=0,branch_exclusive_object_bytes=0,branch_exclusive_manifest_root_bytes=0,branch_exclusive_manifest_node_bytes=0${resetMarks ? ",reachable_object_count=0,reachable_object_bytes=0,reachable_manifest_root_count=0,reachable_manifest_root_bytes=0,reachable_manifest_node_count=0,reachable_manifest_node_bytes=0" : ""}${resetBatches ? ",committed_batches=0,created_at_ms=?" : ""},updated_at_ms=? WHERE singleton=1`,
      resetBatches
        ? [
            resetMarks ? 7 : 1,
            highWater,
            meta.root_mutation_generation,
            meta.last_root_removal_generation,
            now,
            now,
            now,
          ]
        : [
            resetMarks ? 7 : 1,
            highWater,
            meta.root_mutation_generation,
            meta.last_root_removal_generation,
            now,
            now,
          ],
    );
  }
  #scalar(sql: string): number {
    const value = this.#tx.all<{ value: number } & SqliteRow>(sql, [], {
      maxRows: 1,
      maxBytes: 1024,
    })[0]?.value;
    if (typeof value !== "number") throw new Error("ECORRUPT: invalid scalar");
    return value;
  }
}
